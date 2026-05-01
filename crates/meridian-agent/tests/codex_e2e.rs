//! End-to-end smoke test against a *real* `codex app-server` binary.
//!
//! Requires:
//!   - `codex` on PATH (the local Codex CLI with `app-server` subcommand)
//!   - A working ChatGPT login (Codex itself handles auth)
//!
//! Skipped automatically when `codex` is missing. Opt in with
//! `MERIDIAN_RUN_CODEX_E2E=1` because it makes a real API call.

use chrono::Utc;
use serde_json::{json, Value};
use std::time::Duration;
use meridian_agent::{run_session, AgentEvent, RunOutcome, SessionRequest};
use meridian_config::CodexConfig;
use meridian_core::Issue;
use tokio::sync::{mpsc, oneshot};

fn codex_available() -> bool {
    std::process::Command::new("codex")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn issue() -> Issue {
    Issue {
        id: "test-1".into(),
        identifier: "TEST-1".into(),
        title: "Smoke test".into(),
        description: None,
        priority: None,
        state: "Todo".into(),
        branch_name: None,
        url: None,
        labels: vec![],
        blocked_by: vec![],
        created_at: Some(Utc::now()),
        updated_at: None,
        repo: None,
    }
}

#[tokio::test]
async fn ping_pong_against_real_codex() {
    if !codex_available() {
        eprintln!("skipping: `codex` binary not on PATH");
        return;
    }
    if std::env::var("MERIDIAN_RUN_CODEX_E2E").is_err() {
        eprintln!("skipping: set MERIDIAN_RUN_CODEX_E2E=1 to enable (uses real Codex API)");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let codex = CodexConfig {
        command: "codex app-server".into(),
        approval_policy: "never".into(),
        thread_sandbox: "danger-full-access".into(),
        turn_sandbox_policy: json!({"type": "dangerFullAccess"}),
        turn_timeout_ms: 120_000,
        read_timeout_ms: 5_000,
        stall_timeout_ms: 0,
        session_source_override: Some("cli".into()),
    };
    let issue = issue();

    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
    let (_cancel_tx, cancel_rx) = oneshot::channel::<()>();

    let req = SessionRequest {
        issue: &issue,
        workspace_path: tmp.path(),
        codex: &codex,
        max_turns: 1,
        render_first_prompt: Box::new(|| {
            Ok("Reply with the single word: PONG. Do not run any commands.".into())
        }),
        render_continuation: Box::new(|_| String::new()),
        event_tx: tx,
        cancel: cancel_rx,
    };

    let run = tokio::time::timeout(Duration::from_secs(150), run_session(req))
        .await
        .expect("session timed out");
    eprintln!("session outcome: {:?}", run);
    assert!(matches!(run, RunOutcome::TurnsExhausted { turns: 1 }));

    // Drain events and confirm we saw the canonical lifecycle pieces.
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    drop(rx);

    let methods: Vec<&str> = events
        .iter()
        .map(|e| match e {
            AgentEvent::SessionStarted { .. } => "session_started",
            AgentEvent::TurnCompleted { .. } => "turn_completed",
            AgentEvent::TokenUsageUpdated { .. } => "token_usage_updated",
            AgentEvent::RateLimitsUpdated { .. } => "rate_limits_updated",
            AgentEvent::AgentMessageDelta { .. } => "agent_message_delta",
            AgentEvent::ItemEvent { .. } => "item_event",
            AgentEvent::Notification { .. } => "notification",
            AgentEvent::OtherMessage { .. } => "other_message",
            AgentEvent::TurnFailed { .. } => "turn_failed",
            AgentEvent::TurnCancelled { .. } => "turn_cancelled",
            AgentEvent::TurnEndedWithError { .. } => "turn_ended_with_error",
            AgentEvent::TurnInputRequired { .. } => "turn_input_required",
            AgentEvent::ApprovalAutoApproved { .. } => "approval_auto_approved",
            AgentEvent::UnsupportedToolCall { .. } => "unsupported_tool_call",
            AgentEvent::Malformed { .. } => "malformed",
            AgentEvent::StartupFailed { .. } => "startup_failed",
        })
        .collect();
    eprintln!("event methods: {:?}", methods);
    assert!(methods.contains(&"session_started"));
    assert!(methods.contains(&"turn_completed"));

    // Sanity: at least one streaming delta arrived (the model replied).
    let mut full_reply = String::new();
    for ev in &events {
        if let AgentEvent::AgentMessageDelta { delta, .. } = ev {
            full_reply.push_str(delta);
        }
    }
    eprintln!("agent reply: {full_reply:?}");
    assert!(!full_reply.is_empty(), "no agent message deltas received");

    // Sanity: token usage with non-zero totals.
    let saw_tokens = events.iter().any(|e| match e {
        AgentEvent::TokenUsageUpdated { usage, .. } => usage.total_tokens > 0,
        _ => false,
    });
    assert!(saw_tokens, "did not see non-zero token usage");
}

#[allow(dead_code)]
fn _suppress_unused(_: Value) {}
