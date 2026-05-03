use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::loader::{load_workflow, WorkflowDefinition};

/// Live workflow handle (spec §6.2). Holders read the latest known-good
/// definition; subscribers can wait on the watch channel for change events.
#[derive(Clone)]
pub struct ReloadHandle {
    inner: Arc<RwLock<WorkflowDefinition>>,
    notifier: watch::Sender<u64>,
    pub source_path: PathBuf,
}

impl ReloadHandle {
    pub fn current(&self) -> WorkflowDefinition {
        self.inner.read().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notifier.subscribe()
    }

    fn replace(&self, def: WorkflowDefinition, version: u64) {
        *self.inner.write() = def;
        let _ = self.notifier.send(version);
    }
}

pub struct WorkflowWatcher {
    pub handle: ReloadHandle,
    _watcher: Box<dyn Watcher + Send>,
}

impl WorkflowWatcher {
    /// Start watching `path` for changes. The initial definition must already
    /// be loaded; subsequent invalid reloads are kept at the last good value.
    pub async fn start(path: &Path, initial: WorkflowDefinition) -> std::io::Result<Self> {
        let (notifier, _rx) = watch::channel::<u64>(0);
        let handle = ReloadHandle {
            inner: Arc::new(RwLock::new(initial)),
            notifier,
            source_path: path.to_path_buf(),
        };

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let mut watcher = recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(ev) = res {
                if matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = tx.send(());
                }
            }
        })
        .map_err(io_err)?;

        // Watch the parent so renames over the file (editors often do this) still trigger.
        // `Path::new("WORKFLOW.md").parent()` returns `Some("")`, not `None`, so we
        // also have to filter out the empty path before falling back to `.`.
        let watch_target = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        watcher
            .watch(watch_target, RecursiveMode::NonRecursive)
            .map_err(io_err)?;

        let handle_for_task = handle.clone();
        let path_for_task = path.to_path_buf();
        tokio::spawn(async move {
            let mut version: u64 = 1;
            // Coalesce bursts of fs events.
            let debounce = Duration::from_millis(150);
            while rx.recv().await.is_some() {
                tokio::time::sleep(debounce).await;
                while rx.try_recv().is_ok() {}
                match load_workflow(&path_for_task).await {
                    Ok(def) => {
                        info!(path = %path_for_task.display(), "workflow reloaded");
                        handle_for_task.replace(def, version);
                        version = version.wrapping_add(1);
                    }
                    Err(e) => {
                        warn!(error = %e, "workflow reload failed; keeping last known good");
                    }
                }
            }
            error!("workflow watcher channel closed");
        });

        Ok(Self {
            handle,
            _watcher: Box::new(watcher),
        })
    }
}

fn io_err(e: notify::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}
