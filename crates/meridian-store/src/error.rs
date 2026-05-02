use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite open failed: {0}")]
    Open(String),
    #[error("sqlite query failed: {0}")]
    Query(String),
    #[error("sqlite migration failed: version {version}: {source}")]
    Migration {
        version: u32,
        #[source]
        source: rusqlite::Error,
    },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid value: {0}")]
    Invalid(String),
    #[error("blocking task join error: {0}")]
    Join(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Query(e.to_string())
    }
}
