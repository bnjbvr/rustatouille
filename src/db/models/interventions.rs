use chrono::NaiveDateTime;
use sqlx::AnyConnection;

#[derive(Clone, Copy, Debug)]
pub enum Severity {
    PartialOutage,
    FullOutage,
    PerformanceIssue,
}

impl Severity {
    pub fn to_css_class(self) -> &'static str {
        match self {
            Severity::PartialOutage => "partial-outage",
            Severity::FullOutage => "full-outage",
            Severity::PerformanceIssue => "performance-issue",
        }
    }

    // TODO i18n???
    pub fn label(&self) -> &str {
        match *self {
            Severity::PartialOutage => "Partial outage",
            Severity::FullOutage => "Full outage",
            Severity::PerformanceIssue => "Performance issue",
        }
    }

    fn to_db_str(self) -> &'static str {
        match self {
            Self::PartialOutage => "partial_outage",
            Self::FullOutage => "full_outage",
            Self::PerformanceIssue => "performance_issue",
        }
    }

    fn from_db_str(s: &str) -> anyhow::Result<Self> {
        Ok(match s {
            "partial_outage" => Self::PartialOutage,
            "full_outage" => Self::FullOutage,
            "performance_issue" => Self::PerformanceIssue,
            _ => anyhow::bail!("unexpected value for severity: {s}"),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Planned,
    Ongoing,
    UnderSurveillance,
    Identified,
    Resolved,
}

impl Status {
    // TODO i18n???
    pub fn label(&self) -> &str {
        match *self {
            Status::Planned => "Planned",
            Status::Ongoing => "Ongoing",
            Status::UnderSurveillance => "Under surveillance",
            Status::Identified => "Identified",
            Status::Resolved => "Resolved",
        }
    }

    fn to_db_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ongoing => "ongoing",
            Self::UnderSurveillance => "under_surveillance",
            Self::Identified => "identified",
            Self::Resolved => "resolved",
        }
    }

    fn from_db_str(s: &str) -> anyhow::Result<Self> {
        Ok(match s {
            "planned" => Self::Planned,
            "ongoing" => Self::Ongoing,
            "under_surveillance" => Self::UnderSurveillance,
            "identified" => Self::Identified,
            "resolved" => Self::Resolved,
            _ => anyhow::bail!("unexpected value for status: {s}"),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Intervention {
    pub id: Option<i64>,
    pub start_date: NaiveDateTime,
    /// Estimated time it'll take to fix the issue, in minutes
    pub estimated_duration: Option<i64>,
    pub end_date: Option<NaiveDateTime>,
    pub status: Status,
    pub severity: Severity,
    pub is_planned: bool,
    pub title: String,
    pub description: Option<String>,
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
        let status = Status::from_db_str(&status).unwrap();

        let severity: String = row.try_get("severity")?;
        let severity = Severity::from_db_str(&severity).unwrap();

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

#[derive(Eq, Ord, PartialEq, PartialOrd, Clone, Copy)]
pub struct ServiceId(pub i64);

impl<'a, R: sqlx::Row> sqlx::FromRow<'a, R> for ServiceId
where
    &'a std::primitive::str: sqlx::ColumnIndex<R>,
    i64: sqlx::decode::Decode<'a, R::Database>,
    i64: sqlx::types::Type<R::Database>,
{
    fn from_row(row: &'a R) -> Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        Ok(ServiceId(id))
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
            .bind(i.start_date.timestamp())
            .bind(i.estimated_duration)
            .bind(i.end_date.map(|d| d.timestamp()))
            .bind(i.status.to_db_str())
            .bind(i.severity.to_db_str())
            .bind(i.is_planned)
            .bind(&i.title)
            .bind(&i.description)
            .fetch_one(conn)
        .await?;
        Ok(id)
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
        .bind(service_id)
        .bind(id)
        .execute(conn)
        .await?;
        Ok(())
    }

    pub async fn get_service_ids(
        id: i64,
        conn: &mut AnyConnection,
    ) -> anyhow::Result<Vec<ServiceId>> {
        let ids = sqlx::query_as(
            r#"
            SELECT s.id FROM services AS s, interventions_services AS is_, interventions
            WHERE interventions.id = $1
            AND s.id == is_.service_id
            AND interventions.id == is_.intervention_id
        "#,
        )
        .bind(id)
        .fetch_all(conn)
        .await?;
        Ok(ids)
    }

    pub fn is_ongoing(&self) -> bool {
        self.status == Status::Ongoing
    }
    pub fn is_planned(&self) -> bool {
        self.status == Status::Planned
    }
}
