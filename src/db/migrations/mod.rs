use sqlx::{AnyConnection, Executor as _};
use tracing::log;

mod m1;

async fn read_latest_migration(conn: &mut AnyConnection) -> anyhow::Result<i64> {
    let version: Result<(i64,), _> = sqlx::query_as("SELECT version FROM migrations;")
        .fetch_one(&mut *conn)
        .await;

    let version = match version {
        Ok((version,)) => version,
        Err(err) => {
            log::debug!("error when reading latest migration version: {err}, attempting to create the migrations table...");

            create_migration_table(conn).await?;

            let version: (i64,) = sqlx::query_as("SELECT version FROM migrations;")
                .fetch_one(&mut *conn)
                .await?;

            version.0
        }
    };

    Ok(version)
}

async fn create_migration_table(conn: &mut AnyConnection) -> anyhow::Result<()> {
    conn.execute(
        r#"
        CREATE TABLE migrations (
            version INT
        );"#,
    )
    .await?;

    conn.execute("INSERT INTO migrations (version) VALUES (0);")
        .await?;

    Ok(())
}

pub(super) async fn run_migrations(conn: &mut AnyConnection) -> anyhow::Result<()> {
    m1::run(conn).await?;
    Ok(())
}
