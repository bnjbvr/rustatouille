use crate::AppContext;
use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse},
    Extension,
};
use std::{fs, sync::Arc};
use tracing::log;

pub(crate) async fn get(
    Path(path): Path<String>,
    Extension(ctx): Extension<Arc<AppContext>>,
) -> Result<impl IntoResponse, StatusCode> {
    let mut path = ctx.config.cache_dir.join(path);
    if !path.exists() || !path.is_file() {
        path = path.join("index.html");
        if !path.exists() {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            log::error!("unable to read file @ {path:?}: {err}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    Ok(Html(content))
}
