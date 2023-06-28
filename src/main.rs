use anyhow::Context as _;
use axum::{
    routing::{get, post},
    Extension, Router,
};
use axum_extra::routing::RouterExt as _;
use notify::{RecursiveMode, Watcher};
use sqlx::AnyConnection;
use std::{
    env,
    net::Ipv4Addr,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use std::{fs, net::SocketAddr};
use tera::Tera;
use tokio::sync::{mpsc, Mutex};
use tower_http::validate_request::ValidateRequestHeaderLayer;
use tracing as log;

mod controllers;
mod db;
mod regenerate;

pub(crate) struct AppConfig {
    /// which port the app is listening on
    port: u16,

    /// which ipv4 interface the app is listening on
    interface_ipv4: Ipv4Addr,

    /// Path to the cache directory
    cache_dir: PathBuf,

    /// Path to the templates.
    ///
    /// Defaults to "./templates".
    template_dir: PathBuf,

    /// Path to the sqlite file
    db_connection_string: String,

    /// Should the server also respond to static queries, in dev mode?
    dev_server: bool,

    /// What's the administrator password?
    admin_password: String,
}

pub(crate) struct AppContext {
    /// Static configuration for the application, derived from the environment variables.
    config: AppConfig,

    /// Connection pool to the database.
    db_connection: Mutex<AnyConnection>,

    /// Template engine for dynamic pages.
    templates: RwLock<Tera>,

    /// Service-wide (lol) toast notification.
    ///
    /// One toast should be enough for everyone, right?
    toast: RwLock<Option<String>>,

    regenerate_pages: mpsc::Sender<()>,
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

    let template_dir = env::var("TEMPLATE_DIR").unwrap_or_else(|_| "./templates/".to_owned());
    let template_dir = PathBuf::from(template_dir);
    if !template_dir.is_dir() {
        anyhow::bail!("the template directory doesn't exist");
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

    let admin_password = env::var("ADMIN_PASSWORD").context("missing ADMIN_PASSWORD env")?;

    Ok(AppConfig {
        port,
        interface_ipv4,
        cache_dir,
        template_dir,
        db_connection_string,
        dev_server,
        admin_password,
    })
}

/// Copy the static files to the cache directory.
fn copy_static_files_to_cache_dir(config: &AppConfig) -> anyhow::Result<()> {
    // Copy CSS and JavaScript files.
    for dir_entry in fs::read_dir(&config.template_dir)? {
        let dir_entry = dir_entry?;
        let path = dir_entry.path();
        if path
            .extension()
            .map_or(false, |ext| ext == "css" || ext == "js")
        {
            let Some(file_name) = path.file_name() else {
                log::warn!("Static file doesn't have a name??");
                continue;
            };
            fs::copy(&path, config.cache_dir.join(file_name))?;
        }
    }
    Ok(())
}

async fn real_main() -> anyhow::Result<()> {
    // Initialize tracing.
    tracing_subscriber::fmt::init();

    // Parse the configuration.
    let config = parse_app_config()?;

    // Start the database.
    let conn = db::open(&config.db_connection_string).await?;

    // Initialize the template engine.
    let templates = Tera::new(&config.template_dir.join("*.html").to_string_lossy())
        .context("initializing tera")?;

    let (sender, receiver) = mpsc::channel(128);

    let ctx = Arc::new(AppContext {
        config,
        db_connection: Mutex::new(conn),
        templates: RwLock::new(templates),
        toast: RwLock::new(None),
        regenerate_pages: sender,
    });

    tokio::spawn(regenerate::pages(ctx.clone(), receiver));

    // Generate the full web site initially.
    copy_static_files_to_cache_dir(&ctx.config)?;
    ctx.regenerate_pages.send(()).await?;

    // Configure and start the web server.
    let mut app = Router::new();

    let mut _watcher = None;
    if ctx.config.dev_server {
        app = app
            .route("/", get(controllers::r#static::get_root))
            .route("/*path", get(controllers::r#static::get)); // catch-all
        _watcher = Some(setup_hot_reload(ctx.clone()).await?);
    }

    let admin_router = Router::new()
        .route("/", get(controllers::admin::index))
        .route_with_tsr("/service/new", get(controllers::admin::create_service_form))
        .route_with_tsr(
            "/intervention/new",
            get(controllers::admin::create_intervention_form),
        )
        .route_with_tsr("/api/service", post(controllers::admin::create_service))
        .route_with_tsr(
            "/api/intervention",
            post(controllers::admin::create_intervention),
        )
        .route_layer(ValidateRequestHeaderLayer::basic(
            "admin",
            &ctx.config.admin_password,
        ));

    app = app.nest("/admin", admin_router);

    app = app.layer(Extension(ctx.clone()));

    let listen_addr = SocketAddr::from((ctx.config.interface_ipv4, ctx.config.port));
    log::info!("listening on {}", listen_addr);

    // This, in fact, will never return.
    axum::Server::bind(&listen_addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn setup_hot_reload(app: Arc<AppContext>) -> anyhow::Result<notify::RecommendedWatcher> {
    let rt_handle = tokio::runtime::Handle::current();

    let template_dir = app.config.template_dir.clone();

    let mut watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                match event.kind {
                    notify::EventKind::Create(_)
                    | notify::EventKind::Modify(_)
                    | notify::EventKind::Remove(_) => {
                        // Follow through.
                    }
                    notify::EventKind::Access(_)
                    | notify::EventKind::Any
                    | notify::EventKind::Other => {
                        // Nothing to do.
                        return;
                    }
                }

                // If any path is a CSS or HTML file,
                if event.paths.iter().any(|path| {
                    if let Some(ext) = path.extension() {
                        ext == "css" || ext == "html"
                    } else {
                        false
                    }
                }) {
                    let app = app.clone();

                    // spawn a task that will hot-reload the templates, and regenerate all the
                    // files.
                    rt_handle.spawn_blocking(move || {
                        log::info!("Hot-reloading the CSS!");
                        if let Err(err) = copy_static_files_to_cache_dir(&app.config) {
                            log::error!("error when reloading CSS: {err:#}");
                        }

                        log::info!("Hot-reloading the templates!");
                        if let Err(err) = app.templates.write().unwrap().full_reload() {
                            log::error!("error when reloading templates: {err:#}");
                        }

                        log::info!("Regenerating pages!");
                        if let Err(err) = app.regenerate_pages.blocking_send(()) {
                            log::error!("error when regenerating pages: {err:#}");
                        }
                    });
                }
            }
            Err(e) => tracing::warn!("watch error: {e:?}"),
        })?;

    watcher.watch(&template_dir, RecursiveMode::Recursive)?;

    Ok(watcher)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Since this function is under the tokio::main macro, rust-analyzer has issues with it. Put
    // the main in the real_main function instead.
    real_main().await
}
