use thiserror::Error;

#[derive(Debug, Error)]
pub enum TrackerError {
    #[error("unsupported tracker kind: {0}")]
    UnsupportedTrackerKind(String),
    #[error("tracker.repo is required (\"owner/name\")")]
    MissingRepo,
    #[error("tracker.repo is not in \"owner/name\" form: {0}")]
    InvalidRepo(String),
    #[error("failed to spawn `gh`: {0}")]
    GhSpawn(String),
    #[error("`gh` timed out")]
    GhTimeout,
    #[error("`gh` exited with code {code}: {stderr}")]
    GhExit { code: i32, stderr: String },
    #[error("`gh` returned bad output: {0}")]
    GhBadOutput(String),
    #[error("tracker.db_path is required for kind \"sqlite\"")]
    MissingDbPath,
    #[error(transparent)]
    Store(#[from] meridian_store::StoreError),
}
