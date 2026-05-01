//! Issue tracker abstraction + Linear adapter (spec §11).
pub mod error;
pub mod linear;
pub mod tracker;

pub use error::TrackerError;
pub use linear::LinearTracker;
pub use tracker::Tracker;
