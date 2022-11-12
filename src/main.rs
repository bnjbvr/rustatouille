use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Extension, Router,
};
use std::{env, net::Ipv4Addr, path::PathBuf, sync::Arc};
use std::{fs, net::SocketAddr};
use tracing as log;

mod admin;

const DEFAULT_PORT: u16 = 3000;
const DEFAULT_HOST: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);
const DEFAULT_CACHE_DIR: &str = "/tmp/rustatouille_cache";

pub(crate) struct AppConfig {
    /// which port the app is listening on
    port: u16,

    /// which ipv4 interface the app is listening on
    interface_ipv4: Ipv4Addr,

    /// Path to the cache directory
    cache_dir: PathBuf,
}

fn parse_app_config() -> AppConfig {
    // override environment variables with contents of .env file, unless they were already set
    // explicitly.
    dotenvy::dotenv().ok();

    let port = match env::var("PORT") {
        Ok(port_str) => {
            if let Ok(val) = port_str.parse() {
                val
            } else {
                log::error!("invalid port number: must be between 0 and 65535 - exiting");
                std::process::exit(1);
            }
        }
        Err(_) => {
            // use default
            DEFAULT_PORT
        }
    };

    let interface_ipv4 = match env::var("HOST") {
        Ok(interface_str) => {
            if let Ok(val) = interface_str.parse() {
                val
            } else {
                log::error!("invalid host {interface_str:?}");
                std::process::exit(1);
            }
        }
        Err(_) => {
            // use localhost
            DEFAULT_HOST
        }
    };

    let cache_dir = match env::var("CACHE_DIR") {
        Ok(dir_str) => {
            let path = PathBuf::from(dir_str);
            if !path.is_dir() {
                if let Err(err) = fs::create_dir(&path) {
                    log::error!("couldn't create cache directory {path:?}: {err}");
                    std::process::exit(1);
                }
            }
            path
        }
        Err(_) => PathBuf::from(DEFAULT_CACHE_DIR),
    };

    AppConfig {
        port,
        interface_ipv4,
        cache_dir,
    }
}

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let config = Arc::new(parse_app_config());

    // build our application with a route
    let app = Router::new()
        .route("/admin/incident/create", get(admin::create_incident_form))
        .route("/api/admin/incident", post(admin::create_incident))
        .route("/incidents/:incident_id", get(read_incident))
        .layer(Extension(config.clone()));

    // run our app with hyper
    let addr = SocketAddr::from((config.interface_ipv4, config.port));

    log::debug!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn read_incident(
    Path(incident_id): Path<u64>,
    Extension(config): Extension<Arc<AppConfig>>,
) -> Result<impl IntoResponse, StatusCode> {
    let path = config.cache_dir.join(format!("{incident_id}.html"));
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
