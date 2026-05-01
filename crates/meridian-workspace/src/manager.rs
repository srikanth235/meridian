use std::path::{Path, PathBuf};
use meridian_config::HooksConfig;
use meridian_core::{sanitize_workspace_key, Workspace};
use thiserror::Error;
use tracing::{info, warn};

use crate::hooks::{run_hook, HookError, HookKind};

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("workspace path {path} escapes workspace root {root}")]
    OutsideRoot { path: PathBuf, root: PathBuf },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("after_create hook failed: {0}")]
    AfterCreate(#[from] HookError),
}

#[derive(Clone)]
pub struct WorkspaceManager {
    pub root: PathBuf,
}

impl WorkspaceManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolve the per-issue workspace path (no IO). Used by safety checks.
    pub fn workspace_path(&self, identifier: &str) -> PathBuf {
        let key = sanitize_workspace_key(identifier);
        self.root.join(key)
    }

    /// Ensure the per-issue directory exists, run `after_create` if newly
    /// created, and validate the safety invariants from spec §9.5.
    pub async fn ensure(
        &self,
        identifier: &str,
        hooks: &HooksConfig,
    ) -> Result<Workspace, WorkspaceError> {
        let key = sanitize_workspace_key(identifier);
        let path = self.root.join(&key);
        Self::validate_inside_root(&self.root, &path)?;

        // Ensure root exists.
        tokio::fs::create_dir_all(&self.root).await?;
        let created_now = match tokio::fs::metadata(&path).await {
            Ok(m) if m.is_dir() => false,
            Ok(_) => {
                // Path exists but is not a directory — replace.
                tokio::fs::remove_file(&path).await?;
                tokio::fs::create_dir_all(&path).await?;
                true
            }
            Err(_) => {
                tokio::fs::create_dir_all(&path).await?;
                true
            }
        };

        if created_now {
            info!(identifier, path = %path.display(), "workspace created");
            if let Err(e) = run_hook(
                HookKind::AfterCreate,
                hooks.after_create.as_deref(),
                &path,
                hooks.timeout_ms,
            )
            .await
            {
                // Roll back: spec §9.4 says after_create failure aborts creation.
                let _ = tokio::fs::remove_dir_all(&path).await;
                return Err(WorkspaceError::AfterCreate(e));
            }
        }

        Ok(Workspace {
            path,
            workspace_key: key,
            created_now,
        })
    }

    /// Remove the per-issue workspace (used during terminal cleanup).
    pub async fn remove(&self, identifier: &str, hooks: &HooksConfig) {
        let path = self.workspace_path(identifier);
        if !path.exists() {
            return;
        }
        if let Err(e) = run_hook(
            HookKind::BeforeRemove,
            hooks.before_remove.as_deref(),
            &path,
            hooks.timeout_ms,
        )
        .await
        {
            warn!(identifier, error = %e, "before_remove hook failed; continuing cleanup");
        }
        if let Err(e) = tokio::fs::remove_dir_all(&path).await {
            warn!(identifier, error = %e, "workspace remove failed");
        } else {
            info!(identifier, path = %path.display(), "workspace removed");
        }
    }

    /// Spec §9.5 invariant 2: workspace must stay inside the workspace root.
    pub fn validate_inside_root(root: &Path, candidate: &Path) -> Result<(), WorkspaceError> {
        let abs_root = absolutize(root);
        let abs_cand = absolutize(candidate);
        if !abs_cand.starts_with(&abs_root) {
            return Err(WorkspaceError::OutsideRoot {
                path: abs_cand,
                root: abs_root,
            });
        }
        Ok(())
    }
}

fn absolutize(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|c| c.join(p))
            .unwrap_or_else(|_| p.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_config::HooksConfig;

    #[tokio::test]
    async fn ensures_workspace_and_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = WorkspaceManager::new(tmp.path().to_path_buf());
        let hooks = HooksConfig {
            timeout_ms: 5_000,
            ..Default::default()
        };
        let w1 = mgr.ensure("ABC-1", &hooks).await.unwrap();
        assert!(w1.created_now);
        let w2 = mgr.ensure("ABC-1", &hooks).await.unwrap();
        assert!(!w2.created_now);
        assert_eq!(w1.path, w2.path);
    }

    #[tokio::test]
    async fn sanitizes_identifier() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = WorkspaceManager::new(tmp.path().to_path_buf());
        let hooks = HooksConfig { timeout_ms: 5000, ..Default::default() };
        let w = mgr.ensure("../escape", &hooks).await.unwrap();
        assert!(w.path.starts_with(tmp.path()));
        assert_eq!(w.workspace_key, ".._escape");
    }
}
