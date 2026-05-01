use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use meridian_config::CodexConfig;
use meridian_core::{session_id, Issue};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::error::AgentError;
use crate::event::{AgentEvent, TokenUsage};
use crate::protocol;

/// Outcome of `run_session` (spec §10.7).
#[derive(Debug, Clone)]
pub enum RunOutcome {
    /// All allowed turns finished cleanly. Worker can re-check for more work.
    TurnsExhausted { turns: u32 },
    /// Hard stop — propagate as failure.
    Failed(String),
}

pub struct SessionRequest<'a> {
    pub issue: &'a Issue,
    pub workspace_path: &'a std::path::Path,
    pub codex: &'a CodexConfig,
    pub max_turns: u32,
    pub render_first_prompt: Box<dyn FnOnce() -> Result<String, String> + Send>,
    pub render_continuation: Box<dyn Fn(u32) -> String + Send + Sync>,
    pub event_tx: mpsc::UnboundedSender<AgentEvent>,
    pub cancel: oneshot::Receiver<()>,
}

pub async fn run_session(req: SessionRequest<'_>) -> RunOutcome {
    match run_session_inner(req).await {
        Ok(out) => out,
        Err(e) => RunOutcome::Failed(e.to_string()),
    }
}

async fn run_session_inner(mut req: SessionRequest<'_>) -> Result<RunOutcome, AgentError> {
    if !req.workspace_path.is_dir() {
        return Err(AgentError::InvalidWorkspaceCwd(
            req.workspace_path.display().to_string(),
        ));
    }
    // Codex requires an absolute cwd path — `AbsolutePathBuf`.
    let abs_cwd = req
        .workspace_path
        .canonicalize()
        .map_err(|_| AgentError::InvalidWorkspaceCwd(req.workspace_path.display().to_string()))?;

    let mut child = spawn_codex(req.codex, &abs_cwd)?;
    let pid = child.id();
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "codex.stderr", "{}", line);
            }
        });
    }

    let mut conn = Connection::new(stdin, stdout, req.codex.read_timeout_ms);

    // 1. initialize
    conn.request(
        "initialize",
        json!({
            "clientInfo": {"name": "meridian", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {}
        }),
    )
    .await?;

    // 2. initialized notification
    conn.notify("initialized", json!({})).await?;

    // 3. thread/start
    let mut thread_params = json!({
        "approvalPolicy": req.codex.approval_policy,
        "sandbox": req.codex.thread_sandbox,
        "cwd": &abs_cwd,
    });
    // Codex prohibits combining `permissionProfile` with `sandbox`; we only
    // ever send `sandbox`. Workflow can still override at the per-turn level.
    let thread_resp = conn.request("thread/start", thread_params.take()).await?;
    let thread_id = thread_resp
        .get("thread")
        .and_then(|t| t.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AgentError::StartupFailed("thread/start missing thread.id".into()))?
        .to_string();
    // Capture rollout file path for the post-session source override (Codex
    // desktop sidebar workaround — see CodexConfig::session_source_override).
    let rollout_path = thread_resp
        .get("thread")
        .and_then(|t| t.get("path"))
        .and_then(|v| v.as_str())
        .map(|s| std::path::PathBuf::from(s));

    let first_prompt = (req.render_first_prompt)()
        .map_err(|e| AgentError::StartupFailed(format!("render prompt: {e}")))?;

    let mut turns_done: u32 = 0;
    loop {
        if turns_done >= req.max_turns {
            break;
        }
        // Cancel check before each turn.
        if matches!(
            req.cancel.try_recv(),
            Ok(()) | Err(oneshot::error::TryRecvError::Closed)
        ) {
            warn!(thread_id, "cancellation requested before turn start");
            break;
        }

        let prompt_text = if turns_done == 0 {
            first_prompt.clone()
        } else {
            (req.render_continuation)(turns_done + 1)
        };

        let turn_resp = conn
            .request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{"type": "text", "text": prompt_text}],
                    "approvalPolicy": req.codex.approval_policy,
                    "sandboxPolicy": req.codex.turn_sandbox_policy.clone(),
                }),
            )
            .await?;
        let turn_id = turn_resp
            .get("turn")
            .and_then(|t| t.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::StartupFailed("turn/start missing turn.id".into()))?
            .to_string();

        let sid = session_id(&thread_id, &turn_id);
        let _ = req.event_tx.send(AgentEvent::SessionStarted {
            session_id: sid.clone(),
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            codex_app_server_pid: pid,
            timestamp: Utc::now(),
        });

        let outcome = drive_turn(
            &mut conn,
            &sid,
            &turn_id,
            req.codex,
            &req.event_tx,
            &mut req.cancel,
        )
        .await?;
        turns_done += 1;
        match outcome {
            TurnEnd::Completed => continue,
            TurnEnd::Failed(msg) => {
                let _ = terminate(&mut child).await;
                patch_rollout_source(rollout_path.as_deref(), req.codex).await;
                return Ok(RunOutcome::Failed(msg));
            }
            TurnEnd::Cancelled => {
                let _ = terminate(&mut child).await;
                patch_rollout_source(rollout_path.as_deref(), req.codex).await;
                return Ok(RunOutcome::TurnsExhausted { turns: turns_done });
            }
        }
    }

    let _ = terminate(&mut child).await;
    patch_rollout_source(rollout_path.as_deref(), req.codex).await;
    Ok(RunOutcome::TurnsExhausted { turns: turns_done })
}

