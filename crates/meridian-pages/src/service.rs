//! Top-level glue: filesystem watcher + clone-able handle for HTTP routes.
//!
//! The pages subsystem has no scheduler — it's a pure registry. Page
//! execution lives in the renderer's iframe runtime; this crate only
//! discovers folders, parses `meta.toml`, and serves source/queries.

use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use meridian_store::{InboxEntryRecord, PageRecord, ReadOnlyQueryResult, Store};

use crate::meta;
use crate::nl::{generate, generate_fix, GeneratedSpec};
use crate::registry::{prune_missing, refresh, META_FILE, PAGE_FILE};

#[derive(Clone, Debug)]
pub enum PagesEvent {
    Refreshed,
}

#[derive(Clone)]
pub struct PagesHandle {
    inner: Arc<Inner>,
}

struct Inner {
    store: Arc<Store>,
    pages_dir: PathBuf,
    rescan_tx: mpsc::UnboundedSender<()>,
    events: broadcast::Sender<PagesEvent>,
}

impl PagesHandle {
    pub fn store(&self) -> Arc<Store> {
        self.inner.store.clone()
    }

    pub fn pages_dir(&self) -> &Path {
        &self.inner.pages_dir
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PagesEvent> {
        self.inner.events.subscribe()
    }

    pub fn request_rescan(&self) {
        let _ = self.inner.rescan_tx.send(());
    }

    pub async fn list(&self) -> Vec<PageRecord> {
        self.inner.store.list_pages().await.unwrap_or_default()
    }

    pub async fn get(&self, slug: &str) -> Option<PageRecord> {
        self.inner.store.get_page(slug).await.ok().flatten()
    }

    pub async fn read_source(&self, slug: &str) -> Option<String> {
        let row = self.get(slug).await?;
        let path = PathBuf::from(&row.folder_path).join(PAGE_FILE);
        std::fs::read_to_string(&path).ok()
    }

    pub async fn touch_opened(&self, slug: &str) {
        let _ = self.inner.store.touch_page_opened(slug).await;
    }

    /// Run a read-only SQL query on behalf of a page. The slug is informative
    /// for logging; the connection is opened in `SQLITE_OPEN_READ_ONLY` mode
    /// regardless of who calls it.
    pub async fn query(
        &self,
        slug: &str,
        sql: String,
        params: Vec<serde_json::Value>,
        max_rows: usize,
        timeout_ms: u64,
    ) -> Result<ReadOnlyQueryResult, String> {
        match self
            .inner
            .store
            .read_only_query(sql, params, max_rows, timeout_ms)
            .await
        {
            Ok(r) => Ok(r),
            Err(e) => {
                warn!(page = %slug, error = %e, "page query failed");
                Err(e.to_string())
            }
        }
    }

    pub async fn submit_request(&self, nl: &str) -> Result<(String, GeneratedSpec), String> {
        let spec = generate(nl);
        let id = self
            .inner
            .store
            .insert_inbox_entry(
                "page-request",
                &spec.title,
                Some(&spec.body),
                None,
                &["page".into(), "request".into()],
                Some("page-request"),
                Some(&spec.slug),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok((id, spec))
    }

    /// Submit a "fix this" request: piped from the iframe's error boundary.
    /// Includes the captured error and (optionally) a source excerpt so the
    /// harness has enough context to make a focused edit.
    pub async fn submit_fix_request(
        &self,
        slug: &str,
        error: &str,
    ) -> Result<(String, GeneratedSpec), String> {
        let source = self
            .read_source(slug)
            .await
            .unwrap_or_default();
        let excerpt = truncate(&source, 4096);
        let spec = generate_fix(slug, error, &excerpt);
        let id = self
            .inner
            .store
            .insert_inbox_entry(
                "page-fix-request",
                &spec.title,
                Some(&spec.body),
                None,
                &["page".into(), "fix".into()],
                Some("page-fix-request"),
                Some(&spec.slug),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok((id, spec))
    }

    /// Write (create or replace) the two files for a page. Used by the
    /// in-app chat tool `write_page`. Validates the slug, ensures the
    /// `meta.toml` parses, and writes both files atomically (write-rename).
    /// The fs watcher then refreshes the registry.
    pub async fn write_page(
        &self,
        slug: &str,
        page_tsx: &str,
        meta_toml: &str,
    ) -> Result<PageRecord, String> {
        validate_slug(slug)?;
        // Parse meta to fail fast on bad TOML rather than write a broken page.
        let _: meta::Meta = meta::parse(meta_toml).map_err(|e| format!("meta.toml: {e}"))?;
        let folder = self.inner.pages_dir.join(slug);
        std::fs::create_dir_all(&folder).map_err(|e| format!("create dir: {e}"))?;
        atomic_write(&folder.join(PAGE_FILE), page_tsx.as_bytes())
            .map_err(|e| format!("write {PAGE_FILE}: {e}"))?;
        atomic_write(&folder.join(META_FILE), meta_toml.as_bytes())
            .map_err(|e| format!("write {META_FILE}: {e}"))?;
        // Refresh immediately so the caller sees the new row without waiting
        // for the fs watcher debounce.
        let present = refresh(&self.inner.pages_dir, &self.inner.store).await;
        prune_missing(&self.inner.store, &present).await;
        let _ = self.inner.events.send(PagesEvent::Refreshed);
        self.inner
            .store
            .get_page(slug)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "page row missing after write".to_string())
    }

    pub async fn list_inbox_requests(&self) -> Vec<InboxEntryRecord> {
        let entries = self
            .inner
            .store
            .list_inbox_entries(false)
            .await
            .unwrap_or_default();
        entries
            .into_iter()
            .filter(|e| e.kind == "page-request" || e.kind == "page-fix-request")
            .collect()
    }
}

pub struct PagesService;

impl PagesService {
    /// Boot the service. `pages_dir` is `<workflow_parent_dir>/pages/`.
    pub async fn start(
        pages_dir: PathBuf,
        store: Arc<Store>,
    ) -> std::io::Result<PagesHandle> {
        std::fs::create_dir_all(&pages_dir)?;
        info!(path = %pages_dir.display(), "pages dir ready");

        // First-run UX: if the pages dir is empty, seed a `welcome` page so
        // users have something to look at and a working example to copy
        // from. We only seed when literally empty — never overwrite.
        if dir_is_empty(&pages_dir) {
            seed_welcome(&pages_dir);
        }

        let (events_tx, _) = broadcast::channel(64);
        let (rescan_tx, mut rescan_rx) = mpsc::unbounded_channel::<()>();

        let inner = Arc::new(Inner {
            store: store.clone(),
            pages_dir: pages_dir.clone(),
            rescan_tx: rescan_tx.clone(),
            events: events_tx.clone(),
        });

        // Initial scan.
        let initial_present = refresh(&pages_dir, &store).await;
        prune_missing(&store, &initial_present).await;

        // Filesystem watcher: any change in the pages tree → debounced rescan.
        // Recursive so we catch both `meta.toml` edits and folder
        // create/delete events.
        let watch_tx = rescan_tx.clone();
        let mut watcher = recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(ev) = res {
                if matches!(
                    ev.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    let _ = watch_tx.send(());
                }
            }
        })
        .map_err(io_err)?;
        watcher
            .watch(&pages_dir, RecursiveMode::Recursive)
            .map_err(io_err)?;
        Box::leak(Box::new(watcher));

        // Rescan task: debounce events, run refresh + prune.
        let store_for_rescan = store.clone();
        let dir_for_rescan = pages_dir.clone();
        let events_for_rescan = events_tx.clone();
        tokio::spawn(async move {
            loop {
                match rescan_rx.recv().await {
                    None => return,
                    Some(()) => {
                        let _ = tokio::time::timeout(Duration::from_millis(250), async {
                            while rescan_rx.recv().await.is_some() {}
                        })
                        .await;
                        let present = refresh(&dir_for_rescan, &store_for_rescan).await;
                        prune_missing(&store_for_rescan, &present).await;
                        let _ = events_for_rescan.send(PagesEvent::Refreshed);
                    }
                }
            }
        });

        Ok(PagesHandle { inner })
    }
}

