//! Spawn the runner against an automation file and capture its output.

use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::warn;

use meridian_store::Store;

use crate::runtime::RuntimeInfo;
use crate::tokens::{TokenContext, TokenStore};

const RUN_TIMEOUT_SECS: u64 = 120;
const LOG_TAIL_BYTES: usize = 8 * 1024;

pub struct RunOutcome {
    pub success: bool,
    pub log: String,
    pub error: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn execute(
    runner: &Path,
    file: &Path,
    automation_id: &str,
    last_run_at: Option<DateTime<Utc>>,
    dry_run: bool,
    runtime: &RuntimeInfo,
    sdk_base: &str,
    tokens: &TokenStore,
    store: &Arc<Store>,
) -> (i64, RunOutcome) {
    let started_at = Utc::now();
    let run_id = match store
        .insert_automation_run(automation_id, dry_run, started_at)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return (
                0,
                RunOutcome {
                    success: false,
                    log: String::new(),
                    error: Some(format!("failed to record run: {e}")),
                },
            );
        }
    };

    if runtime.missing {
        let outcome = RunOutcome {
            success: false,
            log: String::new(),
            error: Some(
                "no JS runtime detected (install Bun or Node 22.6+)".into(),
            ),
        };
        if let Err(e) = store
            .finish_automation_run(
                run_id,
                Utc::now(),
                false,
                outcome.error.as_deref(),
                Some(""),
            )
            .await
        {
            warn!(run_id, error = %e, "failed to finalize automation run");
        }
        return (run_id, outcome);
    }

    let token = tokens.issue(TokenContext {
        automation_id: automation_id.to_string(),
        run_id,
        dry_run,
    });

    let mut cmd = Command::new(&runtime.command);
    for flag in runtime.flags_for(file) {
        cmd.arg(flag);
    }
    cmd.arg(runner)
        .arg("run")
        .arg(file)
        .env("MERIDIAN_AUTOMATION_BASE", sdk_base)
        .env("MERIDIAN_AUTOMATION_TOKEN", &token)
        .env("MERIDIAN_AUTOMATION_RUN_ID", run_id.to_string())
        .env(
            "MERIDIAN_AUTOMATION_DRY",
            if dry_run { "1" } else { "0" },
        )
        .env(
            "MERIDIAN_LAST_RUN_AT",
            last_run_at.map(|d| d.to_rfc3339()).unwrap_or_default(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let outcome = match cmd.spawn() {
        Ok(child) => collect(child).await,
        Err(e) => RunOutcome {
            success: false,
            log: String::new(),
            error: Some(format!("spawn node: {e}")),
        },
    };
    tokens.revoke(&token);

    if let Err(e) = store
        .finish_automation_run(
            run_id,
            Utc::now(),
            outcome.success,
            outcome.error.as_deref(),
            Some(outcome.log.as_str()),
        )
        .await
    {
        warn!(run_id, error = %e, "failed to finalize automation run");
    }

    (run_id, outcome)
}

async fn collect(mut child: tokio::process::Child) -> RunOutcome {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let log = Arc::new(Mutex::new(String::new()));

    let log1 = log.clone();
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut g = log1.lock().await;
                g.push_str(&line);
                g.push('\n');
            }
        }
    });
    let log2 = log.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut g = log2.lock().await;
                g.push_str("[stderr] ");
                g.push_str(&line);
                g.push('\n');
            }
        }
    });

    let status_res = timeout(Duration::from_secs(RUN_TIMEOUT_SECS), child.wait()).await;
    let _ = tokio::join!(stdout_task, stderr_task);

    let mut log_text = log.lock().await.clone();
    if log_text.len() > LOG_TAIL_BYTES {
        let drop_n = log_text.len() - LOG_TAIL_BYTES;
        log_text = format!("…(truncated {drop_n} bytes)…\n{}", &log_text[drop_n..]);
    }

    match status_res {
        Ok(Ok(status)) => {
            if status.success() {
                RunOutcome {
                    success: true,
                    log: log_text,
                    error: None,
                }
            } else {
                let code = status.code().unwrap_or(-1);
                RunOutcome {
                    success: false,
                    log: log_text,
                    error: Some(format!("automation exited with status {code}")),
                }
            }
        }
        Ok(Err(e)) => RunOutcome {
            success: false,
            log: log_text,
            error: Some(format!("wait error: {e}")),
        },
        Err(_) => RunOutcome {
            success: false,
            log: log_text,
            error: Some(format!("automation exceeded {RUN_TIMEOUT_SECS}s timeout")),
        },
    }
}
