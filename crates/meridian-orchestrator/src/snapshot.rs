use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use meridian_core::Issue;

/// Snapshot returned by the runtime monitoring API (spec §13.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub running: Vec<RunningRow>,
    pub retrying: Vec<RetryRow>,
    pub codex_totals: TotalsRow,
    pub rate_limits: Option<Value>,
    pub generated_at: DateTime<Utc>,
    pub poll_interval_ms: u64,
    pub max_concurrent_agents: u32,
    /// All known issues from the tracker, in the configured kanban-column order
    /// of `tracker.columns` (or active+terminal). UI uses this to render the
    /// board; `running`/`retrying` overlay live state on top.
    #[serde(default)]
    pub kanban: KanbanBoard,
    /// `tracker.repo` from config, surfaced for the UI title bar.
    #[serde(default)]
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KanbanBoard {
    pub columns: Vec<KanbanColumn>,
    /// Issues whose state didn't match any configured column (visibility for
    /// stragglers — e.g. open with no `status:*` label).
    pub unsorted: Vec<Issue>,
    /// True when the underlying tracker fetch has succeeded at least once.
    /// While false, the UI should render an empty/loading state.
    pub loaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanColumn {
    pub state: String,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningRow {
    pub issue: Issue,
    pub workspace_path: String,
    pub started_at: DateTime<Utc>,
    pub session_id: Option<String>,
    pub last_event: Option<String>,
    pub last_event_at: Option<DateTime<Utc>>,
    pub turn_count: u32,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tokens_total: u64,
    /// Tail of the streaming agent message (best-effort).
    pub last_message_tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryRow {
    pub issue_id: String,
    pub identifier: String,
    pub attempt: u32,
    pub due_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalsRow {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub seconds_running: u64,
}
