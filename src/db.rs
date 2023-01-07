use std::{fmt, str::FromStr};

use anyhow::Context as _;
use chrono::NaiveDateTime;
use sqlx::{AnyConnection, Connection, Executor};
use tracing as log;

#[derive(sqlx::FromRow)]
pub struct Service {
    id: Option<i64>,
    name: String,
    url: Option<String>,
}

impl Service {
    pub async fn insert(conn: &mut AnyConnection, s: &Service) -> anyhow::Result<i64> {
        let (id,) = sqlx::query_as::<_, (i64,)>(
            r#"
            INSERT INTO services (name, url) VALUES ($1, $2)
        "#,
        )
        .bind(&s.name)
        .bind(&s.url)
        .fetch_one(conn)
        .await?;
        Ok(id)
    }

    pub async fn get_all(conn: &mut AnyConnection) -> anyhow::Result<Vec<Service>> {
        let services = sqlx::query_as::<_, Service>(
            r#"
            SELECT id, name, url FROM services;
        "#,
        )
        .fetch_all(conn)
        .await?;
        Ok(services)
    }
}

#[derive(Debug)]
enum Status {
    Ongoing,
    UnderSurveillance,
    Identified,
    Resolved,
}

impl fmt::Display for Status {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(
            fmt,
            "{}",
            match *self {
                Self::Ongoing => "ongoing",
                Self::UnderSurveillance => "under_surveillance",
                Self::Identified => "identified",
                Self::Resolved => "resolved",
            }
        )
    }
}

impl FromStr for Status {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "ongoing" => Self::Ongoing,
            "under_surveillance" => Self::UnderSurveillance,
            "identified" => Self::Identified,
            "resolved" => Self::Resolved,
            _ => anyhow::bail!("unexpected value for status: {s}"),
        })
    }
}

#[derive(Debug)]
enum Severity {
    PartialOutage,
    FullOutage,
    PerformanceIssues,
}

impl fmt::Display for Severity {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(
            fmt,
            "{}",
            match *self {
                Self::PartialOutage => "partial_outage",
                Self::FullOutage => "full_outage",
                Self::PerformanceIssues => "performance_issues",
            }
        )
    }
}

impl FromStr for Severity {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "partial_outage" => Self::PartialOutage,
            "full_outage" => Self::FullOutage,
            "performance_issues" => Self::PerformanceIssues,
            _ => anyhow::bail!("unexpected value for severity: {s}"),
        })
    }
}

#[derive(Debug)]
pub struct Intervention {
    id: Option<i64>,
    start_date: NaiveDateTime,
    /// Estimated time it'll take to fix the issue, in minutes
    estimated_duration: Option<i64>,
    end_date: Option<NaiveDateTime>,
    status: Status,
    severity: Severity,
    is_planned: bool,
    title: String,
    description: Option<String>,
}

impl<'a, R: sqlx::Row> sqlx::FromRow<'a, R> for Intervention
where
    &'a std::primitive::str: sqlx::ColumnIndex<R>,
    String: sqlx::decode::Decode<'a, R::Database>,
    String: sqlx::types::Type<R::Database>,
    Option<String>: sqlx::decode::Decode<'a, R::Database>,
    Option<String>: sqlx::types::Type<R::Database>,
    i64: sqlx::decode::Decode<'a, R::Database>,
    i64: sqlx::types::Type<R::Database>,
    bool: sqlx::decode::Decode<'a, R::Database>,
    bool: sqlx::types::Type<R::Database>,
{
    fn from_row(row: &'a R) -> Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        let start_date: i64 = row.try_get("start_date")?;
        let start_date = NaiveDateTime::from_timestamp_opt(start_date, 0).unwrap();

        let estimated_duration: Option<i64> = row.try_get("estimated_duration")?;

        let end_date: Option<i64> = row.try_get("end_date")?;
        let end_date = end_date.and_then(|end_date| NaiveDateTime::from_timestamp_opt(end_date, 0));

        let status: String = row.try_get("status")?;
        let status = Status::from_str(&status).unwrap();

        let severity: String = row.try_get("severity")?;
        let severity = Severity::from_str(&severity).unwrap();

        let is_planned: bool = row.try_get("is_planned")?;
        let title: String = row.try_get("title")?;
        let description: Option<String> = row.try_get("description")?;

        let res = Intervention {
            id: Some(id),
            start_date,
            estimated_duration,
            end_date,
            status,
            severity,
            is_planned,
            title,
            description,
        };

        Ok(res)
    }
}

