//! Automations subsystem.
//!
//! Watches `<workflow_dir>/automations/*.{ts,mjs,js}`, parses each file's
//! default-exported `defineAutomation(...)` via a Node runner, schedules
//! invocations on a sqlite-backed loop, and exposes per-run SDK side
//! effects (GitHub queries, inbox writes, tab opens) over an HTTP surface
//! that's gated by a per-run shared-secret token.
//!
//! The script is the rule: a file in `automations/` is the canonical form.
//! The store records execution history + dedup keys so reruns stay
//! idempotent.

pub mod assets;
pub mod executor;
pub mod nl;
pub mod registry;
pub mod runtime;
pub mod scheduler;
pub mod schedule;
pub mod sdk;
pub mod service;
pub mod tokens;

pub use runtime::{RuntimeInfo, RuntimeKind};
pub use service::{AutomationsHandle, AutomationsService};
pub use sdk::{SdkRequest, SdkResponse, SdkSurface};
