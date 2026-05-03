//! Run an automation file: parse the manifest, evaluate it, record the run.

use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::Arc;
use tracing::warn;

use meridian_store::Store;

use crate::evaluator::{evaluate, EvalOutcome};
use crate::manifest;
use crate::sdk::{RunCtx, SdkSurface};

pub struct RunOutcome {
    pub success: bool,
    pub log: String,
    pub error: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn execute(
    file: &Path,
    automation_id: &str,
    last_run_at: Option<DateTime<Utc>>,
    dry_run: bool,
    surface: &SdkSurface,
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

    let outcome = run_inner(file, automation_id, last_run_at, dry_run, run_id, surface).await;

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

async fn run_inner(
    file: &Path,
    automation_id: &str,
    last_run_at: Option<DateTime<Utc>>,
    dry_run: bool,
    run_id: i64,
    surface: &SdkSurface,
) -> RunOutcome {
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            return RunOutcome {
                success: false,
                log: String::new(),
                error: Some(format!("read {}: {e}", file.display())),
            };
        }
    };
    let manifest = match manifest::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            return RunOutcome {
                success: false,
                log: String::new(),
                error: Some(format!("parse {}: {e}", file.display())),
            };
        }
    };

    let ctx = RunCtx {
        automation_id: automation_id.to_string(),
        run_id,
        dry_run,
        last_run_at,
    };
    let EvalOutcome {
        success,
        log,
        error,
    } = evaluate(&manifest, &ctx, surface).await;
    RunOutcome { success, log, error }
}