impl Intervention {
    pub async fn insert(conn: &mut AnyConnection, i: &Intervention) -> anyhow::Result<i64> {
        let (id, ) = sqlx::query_as::<_, (i64, )>(
            r#"
            INSERT INTO interventions
                (start_date, estimated_duration, end_date, status, severity, is_planned, title, description)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING ID;
        "#,
        )
            .bind(&i.start_date.timestamp())
            .bind(&i.estimated_duration)
            .bind(&i.end_date.map(|d| d.timestamp()))
            .bind(i.status.to_string())
            .bind(i.severity.to_string())
            .bind(&i.is_planned)
            .bind(&i.title)
            .bind(&i.description)
            .fetch_one(conn)
        .await?;
        Ok(id)
    }

    pub async fn remove_all(conn: &mut AnyConnection) -> anyhow::Result<()> {
        conn.execute("DELETE FROM interventions").await?;
        Ok(())
    }

    pub async fn get_all(conn: &mut AnyConnection) -> anyhow::Result<Vec<Intervention>> {
        let interventions = sqlx::query_as::<_, Intervention>(
            r#"
            SELECT * FROM interventions
        "#,
        )
        .fetch_all(conn)
        .await?;
        Ok(interventions)
    }

    pub async fn add_service(
        id: i64,
        service_id: i64,
        conn: &mut AnyConnection,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO interventions_services (service_id, intervention_id)
            VALUES ($1, $2)
        "#,
        )
        .bind(&service_id)
        .bind(&id)
        .execute(conn)
        .await?;
        Ok(())
    }

    pub async fn get_services(id: i64, conn: &mut AnyConnection) -> anyhow::Result<Vec<Service>> {
        let services = sqlx::query_as::<_, Service>(
            r#"
            SELECT s.id, s.name, s.url FROM services AS s, interventions_services AS is_, interventions
            WHERE interventions.id = $1
            AND s.id == is_.service_id
            AND interventions.id == is_.intervention_id
        "#,
        )
        .bind(&id)
        .fetch_all(conn)
        .await?;
        Ok(services)
    }
}

#[derive(sqlx::FromRow)]
pub struct Comment {
    date: NaiveDateTime,
    description: String,
}

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

async fn run_migrations(conn: &mut AnyConnection) -> anyhow::Result<()> {
    run_migration_1(conn).await?;
    Ok(())
}

async fn run_migration_1(conn: &mut AnyConnection) -> anyhow::Result<()> {
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

pub async fn open(path: &str) -> anyhow::Result<AnyConnection> {
    let mut conn = AnyConnection::connect(path)
        .await
        .context("when opening database")?;

    run_migrations(&mut conn).await?;

    Ok(conn)
}

#[allow(dead_code)]
async fn insert_fixtures(conn: &mut AnyConnection) -> anyhow::Result<()> {
    let framasphere = Service {
        id: None,
        name: String::from("Framasphere"),
        url: Some(String::from("https://diaspora-fr.org")),
    };

    let framathunes = Service {
        id: None,
        name: String::from("Framathunes"),
        url: None,
    };

    Service::insert(conn, &framasphere).await?;
    Service::insert(conn, &framathunes).await?;

    let services = Service::get_all(conn).await?;

    let mut framasphere = None;
    for s in services {
        println!("service {} @ {:?}", s.name, s.url);
        if s.name == "Framasphere" {
            framasphere = Some(s);
        }
    }
    let framasphere = framasphere.unwrap();

    Intervention::remove_all(conn).await?;

    let time = chrono::Utc::now().timestamp();
    let intervention = Intervention {
        id: None,
        start_date: NaiveDateTime::from_timestamp_opt(time, 0).unwrap(),
        estimated_duration: Some(20),
        end_date: None,
        status: Status::Identified,
        severity: Severity::FullOutage,
        is_planned: false,
        title: "Framasphère est inaccessible".to_owned(),
        description: Some("C'est la merde frère".to_owned()),
    };

    let int_id = Intervention::insert(conn, &intervention).await?;

    Intervention::add_service(int_id, framasphere.id.unwrap(), conn).await?;

    println!("intervention inserted with id {int_id}");

    let interventions = Intervention::get_all(conn).await?;

    for i in interventions {
        println!("intervention: {i:?}",);
        let services = Intervention::get_services(i.id.unwrap(), conn).await?;
        for s in services {
            println!("- affecting service {}", s.name);
        }
    }

    Ok(())
}
