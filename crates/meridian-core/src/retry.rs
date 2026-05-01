use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Scheduled retry entry (spec §4.1.7).
///
/// `due_at` uses wall-clock UTC for serialization; the orchestrator schedules
/// the actual timer separately and tracks its handle out-of-band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryEntry {
    pub issue_id: String,
    pub identifier: String,
    pub attempt: u32,
    pub due_at: DateTime<Utc>,
    pub error: Option<String>,
}