/// Workaround: rewrite the `source` field in line 1 of the Codex rollout file
/// so the session shows up in the Codex desktop sidebar. `codex app-server`
/// over stdio always tags sessions as `source: vscode`, which the desktop
/// filters out. We rewrite to whatever `codex.session_source_override` says
/// (default `cli`). No-op if the override is `None` or the path is missing.
async fn patch_rollout_source(path: Option<&std::path::Path>, codex: &CodexConfig) {
    let Some(path) = path else { return };
    let Some(target) = codex.session_source_override.as_deref() else {
        return;
    };
    if target.is_empty() {
        return;
    }
    let target = target.to_string();
    let target_for_log = target.clone();
    let path = path.to_path_buf();
    // Run on a blocking thread — small synchronous file ops.
    let res = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let body = std::fs::read_to_string(&path)?;
        let mut lines: Vec<&str> = body.split_inclusive('\n').collect();
        if lines.is_empty() {
            return Ok(());
        }
        let first = lines[0].trim_end_matches('\n');
        let mut meta: serde_json::Value = match serde_json::from_str(first) {
            Ok(v) => v,
            Err(_) => return Ok(()), // not JSON; bail silently
        };
        // Update payload.source if present.
        let changed = meta
            .get_mut("payload")
            .and_then(|p| p.as_object_mut())
            .map(|obj| {
                let cur = obj
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if cur.as_deref() == Some(target.as_str()) {
                    false
                } else {
                    obj.insert(
                        "source".to_string(),
                        serde_json::Value::String(target.clone()),
                    );
                    true
                }
            })
            .unwrap_or(false);
        if !changed {
            return Ok(());
        }
        let mut new_first = serde_json::to_string(&meta).unwrap_or_else(|_| first.to_string());
        new_first.push('\n');
        let owned: String = new_first;
        lines[0] = owned.as_str();
        let joined: String = lines.concat();
        // Atomic-ish write via tmp+rename.
        let mut tmp = path.clone();
        tmp.as_mut_os_string().push(".tmp");
        std::fs::write(&tmp, joined)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    })
    .await;
    match res {
        Ok(Ok(())) => debug!(target: "codex", "patched rollout source -> {target_for_log}"),
        Ok(Err(e)) => warn!(error = %e, "failed to patch rollout source"),
        Err(e) => warn!(error = %e, "rollout patch task panicked"),
    }
}

enum TurnEnd {
    Completed,
    Failed(String),
    Cancelled,
}

async fn drive_turn(
    conn: &mut Connection,
    sid: &str,
    expected_turn_id: &str,
    codex: &CodexConfig,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    cancel: &mut oneshot::Receiver<()>,
) -> Result<TurnEnd, AgentError> {
    let total = Duration::from_millis(codex.turn_timeout_ms);
    let deadline = tokio::time::Instant::now() + total;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            let _ = tx.send(AgentEvent::TurnFailed {
                session_id: sid.into(),
                error: "turn timeout".into(),
                timestamp: Utc::now(),
            });
            return Ok(TurnEnd::Failed(format!(
                "turn_timeout {}ms",
                codex.turn_timeout_ms
            )));
        }
        tokio::select! {
            biased;
            _ = &mut *cancel => {
                let _ = tx.send(AgentEvent::TurnCancelled {
                    session_id: sid.into(),
                    timestamp: Utc::now(),
                });
                return Ok(TurnEnd::Cancelled);
            }
            line = conn.read_line(remaining) => {
                let Some(line) = line? else {
                    return Err(AgentError::PortExit("stdout closed".into()));
                };
                if let Some(end) =
                    handle_line(conn, sid, expected_turn_id, codex, tx, &line).await?
                {
                    return Ok(end);
                }
            }
        }
    }
}

