use crate::AppContext;
use axum::{
    extract::Path,
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    Extension,
};
use std::{fs, path::PathBuf, sync::Arc};
use tracing::log;

fn serve_static(path: &PathBuf) -> Result<impl IntoResponse, StatusCode> {
    // Read the content of the file as a string.
    // We won't have to support binary, right? RIGHT?
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            log::error!("unable to read file @ {path:?}: {err}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Be nice and add some content type so that the browser isn't lost.
    let content_type = HeaderValue::from_static(match path.extension().and_then(|s| s.to_str()) {
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("html") | Some("htm") => "text/html",
        _ => "text/plain",
    });

    Ok(([(header::CONTENT_TYPE, content_type)], content).into_response())
}

/// Get request for the root request in the dev-server. Should not be used in production.
pub(crate) async fn get_root(
    Extension(ctx): Extension<Arc<AppContext>>,
) -> Result<impl IntoResponse, StatusCode> {
    for p in &["index.htm", "index.html"] {
        let path = ctx.config.cache_dir.join(p);
        if path.exists() {
            return serve_static(&path);
        }
    }
    Err(StatusCode::NOT_FOUND)
}

/// Get request for the dev-server. Should not be used in production.
pub(crate) async fn get(
    Path(path): Path<String>,
    Extension(ctx): Extension<Arc<AppContext>>,
) -> Result<impl IntoResponse, StatusCode> {
    let mut path = ctx.config.cache_dir.join(&path);
    if !path.exists() || !path.is_file() {
        let mut found = false;
        for p in &["index.htm", "index.html"] {
            let new_path = path.join(p);
            if new_path.exists() {
                path = new_path;
                found = true;
            }
        }
        if !found {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    serve_static(&path)
}
