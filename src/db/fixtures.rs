use chrono::NaiveDateTime;
use sqlx::AnyConnection;

use crate::db::models::{
    interventions::{Intervention, Severity, Status},
    services::Service,
};

#[allow(dead_code)]
pub async fn insert_fixtures(conn: &mut AnyConnection) -> anyhow::Result<()> {
    let framasphere = Service {
        id: None,
        name: String::from("Framasphere"),
        url: String::from("https://diaspora-fr.org"),
    };

    let framathunes = Service {
        id: None,
        name: String::from("Framathunes"),
        url: String::from("https://kresus.org"),
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
