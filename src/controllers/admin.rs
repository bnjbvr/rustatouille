use axum::extract::RawForm;
use axum::response::Response;
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    Extension, Form,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use std::sync::Arc;
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
                log::error!("error when {}: {:?}", $ctx, err,);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("Ohnoes, something went wrong!").into_response(),
                );
            }
        }
    };
}

fn not_found(text: impl Into<String>) -> (StatusCode, Response) {
    (StatusCode::NOT_FOUND, Html(text.into()).into_response())
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

#[derive(Debug, Serialize)]
struct AdminRenderIntervention {
    pub id: Option<i64>,
    pub title: String,
    pub start_date: NaiveDateTime,
    pub end_date: Option<NaiveDateTime>,
    pub severity_css: String,
    pub severity_label: String,
    /// Estimated time it'll take to fix the issue, in minutes
    pub estimated_duration: Option<i64>,
    pub description: Option<String>,
    pub status: String,
    pub is_planned: String,
}

impl From<&Intervention> for AdminRenderIntervention {
    fn from(value: &Intervention) -> Self {
        Self {
            id: value.id,
            title: value.title.clone(),
            start_date: value.start_date,
            end_date: value.end_date,
            severity_css: value.severity.to_css_class().to_owned(),
            severity_label: value.severity.label().to_owned(),
            estimated_duration: value.estimated_duration,
            description: value.description.clone(),
            status: value.status.label().to_owned(),
            is_planned: value.is_planned.to_string(),
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
    let mut render_ctx = try500!(
        tera::Context::from_serialize(AdminTemplateCtx {
            interventions: interventions.iter().map(From::from).collect(),
            services,
        }),
        "preparing context for admin template"
    );

    {
        let toast = ctx.toast.write().unwrap().take();
        if let Some(t) = toast {
            render_ctx.insert("toast_success", &t);
        }
    }

    let page = try500!(
        ctx.templates
            .read()
            .unwrap()
            .render("admin.html", &render_ctx),
        "rendering admin template"
    );

    (StatusCode::OK, Html(page).into_response())
}

pub(crate) async fn create_service_form(
    Extension(ctx): Extension<Arc<AppContext>>,
) -> impl IntoResponse {
    let page = try500!(
        ctx.templates
            .read()
            .unwrap()
            .render("new-service.html", &tera::Context::new()),
        "rendering new-intervention template"
    );

    (StatusCode::OK, Html(page).into_response())
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

    if let Err(err) = ctx.regenerate_pages.send(()).await {
        log::error!("unable to regenerate page: {err:#}");
    }

    *ctx.toast.write().unwrap() = Some(format!("Service {} created!", service.name));

    redirect("/admin")
}

#[derive(Deserialize)]
pub struct FormIntervention {
    title: String,
    description: String,
    #[serde(rename = "start-date")]
    start_date: String,
    #[serde(rename = "estimated-duration")]
    estimated_duration: Option<i64>,
    severity: Severity,
    status: Status,
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

    #[derive(Serialize)]
    struct ServiceRenderCtx {
        id: i64,
        name: String,
    }

    #[derive(Serialize)]
    struct CreateInterventionFormRenderCtx {
        services: Vec<ServiceRenderCtx>,
    }

    let render_ctx = try500!(
        tera::Context::from_serialize(CreateInterventionFormRenderCtx {
            services: services
                .into_iter()
                .map(|s| ServiceRenderCtx {
                    id: s.id.unwrap(),
                    name: s.name,
                })
                .collect(),
        }),
        "preparing context for new-intervention template"
    );

    let page = try500!(
        ctx.templates
            .read()
            .unwrap()
            .render("new-intervention.html", &render_ctx),
        "rendering new-intervention template"
    );

    (StatusCode::OK, Html(page).into_response())
}

pub(crate) async fn create_intervention(
    Extension(ctx): Extension<Arc<AppContext>>,
    RawForm(request_bytes): RawForm,
) -> impl IntoResponse {
    let payload: FormIntervention = match serde_html_form::from_bytes(&request_bytes) {
        Ok(payload) => payload,
        Err(err) => {
            log::error!("error when parsing new-intervention request: {err:#}");
            return (
                StatusCode::BAD_REQUEST,
                Html("invalid request").into_response(),
            );
        }
    };

    let start_date = try500!(
        NaiveDateTime::parse_from_str(&payload.start_date, "%Y-%m-%dT%H:%M"),
        "converting start date to NaiveDateTime"
    );

    let intervention = Intervention {
        id: None,
        title: payload.title,
        description: Some(payload.description),
        status: payload.status,
        start_date,
        estimated_duration: payload.estimated_duration,
        end_date: None,
        severity: payload.severity,
        is_planned: payload.status == Status::Planned,
    };

    {
        let mut conn = ctx.db_connection.lock().await;

        // Check all the services exist before doing any write.
        for sid in &payload.services {
            let service = try500!(
                Service::by_id(*sid as i64, &mut conn).await,
                "retrieving a service by id"
            );
            if service.is_none() {
                return not_found(format!("Service with id {sid} doesn't exist!"));
            }
        }

        // All the services exists; confirm write.
        let int_id = try500!(
            Intervention::insert(&mut conn, &intervention).await,
            "creating a new intervention"
        );

        for sid in payload.services {
            if let Err(err) = Intervention::add_service(int_id, sid as i64, &mut conn).await {
                log::error!("when adding a service to an intervention: {err}");
            }
        }
    };

    // TODO i18n
    *ctx.toast.write().unwrap() = Some(format!("Intervention {} created!", intervention.title));

    if let Err(err) = ctx.regenerate_pages.send(()).await {
        log::error!("unable to regenerate page: {err:#}");
    }

    redirect("/admin")
}
