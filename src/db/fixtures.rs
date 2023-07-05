use chrono::NaiveDateTime;
use sqlx::AnyConnection;

use crate::db::models::{
    interventions::{Intervention, Severity, Status},
    services::Service,
};

const SERVICES: &[(&str, &str)] = &[
    ("DiasporaFr", "https://diaspora-fr"),
    ("Kresus", "https://kresus.org"),
    ("Framapad", "https://framapad.org"),
    ("Framacalc", "https://framacalc.org"),
    ("Framamap", "https://framamap.org"),
    ("Framavox", "https://framavox.org"),
    ("Framapiaf", "https://framapiaf.org"),
];

const LOREM_IPSUM: &str = r#"
    Contrary to popular belief, Lorem Ipsum is not simply random text. It has roots in a piece of classical Latin literature from 45 BC, making it over 2000 years old. Richard McClintock, a Latin professor at Hampden-Sydney College in Virginia, looked up one of the more obscure Latin words, consectetur, from a Lorem Ipsum passage, and going through the cites of the word in classical literature, discovered the undoubtable source. Lorem Ipsum comes from sections 1.10.32 and 1.10.33 of "de Finibus Bonorum et Malorum" (The Extremes of Good and Evil) by Cicero, written in 45 BC. This book is a treatise on the theory of ethics, very popular during the Renaissance. The first line of Lorem Ipsum, "Lorem ipsum dolor sit amet..", comes from a line in section 1.10.32.
"#;

const NUM_INTERVENTIONS: usize = 200;

pub async fn insert_fixtures(conn: &mut AnyConnection) -> anyhow::Result<()> {
    let mut service_ids = Vec::new();
    for s in SERVICES {
        let id = Service::insert(
            conn,
            &Service {
                id: None,
                name: s.0.to_owned(),
                url: s.1.to_owned(),
            },
        )
        .await?;
        service_ids.push(id);
    }

    for i in 0..NUM_INTERVENTIONS {
        let time_s = chrono::Utc::now().timestamp();

        let estimated_duration = (10 + i * 20) % 150;
        let start_date = time_s - 60 + (100 * (i as i64 % 5));
        let status = match i % 4 {
            0 => Status::Ongoing,
            1 => Status::UnderSurveillance,
            2 => Status::Identified,
            3 => Status::Resolved,
            _ => unreachable!(),
        };
        let severity = match (i + 1) % 3 {
            0 => Severity::PartialOutage,
            1 => Severity::FullOutage,
            2 => Severity::PerformanceIssue,
            _ => unreachable!(),
        };
        let title = match i % 5 {
            0 => "Panne de réveil",
            1 => "Petite forme",
            2 => "Juste la flemme",
            3 => "Serveur en grève",
            4 => "Effondrement de la société occidentale",
            _ => unreachable!(),
        };

        let intervention = Intervention {
            id: None,
            start_date: NaiveDateTime::from_timestamp_opt(start_date, 0).unwrap(),
            estimated_duration: Some(estimated_duration as i64),
            end_date: None,
            status,
            severity,
            is_planned: false,
            title: title.to_owned(),
            description: Some(LOREM_IPSUM.to_owned()),
        };

        let int_id = Intervention::insert(conn, &intervention).await?;

        let num_services = if i % 2 == 0 { 1 } else { i % 5 };
        let mut service_ids = service_ids.clone();
        for j in 0..num_services {
            let service_id = service_ids.remove((j + i + 7) % service_ids.len());
            Intervention::add_service(int_id, service_id, conn).await?;
        }
    }

    Ok(())
}
