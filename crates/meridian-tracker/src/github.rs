use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use meridian_config::{
    TrackerConfig, PR_APPROVED, PR_CLOSED, PR_MERGED, PR_PENDING_REVIEW, PR_REVIEWED,
};
use meridian_core::issue::kind as task_kind;
use meridian_core::Issue;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::error::TrackerError;
use crate::tracker::Tracker;

const LIST_FIELDS: &str =
    "id,number,title,body,state,labels,url,createdAt,updatedAt,assignees";
const VIEW_FIELDS: &str = LIST_FIELDS;
const PR_LIST_FIELDS: &str =
    "number,title,body,state,labels,url,createdAt,updatedAt,author,reviewDecision,reviews,headRefName,isDraft";
const PR_VIEW_FIELDS: &str = PR_LIST_FIELDS;
const OPEN_LIMIT: u32 = 200;
const CLOSED_LIMIT: u32 = 100;
const PR_LIMIT: u32 = 200;
const GH_TIMEOUT: Duration = Duration::from_secs(30);
const PR_ID_SEGMENT: &str = "pr";

/// Tracker backed by the `gh` CLI. State semantics:
///   - GitHub issue is `CLOSED`  → state = `"closed"`
///   - GitHub issue is `OPEN`    → state = first matching `status:*` label
///                                 from configured active/terminal states,
///                                 or `"open"` as a fallback.
///
/// Multi-repo aware: `Issue.id` is `<owner>/<name>/<number>` so it stays
/// globally unique. `Issue.repo` carries `owner/name` for UI grouping.
#[derive(Clone)]
pub struct GithubTracker {
    repos: Vec<String>,
    active_states_lc: Vec<String>,
    terminal_states_lc: Vec<String>,
    review_prs: bool,
}

impl GithubTracker {
    pub fn from_config(cfg: &TrackerConfig) -> Result<Self, TrackerError> {
        if cfg.kind != "github" {
            return Err(TrackerError::UnsupportedTrackerKind(cfg.kind.clone()));
        }
        if cfg.repos.is_empty() {
            return Err(TrackerError::MissingRepo);
        }
        for repo in &cfg.repos {
            if !repo.contains('/') {
                return Err(TrackerError::InvalidRepo(repo.clone()));
            }
        }
        Ok(Self {
            repos: cfg.repos.clone(),
            active_states_lc: cfg.active_states.iter().map(|s| s.to_lowercase()).collect(),
            terminal_states_lc: cfg
                .terminal_states
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            review_prs: cfg.review_prs,
        })
    }

    fn known_states_lc(&self) -> Vec<String> {
        let mut all = self.active_states_lc.clone();
        all.extend(self.terminal_states_lc.iter().cloned());
        all
    }

