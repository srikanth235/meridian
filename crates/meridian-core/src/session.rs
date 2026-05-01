use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Live coding-agent session metadata (spec §4.1.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSession {
    pub session_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub codex_app_server_pid: Option<u32>,
    pub last_codex_event: Option<String>,
    pub last_codex_timestamp: Option<DateTime<Utc>>,
    pub last_codex_message: Option<Value>,

    pub codex_input_tokens: u64,
    pub codex_output_tokens: u64,
    pub codex_total_tokens: u64,

    pub last_reported_input_tokens: u64,
    pub last_reported_output_tokens: u64,
    pub last_reported_total_tokens: u64,

    pub turn_count: u32,
}

impl LiveSession {
    pub fn new(thread_id: String, turn_id: String, pid: Option<u32>) -> Self {
        let session_id = crate::session_id(&thread_id, &turn_id);
        Self {
            session_id,
            thread_id,
            turn_id,
            codex_app_server_pid: pid,
            last_codex_event: None,
            last_codex_timestamp: None,
            last_codex_message: None,
            codex_input_tokens: 0,
            codex_output_tokens: 0,
            codex_total_tokens: 0,
            last_reported_input_tokens: 0,
            last_reported_output_tokens: 0,
            last_reported_total_tokens: 0,
            turn_count: 0,
        }
    }

    /// Apply absolute thread totals, returning the deltas (spec §13.5).
    /// Delta-style payloads should be ignored at the call site, not here.
    pub fn apply_absolute_totals(
        &mut self,
        input: u64,
        output: u64,
        total: u64,
    ) -> (u64, u64, u64) {
        let d_in = input.saturating_sub(self.last_reported_input_tokens);
        let d_out = output.saturating_sub(self.last_reported_output_tokens);
        let d_total = total.saturating_sub(self.last_reported_total_tokens);

        self.codex_input_tokens = self.codex_input_tokens.saturating_add(d_in);
        self.codex_output_tokens = self.codex_output_tokens.saturating_add(d_out);
        self.codex_total_tokens = self.codex_total_tokens.saturating_add(d_total);

        self.last_reported_input_tokens = input;
        self.last_reported_output_tokens = output;
        self.last_reported_total_tokens = total;

        (d_in, d_out, d_total)
    }
}
