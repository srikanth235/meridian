//! Workspace lifecycle management (spec §9).
pub mod hooks;
pub mod manager;

pub use hooks::{run_hook, HookError, HookKind};
pub use manager::{WorkspaceError, WorkspaceManager};
