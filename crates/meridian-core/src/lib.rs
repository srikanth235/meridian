//! Meridian core domain types (spec §4).
//!
//! This crate has no IO and minimal dependencies — it is the shared vocabulary
//! used by every other crate in the workspace.

pub mod identifier;
pub mod issue;
pub mod orchestrator_state;
pub mod retry;
pub mod session;
pub mod workspace;

pub use identifier::{sanitize_workspace_key, session_id};
pub use issue::{Blocker, Issue, IssueState};
pub use orchestrator_state::{
    CodexRateLimits, CodexTotals, OrchestratorRuntimeState, RunningEntry,
};
pub use retry::RetryEntry;
pub use session::LiveSession;
pub use workspace::Workspace;
