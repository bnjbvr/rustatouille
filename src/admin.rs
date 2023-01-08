use std::{fs, sync::Arc};

use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
    Extension, Form,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use tracing as log;

use crate::{
    db::{Intervention, Service, Severity, Status},
    AppContext,
};

pub async fn index() -> Html<&'static str> {
    Html(include_str!("./view/admin.html"))
}

pub async fn create_service_form() -> Html<&'static str> {
    Html(include_str!("./view/new-service.html"))
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
        url: Some(payload.url),
    };

    {
        let mut conn = ctx.conn.lock().await;
        match Service::insert(&mut conn, &service).await {
            Ok(_id) => {}
            Err(err) => {
                log::error!("when inserting a new service: {err}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("Ohnoes, something went wrong!"),
                );
            }
        }
    }

    (
        StatusCode::CREATED,
        // TODO(fla) better page after creating
        Html(r#"<a href="/admin">It worked!</a>"#),
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
        let mut conn = ctx.conn.lock().await;
        match Service::get_all(&mut conn).await {
            Ok(s) => s,
            Err(err) => {
                log::error!("when retrieving services when creating an intervention: {err}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("Oh noes, something went wrong :(".to_string()),
                );
            }
        }
    };

    let services_string = services
        .into_iter()
        .map(|s| format!(r#"<option value="{}">{}</option>"#, s.id.unwrap(), s.name))
        .collect::<Vec<_>>()
        .join("\n");

    let page =
        include_str!("./view/new-intervention.html").replace("{{SERVICES}}", &services_string);

    (StatusCode::OK, Html(page))
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
        let mut conn = ctx.conn.lock().await;
        let int_id = match Intervention::insert(&mut conn, &intervention).await {
            Ok(id) => id,
            Err(err) => {
                log::error!("when inserting a new intervention: {err}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("Ohnoes, something went wrong!"),
                );
            }
        };

        for sid in payload.services {
            let Ok(service) = Service::by_id(sid as i64, &mut conn).await else {
                return (StatusCode::INTERNAL_SERVER_ERROR, Html("Ohnoes, something went wrong!"));
            };
            if service.is_none() {
                return (StatusCode::NOT_FOUND, Html("Service not found!"));
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
        Html(r#"<a href="/admin">It worked!</a>"#),
    )
}
