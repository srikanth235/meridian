use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Normalized blocker reference (spec §4.1.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocker {
    pub id: Option<String>,
    pub identifier: Option<String>,
    pub state: Option<String>,
}

/// Convenience wrapper for the tracker-state classification used in
/// dispatch/reconciliation logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Active,
    Terminal,
    Other,
}

/// Task kind constants. The dispatched unit is uniformly an [`Issue`], but
/// `kind` switches the agent's behaviour: `"issue"` runs the implement-an-issue
/// flow, `"pr_review"` runs the review-a-pull-request flow (see WORKFLOW.md).
pub mod kind {
    pub const ISSUE: &str = "issue";
    pub const PR_REVIEW: &str = "pr_review";
}

fn default_kind() -> String {
    kind::ISSUE.to_string()
}

/// Normalized issue record (spec §4.1.1).
///
/// `id` is globally unique across repos when multi-repo is configured: the
/// GitHub adapter sets it to `<owner>/<name>/<number>` for issues and
/// `<owner>/<name>/pr/<number>` for PR-review tasks. `identifier` is the
/// short display form (e.g. `#42`, `PR #42`) and `repo` carries the source
/// (`owner/name`) so the UI can disambiguate when aggregating across projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub state: String,
    pub branch_name: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<Blocker>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    /// Source repository (`owner/name`) when the tracker has a notion of it.
    #[serde(default)]
    pub repo: Option<String>,
    /// Task kind — `"issue"` (default) or `"pr_review"`. Bound into the
    /// prompt template as `task.kind` to switch agent behaviour.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Author login when the upstream record carries one (currently only
    /// populated for PR-review tasks). Surfaced as `pr.author` in templates.
    #[serde(default)]
    pub author: Option<String>,
}

impl Issue {
    /// Compare states case-insensitively per spec §4.2.
    pub fn classify(&self, active: &[String], terminal: &[String]) -> IssueState {
        let s = self.state.to_lowercase();
        if terminal.iter().any(|t| t.to_lowercase() == s) {
            IssueState::Terminal
        } else if active.iter().any(|a| a.to_lowercase() == s) {
            IssueState::Active
        } else {
            IssueState::Other
        }
    }

    /// True when the issue is in `Todo` state and any blocker is non-terminal
    /// (spec §8.2).
    pub fn blocked_by_non_terminal(&self, terminal: &[String]) -> bool {
        if !self.state.eq_ignore_ascii_case("Todo") {
            return false;
        }
        let term_lc: Vec<String> = terminal.iter().map(|s| s.to_lowercase()).collect();
        self.blocked_by.iter().any(|b| match &b.state {
            Some(st) => !term_lc.iter().any(|t| t == &st.to_lowercase()),
            None => true,
        })
    }
}
