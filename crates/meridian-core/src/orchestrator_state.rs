use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::{Issue, LiveSession, RetryEntry};

/// Per-issue running entry held in [`OrchestratorRuntimeState::running`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningEntry {
    pub issue: Issue,
    pub workspace_path: String,
    pub started_at: DateTime<Utc>,
    pub session: Option<LiveSession>,
    /// Total turns dispatched within the lifetime of this worker (spec §4.1.6).
    pub turn_count: u32,
}

/// Aggregate Codex token + runtime accounting (spec §4.1.8 / §13.5).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    /// Cumulative seconds for *ended* sessions only. Live sessions are added
    /// at snapshot time per spec §13.5.
    pub ended_seconds_running: u64,
}

/// Latest rate-limit payload echoed from the coding agent (spec §4.1.8).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexRateLimits {
    pub last_payload: Option<Value>,
    pub last_seen: Option<DateTime<Utc>>,
}

/// Authoritative orchestrator state (spec §4.1.8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorRuntimeState {
    pub poll_interval_ms: u64,
    pub max_concurrent_agents: u32,
    pub running: HashMap<String, RunningEntry>,
    pub claimed: HashSet<String>,
    pub retry_attempts: HashMap<String, RetryEntry>,
    pub completed: HashSet<String>,
    pub codex_totals: CodexTotals,
    pub codex_rate_limits: CodexRateLimits,
}

impl OrchestratorRuntimeState {
    pub fn new(poll_interval_ms: u64, max_concurrent_agents: u32) -> Self {
        Self {
            poll_interval_ms,
            max_concurrent_agents,
            running: HashMap::new(),
            claimed: HashSet::new(),
            retry_attempts: HashMap::new(),
            completed: HashSet::new(),
            codex_totals: CodexTotals::default(),
            codex_rate_limits: CodexRateLimits::default(),
        }
    }

    pub fn available_global_slots(&self) -> u32 {
        (self.max_concurrent_agents as i64 - self.running.len() as i64).max(0) as u32
    }
}
