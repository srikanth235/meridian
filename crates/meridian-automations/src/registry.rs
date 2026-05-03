//! Discover automation files and parse their metadata.

use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use meridian_store::Store;

use crate::manifest;
use crate::schedule::initial_next_run_at;

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
    matches!(p.extension().and_then(|s| s.to_str()), Some("toml"))
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

/// Refresh the registry from disk: rescan files, parse each, upsert into
/// store. Returns the list of (id, file_path) currently present so the caller
/// can prune deleted automations.
pub async fn refresh(
    automations_dir: &Path,
    store: &Arc<Store>,
) -> Vec<(String, PathBuf)> {
    let files = scan(automations_dir).await;
    let mut present: Vec<(String, PathBuf)> = Vec::with_capacity(files.len());
    for file in files {
        let id = id_for_file(automations_dir, &file);
        present.push((id.clone(), file.clone()));
        let src = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                let err = format!("read: {e}");
                warn!(automation = %id, error = %err, "failed to read automation");
                let _ = store
                    .upsert_automation(
                        &id,
                        &file.to_string_lossy(),
                        &id,
                        "{}",
                        None,
                        Some(&err),
                        None,
                    )
                    .await;
                continue;
            }
        };
        match manifest::parse(&src) {
            Ok(m) => {
                let schedule_json =
                    serde_json::to_string(&m.schedule).unwrap_or_else(|_| "{}".into());
                if let Err(e) = store
                    .upsert_automation(
                        &id,
                        &file.to_string_lossy(),
                        &m.name,
                        &schedule_json,
                        None,
                        None,
                        Some(initial_next_run_at(Utc::now())),
                    )
                    .await
                {
                    warn!(automation = %id, error = %e, "failed to upsert automation");
                } else {
                    info!(automation = %id, name = %m.name, "registered automation");
                }
            }
            Err(err) => {
                warn!(automation = %id, error = %err, "failed to parse automation");
                let _ = store
                    .upsert_automation(
                        &id,
                        &file.to_string_lossy(),
                        &id,
                        "{}",
                        None,
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