async fn handle_line(
    conn: &mut Connection,
    sid: &str,
    expected_turn_id: &str,
    codex: &CodexConfig,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    line: &str,
) -> Result<Option<TurnEnd>, AgentError> {
    let _ = codex;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed: protocol::JsonRpcResponse = match serde_json::from_str(trimmed) {
        Ok(p) => p,
        Err(_) => {
            let _ = tx.send(AgentEvent::Malformed {
                line: trimmed.into(),
                timestamp: Utc::now(),
            });
            return Ok(None);
        }
    };

    // Notification / server-request branch.
    if let Some(method) = parsed.method.as_deref() {
        let params = parsed.params.unwrap_or(Value::Null);
        // If this carries an `id`, it's a server request — needs a reply.
        if parsed.id.is_some() {
            let id = parsed.id.clone().unwrap_or(Value::Null);
            return handle_server_request(conn, sid, tx, method, id, &params).await;
        }
        return handle_notification(sid, expected_turn_id, tx, method, params);
    }

    // Otherwise — response to one of our requests, dispatched by id.
    if let Some(id) = parsed.id.clone().and_then(|v| v.as_u64()) {
        conn.deliver_response(id, parsed);
        return Ok(None);
    }

    let _ = tx.send(AgentEvent::OtherMessage {
        session_id: Some(sid.into()),
        payload: serde_json::to_value(&parsed).unwrap_or(Value::Null),
        timestamp: Utc::now(),
    });
    Ok(None)
}

