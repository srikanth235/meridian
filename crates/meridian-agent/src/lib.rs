//! Codex app-server client (spec §10).
pub mod client;
pub mod error;
pub mod event;
pub mod protocol;

pub use client::{run_session, RunOutcome, SessionRequest};
pub use error::AgentError;
pub use event::AgentEvent;
