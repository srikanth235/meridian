use chrono::Utc;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use meridian_agent::{run_session, AgentEvent, RunOutcome, SessionRequest};
use meridian_config::{render_prompt, ReloadHandle, ServiceConfig};
use meridian_core::{
    CodexRateLimits, CodexTotals, Issue, IssueState, LiveSession, OrchestratorRuntimeState,
    RetryEntry, RunningEntry,
};
use meridian_tracker::Tracker;
use meridian_workspace::WorkspaceManager;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::snapshot::{RetryRow, RunningRow, Snapshot, TotalsRow};

const CONTINUATION_DELAY_MS: u64 = 1_000;
const FAILURE_BASE_DELAY_MS: u64 = 10_000;

/// Cheap clone-able handle for callers (HTTP server, status surfaces).
#[derive(Clone)]
pub struct OrchestratorHandle {
    inner: Arc<OrchestratorInner>,
}

impl OrchestratorHandle {
    pub fn snapshot(&self) -> Snapshot {
        self.inner.snapshot()
    }
    pub fn subscribe_events(&self) -> broadcast::Receiver<SnapshotEvent> {
        self.inner.events.subscribe()
    }
    pub fn poke(&self) {
        self.inner.events.send(SnapshotEvent::StateChanged).ok();
    }
}

/// Lightweight signal so WS clients know to re-fetch the snapshot.
#[derive(Debug, Clone)]
pub enum SnapshotEvent {
    StateChanged,
}

pub struct Orchestrator {
    inner: Arc<OrchestratorInner>,
}

struct OrchestratorInner {
    state: Mutex<OrchestratorRuntimeState>,
    cancels: Mutex<HashMap<String, oneshot::Sender<()>>>,
    retry_handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    workspace: WorkspaceManager,
    tracker: Arc<dyn Tracker>,
    workflow: ReloadHandle,
    events: broadcast::Sender<SnapshotEvent>,
}

impl Orchestrator {
    pub fn new(
        tracker: Arc<dyn Tracker>,
        workflow: ReloadHandle,
    ) -> Self {
        let cfg = workflow.current().config;
        let state = OrchestratorRuntimeState::new(
            cfg.polling.interval_ms,
            cfg.agent.max_concurrent_agents,
        );
        let workspace = WorkspaceManager::new(cfg.workspace.root.clone());
        let (tx, _) = broadcast::channel(64);
        let inner = Arc::new(OrchestratorInner {
            state: Mutex::new(state),
            cancels: Mutex::new(HashMap::new()),
            retry_handles: Mutex::new(HashMap::new()),
            workspace,
            tracker,
            workflow,
            events: tx,
        });
        Self { inner }
    }

    pub fn handle(&self) -> OrchestratorHandle {
        OrchestratorHandle {
            inner: self.inner.clone(),
        }
    }

    /// Run forever: startup cleanup, then the poll loop.
    pub async fn run(self) {
        let inner = self.inner.clone();
        inner.startup_cleanup().await;
        loop {
            OrchestratorInner::tick(&inner).await;
            let interval = inner.state.lock().poll_interval_ms;
            tokio::time::sleep(Duration::from_millis(interval.max(100))).await;
        }
    }
}

impl OrchestratorInner {
    fn current_config(&self) -> ServiceConfig {
        self.workflow.current().config
    }

    async fn startup_cleanup(&self) {
        let cfg = self.current_config();
        if let Err(e) = cfg.preflight() {
            warn!(error = %e, "startup preflight failed; skipping initial cleanup");
            return;
        }
        match self
            .tracker
            .fetch_issues_by_states(&cfg.tracker.terminal_states)
            .await
        {
            Ok(issues) => {
                if cfg.workspace.delete_on_terminal {
                    for issue in issues {
                        self.workspace.remove(&issue.identifier, &cfg.hooks).await;
                    }
                } else {
                    debug!(
                        count = issues.len(),
                        "startup: workspace.delete_on_terminal=false, keeping terminal workspaces"
                    );
                }
            }
            Err(e) => warn!(error = %e, "startup terminal cleanup failed"),
        }
    }

