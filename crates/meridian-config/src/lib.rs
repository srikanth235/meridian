//! Workflow loader, typed config view, prompt rendering, and live reload
//! (spec §5 + §6 + §12).

pub mod config;
pub mod error;
pub mod loader;
pub mod prompt;
pub mod reload;

// Re-export the prompt module so other crates can call helpers like
// `meridian_config::prompt::continuation_prompt(...)`.

pub use config::{
    AgentConfig, CodexConfig, HooksConfig, PollingConfig, ServerConfig, ServiceConfig,
    TrackerConfig, WorkerConfig, WorkspaceConfig,
};
pub use error::ConfigError;
pub use loader::{load_workflow, parse_workflow, WorkflowDefinition};
pub use prompt::render_prompt;
pub use reload::{ReloadHandle, WorkflowWatcher};
