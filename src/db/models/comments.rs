use chrono::NaiveDateTime;

#[allow(dead_code)]
#[derive(sqlx::FromRow)]
pub struct Comment {
    date: NaiveDateTime,
    description: String,
}
