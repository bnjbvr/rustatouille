use anyhow::Context as _;
use sqlx::{AnyConnection, Executor as _};

use super::read_latest_migration;

/// Migration 1: initial version of the database.
pub(super) async fn run(conn: &mut AnyConnection) -> anyhow::Result<()> {
    let latest_version = read_latest_migration(conn).await?;
    if latest_version >= 1 {
        return Ok(());
    }

    conn.execute(
        r#"
            CREATE TABLE services (
                id INTEGER PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                url VARCHAR(255)
            );
        "#,
    )
    .await?;

    conn.execute(
        r#"
            CREATE TABLE interventions (
                id INTEGER PRIMARY KEY,
                start_date INTEGER NOT NULL,
                estimated_duration INTEGER,
                end_date INTEGER,
                status VARCHAR(63) NOT NULL,
                severity VARCHAR(63) NOT NULL,
                is_planned BOOLEAN NOT NULL,
                title VARCHAR(255) NOT NULL,
                description TEXT
            );
    "#,
    )
    .await?;

    conn.execute(
        r#"
            CREATE TABLE interventions_services (
                id INTEGER PRIMARY KEY,
                service_id INTEGER NOT NULL,
                intervention_id INTEGER NOT NULL,
                FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE,
                FOREIGN KEY (intervention_id) REFERENCES interventions(id) ON DELETE CASCADE
            );
            "#,
    )
    .await?;

    conn.execute(
        r#"
            CREATE TABLE comments (
                id INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                date INTEGER NOT NULL
            );
    "#,
    )
    .await?;

    conn.execute(
        r#"
            CREATE TABLE interventions_comments (
                id INTEGER PRIMARY KEY,
                intervention_id INTEGER NOT NULL,
                comment_id INTEGER NOT NULL,
                FOREIGN KEY (intervention_id) REFERENCES interventions(id) on DELETE CASCADE,
                FOREIGN KEY (comment_id) REFERENCES comments(id) on DELETE CASCADE
            );
        "#,
    )
    .await?;

    conn.execute("UPDATE migrations SET version = 1 WHERE version = 0;")
        .await
        .context("when upgrading db version number")?;

    Ok(())
}