fn io_err(e: notify::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}

/// Slugs are folder names — keep them URL-safe and free of path traversal.
/// Allow lowercase ASCII alphanum, dash, underscore. No leading/trailing dash.
fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() || slug.len() > 80 {
        return Err("slug must be 1-80 chars".into());
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err("slug cannot start or end with '-'".into());
    }
    for c in slug.chars() {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
        if !ok {
            return Err(format!("slug contains invalid char {c:?} (allowed: a-z 0-9 - _)"));
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir")
    })?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("write")
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

fn dir_is_empty(p: &Path) -> bool {
    match std::fs::read_dir(p) {
        Ok(rd) => rd.flatten().next().is_none(),
        Err(_) => true,
    }
}

const WELCOME_META: &str = r#"title = "Welcome"
icon = "table"
position = 0
meta_version = 1
"#;

const WELCOME_PAGE: &str = r#"import { useEffect, useState } from "react";
import { query } from "@symphony/page-runtime";

interface Row {
  table: string;
  rows: number;
}

export default function Welcome() {
  const [rows, setRows] = useState<Row[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const tables = await query(
          "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        );
        const out: Row[] = [];
        for (const [name] of tables.rows) {
          const t = String(name);
          // Quoting the table name keeps SQL injection paranoia at bay even
          // though the source is sqlite_master itself.
          const c = await query(`SELECT COUNT(*) FROM "${t.replace(/"/g, '""')}"`);
          const n = Number(c.rows[0]?.[0] ?? 0);
          out.push({ table: t, rows: n });
        }
        setRows(out);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  if (error) {
    return (
      <div style={{ padding: 20, color: "var(--textMute)" }}>
        Failed: {error}
      </div>
    );
  }
  if (!rows) {
    return <div style={{ padding: 20, color: "var(--textMute)" }}>loading…</div>;
  }

  return (
    <div style={{ padding: 24, color: "var(--text)" }}>
      <h1 style={{ marginTop: 0, fontSize: 22, letterSpacing: -0.3 }}>
        Welcome to Pages
      </h1>
      <p style={{ color: "var(--textDim)", lineHeight: 1.5, maxWidth: 640 }}>
        This page is rendered inside a sandboxed iframe and reads the local
        SQLite store via a read-only <code>query()</code> shim. Edit
        <code style={{ marginLeft: 4, marginRight: 4 }}>pages/welcome/page.tsx</code>
        and the page reloads.
      </p>
      <table
        style={{
          marginTop: 20,
          borderCollapse: "collapse",
          fontSize: 13,
          minWidth: 320,
        }}
      >
        <thead>
          <tr style={{ textAlign: "left", color: "var(--textDim)" }}>
            <th style={{ padding: "6px 10px", borderBottom: "1px solid var(--border)" }}>Table</th>
            <th style={{ padding: "6px 10px", borderBottom: "1px solid var(--border)", textAlign: "right" }}>Rows</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.table}>
              <td style={{ padding: "6px 10px", fontFamily: "ui-monospace, monospace" }}>
                {r.table}
              </td>
              <td style={{ padding: "6px 10px", textAlign: "right", fontFamily: "ui-monospace, monospace" }}>
                {r.rows.toLocaleString()}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
"#;

fn seed_welcome(pages_dir: &Path) {
    let folder = pages_dir.join("welcome");
    if folder.exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&folder) {
        warn!(error = %e, "could not create welcome page dir");
        return;
    }
    if let Err(e) = std::fs::write(folder.join(crate::registry::META_FILE), WELCOME_META) {
        warn!(error = %e, "could not write welcome meta.toml");
    }
    if let Err(e) = std::fs::write(folder.join(crate::registry::PAGE_FILE), WELCOME_PAGE) {
        warn!(error = %e, "could not write welcome page.tsx");
    }
    info!(path = %folder.display(), "seeded welcome page");
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…(truncated)", &s[..end])
}
