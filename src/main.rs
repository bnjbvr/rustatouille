use anyhow::Context as _;
use axum::{
    routing::{get, post},
    Extension, Router,
};
use sqlx::AnyConnection;
use std::{
    env,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
};
use std::{fs, net::SocketAddr};
use tera::Tera;
use tokio::sync::Mutex;
use tracing as log;

mod controllers;
mod db;

pub(crate) struct AppConfig {
    /// which port the app is listening on
    port: u16,

    /// which ipv4 interface the app is listening on
    interface_ipv4: Ipv4Addr,

    /// Path to the cache directory
    cache_dir: PathBuf,

    /// Path to the sqlite file
    db_connection_string: String,

    /// Should the server also respond to static queries, in dev mode?
    dev_server: bool,
}

pub(crate) struct AppContext {
    /// Static configuration for the application, derived from the environment variables.
    config: AppConfig,

    /// Connection pool to the database.
    db_connection: Mutex<AnyConnection>,

    /// Templates for dynamic pages.
    templates: Tera,
}

fn parse_app_config() -> anyhow::Result<AppConfig> {
    // override environment variables with contents of .env file, unless they were already set
    // explicitly.
    dotenvy::dotenv().ok();

    let port = env::var("PORT")
        .context("missing PORT variable")?
        .parse()
        .context("PORT isn't a u16 value")?;

    let interface_ipv4 = env::var("HOST")
        .context("missing HOST variable")?
        .parse()
        .context("HOST must be an ipv4 addr specification")?;

    let cache_dir = env::var("CACHE_DIR").context("missing CACHE_DIR env")?;
    let cache_dir = PathBuf::from(cache_dir);
    if !cache_dir.is_dir() {
        fs::create_dir(&cache_dir).context("couldn't create cache directory")?;
    }

    let db_connection_env =
        PathBuf::from(env::var("DB_CONNECTION").context("missing DB_CONNECTION")?);
    let db_connection_string = db_connection_env
        .to_str()
        .context("DB_CONNECTION doesn't designate an utf8 path")?
        .to_owned();

    let dev_server = env::var("DEV_SERVER")
        .context("missing DEV_SERVER env")?
        .to_lowercase();
    let dev_server = ["true", "yes", "y"].iter().any(|v| dev_server == *v);

    Ok(AppConfig {
        port,
        interface_ipv4,
        cache_dir,
        db_connection_string,
        dev_server,
    })
}

/// Copy the static files to the cache directory.
///
/// TODO: should also do it on change on disk, in the DEV_SERVER mode?
fn copy_static_files_to_cache_dir(cache_dir: &Path) -> anyhow::Result<()> {
    // Copy the style.
    let style = include_str!("../templates/style.css");
    fs::write(cache_dir.join("style.css"), style)?;

    let style = include_str!("../templates/admin.css");
    fs::write(cache_dir.join("admin.css"), style)?;

    Ok(())
}

async fn real_main() -> anyhow::Result<()> {
    // Initialize tracing.
    tracing_subscriber::fmt::init();

    // Parse the configuration.
    let config = parse_app_config()?;

    // Start the database.
    let conn = db::open(&config.db_connection_string).await?;

    let templates = Tera::new("templates/*.html").context("initializing tera")?;

    let ctx = Arc::new(AppContext {
        config,
        db_connection: Mutex::new(conn),
        templates,
    });

    // Generate the full web site initially.
    copy_static_files_to_cache_dir(&ctx.config.cache_dir)?;

    // Configure and start the web server.
    let mut app = Router::new()
        .route("/admin", get(controllers::admin::index))
        .route(
            "/admin/service/new",
            get(controllers::admin::create_service_form),
        )
        .route(
            "/admin/intervention/new",
            get(controllers::admin::create_intervention_form),
        )
        .route(
            "/admin/api/service",
            post(controllers::admin::create_service),
        )
        .route(
            "/admin/api/intervention",
            post(controllers::admin::create_intervention),
        );

    if ctx.config.dev_server {
        app = app.route("/*path", get(controllers::r#static::get)); // catch-all
    }

    app = app.layer(Extension(ctx.clone()));

    let listen_addr = SocketAddr::from((ctx.config.interface_ipv4, ctx.config.port));
    log::debug!("listening on {}", listen_addr);

    // This, in fact, will never return.
    axum::Server::bind(&listen_addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Since this function is under the tokio::main macro, rust-analyzer has issues with it. Put
    // the main in the real_main function instead.
    real_main().await
}
