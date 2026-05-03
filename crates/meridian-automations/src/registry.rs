//! Discover automation files and parse their metadata.
//!
//! "Parse" here means asking Node to import the file and print the
//! default-export's `{name, schedule}` — the runner does this in `describe`
//! mode. Cheap (one Node spawn per file change) and avoids us reimplementing
//! TS parsing in Rust.

use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};

use meridian_store::Store;

use crate::runtime::RuntimeInfo;
use crate::schedule::{initial_next_run_at, Schedule};

const SCRIPT_EXTENSIONS: &[&str] = &["ts", "mjs", "js", "mts"];

#[derive(Debug, Clone)]
pub struct DescribeResult {
    pub name: String,
    pub schedule: Schedule,
    pub schedule_json: String,
}

pub fn id_for_file(automations_dir: &Path, file: &Path) -> String {
    let rel = file
        .strip_prefix(automations_dir)
        .unwrap_or(file)
        .with_extension("");
    rel.to_string_lossy().replace(['/', '\\'], "-")
}

pub fn is_automation_file(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if name.starts_with('.') || name.starts_with('_') {
        return false;
    }
    let Some(ext) = p.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    if !SCRIPT_EXTENSIONS.contains(&ext) {
        return false;
    }
    // Skip files inside node_modules / .runtime.
    let s = p.to_string_lossy();
    if s.contains("/node_modules/") || s.contains("/.runtime/") {
        return false;
    }
    true
}

pub async fn scan(automations_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(automations_dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if is_automation_file(&p) {
            out.push(p);
        }
    }
    out
}

pub fn hash_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Some(hex::encode(h.finalize()))
}

pub async fn describe(
    runner: &Path,
    file: &Path,
    runtime: &RuntimeInfo,
) -> Result<DescribeResult, String> {
    if runtime.missing {
        return Err("no JS runtime detected (install Bun or Node 22.6+)".into());
    }
    let needs_ts = file
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| e == "ts" || e == "mts")
        .unwrap_or(false);
    if needs_ts && !runtime.supports_ts {
        return Err(runtime
            .hint
            .clone()
            .unwrap_or_else(|| "this runtime can't parse .ts files".into()));
    }
    let mut cmd = Command::new(&runtime.command);
    for flag in runtime.flags_for(file) {
        cmd.arg(flag);
    }
    cmd.arg(runner)
        .arg("describe")
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = match timeout(Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(format!("spawn node: {e}")),
        Err(_) => return Err("describe timed out after 20s".into()),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!(
            "node exit {}: {stderr}",
            out.status.code().unwrap_or(-1)
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("describe output not JSON: {e} (got: {stdout})"))?;
    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("missing name")?
        .to_string();
    let schedule_value = parsed.get("schedule").ok_or("missing schedule")?.clone();
    let schedule_json = serde_json::to_string(&schedule_value).unwrap_or_else(|_| "{}".into());
    let schedule: Schedule =
        serde_json::from_value(schedule_value).map_err(|e| format!("schedule shape: {e}"))?;
    Ok(DescribeResult { name, schedule, schedule_json })
}

/// Refresh the registry from disk: rescan files, describe each, upsert into
/// store. Returns the list of (id, file_path) currently present so the caller
/// can prune deleted automations.
pub async fn refresh(
    automations_dir: &Path,
    runner: &Path,
    runtime: &RuntimeInfo,
    store: &Arc<Store>,
) -> Vec<(String, PathBuf)> {
    let files = scan(automations_dir).await;
    let mut present: Vec<(String, PathBuf)> = Vec::with_capacity(files.len());
    for file in files {
        let id = id_for_file(automations_dir, &file);
        present.push((id.clone(), file.clone()));
        let hash = hash_file(&file);
        // If hash matches what we already have, skip describing (cheap).
        if let Ok(Some(existing)) = store.get_automation(&id).await {
            if existing.source_hash.as_deref() == hash.as_deref() && existing.parse_error.is_none()
            {
                continue;
            }
        }
        match describe(runner, &file, runtime).await {
            Ok(d) => {
                if let Err(e) = store
                    .upsert_automation(
                        &id,
                        &file.to_string_lossy(),
                        &d.name,
                        &d.schedule_json,
                        hash.as_deref(),
                        None,
                        Some(initial_next_run_at(Utc::now())),
                    )
                    .await
                {
                    warn!(automation = %id, error = %e, "failed to upsert automation");
                } else {
                    info!(automation = %id, name = %d.name, "registered automation");
                }
            }
            Err(err) => {
                warn!(automation = %id, error = %err, "failed to describe automation");
                let _ = store
                    .upsert_automation(
                        &id,
                        &file.to_string_lossy(),
                        &id, // fallback name
                        "{}",
                        hash.as_deref(),
                        Some(&err),
                        None,
                    )
                    .await;
            }
        }
    }
    present
}

/// Drop store rows whose files have disappeared from disk.
pub async fn prune_missing(
    store: &Arc<Store>,
    present: &[(String, PathBuf)],
) {
    let present_ids: std::collections::HashSet<&str> =
        present.iter().map(|(id, _)| id.as_str()).collect();
    let Ok(all) = store.list_automations().await else {
        return;
    };
    for row in all {
        if !present_ids.contains(row.id.as_str()) {
            // Also verify file is actually gone (avoids races on rapid
            // re-saves where we briefly see no entries).
            let path = PathBuf::from(&row.file_path);
            if !path.exists() {
                if let Err(e) = store.delete_automation(&row.id).await {
                    warn!(automation = %row.id, error = %e, "failed to delete missing automation");
                } else {
                    info!(automation = %row.id, "removed automation (file gone)");
                }
            }
        }
    }
}
