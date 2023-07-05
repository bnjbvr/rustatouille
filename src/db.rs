use anyhow::Context as _;
use sqlx::{AnyConnection, Connection};

mod fixtures;
mod migrations;
pub mod models;

pub use fixtures::insert_fixtures;

/// Open the database and run migrations at start.
pub async fn open(path: &str) -> anyhow::Result<AnyConnection> {
    let mut conn = AnyConnection::connect(path)
        .await
        .context("when opening database")?;

    migrations::run_migrations(&mut conn).await?;

    Ok(conn)
}
