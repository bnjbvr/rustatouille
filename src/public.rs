use std::{fs, sync::Arc};

use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse},
    Extension,
};
use tracing::log;

use crate::AppContext;

pub(crate) async fn read_incident(
    Path(incident_id): Path<u64>,
    Extension(ctx): Extension<Arc<AppContext>>,
) -> Result<impl IntoResponse, StatusCode> {
    let path = ctx.config.cache_dir.join(format!("{incident_id}.html"));
    if !path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            log::error!("unable to read incident @ {path:?}: {err}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(Html(content))
}
