use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy)]
pub enum HookKind {
    AfterCreate,
    BeforeRun,
    AfterRun,
    BeforeRemove,
}

impl HookKind {
    fn name(self) -> &'static str {
        match self {
            HookKind::AfterCreate => "after_create",
            HookKind::BeforeRun => "before_run",
            HookKind::AfterRun => "after_run",
            HookKind::BeforeRemove => "before_remove",
        }
    }
}

#[derive(Debug, Error)]
pub enum HookError {
    #[error("hook {hook} timed out after {timeout_ms}ms")]
    Timeout { hook: &'static str, timeout_ms: u64 },
    #[error("hook {hook} exited with status {code:?}: {stderr}")]
    NonZeroExit {
        hook: &'static str,
        code: Option<i32>,
        stderr: String,
    },
    #[error("io error running hook {hook}: {source}")]
    Io {
        hook: &'static str,
        #[source]
        source: std::io::Error,
    },
}

/// Run a workspace hook script in `cwd` via `bash -lc <script>`
/// (spec §9.4). Returns Ok(()) when the script is `None` or runs cleanly.
pub async fn run_hook(
    kind: HookKind,
    script: Option<&str>,
    cwd: &Path,
    timeout_ms: u64,
) -> Result<(), HookError> {
    let Some(script) = script else { return Ok(()) };
    if script.trim().is_empty() {
        return Ok(());
    }
    let name = kind.name();
    info!(hook = name, cwd = %cwd.display(), "hook starting");

    let mut cmd = Command::new("bash");
    cmd.arg("-lc")
        .arg(script)
        .current_dir(cwd)
        .kill_on_drop(true);

    let fut = cmd.output();
    let out = match tokio::time::timeout(Duration::from_millis(timeout_ms), fut).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            warn!(hook = name, error = %e, "hook io error");
            return Err(HookError::Io { hook: name, source: e });
        }
        Err(_) => {
            warn!(hook = name, timeout_ms, "hook timeout");
            return Err(HookError::Timeout { hook: name, timeout_ms });
        }
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        warn!(hook = name, code = ?out.status.code(), stderr = %stderr, "hook failed");
        return Err(HookError::NonZeroExit {
            hook: name,
            code: out.status.code(),
            stderr,
        });
    }
    Ok(())
}