    async fn gh_json(args: &[&str]) -> Result<Value, TrackerError> {
        let mut cmd = Command::new("gh");
        cmd.args(args);
        debug!(?args, "running gh");
        let fut = cmd.output();
        let out = match tokio::time::timeout(GH_TIMEOUT, fut).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(TrackerError::GhSpawn(e.to_string())),
            Err(_) => return Err(TrackerError::GhTimeout),
        };
        if !out.status.success() {
            return Err(TrackerError::GhExit {
                code: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        let stdout = String::from_utf8(out.stdout)
            .map_err(|e| TrackerError::GhBadOutput(e.to_string()))?;
        serde_json::from_str(&stdout)
            .map_err(|e| TrackerError::GhBadOutput(format!("invalid json: {e}")))
    }

    async fn list(repo: &str, gh_state: &str, limit: u32) -> Result<Vec<Value>, TrackerError> {
        let limit_str = limit.to_string();
        let v = Self::gh_json(&[
            "issue", "list",
            "--repo", repo,
            "--state", gh_state,
            "--limit", &limit_str,
            "--json", LIST_FIELDS,
        ])
        .await?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }

    /// `gh pr list --state open` — used for the PR-review scenario.
    async fn list_open_prs(repo: &str, limit: u32) -> Result<Vec<Value>, TrackerError> {
        let limit_str = limit.to_string();
        let v = Self::gh_json(&[
            "pr", "list",
            "--repo", repo,
            "--state", "open",
            "--limit", &limit_str,
            "--json", PR_LIST_FIELDS,
        ])
        .await?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }

    /// Decide the `state` field for an issue node, given the configured states
    /// the caller cares about.
    fn classify_state(&self, node: &Value, wanted_lc: &[String]) -> String {
        let gh_state = node
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("OPEN")
            .to_uppercase();
        if gh_state == "CLOSED" {
            return "closed".to_string();
        }
        let label_names: Vec<String> = node
            .get("labels")
            .and_then(|l| l.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        v.get("name").and_then(|n| n.as_str()).map(|s| s.to_lowercase())
                    })
                    .collect()
            })
            .unwrap_or_default();
        for w in wanted_lc {
            if w == "open" || w == "closed" {
                continue;
            }
            if label_names.iter().any(|l| l == w) {
                return w.clone();
            }
        }
        for known in self.known_states_lc() {
            if known == "open" || known == "closed" {
                continue;
            }
            if label_names.iter().any(|l| l == &known) {
                return known;
            }
        }
        "open".to_string()
    }

    /// Split a global Issue.id back into its parts.
    ///
    /// Issue ids look like `owner/name/<number>`. PR-review ids look like
    /// `owner/name/pr/<number>`. Returns `(repo, number, is_pr)`.
    fn split_id(id: &str) -> Option<(String, String, bool)> {
        let last = id.rfind('/')?;
        let head = &id[..last];
        let tail = &id[last + 1..];
        if tail.is_empty() {
            return None;
        }
        if let Some(repo) = head.strip_suffix(&format!("/{PR_ID_SEGMENT}")) {
            if !repo.contains('/') {
                return None;
            }
            return Some((repo.to_string(), tail.to_string(), true));
        }
        if !head.contains('/') {
            return None;
        }
        Some((head.to_string(), tail.to_string(), false))
    }
}

#[async_trait]
impl Tracker for GithubTracker {
    async fn fetch_issues_by_states(
        &self,
        state_names: &[String],
    ) -> Result<Vec<Issue>, TrackerError> {
        let wanted_lc: Vec<String> = state_names.iter().map(|s| s.to_lowercase()).collect();
        // Issue side: keep the existing semantics (open/closed only). Anything
        // matching a `pr:*` token is handled in the PR branch below.
        let issue_wanted: Vec<String> = wanted_lc
            .iter()
            .filter(|s| !s.starts_with("pr:"))
            .cloned()
            .collect();
        let want_open = issue_wanted.iter().any(|s| s != "closed");
        let want_closed = issue_wanted.iter().any(|s| s == "closed");
        let want_prs = self.review_prs && wanted_lc.iter().any(|s| s.starts_with("pr:"));

        // Fan out across repos. Each repo gets up to two `gh issue list` calls
        // (open/closed). We do them sequentially per repo but parallelize across
        // repos.
        let mut tasks = Vec::new();
        for repo in &self.repos {
            let repo = repo.clone();
            tasks.push(tokio::spawn(async move {
                let mut issue_nodes: Vec<Value> = Vec::new();
                if want_open {
                    issue_nodes.extend(Self::list(&repo, "open", OPEN_LIMIT).await?);
                }
                if want_closed {
                    issue_nodes.extend(Self::list(&repo, "closed", CLOSED_LIMIT).await?);
                }
                let pr_nodes: Vec<Value> = if want_prs {
                    Self::list_open_prs(&repo, PR_LIMIT).await?
                } else {
                    Vec::new()
                };
                Ok::<_, TrackerError>((repo, issue_nodes, pr_nodes))
            }));
        }

        let mut out = Vec::new();
        for task in tasks {
            let (repo, issue_nodes, pr_nodes) = task
                .await
                .map_err(|e| TrackerError::GhSpawn(format!("join error: {e}")))??;
            for node in &issue_nodes {
                let computed_state = self.classify_state(node, &wanted_lc);
                if let Some(issue) = normalize_issue(node, &repo, &computed_state) {
                    out.push(issue);
                } else {
                    warn!(node = %node, "skipping unparseable github issue");
                }
            }
            for node in &pr_nodes {
                let pr_state = classify_pr_state(node);
                if let Some(issue) = normalize_pr(node, &repo, pr_state) {
                    out.push(issue);
                } else {
                    warn!(node = %node, "skipping unparseable github pr");
                }
            }
        }
        Ok(out)
    }

