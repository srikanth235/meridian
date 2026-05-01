use async_trait::async_trait;
use std::collections::HashMap;
use meridian_core::Issue;

use crate::error::TrackerError;

/// Generic tracker abstraction (spec §11.1) so non-Linear adapters can plug in.
///
/// `fetch_candidate_issues` from spec §11.1 is just `fetch_issues_by_states`
/// with the configured active states; the orchestrator owns that decision so
/// the trait does not duplicate it.
#[async_trait]
pub trait Tracker: Send + Sync {
    async fn fetch_issues_by_states(
        &self,
        state_names: &[String],
    ) -> Result<Vec<Issue>, TrackerError>;

    async fn fetch_issue_states_by_ids(
        &self,
        issue_ids: &[String],
    ) -> Result<HashMap<String, Issue>, TrackerError>;
}
