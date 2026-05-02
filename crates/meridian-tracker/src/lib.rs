//! Issue tracker abstraction + adapters.
//!
//! - [`GithubTracker`] (legacy): polls `gh` CLI, label-driven states.
//! - [`SqliteTracker`]: Linear-shaped SQLite schema via [`meridian_store::Store`].
pub mod error;
pub mod github;
pub mod sqlite;
pub mod tracker;

pub use error::TrackerError;
pub use github::GithubTracker;
pub use sqlite::SqliteTracker;
pub use tracker::Tracker;