    async fn fetch_issue_states_by_ids(
        &self,
        issue_ids: &[String],
    ) -> Result<HashMap<String, Issue>, TrackerError> {
        let mut out = HashMap::new();
        if issue_ids.is_empty() {
            return Ok(out);
        }
        let known = self.known_states_lc();
        for id in issue_ids {
            let Some((repo, number, is_pr)) = Self::split_id(id) else {
                warn!(%id, "skipping unparseable id (expected owner/name/number or owner/name/pr/number)");
                continue;
            };
            if is_pr {
                let view = Self::gh_json(&[
                    "pr", "view", &number,
                    "--repo", &repo,
                    "--json", PR_VIEW_FIELDS,
                ])
                .await?;
                let pr_state = classify_pr_state(&view);
                if let Some(issue) = normalize_pr(&view, &repo, pr_state) {
                    out.insert(issue.id.clone(), issue);
                }
            } else {
                let view = Self::gh_json(&[
                    "issue", "view", &number,
                    "--repo", &repo,
                    "--json", VIEW_FIELDS,
                ])
                .await?;
                let computed = self.classify_state(&view, &known);
                if let Some(issue) = normalize_issue(&view, &repo, &computed) {
                    out.insert(issue.id.clone(), issue);
                }
            }
        }
        Ok(out)
    }
}

fn normalize_issue(node: &Value, repo: &str, computed_state: &str) -> Option<Issue> {
    let number = node.get("number").and_then(|v| v.as_i64())?;
    let id = format!("{repo}/{number}");
    let identifier = format!("#{number}");
    let title = node.get("title")?.as_str()?.to_string();
    let description = node
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let url = node
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let labels: Vec<String> = node
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_lowercase())
                })
                .collect()
        })
        .unwrap_or_default();
    let created_at = node
        .get("createdAt")
        .and_then(|v| v.as_str())
        .and_then(parse_ts);
    let updated_at = node
        .get("updatedAt")
        .and_then(|v| v.as_str())
        .and_then(parse_ts);
    let branch_name = Some(branch_for(number, &title));

    Some(Issue {
        id,
        identifier,
        title,
        description,
        priority: None,
        state: computed_state.to_string(),
        branch_name,
        url,
        labels,
        blocked_by: Vec::new(),
        created_at,
        updated_at,
        repo: Some(repo.to_string()),
        kind: meridian_core::issue::kind::ISSUE.to_string(),
        author: None,
    })
}

/// Classify an open/closed PR's review-state into one of the synthetic
/// `pr:*` state strings the orchestrator recognises.
///
/// Rules (first match wins):
/// 1. PR is merged                                        → `pr:merged`
/// 2. PR is closed (not merged)                           → `pr:closed`
/// 3. PR is a draft                                       → `pr:pending-review`
///    (drafts can't be reviewed yet but stay active so a later flip out of
///    draft re-fires)
/// 4. `reviewDecision == "APPROVED"`                      → `pr:approved`
/// 5. `reviewDecision == "CHANGES_REQUESTED"` OR any
///    review with state ∈ {APPROVED, CHANGES_REQUESTED,
///    COMMENTED} exists in `reviews[]`                    → `pr:reviewed`
///    (treats "any submitted review" as terminal so we don't loop on
///    open PRs that already have feedback)
/// 6. otherwise                                            → `pr:pending-review`
fn classify_pr_state(node: &Value) -> &'static str {
    let gh_state = node
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("OPEN")
        .to_uppercase();
    if gh_state == "MERGED" {
        return PR_MERGED;
    }
    if gh_state == "CLOSED" {
        return PR_CLOSED;
    }
    if node.get("isDraft").and_then(|v| v.as_bool()).unwrap_or(false) {
        return PR_PENDING_REVIEW;
    }
    let decision = node
        .get("reviewDecision")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_uppercase();
    if decision == "APPROVED" {
        return PR_APPROVED;
    }
    if decision == "CHANGES_REQUESTED" {
        return PR_REVIEWED;
    }
    let any_submitted = node
        .get("reviews")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().any(|r| {
                let st = r
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_uppercase();
                matches!(st.as_str(), "APPROVED" | "CHANGES_REQUESTED" | "COMMENTED")
            })
        })
        .unwrap_or(false);
    if any_submitted {
        return PR_REVIEWED;
    }
    PR_PENDING_REVIEW
}

