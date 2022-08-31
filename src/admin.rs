use std::{fs, sync::Arc};

use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
    Extension, Form,
};
use serde::{Deserialize, Serialize};
use tracing as log;

use crate::AppConfig;

#[derive(Serialize, Deserialize)]
enum IncidentStatus {
    Ongoing,
    Identified,
    UnderSurveillance,
    Resolved,
}

#[derive(Serialize, Deserialize)]
enum IncidentCriticality {
    PartialOutage,
    FullOutage,
    PerformanceIssues,
}

#[derive(Serialize)]
struct Incident {
    id: u64,
    date: chrono::DateTime<chrono::Utc>,
    title: String,
    description: String,
    status: IncidentStatus,
    criticality: IncidentCriticality,
}

#[derive(Deserialize)]
pub struct CreateIncident {
    title: String,
    description: String,
    status: IncidentStatus,
    criticality: IncidentCriticality,
}

// basic handler that responds with a static string
pub async fn create_incident_form() -> Html<&'static str> {
    Html(
        r#"
    <html>
        <head>Statoo - create form</head>
        <body>
            <form action="/api/admin/incident" method="post">
                <p>Title <input type="text" name="title" /></p>

                <p>Description <textarea name="description"></textarea></p>

                <p>
                    Criticality
                    <select name="criticality">
                        <option value="PartialOutage">Partial outage</option>
                        <option value="FullOutage">Full outage</option>
                        <option value="PerformanceIssues">Performance issues</option>
                    </select>
                </p>

                <p>
                    Status
                    <select name="status">
                        <option value="Ongoing">Ongoing</option>
                        <option value="Identified">Identified</option>
                        <option value="UnderSurveillance">Under surveillance</option>
                        <option value="Resolved">Resolved</option>
                    </select>
                </p>

                <input type="submit" value="create">
            </form>
        </body>
    </html>
    "#,
    )
}

// basic handler that responds with a static string
pub(crate) async fn create_incident(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateIncident` type
    Form(payload): Form<CreateIncident>,
    Extension(config): Extension<Arc<AppConfig>>,
) -> impl IntoResponse {
    let incident = Incident {
        id: 0,
        date: chrono::Utc::now(),
        title: payload.title,
        description: payload.description,
        status: payload.status,
        criticality: payload.criticality,
    };

    // TODO save in db
    // TODO spawn a regenerate static page task

    let incident_page = r#"
    <html>
        <head><title>Statoo - {{title}}</title></head>
        <body>
            <h1>{{title}}</h1>
            <h3>{{date}}</h3>
            <p>{{description}}</p>
        </body>
    </html>
    "#
    .replace("{{title}}", &incident.title)
    .replace("{{description}}", &incident.description)
    .replace("{{date}}", &incident.date.to_rfc2822()); // TODO better date display

    let path = config.cache_dir.join(format!("{}.html", incident.id));
    if let Err(err) = fs::write(&path, incident_page) {
        log::error!("unable to write incident page @ {path:?}: {err}");
    }

    StatusCode::CREATED
}
