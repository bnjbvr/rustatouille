use crate::{
    db::models::{
        interventions::{Intervention, ServiceId},
        services::Service,
    },
    AppContext,
};
use anyhow::Context as _;
use chrono::NaiveDateTime;
use serde::Serialize;
use std::{collections::BTreeMap, fs, sync::Arc, time::Instant};
use tokio::sync::mpsc;
use tracing as log;

#[derive(Serialize)]
struct ShortInterventionCtx {
    id: i64,
    title: String,
    start_date: String, // TODO?
}

#[derive(Serialize)]
struct ServiceCtx {
    section_class: String,
    url: String,
    title: String,
    planned: Vec<ShortInterventionCtx>,
    ongoing: Vec<ShortInterventionCtx>,
}

#[derive(Clone, Serialize)]
struct InterventionCtx {
    id: i64,
    title: String,
    start_date: String, // TODO?
    severity_class: String,
    severity: String,
    estimated_duration: String,
    rendered_description: String,
    services: Vec<String>,
}

#[derive(Serialize)]
struct RegenerateIndexCtx {
    current_interventions: Vec<InterventionCtx>,
    interventions: Vec<InterventionCtx>,
    services: Vec<ServiceCtx>,
}

async fn regenerate_index(ctx: &Arc<AppContext>) -> anyhow::Result<()> {
    log::debug!("regenerating the index");
    let timer = Instant::now();

    let mut conn = ctx.db_connection.lock().await;

    // TODO sort services like that:
    // - first, all that have *current* interventions
    // - second, all that have *planned* interventions
    // - finally, those that have *no* interventions
    //
    // Internally within a single category, sort by priority: full outage > partial > performance
    let services = Service::get_all(&mut conn).await?;

    let interventions = Intervention::get_all(&mut conn).await?;
    let mut intervention_by_service: BTreeMap<ServiceId, Vec<&Intervention>> = BTreeMap::new();

    let mut interventions_ctx = Vec::with_capacity(interventions.len());
    for intervention in &interventions {
        let affected_services =
            Intervention::get_service_ids(intervention.id.unwrap(), &mut conn).await?;

        // linear search ftw
        let service_names = affected_services
            .into_iter()
            .map(|service_id| {
                intervention_by_service
                    .entry(service_id)
                    .or_insert_with(Default::default)
                    .push(intervention);

                Ok(services
                    .iter()
                    .find(|service| service.id.unwrap() == service_id.0)
                    .context("unknown service with id {sid}")?
                    .name
                    .clone())
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let ictx = InterventionCtx {
            id: intervention.id.unwrap(),
            title: intervention.title.clone(),
            start_date: intervention.start_date.to_string(),
            severity_class: intervention.severity.to_css_class().to_owned(),
            severity: intervention.severity.label().to_owned(),
            estimated_duration: intervention
                .estimated_duration
                .map(|int| format!("{int} minutes")) // TODO i18n
                .unwrap_or_else(|| "unknown".to_owned()), // TODO i18n
            rendered_description: intervention
                .description // TODO render as markdown?
                .clone()
                .unwrap_or_else(|| "<???>".to_owned()), // TODO???
            services: service_names,
        };
        interventions_ctx.push(ictx);
    }

    let now = NaiveDateTime::from_timestamp_opt(chrono::Utc::now().timestamp(), 0).unwrap();

    let mut services_ctx = Vec::with_capacity(services.len());
    for s in &services {
        let (planned, ongoing) = intervention_by_service
            .get(&ServiceId(s.id.unwrap()))
            .map(|interventions| {
                let planned = interventions
                    .iter()
                    .filter(|int| int.is_planned(now))
                    .collect();
                let ongoing = interventions
                    .iter()
                    .filter(|int| int.is_ongoing(now))
                    .collect();

                (planned, ongoing)
            })
            .unwrap_or_else(|| (vec![], vec![]));

        let section_class = if !ongoing.is_empty() {
            "error"
        } else if !planned.is_empty() {
            "warning"
        } else {
            "success"
        };

        services_ctx.push(ServiceCtx {
            section_class: section_class.to_owned(),
            url: s.url.clone(),
            title: s.name.clone(),

            planned: planned
                .into_iter()
                .map(|int| ShortInterventionCtx {
                    id: int.id.unwrap(),
                    title: int.title.clone(),
                    start_date: int.start_date.to_string(),
                })
                .collect(),

            ongoing: ongoing
                .into_iter()
                .map(|int| ShortInterventionCtx {
                    id: int.id.unwrap(),
                    title: int.title.clone(),
                    start_date: int.start_date.to_string(),
                })
                .collect(),
        });
    }

    // TODO sort them by start date?
    let current_interventions_ctx = interventions
        .iter()
        .zip(interventions_ctx.iter())
        .filter_map(|(i, ictx)| {
            if i.is_ongoing(now) {
                Some(ictx.clone())
            } else {
                None
            }
        })
        .collect();

    let index_ctx = RegenerateIndexCtx {
        current_interventions: current_interventions_ctx,
        interventions: interventions_ctx,
        services: services_ctx,
    };
    let index_ctx = tera::Context::from_serialize(index_ctx)?;

    let index_content = ctx
        .templates
        .read()
        .unwrap()
        .render("index.html", &index_ctx)?;

    fs::write(ctx.config.cache_dir.join("index.html"), index_content)?;

    log::debug!(
        "regenerating the index took {}ms",
        timer.elapsed().as_millis()
    );

    Ok(())
}

#[derive(Serialize)]
struct FeedEntryCtx {
    title: String,
    /// TODO generate once and store in db?
    feed_entry_id: String,
    author: String,
    link: String,
    published: String,
    updated: Option<String>,
    content: String,
}

#[derive(Serialize)]
struct FeedCtx {
    title: String,
    feed_url: String,
    page_url: String,

    /// Example: urn:uuid:[...]
    /// TODO maybe generate once and store it in the db, in a global kv table?
    feed_id: String,

    /// Example: 2023-06-12T14:35:00+02:00
    update_date: String,

    entries: FeedEntryCtx,
}

async fn regenerate_feed(ctx: &Arc<AppContext>) -> anyhow::Result<()> {
    // TODO
    Ok(())
}

async fn regenerate_all(ctx: &Arc<AppContext>) -> anyhow::Result<()> {
    regenerate_index(ctx).await?;
    regenerate_feed(ctx).await?;
    Ok(())
}

pub(crate) async fn pages(app: Arc<AppContext>, mut receiver: mpsc::Receiver<()>) {
    let mut start = false;

    // Small mechanism to regenerate all the pages, at most once at a time:
    // - either wait for a start message,
    // - or, start a task and wait for another start message; if the latter arrives, then restart
    // the loop immediately.

    loop {
        if start {
            tokio::select! {
                biased;

                _ = receiver.recv() => {
                    // Reaching this arm will interrupt the other branch, and restart it all.
                    start = true;
                    continue;
                }

                res = regenerate_all(&app) => {
                    start = false;
                    if let Err(err) = res {
                        log::error!("Unable to render the index: {err:#}");
                    }
                }
            }
        } else {
            let received = receiver.recv().await;
            match received {
                Some(_) => {
                    // On the next iteration, start an actual regenerate task.
                    start = true;
                }
                None => {
                    // okthxbye
                    break;
                }
            }
        }
    }
}
