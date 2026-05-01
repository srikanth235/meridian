//! Issue tracker abstraction + GitHub adapter (spec §11).
pub mod error;
pub mod github;
pub mod tracker;

pub use error::TrackerError;
pub use github::GithubTracker;
pub use tracker::Tracker;
