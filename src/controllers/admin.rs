use std::{fs, sync::Arc};

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    Extension, Form,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use tracing as log;

use crate::{
    db::{
        models::interventions::{Intervention, Severity, Status},
        models::services::Service,
    },
    AppContext,
};

macro_rules! try500 {
    ($val:expr, $ctx:literal) => {
        match $val {
            Ok(r) => r,
            Err(err) => {
                log::error!("error {}: {err}", $ctx);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("Ohnoes, something went wrong!").into_response(),
                );
            }
        }
    };
}

pub(crate) async fn index(Extension(ctx): Extension<Arc<AppContext>>) -> impl IntoResponse {
    let page = include_str!("../view/admin.html");

    let (services, interventions) = {
        let mut conn = ctx.db_connection.lock().await;
        let services = try500!(
            Service::get_with_num_interventions(&mut conn).await,
            "retrieving list of services for admin index"
        );

        let interventions = try500!(
            Intervention::get_all(&mut conn).await,
            "retrieveing list of interventions for admin index"
        );

        (services, interventions)
    };

    let page = page.replace(
        "{{SERVICE_FRAGMENTS}}",
        &services
            .into_iter()
            .map(|s| {
                include_str!("../view/service-fragment.html")
                    .replace("{service.url}", &s.url)
                    .replace("{service.title}", &s.name)
                    .replace(
                        "{service.interventions.length}",
                        &s.num_interventions.to_string(),
                    )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );

    let page = page.replace(
        "{{INTERVENTION_FRAGMENTS}}",
        &interventions
            .into_iter()
            .map(|i| {
                include_str!("../view/intervention-fragment.html")
                    .replace("{intervention.title}", &i.title)
                    .replace("{intervention.start_date}", &i.start_date.to_string())
                    .replace("{intervention.severity}", i.severity.kebab_case())
                    .replace("{intervention.severity.label}", &i.severity.to_string())
                    .replace(
                        "{intervention.estimated_duration}",
                        &i.estimated_duration
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "<null>".to_owned()),
                    )
                    .replace(
                        "{intervention.description}",
                        i.description.as_deref().unwrap_or("<null>"),
                    ) // TODO markdown!
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );

    (StatusCode::OK, Html(page).into_response())
}

pub async fn create_service_form() -> Html<&'static str> {
    Html(include_str!("../view/new-service.html"))
}

#[derive(Deserialize)]
pub struct CreateService {
    name: String,
    url: String,
}

pub(crate) async fn create_service(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateService` type
    Extension(ctx): Extension<Arc<AppContext>>,
    Form(payload): Form<CreateService>,
) -> impl IntoResponse {
    let service = Service {
        id: None,
        name: payload.name,
        url: payload.url,
    };

    {
        let mut conn = ctx.db_connection.lock().await;
        let s_id = Service::insert(&mut conn, &service).await;
        let _ = try500!(s_id, "inserting a new service");
    }

    let location = HeaderValue::from_static("/admin");
    (
        StatusCode::FOUND,
        [(header::LOCATION, location)].into_response(),
    )
}

#[derive(Deserialize)]
pub struct CreateIntervention {
    title: String,
    description: String,
    #[serde(rename(deserialize = "start-date"))]
    start_date: NaiveDateTime,
    estimated_duration: u64,
    severity: Severity,
    services: Vec<u64>,
}

pub(crate) async fn create_intervention_form(
    Extension(ctx): Extension<Arc<AppContext>>,
) -> impl IntoResponse {
    let services = {
        let mut conn = ctx.db_connection.lock().await;
        try500!(
            Service::get_all(&mut conn).await,
            "retrieving services when creating an intervention"
        )
    };

    let services_string = services
        .into_iter()
        .map(|s| format!(r#"<option value="{}">{}</option>"#, s.id.unwrap(), s.name))
        .collect::<Vec<_>>()
        .join("\n");

    let page =
        include_str!("../view/new-intervention.html").replace("{{SERVICES}}", &services_string);

    (StatusCode::OK, Html(page).into_response())
}

pub(crate) async fn create_intervention(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateIntervention` type
    Extension(ctx): Extension<Arc<AppContext>>,
    Form(payload): Form<CreateIntervention>,
) -> impl IntoResponse {
    let intervention = Intervention {
        id: None,
        title: payload.title,
        description: Some(payload.description),
        status: Status::Identified,
        start_date: payload.start_date,
        estimated_duration: Some(payload.estimated_duration as i64),
        end_date: None,
        severity: payload.severity,
        is_planned: false,
    };

    let id = {
        let mut conn = ctx.db_connection.lock().await;
        let int_id = try500!(
            Intervention::insert(&mut conn, &intervention).await,
            "when inserting a new intervention"
        );

        for sid in payload.services {
            let service = try500!(
                Service::by_id(sid as i64, &mut conn).await,
                "retrieving a service by id"
            );
            if service.is_none() {
                return (
                    StatusCode::NOT_FOUND,
                    Html("Service not found!").into_response(),
                );
            }
            if let Err(err) = Intervention::add_service(int_id, sid as i64, &mut conn).await {
                log::error!("when adding a service to an intervention: {err}");
            }
        }

        int_id
    };

    // TODO spawn a regenerate static page task

    let intervention_page = r#"
    <html>
        <head><title>rustatouille - {{title}}</title></head>
        <body>
            <h1>{{title}}</h1>
            <h3>{{date}}</h3>
            <p>{{description}}</p>
        </body>
    </html>
    "#
    .replace("{{title}}", &intervention.title)
    .replace(
        "{{description}}",
        intervention.description.as_ref().unwrap(),
    )
    .replace(
        "{{date}}",
        &DateTime::<Utc>::from_utc(intervention.start_date, Utc).to_rfc2822(),
    ); // TODO better date display

    let path = ctx.config.cache_dir.join(format!("{id}.html"));
    if let Err(err) = fs::write(&path, intervention_page) {
        log::error!("unable to write intervention page @ {path:?}: {err}");
    }

    // TODO regenerate index.html

    (
        StatusCode::CREATED,
        Html(r#"<a href="/admin">It worked!</a>"#).into_response(),
    )
}
