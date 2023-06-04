use std::error::Error as _;
use std::fs;
use std::sync::Arc;

use axum::response::Response;
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    Extension, Form,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing as log;

use crate::{
    db::{
        models::interventions::{Intervention, Severity, Status},
        models::services::{Service, ServiceWithNumInterventions},
    },
    AppContext,
};

macro_rules! try500 {
    ($val:expr, $ctx:literal) => {
        match $val {
            Ok(r) => r,
            Err(err) => {
                log::error!("error when {}: {}", $ctx, err,);
                if let Some(source) = err.source() {
                    log::error!("> caused by: {source}");
                }
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("Ohnoes, something went wrong!").into_response(),
                );
            }
        }
    };
}

fn not_found(text: &'static str) -> (StatusCode, Response) {
    return (StatusCode::NOT_FOUND, Html(text).into_response());
}

fn redirect(to_url: &'static str) -> (StatusCode, Response) {
    let location = HeaderValue::from_static(to_url);
    (
        StatusCode::FOUND,
        [(header::LOCATION, location)].into_response(),
    )
}

/// Status read from the form, using the "value" HTML fields.
#[derive(Deserialize)]
enum FormStatus {
    #[serde(rename = "ongoing")]
    Ongoing,
    #[serde(rename = "under-surveillance")]
    UnderSurveillance,
    #[serde(rename = "identified")]
    Identified,
    #[serde(rename = "resolved")]
    Resolved,
}

impl From<FormStatus> for Status {
    fn from(value: FormStatus) -> Self {
        match value {
            FormStatus::Ongoing => Self::Ongoing,
            FormStatus::UnderSurveillance => Self::UnderSurveillance,
            FormStatus::Identified => Self::Identified,
            FormStatus::Resolved => Self::Resolved,
        }
    }
}

/// Severity read from the form, using the "value" HTML fields.
#[derive(Deserialize)]
enum FormSeverity {
    #[serde(rename = "partial-outage")]
    PartialOutage,
    #[serde(rename = "full-outage")]
    FullOutage,
    #[serde(rename = "performance-issue")]
    PerformanceIssue,
}

impl From<FormSeverity> for Severity {
    fn from(value: FormSeverity) -> Self {
        match value {
            FormSeverity::PartialOutage => Self::PartialOutage,
            FormSeverity::FullOutage => Self::FullOutage,
            FormSeverity::PerformanceIssue => Self::PerformanceIssue,
        }
    }
}

#[derive(Debug, Serialize)]
struct AdminRenderIntervention {
    pub title: String,
    pub start_date: NaiveDateTime,
    pub severity_css: String,
    pub severity_label: String,
    /// Estimated time it'll take to fix the issue, in minutes
    pub estimated_duration: Option<i64>,
    pub description: Option<String>,
}

impl From<&Intervention> for AdminRenderIntervention {
    fn from(value: &Intervention) -> Self {
        Self {
            title: value.title.clone(),
            start_date: value.start_date.clone(),
            severity_css: value.severity.to_css_class().to_owned(),
            severity_label: value.severity.label().to_owned(),
            estimated_duration: value.estimated_duration,
            description: value.description.clone(),
        }
    }
}

#[derive(Serialize)]
struct AdminTemplateCtx {
    interventions: Vec<AdminRenderIntervention>,
    services: Vec<ServiceWithNumInterventions>,
}

pub(crate) async fn index(Extension(ctx): Extension<Arc<AppContext>>) -> impl IntoResponse {
    let (services, interventions) = {
        let mut conn = ctx.db_connection.lock().await;
        let services = try500!(
            Service::get_with_num_interventions(&mut conn).await,
            "retrieving list of services for admin index"
        );

        let interventions = try500!(
            Intervention::get_all(&mut conn).await,
            "retrieving list of interventions for admin index"
        );

        (services, interventions)
    };

    // TODO: render intervention.description as Markdown
    let render_ctx = try500!(
        tera::Context::from_serialize(AdminTemplateCtx {
            interventions: interventions.iter().map(From::from).collect(),
            services,
        }),
        "preparing context for admin template"
    );

    let page = try500!(
        ctx.templates.render("admin.html", &render_ctx),
        "rendering admin template"
    );

    (StatusCode::OK, Html(page).into_response())
}

pub async fn create_service_form() -> Html<&'static str> {
    Html(include_str!("../../templates/new-service.html"))
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
        let id = try500!(s_id, "inserting a new service");
        log::trace!("service {} created with id {}", service.name, id);
    }

    redirect("/admin")
}

#[derive(Deserialize)]
pub struct FormIntervention {
    title: String,
    description: String,
    #[serde(rename(deserialize = "start-date"))]
    start_date: String,
    #[serde(rename = "estimated-duration")]
    estimated_duration: u64,
    severity: FormSeverity,

    // TODO: support multiple services at once! but first, serde_urlencoded must be fixed :/
    //services: Vec<u64>,
    services: u64,
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

    let page = include_str!("../../templates/new-intervention.html")
        .replace("{{SERVICES}}", &services_string);

    (StatusCode::OK, Html(page).into_response())
}

pub(crate) async fn create_intervention(
    Extension(ctx): Extension<Arc<AppContext>>,
    Form(payload): Form<FormIntervention>,
) -> impl IntoResponse {
    // TODO check that it works also with non-Firefox browsers?
    let Ok(start_date) = NaiveDateTime::parse_from_str(&payload.start_date, "%Y-%m-%dT%H:%M") else {
        // Couldn't parse, likely invalid input.
        return (
            StatusCode::BAD_REQUEST,
            Html("Invalid start date").into_response(),
        );
    };

    let intervention = Intervention {
        id: None,
        title: payload.title,
        description: Some(payload.description),
        status: Status::Identified,
        start_date,
        estimated_duration: Some(payload.estimated_duration as i64),
        end_date: None,
        severity: payload.severity.into(),
        is_planned: false,
    };

    let id = {
        let mut conn = ctx.db_connection.lock().await;
        let int_id = try500!(
            Intervention::insert(&mut conn, &intervention).await,
            "when inserting a new intervention"
        );

        //for sid in payload.services {
        let sid = payload.services;

        let service = try500!(
            Service::by_id(sid as i64, &mut conn).await,
            "retrieving a service by id"
        );
        if service.is_none() {
            return not_found("Service not found!");
        }
        if let Err(err) = Intervention::add_service(int_id, sid as i64, &mut conn).await {
            log::error!("when adding a service to an intervention: {err}");
        }
        //}

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