fn handle_notification(
    sid: &str,
    expected_turn_id: &str,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    method: &str,
    params: Value,
) -> Result<Option<TurnEnd>, AgentError> {
    let now = Utc::now();
    match method {
        "turn/completed" => {
            let turn = params.get("turn").cloned().unwrap_or(Value::Null);
            let status = turn
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("completed");
            // Ignore completions for older turns belonging to the same thread —
            // shouldn't happen with our serial driver but guard anyway.
            let turn_id = turn.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if !turn_id.is_empty() && turn_id != expected_turn_id {
                debug!(turn_id, expected_turn_id, "ignoring completion for stale turn");
                return Ok(None);
            }
            match status {
                "completed" => {
                    let _ = tx.send(AgentEvent::TurnCompleted {
                        session_id: sid.into(),
                        usage: None,
                        timestamp: now,
                    });
                    Ok(Some(TurnEnd::Completed))
                }
                "interrupted" => {
                    let _ = tx.send(AgentEvent::TurnCancelled {
                        session_id: sid.into(),
                        timestamp: now,
                    });
                    Ok(Some(TurnEnd::Cancelled))
                }
                _ => {
                    let err = turn
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("turn failed")
                        .to_string();
                    let _ = tx.send(AgentEvent::TurnFailed {
                        session_id: sid.into(),
                        error: err.clone(),
                        timestamp: now,
                    });
                    Ok(Some(TurnEnd::Failed(err)))
                }
            }
        }
        "thread/tokenUsage/updated" => {
            // params.tokenUsage.total.{inputTokens,outputTokens,totalTokens}
            if let Some(total) = params.get("tokenUsage").and_then(|t| t.get("total")) {
                let usage = TokenUsage {
                    input_tokens: total.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    output_tokens: total
                        .get("outputTokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    total_tokens: total.get("totalTokens").and_then(|v| v.as_u64()).unwrap_or(0),
                };
                let _ = tx.send(AgentEvent::TokenUsageUpdated {
                    session_id: sid.into(),
                    usage,
                    timestamp: now,
                });
            }
            Ok(None)
        }
        "account/rateLimits/updated" => {
            let payload = params
                .get("rateLimits")
                .cloned()
                .unwrap_or(params.clone());
            let _ = tx.send(AgentEvent::RateLimitsUpdated {
                payload,
                timestamp: now,
            });
            Ok(None)
        }
        "error" => {
            // Streaming error notification (params.error.message + willRetry flag).
            let will_retry = params
                .get("willRetry")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let msg = params
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("error notification")
                .to_string();
            let _ = tx.send(AgentEvent::Notification {
                session_id: Some(sid.into()),
                payload: json!({"error": msg, "willRetry": will_retry}),
                timestamp: now,
            });
            Ok(None)
        }
        "item/agentMessage/delta" => {
            let delta = params
                .get("delta")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !delta.is_empty() {
                let _ = tx.send(AgentEvent::AgentMessageDelta {
                    session_id: sid.into(),
                    delta,
                    timestamp: now,
                });
            }
            Ok(None)
        }
        "item/completed" | "item/started" => {
            let kind = params
                .get("item")
                .and_then(|i| i.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("item")
                .to_string();
            let _ = tx.send(AgentEvent::ItemEvent {
                session_id: sid.into(),
                method: method.into(),
                kind,
                payload: params,
                timestamp: now,
            });
            Ok(None)
        }
        _ => {
            // Pass-through for everything else (mcp startup, skills/changed, etc).
            let _ = tx.send(AgentEvent::OtherMessage {
                session_id: Some(sid.into()),
                payload: json!({"method": method, "params": params}),
                timestamp: now,
            });
            Ok(None)
        }
    }
}

async fn handle_server_request(
    conn: &mut Connection,
    sid: &str,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    method: &str,
    id: Value,
    params: &Value,
) -> Result<Option<TurnEnd>, AgentError> {
    let now = Utc::now();
    match method {
        // High-trust auto-approvals — spec §10.5 example posture.
        "item/commandExecution/requestApproval"
        | "execCommandApproval"
        | "item/fileChange/requestApproval"
        | "applyPatchApproval" => {
            conn.write_response(id, json!({"decision": "accept"})).await?;
            let _ = tx.send(AgentEvent::ApprovalAutoApproved {
                session_id: sid.into(),
                kind: method.into(),
                timestamp: now,
            });
            Ok(None)
        }
        "item/permissions/requestApproval" => {
            // Grant unrestricted managed profile for the session under high-trust posture.
            let resp = json!({
                "permissions": {
                    "type": "managed",
                    "fileSystem": {"type": "unrestricted"},
                    "network": {"enabled": true}
                },
                "scope": "session"
            });
            conn.write_response(id, resp).await?;
            let _ = tx.send(AgentEvent::ApprovalAutoApproved {
                session_id: sid.into(),
                kind: method.into(),
                timestamp: now,
            });
            Ok(None)
        }
        "item/tool/requestUserInput" => {
            // Spec §10.5: hard-fail the run.
            // Reply with empty answers so the agent stops cleanly, then signal failure.
            let _ = conn
                .write_response(id, json!({"answers": {}}))
                .await;
            let _ = tx.send(AgentEvent::TurnInputRequired {
                session_id: sid.into(),
                timestamp: now,
            });
            Ok(Some(TurnEnd::Failed("user input required".into())))
        }
        "item/tool/call" => {
            // Meridian does not advertise any client-side tools at thread/start
            // (Codex's ThreadStartParams has no `clientTools` field). Any
            // dynamic tool call here is therefore unsupported by design — we
            // reply failure and keep the session running per spec §10.5.
            let tool = params
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let _ = tx.send(AgentEvent::UnsupportedToolCall {
                session_id: sid.into(),
                tool: tool.clone(),
                timestamp: now,
            });
            let result = json!({
                "success": false,
                "contentItems": [
                    {"type": "inputText", "text": format!("unsupported_tool_call: {tool}")}
                ]
            });
            conn.write_response(id, result).await?;
            Ok(None)
        }
        "mcpServer/elicitation/request" => {
            // No interactive operator — decline with empty content.
            conn.write_response(id, json!({"action": "decline"})).await?;
            Ok(None)
        }
        "account/chatgptAuthTokens/refresh" => {
            // We don't manage ChatGPT auth — return error so codex falls back.
            conn.write_response(id, json!({"error": "not supported by meridian"}))
                .await?;
            Ok(None)
        }
        _ => {
            warn!(method, "unhandled server request; replying with error");
            conn.write_response(id, json!({"error": format!("unhandled: {method}")}))
                .await?;
            Ok(None)
        }
    }
}

fn spawn_codex(codex: &CodexConfig, cwd: &std::path::Path) -> Result<Child, AgentError> {
    let mut cmd = Command::new("bash");
    cmd.arg("-lc")
        .arg(&codex.command)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    info!(command = %codex.command, cwd = %cwd.display(), "spawning codex app-server");
    cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AgentError::CodexNotFound(codex.command.clone())
        } else {
            AgentError::Io(e)
        }
    })
}

