use thiserror::Error;

/// Workflow / config errors (spec §5.5 + §6.3).
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing workflow file: {path}")]
    MissingWorkflowFile { path: String },

    #[error("workflow parse error: {0}")]
    WorkflowParseError(String),

    #[error("workflow front matter is not a map")]
    WorkflowFrontMatterNotAMap,

    #[error("template parse error: {0}")]
    TemplateParseError(String),

    #[error("template render error: {0}")]
    TemplateRenderError(String),

    #[error("dispatch preflight failed: {0}")]
    PreflightFailed(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