/// Convert a `gh pr` JSON node into an [`Issue`] with `kind = "pr_review"`.
fn normalize_pr(node: &Value, repo: &str, state: &'static str) -> Option<Issue> {
    let number = node.get("number").and_then(|v| v.as_i64())?;
    let id = format!("{repo}/{PR_ID_SEGMENT}/{number}");
    let identifier = format!("PR #{number}");
    let title = node.get("title")?.as_str()?.to_string();
    let description = node
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let url = node
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let labels: Vec<String> = node
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_lowercase())
                })
                .collect()
        })
        .unwrap_or_default();
    let created_at = node
        .get("createdAt")
        .and_then(|v| v.as_str())
        .and_then(parse_ts);
    let updated_at = node
        .get("updatedAt")
        .and_then(|v| v.as_str())
        .and_then(parse_ts);
    let branch_name = node
        .get("headRefName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = node
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(Issue {
        id,
        identifier,
        title,
        description,
        priority: None,
        state: state.to_string(),
        branch_name,
        url,
        labels,
        blocked_by: Vec::new(),
        created_at,
        updated_at,
        repo: Some(repo.to_string()),
        kind: task_kind::PR_REVIEW.to_string(),
        author,
    })
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn branch_for(number: i64, title: &str) -> String {
    let slug: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let collapsed = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        format!("issue-{number}")
    } else {
        format!("issue-{number}-{collapsed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mk_tracker(active: &[&str], terminal: &[&str]) -> GithubTracker {
        GithubTracker {
            repos: vec!["owner/repo".into()],
            active_states_lc: active.iter().map(|s| s.to_lowercase()).collect(),
            terminal_states_lc: terminal.iter().map(|s| s.to_lowercase()).collect(),
            review_prs: false,
        }
    }

    #[test]
    fn classifies_closed() {
        let t = mk_tracker(&["status:todo"], &["closed"]);
        let node = json!({"state": "CLOSED", "labels": []});
        assert_eq!(t.classify_state(&node, &["closed".into()]), "closed");
    }

    #[test]
    fn classifies_label_match() {
        let t = mk_tracker(&["status:todo", "status:in-progress"], &["closed"]);
        let node = json!({"state": "OPEN", "labels": [{"name": "status:in-progress"}]});
        let wanted = vec!["status:todo".to_string(), "status:in-progress".to_string()];
        assert_eq!(t.classify_state(&node, &wanted), "status:in-progress");
    }

    #[test]
    fn falls_back_to_open_when_no_status_label() {
        let t = mk_tracker(&["status:todo"], &["closed"]);
        let node = json!({"state": "OPEN", "labels": [{"name": "bug"}]});
        assert_eq!(t.classify_state(&node, &["status:todo".into()]), "open");
    }

    #[test]
    fn falls_back_to_known_label_when_caller_didnt_ask() {
        let t = mk_tracker(&["status:todo", "status:in-progress"], &["closed"]);
        let node = json!({"state": "OPEN", "labels": [{"name": "status:in-progress"}]});
        let known = t.known_states_lc();
        assert_eq!(t.classify_state(&node, &known), "status:in-progress");
    }

    #[test]
    fn normalizes_id_and_repo() {
        let node = json!({
            "number": 42, "title": "Fix login bug",
            "body": "details here", "state": "OPEN",
            "labels": [{"name": "Status:Todo"}],
            "url": "https://github.com/owner/repo/issues/42",
            "createdAt": "2026-01-01T00:00:00Z",
        });
        let i = normalize_issue(&node, "owner/repo", "status:todo").unwrap();
        assert_eq!(i.id, "owner/repo/42");
        assert_eq!(i.identifier, "#42");
        assert_eq!(i.repo.as_deref(), Some("owner/repo"));
        assert_eq!(i.state, "status:todo");
    }

    #[test]
    fn split_id_round_trip() {
        assert_eq!(
            GithubTracker::split_id("owner/repo/42"),
            Some(("owner/repo".to_string(), "42".to_string(), false))
        );
        assert_eq!(
            GithubTracker::split_id("owner/repo/pr/42"),
            Some(("owner/repo".to_string(), "42".to_string(), true))
        );
        assert_eq!(GithubTracker::split_id("just-number"), None);
        assert_eq!(GithubTracker::split_id("repo/42"), None); // owner/name needs slash
        assert_eq!(GithubTracker::split_id("only/pr/7"), None); // repo missing owner segment
    }

    #[test]
    fn classify_pr_merged_and_closed() {
        let merged = json!({"state": "MERGED"});
        assert_eq!(classify_pr_state(&merged), PR_MERGED);
        let closed = json!({"state": "CLOSED"});
        assert_eq!(classify_pr_state(&closed), PR_CLOSED);
    }

    #[test]
    fn classify_pr_approved_and_pending() {
        let approved = json!({"state": "OPEN", "reviewDecision": "APPROVED"});
        assert_eq!(classify_pr_state(&approved), PR_APPROVED);
        let pending = json!({"state": "OPEN", "reviewDecision": "REVIEW_REQUIRED"});
        assert_eq!(classify_pr_state(&pending), PR_PENDING_REVIEW);
        let blank = json!({"state": "OPEN"});
        assert_eq!(classify_pr_state(&blank), PR_PENDING_REVIEW);
    }

    #[test]
    fn classify_pr_changes_requested_or_commented_is_reviewed() {
        let cr = json!({"state": "OPEN", "reviewDecision": "CHANGES_REQUESTED"});
        assert_eq!(classify_pr_state(&cr), PR_REVIEWED);
        let with_comment = json!({
            "state": "OPEN",
            "reviewDecision": "",
            "reviews": [{"state": "COMMENTED"}],
        });
        assert_eq!(classify_pr_state(&with_comment), PR_REVIEWED);
    }

    #[test]
    fn draft_pr_stays_pending() {
        let draft = json!({"state": "OPEN", "isDraft": true, "reviewDecision": "APPROVED"});
        assert_eq!(classify_pr_state(&draft), PR_PENDING_REVIEW);
    }

    #[test]
    fn normalize_pr_sets_kind_and_id() {
        let node = json!({
            "number": 7,
            "title": "Add feature X",
            "body": "Closes #1",
            "state": "OPEN",
            "labels": [{"name": "feature"}],
            "url": "https://github.com/owner/repo/pull/7",
            "createdAt": "2026-01-01T00:00:00Z",
            "headRefName": "feat-x",
            "author": {"login": "alice"},
            "reviewDecision": "REVIEW_REQUIRED",
        });
        let pr = normalize_pr(&node, "owner/repo", PR_PENDING_REVIEW).unwrap();
        assert_eq!(pr.id, "owner/repo/pr/7");
        assert_eq!(pr.identifier, "PR #7");
        assert_eq!(pr.kind, "pr_review");
        assert_eq!(pr.author.as_deref(), Some("alice"));
        assert_eq!(pr.branch_name.as_deref(), Some("feat-x"));
        assert_eq!(pr.state, PR_PENDING_REVIEW);
    }

    #[test]
    fn branch_for_handles_unicode() {
        assert_eq!(branch_for(7, "Hello 世界 World"), "issue-7-hello-world");
        assert_eq!(branch_for(8, "!!!"), "issue-8");
    }
}