    async fn tick(self: &Arc<Self>) {
        let cfg = self.current_config();

        // Re-apply potentially-changed settings live.
        {
            let mut s = self.state.lock();
            s.poll_interval_ms = cfg.polling.interval_ms;
            s.max_concurrent_agents = cfg.agent.max_concurrent_agents;
        }

        self.reconcile(&cfg).await;

        if let Err(e) = cfg.preflight() {
            warn!(error = %e, "dispatch preflight failed; skipping dispatch");
            return;
        }

        let candidates = match self
            .tracker
            .fetch_issues_by_states(&cfg.tracker.active_states)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "candidate fetch failed; skipping dispatch this tick");
                return;
            }
        };

        let chosen = self.select_dispatchable(&cfg, candidates);
        for issue in chosen {
            self.dispatch(issue, None);
        }
        let _ = self.events.send(SnapshotEvent::StateChanged);
    }

    fn select_dispatchable(&self, cfg: &ServiceConfig, mut issues: Vec<Issue>) -> Vec<Issue> {
        // Sort: priority asc (None last), created_at oldest first, identifier asc.
        issues.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(i32::MAX);
            let pb = b.priority.unwrap_or(i32::MAX);
            pa.cmp(&pb)
                .then_with(|| {
                    a.created_at.unwrap_or(chrono::DateTime::<Utc>::MIN_UTC).cmp(
                        &b.created_at.unwrap_or(chrono::DateTime::<Utc>::MIN_UTC),
                    )
                })
                .then_with(|| a.identifier.cmp(&b.identifier))
        });

        let s = self.state.lock();
        let mut chosen = Vec::new();
        let mut sim_running_by_state: HashMap<String, u32> = HashMap::new();
        for entry in s.running.values() {
            *sim_running_by_state
                .entry(entry.issue.state.to_lowercase())
                .or_default() += 1;
        }
        let mut sim_global_running = s.running.len() as u32;

        for issue in issues {
            if s.running.contains_key(&issue.id) || s.claimed.contains(&issue.id) {
                continue;
            }
            match issue.classify(&cfg.tracker.active_states, &cfg.tracker.terminal_states) {
                IssueState::Active => {}
                _ => continue,
            }
            if issue.blocked_by_non_terminal(&cfg.tracker.terminal_states) {
                continue;
            }
            if sim_global_running >= cfg.agent.max_concurrent_agents {
                break;
            }
            let lc = issue.state.to_lowercase();
            let per_state_cap = cfg
                .agent
                .max_concurrent_agents_by_state
                .get(&lc)
                .copied()
                .unwrap_or(cfg.agent.max_concurrent_agents);
            let cur = sim_running_by_state.get(&lc).copied().unwrap_or(0);
            if cur >= per_state_cap {
                continue;
            }
            sim_global_running += 1;
            *sim_running_by_state.entry(lc).or_default() += 1;
            chosen.push(issue);
        }
        chosen
    }

    fn dispatch(self: &Arc<Self>, issue: Issue, attempt: Option<u32>) {
        let id = issue.id.clone();
        // Reserve.
        {
            let mut s = self.state.lock();
            if s.running.contains_key(&id) {
                return;
            }
            s.claimed.insert(id.clone());
        }
        let cfg = self.current_config();
        let workspace_root_path = match self.workspace.workspace_path(&issue.identifier).into_os_string().into_string() {
            Ok(p) => p,
            Err(_) => {
                warn!(issue = %issue.identifier, "workspace path is not utf-8; skipping");
                self.release(&id);
                return;
            }
        };
        let inner = self.clone();
        let issue_for_task = issue.clone();
        let attempt_for_task = attempt;
        tokio::spawn(async move {
            inner
                .run_worker(issue_for_task, attempt_for_task, workspace_root_path, cfg)
                .await;
        });
    }

    async fn run_worker(
        self: Arc<Self>,
        issue: Issue,
        attempt: Option<u32>,
        workspace_path_str: String,
        cfg: ServiceConfig,
    ) {
        // Prepare workspace.
        let workspace = match self.workspace.ensure(&issue.identifier, &cfg.hooks).await {
            Ok(w) => w,
            Err(e) => {
                warn!(issue = %issue.identifier, error = %e, "workspace prepare failed");
                self.fail_and_retry(issue, attempt, e.to_string());
                return;
            }
        };
        // before_run hook (spec §9.4).
        if let Err(e) = meridian_workspace::run_hook(
            meridian_workspace::HookKind::BeforeRun,
            cfg.hooks.before_run.as_deref(),
            &workspace.path,
            cfg.hooks.timeout_ms,
        )
        .await
        {
            self.fail_and_retry(issue, attempt, format!("before_run: {e}"));
            return;
        }

        let started_at = Utc::now();
        let entry = RunningEntry {
            issue: issue.clone(),
            workspace_path: workspace_path_str.clone(),
            started_at,
            session: None,
            turn_count: 0,
        };
        {
            let mut s = self.state.lock();
            s.running.insert(issue.id.clone(), entry);
        }
        let _ = self.events.send(SnapshotEvent::StateChanged);

        // Set up event channel + cancellation.
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        self.cancels.lock().insert(issue.id.clone(), cancel_tx);

        // Consume events into orchestrator state in the background.
        let inner = self.clone();
        let issue_id_for_events = issue.id.clone();
        let event_task = tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                inner.apply_agent_event(&issue_id_for_events, ev);
            }
        });

        let issue_clone = issue.clone();
        let codex_cfg = cfg.codex.clone();
        let max_turns = cfg.agent.max_turns;
        let prompt_template = self.workflow.current().prompt_template.clone();
        let issue_for_prompt = issue.clone();
        let attempt_for_prompt = attempt;
        let issue_for_continuation = issue.clone();

        let workspace_path = workspace.path.clone();
        let req = SessionRequest {
            issue: &issue_clone,
            workspace_path: &workspace_path,
            codex: &codex_cfg,
            max_turns,
            render_first_prompt: Box::new(move || {
                render_prompt(&prompt_template, &issue_for_prompt, attempt_for_prompt)
                    .map_err(|e| e.to_string())
            }),
            render_continuation: Box::new(move |attempt| {
                meridian_config::prompt::continuation_prompt(&issue_for_continuation, attempt)
            }),
            event_tx,
            cancel: cancel_rx,
        };

        let outcome = run_session(req).await;
        // Stop event task once subprocess is done.
        drop(event_task); // task continues until rx is empty + closed

        // after_run hook always runs, ignore failure.
        let _ = meridian_workspace::run_hook(
            meridian_workspace::HookKind::AfterRun,
            cfg.hooks.after_run.as_deref(),
            &workspace.path,
            cfg.hooks.timeout_ms,
        )
        .await;

        // Update aggregate runtime + remove running entry.
        let elapsed = (Utc::now() - started_at).num_seconds().max(0) as u64;
        {
            let mut s = self.state.lock();
            if let Some(removed) = s.running.remove(&issue.id) {
                s.codex_totals.ended_seconds_running =
                    s.codex_totals.ended_seconds_running.saturating_add(elapsed);
                if let Some(sess) = removed.session {
                    debug!(session_id = %sess.session_id, "session ended");
                }
            }
        }
        self.cancels.lock().remove(&issue.id);

        match outcome {
            RunOutcome::TurnsExhausted { turns } => {
                info!(issue = %issue.identifier, turns, "worker turns exhausted; scheduling continuation re-check");
                self.schedule_continuation(issue);
            }
            RunOutcome::Failed(err) => {
                warn!(issue = %issue.identifier, error = %err, "worker failed; scheduling backoff retry");
                self.fail_and_retry(issue, attempt, err);
            }
        }
        let _ = self.events.send(SnapshotEvent::StateChanged);
    }

    fn apply_agent_event(&self, issue_id: &str, ev: AgentEvent) {
        let mut s = self.state.lock();
        let mut delta: (u64, u64, u64) = (0, 0, 0);
        match ev {
            AgentEvent::SessionStarted {
                session_id,
                thread_id,
                turn_id,
                codex_app_server_pid,
                timestamp,
            } => {
                if let Some(entry) = s.running.get_mut(issue_id) {
                    let mut live = LiveSession::new(thread_id, turn_id, codex_app_server_pid);
                    live.session_id = session_id;
                    live.last_codex_event = Some("session_started".into());
                    live.last_codex_timestamp = Some(timestamp);
                    entry.turn_count += 1;
                    live.turn_count = entry.turn_count;
                    entry.session = Some(live);
                }
            }
            AgentEvent::TokenUsageUpdated { usage, timestamp, .. } => {
                if let Some(entry) = s.running.get_mut(issue_id) {
                    if let Some(live) = entry.session.as_mut() {
                        delta = live.apply_absolute_totals(
                            usage.input_tokens,
                            usage.output_tokens,
                            usage.total_tokens,
                        );
                        live.last_codex_event = Some("token_usage_updated".into());
                        live.last_codex_timestamp = Some(timestamp);
                    }
                }
            }
            AgentEvent::TurnCompleted { usage, timestamp, .. } => {
                if let Some(entry) = s.running.get_mut(issue_id) {
                    if let (Some(u), Some(live)) = (usage, entry.session.as_mut()) {
                        delta = live.apply_absolute_totals(
                            u.input_tokens,
                            u.output_tokens,
                            u.total_tokens,
                        );
                        live.last_codex_event = Some("turn_completed".into());
                        live.last_codex_timestamp = Some(timestamp);
                    }
                }
            }
            AgentEvent::RateLimitsUpdated { payload, timestamp } => {
                s.codex_rate_limits = CodexRateLimits {
                    last_payload: Some(payload),
                    last_seen: Some(timestamp),
                };
            }
            AgentEvent::AgentMessageDelta { delta, timestamp, .. } => {
                if let Some(entry) = s.running.get_mut(issue_id) {
                    if let Some(live) = entry.session.as_mut() {
                        live.last_codex_event = Some("agent_message".into());
                        live.last_codex_timestamp = Some(timestamp);
                        // Keep last ~200 *characters* (not bytes — must respect
                        // UTF-8 boundaries) of streaming text for the UI snapshot.
                        let mut buf = live
                            .last_codex_message
                            .as_ref()
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .unwrap_or_default();
                        buf.push_str(&delta);
                        let char_count = buf.chars().count();
                        if char_count > 200 {
                            let skip = char_count - 200;
                            buf = buf.chars().skip(skip).collect();
                        }
                        live.last_codex_message = Some(serde_json::Value::String(buf));
                    }
                }
            }
            AgentEvent::ItemEvent { method, kind, timestamp, .. } => {
                if let Some(entry) = s.running.get_mut(issue_id) {
                    if let Some(live) = entry.session.as_mut() {
                        live.last_codex_event = Some(format!("{method} ({kind})"));
                        live.last_codex_timestamp = Some(timestamp);
                    }
                }
            }
            other => {
                if let Some(entry) = s.running.get_mut(issue_id) {
                    if let Some(live) = entry.session.as_mut() {
                        live.last_codex_event =
                            Some(format!("{other:?}").chars().take(80).collect());
                        live.last_codex_timestamp = Some(Utc::now());
                    }
                }
            }
        }
        if delta != (0, 0, 0) {
            s.codex_totals.input_tokens =
                s.codex_totals.input_tokens.saturating_add(delta.0);
            s.codex_totals.output_tokens =
                s.codex_totals.output_tokens.saturating_add(delta.1);
            s.codex_totals.total_tokens =
                s.codex_totals.total_tokens.saturating_add(delta.2);
        }
    }

    fn release(&self, issue_id: &str) {
        let mut s = self.state.lock();
        s.claimed.remove(issue_id);
        s.retry_attempts.remove(issue_id);
        self.retry_handles.lock().remove(issue_id);
    }

    fn schedule_continuation(self: &Arc<Self>, issue: Issue) {
        self.schedule_retry_inner(issue, 1, None, CONTINUATION_DELAY_MS);
    }

    fn fail_and_retry(self: &Arc<Self>, issue: Issue, prev_attempt: Option<u32>, error: String) {
        let attempt = prev_attempt.unwrap_or(0) + 1;
        let cap = self.current_config().agent.max_retry_backoff_ms;
        let exp = (FAILURE_BASE_DELAY_MS as u128) << ((attempt - 1).min(20) as u128);
        let delay = exp.min(cap as u128) as u64;
        self.schedule_retry_inner(issue, attempt, Some(error), delay);
    }

    fn schedule_retry_inner(
        self: &Arc<Self>,
        issue: Issue,
        attempt: u32,
        error: Option<String>,
        delay_ms: u64,
    ) {
        let due_at = Utc::now() + chrono::Duration::milliseconds(delay_ms as i64);
        let entry = RetryEntry {
            issue_id: issue.id.clone(),
            identifier: issue.identifier.clone(),
            attempt,
            due_at,
            error,
        };
        {
            let mut s = self.state.lock();
            s.retry_attempts.insert(issue.id.clone(), entry);
            s.claimed.insert(issue.id.clone());
        }
        // Replace any existing timer.
        if let Some(h) = self.retry_handles.lock().remove(&issue.id) {
            h.abort();
        }
        let inner = self.clone();
        let issue_clone = issue.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            inner.handle_retry(issue_clone, attempt).await;
        });
        self.retry_handles.lock().insert(issue.id, handle);
    }

    async fn handle_retry(self: Arc<Self>, issue: Issue, attempt: u32) {
        let cfg = self.current_config();
        let candidates = match self
            .tracker
            .fetch_issues_by_states(&cfg.tracker.active_states)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                warn!(issue = %issue.identifier, error = %e, "retry candidate fetch failed; releasing");
                self.release(&issue.id);
                return;
            }
        };
        let still_eligible = candidates.into_iter().find(|c| c.id == issue.id);
        let Some(current) = still_eligible else {
            info!(issue = %issue.identifier, "retry: issue no longer active; releasing");
            self.release(&issue.id);
            return;
        };
        if matches!(
            current.classify(&cfg.tracker.active_states, &cfg.tracker.terminal_states),
            IssueState::Active
        ) {
            // Slot check.
            let available = self.state.lock().available_global_slots();
            if available == 0 {
                warn!(issue = %current.identifier, "retry: no orchestrator slots; requeueing");
                self.schedule_retry_inner(
                    current,
                    attempt,
                    Some("no available orchestrator slots".into()),
                    CONTINUATION_DELAY_MS * 2,
                );
                return;
            }
            // Move from retry to running.
            {
                let mut s = self.state.lock();
                s.retry_attempts.remove(&current.id);
            }
            self.retry_handles.lock().remove(&current.id);
            self.dispatch(current, Some(attempt));
        } else {
            self.release(&current.id);
        }
    }

    async fn reconcile(&self, cfg: &ServiceConfig) {
        // Stall detection (Part A).
        let now = Utc::now();
        let mut to_stall: Vec<String> = Vec::new();
        if cfg.codex.stall_timeout_ms > 0 {
            let s = self.state.lock();
            for (id, entry) in &s.running {
                let last = entry
                    .session
                    .as_ref()
                    .and_then(|s| s.last_codex_timestamp)
                    .unwrap_or(entry.started_at);
                let elapsed = (now - last).num_milliseconds().max(0) as i64;
                if elapsed > cfg.codex.stall_timeout_ms {
                    to_stall.push(id.clone());
                }
            }
        }
        for id in to_stall {
            warn!(issue_id = %id, "stall timeout; cancelling worker");
            if let Some(c) = self.cancels.lock().remove(&id) {
                let _ = c.send(());
            }
        }

        // Tracker state refresh (Part B).
        let running_ids: Vec<String> = self.state.lock().running.keys().cloned().collect();
        if running_ids.is_empty() {
            return;
        }
        let states = match self.tracker.fetch_issue_states_by_ids(&running_ids).await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "running state refresh failed; will retry next tick");
                return;
            }
        };
        let mut to_terminal: HashSet<String> = HashSet::new();
        let mut to_other: HashSet<String> = HashSet::new();
        let mut updated_issues: HashMap<String, Issue> = HashMap::new();
        {
            let s = self.state.lock();
            for (id, entry) in &s.running {
                let Some(latest) = states.get(id) else {
                    // Issue not returned — treat as terminal/missing.
                    to_terminal.insert(id.clone());
                    continue;
                };
                match latest.classify(&cfg.tracker.active_states, &cfg.tracker.terminal_states) {
                    IssueState::Active => {
                        let _ = entry; // keep running
                        updated_issues.insert(id.clone(), latest.clone());
                    }
                    IssueState::Terminal => {
                        to_terminal.insert(id.clone());
                    }
                    IssueState::Other => {
                        to_other.insert(id.clone());
                    }
                }
            }
        }
        for (id, latest) in updated_issues {
            if let Some(entry) = self.state.lock().running.get_mut(&id) {
                entry.issue = latest;
            }
        }
        for id in to_terminal {
            info!(issue_id = %id, "issue is terminal; cancelling worker");
            if let Some(c) = self.cancels.lock().remove(&id) {
                let _ = c.send(());
            }
            if cfg.workspace.delete_on_terminal {
                // Clean workspace using a snapshot of the identifier.
                let identifier = self.state.lock().running.get(&id).map(|e| e.issue.identifier.clone());
                if let Some(ident) = identifier {
                    self.workspace.remove(&ident, &cfg.hooks).await;
                }
            }
        }
        for id in to_other {
            info!(issue_id = %id, "issue no longer active; cancelling worker (workspace preserved)");
            if let Some(c) = self.cancels.lock().remove(&id) {
                let _ = c.send(());
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        let s = self.state.lock();
        let now = Utc::now();
        let running: Vec<RunningRow> = s
            .running
            .values()
            .map(|e| {
                let live = e.session.as_ref();
                RunningRow {
                    issue: e.issue.clone(),
                    workspace_path: e.workspace_path.clone(),
                    started_at: e.started_at,
                    session_id: live.map(|l| l.session_id.clone()),
                    last_event: live.and_then(|l| l.last_codex_event.clone()),
                    last_event_at: live.and_then(|l| l.last_codex_timestamp),
                    turn_count: e.turn_count,
                    tokens_input: live.map(|l| l.codex_input_tokens).unwrap_or(0),
                    tokens_output: live.map(|l| l.codex_output_tokens).unwrap_or(0),
                    tokens_total: live.map(|l| l.codex_total_tokens).unwrap_or(0),
                    last_message_tail: live
                        .and_then(|l| l.last_codex_message.as_ref())
                        .and_then(|v| v.as_str().map(|s| s.to_string())),
                }
            })
            .collect();
        let retrying: Vec<RetryRow> = s
            .retry_attempts
            .values()
            .map(|r| RetryRow {
                issue_id: r.issue_id.clone(),
                identifier: r.identifier.clone(),
                attempt: r.attempt,
                due_at: r.due_at,
                error: r.error.clone(),
            })
            .collect();
        let live_seconds: u64 = s
            .running
            .values()
            .map(|e| (now - e.started_at).num_seconds().max(0) as u64)
            .sum();
        let totals = TotalsRow {
            input_tokens: s.codex_totals.input_tokens,
            output_tokens: s.codex_totals.output_tokens,
            total_tokens: s.codex_totals.total_tokens,
            seconds_running: s
                .codex_totals
                .ended_seconds_running
                .saturating_add(live_seconds),
        };
        Snapshot {
            running,
            retrying,
            codex_totals: totals,
            rate_limits: s.codex_rate_limits.last_payload.clone(),
            generated_at: now,
            poll_interval_ms: s.poll_interval_ms,
            max_concurrent_agents: s.max_concurrent_agents,
        }
    }
}

// suppress unused warnings for fields that future code will use
#[allow(dead_code)]
fn _unused(_: CodexTotals) {}
