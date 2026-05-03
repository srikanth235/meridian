//! Top-level glue: install runtime assets, kick off the filesystem watcher,
//! launch the scheduler, expose a clone-able handle for HTTP routes.

use chrono::Utc;
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use meridian_store::{AutomationRecord, AutomationRunRecord, InboxEntryRecord, Store};

use crate::assets::install_runtime;
use crate::executor::execute;
use crate::nl::{generate, GeneratedSpec};
use crate::registry::{prune_missing, refresh};
use crate::runtime::RuntimeInfo;
use crate::scheduler::{self, SchedulerConfig, SchedulerEvent};
use crate::sdk::{SdkRequest, SdkResponse, SdkSurface};
use crate::tokens::{TokenContext, TokenStore};

#[derive(Clone)]
pub struct AutomationsHandle {
    inner: Arc<Inner>,
}

struct Inner {
    store: Arc<Store>,
    surface: SdkSurface,
    tokens: TokenStore,
    automations_dir: PathBuf,
    runner_path: PathBuf,
    runtime: RuntimeInfo,
    sdk_base: String,
    rescan_tx: mpsc::UnboundedSender<()>,
    events: broadcast::Sender<SchedulerEvent>,
}

impl AutomationsHandle {
    pub fn store(&self) -> Arc<Store> {
        self.inner.store.clone()
    }

    pub fn surface(&self) -> SdkSurface {
        self.inner.surface.clone()
    }

