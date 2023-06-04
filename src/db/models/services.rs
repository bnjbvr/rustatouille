use sqlx::AnyConnection;

#[derive(sqlx::FromRow)]
pub struct Service {
    pub id: Option<i64>,
    pub name: String,
    pub url: String,
}

#[derive(sqlx::FromRow)]
pub struct ServiceWithNumInterventions {
    pub name: String,
    pub url: String,
    pub num_interventions: i64,
}

impl Service {
    pub async fn insert(conn: &mut AnyConnection, s: &Service) -> anyhow::Result<i64> {
        let (id,) = sqlx::query_as::<_, (i64,)>(
            r#"
            INSERT INTO services (name, url) VALUES ($1, $2) RETURNING id
        "#,
        )
        .bind(&s.name)
        .bind(&s.url)
        .fetch_one(conn)
        .await?;
        Ok(id)
    }

    pub async fn by_id(id: i64, conn: &mut AnyConnection) -> anyhow::Result<Option<Service>> {
        let services = sqlx::query_as::<_, Service>(
            r#"
            SELECT id, name, url FROM services WHERE id = $1;
        "#,
        )
        .bind(id)
        .fetch_optional(conn)
        .await?;
        Ok(services)
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

    pub async fn get_with_num_interventions(
        conn: &mut AnyConnection,
    ) -> anyhow::Result<Vec<ServiceWithNumInterventions>> {
        let services = sqlx::query_as::<_, ServiceWithNumInterventions>(
            r#"
            SELECT
                s.id,
                count(is_.id) as num_interventions,
                s.name,
                s.url
            FROM services as s
            LEFT JOIN interventions_services as is_ on s.id == is_.service_id
            GROUP BY s.id;
        "#,
        )
        .fetch_all(conn)
        .await?;
        Ok(services)
    }
}