async fn terminate(child: &mut Child) -> std::io::Result<()> {
    if let Some(id) = child.id() {
        debug!(pid = id, "terminating codex");
    }
    child.kill().await
}

struct Connection {
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    next_id: u64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<protocol::JsonRpcResponse>>>>,
    read_timeout_ms: u64,
    line_buf: String,
}

impl Connection {
    fn new(stdin: ChildStdin, stdout: ChildStdout, read_timeout_ms: u64) -> Self {
        Self {
            stdin,
            reader: BufReader::with_capacity(10 * 1024 * 1024, stdout),
            next_id: 0,
            pending: Arc::new(Mutex::new(HashMap::new())),
            read_timeout_ms,
            line_buf: String::new(),
        }
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, AgentError> {
        self.next_id += 1;
        let id = self.next_id;
        let (tx, mut rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let body = protocol::request(id, method, params);
        self.write(&body).await?;

        // For thread/start the model warm-up can take longer than read_timeout_ms;
        // give thread/start and turn/start a longer ceiling.
        let wait_ms = if matches!(method, "thread/start" | "turn/start") {
            self.read_timeout_ms.max(30_000)
        } else {
            self.read_timeout_ms.max(1)
        };
        let result = timeout(Duration::from_millis(wait_ms), async {
            loop {
                if let Ok(resp) = rx.try_recv() {
                    return Ok::<_, AgentError>(resp);
                }
                let line_opt = self.read_line_inner().await?;
                let Some(line) = line_opt else {
                    return Err(AgentError::PortExit("stdout closed".into()));
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(resp) = serde_json::from_str::<protocol::JsonRpcResponse>(trimmed) {
                    if let Some(id_u64) = resp.id.as_ref().and_then(|v| v.as_u64()) {
                        if id_u64 == id {
                            return Ok(resp);
                        }
                        if let Some(tx) = self.pending.lock().await.remove(&id_u64) {
                            let _ = tx.send(resp);
                        }
                    }
                    // notifications during startup are silently dropped here —
                    // they'll re-appear once `drive_turn` takes over reading.
                }
            }
        })
        .await;
        match result {
            Ok(Ok(resp)) => {
                if let Some(err) = resp.error {
                    return Err(AgentError::ResponseError(err.to_string()));
                }
                Ok(resp.result.unwrap_or(Value::Null))
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AgentError::ResponseTimeout {
                request: method.into(),
            }),
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), AgentError> {
        let body = protocol::notification(method, params);
        self.write(&body).await
    }

    async fn write_response(&mut self, id: Value, result: Value) -> Result<(), AgentError> {
        let body = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
        .expect("serialize");
        self.write(&body).await
    }

    fn deliver_response(&mut self, id: u64, resp: protocol::JsonRpcResponse) {
        let pending = self.pending.clone();
        tokio::spawn(async move {
            if let Some(tx) = pending.lock().await.remove(&id) {
                let _ = tx.send(resp);
            }
        });
    }

    async fn write(&mut self, body: &str) -> Result<(), AgentError> {
        tracing::debug!(target: "codex.send", "{}", body);
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn read_line(&mut self, max_wait: Duration) -> Result<Option<String>, AgentError> {
        match timeout(max_wait, self.read_line_inner()).await {
            Ok(r) => r,
            Err(_) => Err(AgentError::TurnTimeout {
                ms: max_wait.as_millis() as u64,
            }),
        }
    }

    async fn read_line_inner(&mut self) -> Result<Option<String>, AgentError> {
        self.line_buf.clear();
        let n = self.reader.read_line(&mut self.line_buf).await?;
        if n == 0 {
            return Ok(None);
        }
        let out = std::mem::take(&mut self.line_buf);
        tracing::debug!(target: "codex.recv", "{}", out.trim_end());
        Ok(Some(out))
    }
}