    pub fn tokens(&self) -> TokenStore {
        self.inner.tokens.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SchedulerEvent> {
        self.inner.events.subscribe()
    }

    pub fn automations_dir(&self) -> &Path {
        &self.inner.automations_dir
    }

    pub fn runtime(&self) -> &RuntimeInfo {
        &self.inner.runtime
    }

    pub fn request_rescan(&self) {
        let _ = self.inner.rescan_tx.send(());
    }

    pub async fn list(&self) -> Vec<AutomationRecord> {
        self.inner.store.list_automations().await.unwrap_or_default()
    }

    pub async fn get(&self, id: &str) -> Option<AutomationRecord> {
        self.inner.store.get_automation(id).await.ok().flatten()
    }

    pub async fn read_source(&self, id: &str) -> Option<String> {
        let row = self.get(id).await?;
        std::fs::read_to_string(&row.file_path).ok()
    }

    pub async fn list_runs(&self, id: &str, limit: i64) -> Vec<AutomationRunRecord> {
        self.inner
            .store
            .list_automation_runs(id, limit)
            .await
            .unwrap_or_default()
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.inner
            .store
            .set_automation_enabled(id, enabled)
            .await
            .map_err(|e| e.to_string())
    }

    /// Trigger an out-of-band run (Run-now or Dry-run buttons). Bypasses the
    /// scheduler tick but still respects the running_since lock.
    pub async fn run_now(&self, id: &str, dry_run: bool) -> Result<i64, String> {
        let row = self
            .inner
            .store
            .get_automation(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("automation not found")?;
        if row.parse_error.is_some() {
            return Err(format!(
                "automation has a parse error: {}",
                row.parse_error.unwrap_or_default()
            ));
        }
        if row.running_since.is_some() {
            return Err("automation is already running".into());
        }
        // Dry-runs don't claim the lock — the user can fire them concurrently
        // with scheduled runs without blocking the next tick.
        if !dry_run {
            let claimed = self
                .inner
                .store
                .claim_automation(id, Utc::now())
                .await
                .map_err(|e| e.to_string())?;
            if !claimed {
                return Err("could not claim automation".into());
            }
        }
        let inner = self.inner.clone();
        let id = id.to_string();
        let file = PathBuf::from(&row.file_path);
        let last_run_at = row.last_run_at;
        let row_failure_count = row.failure_count;
        let schedule_json = row.schedule_json.clone();
        let row_name = row.name.clone();
        let id_clone = id.clone();
        tokio::spawn(async move {
            let started = Utc::now();
            let (run_id, outcome) = execute(
                &inner.runner_path,
                &file,
                &id_clone,
                last_run_at,
                dry_run,
                &inner.runtime,
                &inner.sdk_base,
                &inner.tokens,
                &inner.store,
            )
            .await;
            if !dry_run {
                let schedule: Option<crate::schedule::Schedule> =
                    serde_json::from_str(&schedule_json).ok();
                let (next_at, failure_count) = if outcome.success {
                    let next = schedule
                        .as_ref()
                        .map(|s| s.next_after(started))
                        .unwrap_or_else(|| started + chrono::Duration::hours(1));
                    (next, 0)
                } else {
                    let next =
                        started + crate::schedule::backoff_for(row_failure_count + 1);
                    (next, row_failure_count + 1)
                };
                if let Err(e) = inner
                    .store
                    .release_automation(
                        &id_clone,
                        outcome.success,
                        Some(started),
                        Some(next_at),
                        outcome.error.as_deref(),
                        failure_count,
                    )
                    .await
                {
                    warn!(automation = %id_clone, error = %e, "release_automation failed");
                }
                if !outcome.success {
                    let title = format!("Automation '{}' failed", row_name);
                    let body = format!(
                        "{}\n\n--- log tail ---\n{}",
                        outcome.error.clone().unwrap_or_default(),
                        outcome.log
                    );
                    let dedup = format!("automation-error:{}:{}", id_clone, started.timestamp());
                    let _ = inner
                        .store
                        .insert_inbox_entry(
                            "automation-error",
                            &title,
                            Some(&body),
                            None,
                            &["automation".into(), "error".into()],
                            Some(&format!("automation:{}", id_clone)),
                            Some(&dedup),
                        )
                        .await;
                }
            }
            let _ = inner.events.send(SchedulerEvent::RunFinished {
                automation_id: id_clone.clone(),
                run_id,
                success: outcome.success,
            });
        });
        // Run id is allocated inside the spawn; for the synchronous return we
        // hand back 0 — clients should poll list_runs to see history. (We
        // could plumb through a oneshot but the UI already polls.)
        Ok(0)
    }

    /// Submit a natural-language request → write an inbox entry containing
    /// the spec the harness should follow. Returns the inbox entry id.
    pub async fn submit_request(&self, nl: &str) -> Result<(String, GeneratedSpec), String> {
        let spec = generate(nl);
        let id = self
            .inner
            .store
            .insert_inbox_entry(
                "automation-request",
                &spec.title,
                Some(&spec.body),
                None,
                &["automation".into(), "request".into()],
                Some("automation-request"),
                Some(&spec.slug),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok((id, spec))
    }

    pub async fn list_inbox(&self) -> Vec<InboxEntryRecord> {
        self.inner
            .store
            .list_inbox_entries(false)
            .await
            .unwrap_or_default()
    }

    pub async fn get_inbox(&self, id: &str) -> Option<InboxEntryRecord> {
        self.inner.store.get_inbox_entry(id).await.ok().flatten()
    }

    pub async fn dismiss_inbox(&self, id: &str) -> Result<(), String> {
        self.inner
            .store
            .dismiss_inbox_entry(id)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn handle_sdk(
        &self,
        ctx: &TokenContext,
        req: SdkRequest,
    ) -> Result<SdkResponse, String> {
        self.inner.surface.handle(ctx, req).await
    }
}

pub struct AutomationsService;

impl AutomationsService {
    /// Boot the service. `automations_dir` defaults to
    /// `<workflow_parent_dir>/automations/` if you pass that in. `sdk_base`
    /// is the absolute URL the SDK should call back to (e.g.
    /// `http://127.0.0.1:7878/api/automations/sdk`). The JS runtime is
    /// auto-detected (Bun preferred, then Node 22.6+) — override with
    /// `MERIDIAN_NODE_BIN`.
    pub async fn start(
        automations_dir: PathBuf,
        sdk_base: String,
        store: Arc<Store>,
    ) -> std::io::Result<AutomationsHandle> {
        let layout = install_runtime(&automations_dir)?;
        info!(path = %automations_dir.display(), "automations dir ready");

        let runtime = crate::runtime::detect().await;

        let tokens = TokenStore::new();
        let surface = SdkSurface::new(store.clone());
        let (events_tx, _) = broadcast::channel(64);
        let (rescan_tx, mut rescan_rx) = mpsc::unbounded_channel::<()>();

        let inner = Arc::new(Inner {
            store: store.clone(),
            surface,
            tokens: tokens.clone(),
            automations_dir: automations_dir.clone(),
            runner_path: layout.runner.clone(),
            runtime: runtime.clone(),
            sdk_base: sdk_base.clone(),
            rescan_tx: rescan_tx.clone(),
            events: events_tx.clone(),
        });

        // Initial scan.
        let initial_present = refresh(&automations_dir, &layout.runner, &runtime, &store).await;
        prune_missing(&store, &initial_present).await;

        // Filesystem watcher: any change in the automations dir → trigger
        // a debounced rescan via the channel. We watch non-recursively so
        // edits to node_modules don't fire constantly.
        let watch_tx = rescan_tx.clone();
        let mut watcher = recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(ev) = res {
                if matches!(
                    ev.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    let _ = watch_tx.send(());
                }
            }
        })
        .map_err(io_err)?;
        watcher
            .watch(&automations_dir, RecursiveMode::NonRecursive)
            .map_err(io_err)?;
        // Keep the watcher alive for the process lifetime.
        Box::leak(Box::new(watcher));

        // Rescan task: debounce events, run refresh + prune.
        let store_for_rescan = store.clone();
        let dir_for_rescan = automations_dir.clone();
        let runner_for_rescan = layout.runner.clone();
        let runtime_for_rescan = runtime.clone();
        let events_for_rescan = events_tx.clone();
        tokio::spawn(async move {
            loop {
                match rescan_rx.recv().await {
                    None => return,
                    Some(()) => {
                        // Debounce: drain any further messages within 250ms.
                        let _ =
                            tokio::time::timeout(Duration::from_millis(250), async {
                                while rescan_rx.recv().await.is_some() {}
                            })
                            .await;
                        let present = refresh(
                            &dir_for_rescan,
                            &runner_for_rescan,
                            &runtime_for_rescan,
                            &store_for_rescan,
                        )
                        .await;
                        prune_missing(&store_for_rescan, &present).await;
                        let _ = events_for_rescan.send(SchedulerEvent::RunFinished {
                            automation_id: String::new(),
                            run_id: 0,
                            success: true,
                        });
                    }
                }
            }
        });

        // Scheduler task.
        let cfg = SchedulerConfig {
            runner_path: layout.runner.clone(),
            runtime: runtime.clone(),
            sdk_base: sdk_base.clone(),
        };
        let store_for_sched = store.clone();
        let tokens_for_sched = tokens.clone();
        let events_for_sched = events_tx.clone();
        tokio::spawn(async move {
            scheduler::run(cfg, store_for_sched, tokens_for_sched, events_for_sched).await;
        });

        Ok(AutomationsHandle { inner })
    }
}

fn io_err(e: notify::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}
