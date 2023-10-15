use crate::{
    db::models::{
        interventions::{Intervention, ServiceId},
        services::Service,
    },
    AppContext,
};
use anyhow::Context as _;
use serde::Serialize;
use std::{collections::BTreeMap, fs, sync::Arc, time::Instant};
use tokio::sync::mpsc;
use tracing as log;

/// Render context for a single intervention on a given service.
#[derive(Clone, Serialize)]
struct ServiceInterventionCtx {
    id: i64,
    title: String,
    start_date: String, // TODO?
    estimated_duration: String,
    description: Option<String>,
}

/// Render context for a given service.
#[derive(Serialize)]
struct ServiceCtx {
    id: i64,
    section_class: String,
    url: String,
    title: String,
    planned: Vec<ServiceInterventionCtx>,
    ongoing: Vec<ServiceInterventionCtx>,
}

/// Render context for a service associated to a given intervention.
#[derive(Clone, Serialize)]
struct InterventionServiceDetailsCtx {
    id: i64,
    title: String,
}

/// Render context for a given intervention.
#[derive(Clone, Serialize)]
struct InterventionCtx {
    id: i64,
    title: String,
    start_date: String, // TODO?
    severity_class: String,
    severity: String,
    estimated_duration: String,
    rendered_description: String,
    services: Vec<InterventionServiceDetailsCtx>,
}

#[derive(Serialize)]
struct RegenerateIndexCtx {
    ongoing: Vec<InterventionCtx>,
    planned: Vec<InterventionCtx>,
    interventions: Vec<InterventionCtx>,
    services: Vec<ServiceCtx>,
}

async fn regenerate_index(ctx: &Arc<AppContext>) -> anyhow::Result<()> {
    log::debug!("regenerating the index");
    let timer = Instant::now();

    let mut conn = ctx.db_connection.lock().await;

    // Internally within a single category, sort by priority: full outage > partial > performance
    let services = Service::get_all(&mut conn).await?;

    let mut interventions = Intervention::get_all(&mut conn).await?;

    // Sort interventions: most recent go first.
    interventions.sort_by_key(|int| -int.start_date.timestamp());

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
                    .or_default()
                    .push(intervention);

                let service = services
                    .iter()
                    .find(|service| service.id.unwrap() == service_id.0)
                    .context("unknown service with id {sid}")?;

                Ok(InterventionServiceDetailsCtx {
                    id: service.id.unwrap(),
                    title: service.name.clone(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        interventions_ctx.push(InterventionCtx {
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
        });
    }

    let mut services_ctx = Vec::with_capacity(services.len());
    for s in &services {
        let interventions = intervention_by_service
            .get(&ServiceId(s.id.unwrap()))
            .cloned()
            .unwrap_or_default();

        let section_class = if interventions.iter().any(|i| i.is_ongoing()) {
            "error"
        } else if interventions.iter().any(|i| i.is_planned()) {
            "warning"
        } else {
            "success"
        };

        let ctx_interventions: Vec<_> = interventions
            .into_iter()
            .map(|int| {
                let int_ctx = ServiceInterventionCtx {
                    id: int.id.unwrap(),
                    title: int.title.clone(),
                    start_date: int.start_date.to_string(),
                    description: int.description.clone(), // TODO markdown
                    estimated_duration: int
                        .estimated_duration
                        .map(|int| format!("{int} minutes")) // TODO i18n
                        .unwrap_or_else(|| "unknown".to_owned()), // TODO i18n
                };
                (int.is_ongoing(), int_ctx)
            })
            .collect();

        services_ctx.push(ServiceCtx {
            id: s.id.unwrap(),
            section_class: section_class.to_owned(),
            url: s.url.clone(),
            title: s.name.clone(),

            planned: ctx_interventions
                .iter()
                .filter_map(|(is_ongoing, int)| (!is_ongoing).then_some(int))
                .cloned()
                .collect(),

            ongoing: ctx_interventions
                .into_iter()
                .filter_map(|(is_ongoing, int)| is_ongoing.then_some(int))
                .collect(),
        });
    }

    // Current interventions are sorted because interventions are sorted.
    let mut ongoing_ctx = Vec::new();
    let mut planned_ctx = Vec::new();
    for (int, ctx) in interventions.iter().zip(interventions_ctx.iter()) {
        if int.is_ongoing() {
            ongoing_ctx.push(ctx.clone());
        } else {
            planned_ctx.push(ctx.clone());
        }
    }

    let index_ctx = tera::Context::from_serialize(RegenerateIndexCtx {
        ongoing: ongoing_ctx,
        planned: planned_ctx,
        interventions: interventions_ctx,
        services: services_ctx,
    })?;

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

                res = regenerate_index(&app) => {
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
