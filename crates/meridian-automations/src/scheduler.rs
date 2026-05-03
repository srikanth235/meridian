//! Periodic tick that finds due automations, claims them, and dispatches.

use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};

use meridian_store::Store;

use crate::executor::execute;
use crate::schedule::{backoff_for, Schedule};
use crate::sdk::SdkSurface;

const TICK_INTERVAL_SECS: u64 = 5;
const SEEN_KEY_TTL_DAYS: i64 = 90;

#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    RunStarted { automation_id: String, run_id: i64 },
    RunFinished {
        automation_id: String,
        run_id: i64,
        success: bool,
    },
}

#[derive(Clone)]
pub struct SchedulerConfig {
    pub surface: SdkSurface,
}

pub async fn run(
    cfg: SchedulerConfig,
    store: Arc<Store>,
    events: broadcast::Sender<SchedulerEvent>,
) {
    if let Err(e) = store.clear_running_automations().await {
        warn!(error = %e, "failed to clear orphaned running flags at startup");
    }

    let mut prune_ticker =
        tokio::time::interval(Duration::from_secs(60 * 60));
    prune_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tick(&cfg, &store, &events).await;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(TICK_INTERVAL_SECS)) => {}
            _ = prune_ticker.tick() => {
                let cutoff = Utc::now() - chrono::Duration::days(SEEN_KEY_TTL_DAYS);
                if let Err(e) = store.prune_seen_keys(cutoff).await {
                    warn!(error = %e, "prune_seen_keys failed");
                }
            }
        }
    }
}

async fn tick(
    cfg: &SchedulerConfig,
    store: &Arc<Store>,
    events: &broadcast::Sender<SchedulerEvent>,
) {
    let now = Utc::now();
    let rows = match store.list_automations().await {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "list_automations failed");
            return;
        }
    };
    for row in rows {
        if !row.enabled {
            continue;
        }
        if row.parse_error.is_some() {
            continue;
        }
        if row.running_since.is_some() {
            continue;
        }
        let due = row
            .next_run_at
            .map(|t| t <= now)
            .unwrap_or(true);
        if !due {
            continue;
        }
        let claimed = match store.claim_automation(&row.id, now).await {
            Ok(b) => b,
            Err(e) => {
                warn!(automation = %row.id, error = %e, "claim failed");
                continue;
            }
        };
        if !claimed {
            continue;
        }
        let cfg = cfg.clone();
        let store = store.clone();
        let events = events.clone();
        tokio::spawn(async move {
            let tick_started_at = Utc::now();
            let file = PathBuf::from(&row.file_path);
            let (run_id, outcome) = execute(
                &file,
                &row.id,
                row.last_run_at,
                /* dry_run */ false,
                &cfg.surface,
                &store,
            )
            .await;
            let _ = events.send(SchedulerEvent::RunStarted {
                automation_id: row.id.clone(),
                run_id,
            });
            let schedule: Option<Schedule> = serde_json::from_str(&row.schedule_json).ok();
            let (next_at, failure_count) = if outcome.success {
                let next = schedule
                    .as_ref()
                    .map(|s| s.next_after(tick_started_at))
                    .unwrap_or_else(|| tick_started_at + chrono::Duration::hours(1));
                (next, 0i64)
            } else {
                let next = tick_started_at + backoff_for(row.failure_count + 1);
                (next, row.failure_count + 1)
            };
            if let Err(e) = store
                .release_automation(
                    &row.id,
                    outcome.success,
                    Some(tick_started_at),
                    Some(next_at),
                    outcome.error.as_deref(),
                    failure_count,
                )
                .await
            {
                warn!(automation = %row.id, error = %e, "release_automation failed");
            }
            if !outcome.success {
                let title = format!("Automation '{}' failed", row.name);
                let body = format!(
                    "{}\n\n--- log tail ---\n{}",
                    outcome.error.clone().unwrap_or_default(),
                    outcome.log
                );
                let dedup = format!(
                    "automation-error:{}:{}",
                    row.id,
                    tick_started_at.timestamp()
                );
                if let Err(e) = store
                    .insert_inbox_entry(
                        "automation-error",
                        &title,
                        Some(&body),
                        None,
                        &["automation".into(), "error".into()],
                        Some(&format!("automation:{}", row.id)),
                        Some(&dedup),
                    )
                    .await
                {
                    warn!(error = %e, "failed to record automation error in inbox");
                }
            }
            info!(
                automation = %row.id,
                run_id,
                success = outcome.success,
                "automation run finished"
            );
            let _ = events.send(SchedulerEvent::RunFinished {
                automation_id: row.id,
                run_id,
                success: outcome.success,
            });
        });
    }
}
