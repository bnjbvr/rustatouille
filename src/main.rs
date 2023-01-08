use anyhow::Context as _;
use axum::{
    routing::{get, post},
    Extension, Router,
};
use sqlx::AnyConnection;
use std::{env, net::Ipv4Addr, path::PathBuf, sync::Arc};
use std::{fs, net::SocketAddr};
use tokio::sync::Mutex;
use tracing as log;

mod admin;
mod db;
mod r#static;

pub(crate) struct AppConfig {
    /// which port the app is listening on
    port: u16,

    /// which ipv4 interface the app is listening on
    interface_ipv4: Ipv4Addr,

    /// Path to the cache directory
    cache_dir: PathBuf,

    /// Path to the sqlite file
    db_connection_string: String,
}

pub(crate) struct AppContext {
    config: AppConfig,
    conn: Mutex<AnyConnection>,
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

    Ok(AppConfig {
        port,
        interface_ipv4,
        cache_dir,
        db_connection_string,
    })
}

fn copy_static_files_to_cache_dir(ctx: &AppContext) -> anyhow::Result<()> {
    // Copy the style.
    let style = include_str!("./view/style.css");
    fs::write(ctx.config.cache_dir.join("style.css"), style)?;

    let style = include_str!("./view/admin.css");
    fs::write(ctx.config.cache_dir.join("admin.css"), style)?;

    Ok(())
}

async fn real_main() -> anyhow::Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let config = parse_app_config()?;

    let listen_addr = SocketAddr::from((config.interface_ipv4, config.port));

    let conn = db::open(&config.db_connection_string).await?;

    let ctx = Arc::new(AppContext {
        config,
        conn: Mutex::new(conn),
    });

    copy_static_files_to_cache_dir(&ctx)?;

    // build our application with a route
    let app = Router::new()
        .route("/admin", get(admin::index))
        .route("/admin/service/new", get(admin::create_service_form))
        .route(
            "/admin/intervention/new",
            get(admin::create_intervention_form),
        )
        .route("/admin/api/service", post(admin::create_service))
        .route("/admin/api/intervention", post(admin::create_intervention))
        .route("/*path", get(r#static::get)) // catch-all
        .layer(Extension(ctx));

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
