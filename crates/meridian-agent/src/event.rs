use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Event flowing from the agent runner to the orchestrator (spec §10.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        session_id: String,
        thread_id: String,
        turn_id: String,
        codex_app_server_pid: Option<u32>,
        timestamp: DateTime<Utc>,
    },
    StartupFailed {
        error: String,
        timestamp: DateTime<Utc>,
    },
    TurnCompleted {
        session_id: String,
        usage: Option<TokenUsage>,
        timestamp: DateTime<Utc>,
    },
    TurnFailed {
        session_id: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
    TurnCancelled {
        session_id: String,
        timestamp: DateTime<Utc>,
    },
    TurnEndedWithError {
        session_id: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
    TurnInputRequired {
        session_id: String,
        timestamp: DateTime<Utc>,
    },
    ApprovalAutoApproved {
        session_id: String,
        kind: String,
        timestamp: DateTime<Utc>,
    },
    UnsupportedToolCall {
        session_id: String,
        tool: String,
        timestamp: DateTime<Utc>,
    },
    Notification {
        session_id: Option<String>,
        payload: Value,
        timestamp: DateTime<Utc>,
    },
    OtherMessage {
        session_id: Option<String>,
        payload: Value,
        timestamp: DateTime<Utc>,
    },
    Malformed {
        line: String,
        timestamp: DateTime<Utc>,
    },
    TokenUsageUpdated {
        session_id: String,
        usage: TokenUsage,
        timestamp: DateTime<Utc>,
    },
    RateLimitsUpdated {
        payload: Value,
        timestamp: DateTime<Utc>,
    },
    AgentMessageDelta {
        session_id: String,
        delta: String,
        timestamp: DateTime<Utc>,
    },
    ItemEvent {
        session_id: String,
        method: String,
        kind: String,
        payload: Value,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}
