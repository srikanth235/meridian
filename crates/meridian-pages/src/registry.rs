//! Discover page folders + parse their meta.toml.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use meridian_store::Store;

use crate::meta;

pub const PAGE_FILE: &str = "page.tsx";
pub const META_FILE: &str = "meta.toml";

/// True when `<dir>/page.tsx` and `<dir>/meta.toml` both exist (a valid page
/// folder). Hidden / underscore-prefixed dirs are skipped so users have an
/// escape hatch for drafts.
pub fn is_page_folder(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    if let Some(name) = dir.file_name().and_then(|s| s.to_str()) {
        if name.starts_with('.') || name.starts_with('_') {
            return false;
        }
    }
    dir.join(PAGE_FILE).is_file() && dir.join(META_FILE).is_file()
}

/// Return the slug for a folder under `pages_dir`.
pub fn slug_for(pages_dir: &Path, folder: &Path) -> String {
    folder
        .strip_prefix(pages_dir)
        .unwrap_or(folder)
        .to_string_lossy()
        .to_string()
}

pub fn scan(pages_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(pages_dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if is_page_folder(&p) {
            out.push(p);
        }
    }
    out
}

/// Refresh the registry from disk: rescan folders, parse each meta.toml,
/// upsert into the store. Returns the slugs currently present so callers
/// can prune deleted pages.
pub async fn refresh(pages_dir: &Path, store: &Arc<Store>) -> Vec<String> {
    let folders = scan(pages_dir);
    let mut present: Vec<String> = Vec::with_capacity(folders.len());
    for folder in folders {
        let slug = slug_for(pages_dir, &folder);
        present.push(slug.clone());
        let meta_path = folder.join(META_FILE);
        match std::fs::read_to_string(&meta_path) {
            Ok(src) => match meta::parse(&src) {
                Ok(m) => {
                    if let Err(e) = store
                        .upsert_page(
                            &slug,
                            &folder.to_string_lossy(),
                            &m.title,
                            m.icon.as_deref(),
                            m.position,
                            m.meta_version,
                            None,
                        )
                        .await
                    {
                        warn!(page = %slug, error = %e, "failed to upsert page");
                    } else {
                        info!(page = %slug, title = %m.title, "registered page");
                    }
                }
                Err(err) => {
                    warn!(page = %slug, error = %err, "failed to parse meta.toml");
                    let _ = store
                        .upsert_page(
                            &slug,
                            &folder.to_string_lossy(),
                            &slug,
                            None,
                            100,
                            meta::DEFAULT_META_VERSION,
                            Some(&err),
                        )
                        .await;
                }
            },
            Err(e) => {
                let err = format!("read {META_FILE}: {e}");
                warn!(page = %slug, error = %err, "failed to read meta.toml");
                let _ = store
                    .upsert_page(
                        &slug,
                        &folder.to_string_lossy(),
                        &slug,
                        None,
                        100,
                        meta::DEFAULT_META_VERSION,
                        Some(&err),
                    )
                    .await;
            }
        }
    }
    present
}

/// Drop store rows whose folders have disappeared.
pub async fn prune_missing(store: &Arc<Store>, present: &[String]) {
    let present_set: std::collections::HashSet<&str> = present.iter().map(|s| s.as_str()).collect();
    let Ok(all) = store.list_pages().await else {
        return;
    };
    for row in all {
        if !present_set.contains(row.slug.as_str()) {
            let path = PathBuf::from(&row.folder_path);
            if !path.exists() || !is_page_folder(&path) {
                if let Err(e) = store.delete_page(&row.slug).await {
                    warn!(page = %row.slug, error = %e, "failed to delete missing page");
                } else {
                    info!(page = %row.slug, "removed page (folder gone)");
                }
            }
        }
    }
}
