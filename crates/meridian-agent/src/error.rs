use thiserror::Error;

/// Normalized agent errors (spec §10.6).
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("codex command not found: {0}")]
    CodexNotFound(String),
    #[error("invalid workspace cwd: {0}")]
    InvalidWorkspaceCwd(String),
    #[error("response timeout waiting for {request}")]
    ResponseTimeout { request: String },
    #[error("turn timeout after {ms}ms")]
    TurnTimeout { ms: u64 },
    #[error("codex subprocess exited unexpectedly: {0}")]
    PortExit(String),
    #[error("response error: {0}")]
    ResponseError(String),
    #[error("turn failed: {0}")]
    TurnFailed(String),
    #[error("turn cancelled")]
    TurnCancelled,
    #[error("turn requires user input")]
    TurnInputRequired,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("startup failed: {0}")]
    StartupFailed(String),
}
