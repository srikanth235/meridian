//! Automations subsystem.
//!
//! Watches `<workflow_dir>/automations/*.toml`, parses each file as a
//! declarative `Manifest` (name + schedule + source + action), and runs
//! eligible automations on a sqlite-backed scheduler. Side effects (GitHub
//! queries, inbox writes, tab opens) are dispatched in-process via
//! `SdkSurface` — no JS runtime, no subprocess.
//!
//! The file is the rule: a `*.toml` file in `automations/` is the canonical
//! form. The store records execution history + dedup keys so reruns stay
//! idempotent.

pub mod evaluator;
pub mod executor;
pub mod manifest;
pub mod nl;
pub mod registry;
pub mod scheduler;
pub mod schedule;
pub mod sdk;
pub mod service;

pub use sdk::SdkSurface;
pub use service::{AutomationsHandle, AutomationsService};
