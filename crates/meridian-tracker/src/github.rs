use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use meridian_config::TrackerConfig;
use meridian_core::Issue;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::error::TrackerError;
use crate::tracker::Tracker;

const LIST_FIELDS: &str =
    "id,number,title,body,state,labels,url,createdAt,updatedAt,assignees";
const VIEW_FIELDS: &str = LIST_FIELDS;
const OPEN_LIMIT: u32 = 200;
const CLOSED_LIMIT: u32 = 100;
const GH_TIMEOUT: Duration = Duration::from_secs(30);

/// Tracker backed by the `gh` CLI. State semantics:
///   - GitHub issue is `CLOSED`  → state = `"closed"`
///   - GitHub issue is `OPEN`    → state = first matching `status:*` label
///                                 from configured active/terminal states,
///                                 or `"open"` as a fallback.
///
/// `Issue.id` is the issue *number* (string), so reconciliation can call
/// `gh issue view <number>` directly.
#[derive(Clone)]
pub struct GithubTracker {
    repo: String,
    active_states_lc: Vec<String>,
    terminal_states_lc: Vec<String>,
}

impl GithubTracker {
    pub fn from_config(cfg: &TrackerConfig) -> Result<Self, TrackerError> {
        if cfg.kind != "github" {
            return Err(TrackerError::UnsupportedTrackerKind(cfg.kind.clone()));
        }
        let repo = cfg.repo.clone().ok_or(TrackerError::MissingRepo)?;
        if !repo.contains('/') {
            return Err(TrackerError::InvalidRepo(repo));
        }
        Ok(Self {
            repo,
            active_states_lc: cfg.active_states.iter().map(|s| s.to_lowercase()).collect(),
            terminal_states_lc: cfg
                .terminal_states
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
        })
    }

    fn known_states_lc(&self) -> Vec<String> {
        let mut all = self.active_states_lc.clone();
        all.extend(self.terminal_states_lc.iter().cloned());
        all
    }

    async fn gh_json(&self, args: &[&str]) -> Result<Value, TrackerError> {
        let mut cmd = Command::new("gh");
        cmd.args(args);
        debug!(?args, repo = %self.repo, "running gh");
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

    async fn list(&self, gh_state: &str, limit: u32) -> Result<Vec<Value>, TrackerError> {
        let limit_str = limit.to_string();
        let v = self
            .gh_json(&[
                "issue", "list",
                "--repo", &self.repo,
                "--state", gh_state,
                "--limit", &limit_str,
                "--json", LIST_FIELDS,
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
        // Prefer a label that matches what the caller asked for; if none of
        // the wanted labels matches, fall back to any known status label so
        // the kanban groups it into the right column anyway.
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
}

#[async_trait]
impl Tracker for GithubTracker {
    async fn fetch_issues_by_states(
        &self,
        state_names: &[String],
    ) -> Result<Vec<Issue>, TrackerError> {
        let wanted_lc: Vec<String> = state_names.iter().map(|s| s.to_lowercase()).collect();
        let want_open = wanted_lc.iter().any(|s| s != "closed");
        let want_closed = wanted_lc.iter().any(|s| s == "closed");

        let mut nodes: Vec<Value> = Vec::new();
        if want_open {
            nodes.extend(self.list("open", OPEN_LIMIT).await?);
        }
        if want_closed {
            nodes.extend(self.list("closed", CLOSED_LIMIT).await?);
        }

        // Return every fetched node, not just those whose computed state is in
        // the wanted list. The orchestrator's `build_board` puts unmatched
        // issues in an "unsorted" bucket so the UI can still display them, and
        // dispatch is gated separately by `active_states`.
        let mut out = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let computed_state = self.classify_state(node, &wanted_lc);
            if let Some(issue) = normalize_issue(node, &computed_state) {
                out.push(issue);
            } else {
                warn!(node = %node, "skipping unparseable github issue");
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
            let view = self
                .gh_json(&[
                    "issue", "view", id,
                    "--repo", &self.repo,
                    "--json", VIEW_FIELDS,
                ])
                .await?;
            let computed = self.classify_state(&view, &known);
            if let Some(issue) = normalize_issue(&view, &computed) {
                out.insert(issue.id.clone(), issue);
            }
        }
        Ok(out)
    }
}

fn normalize_issue(node: &Value, computed_state: &str) -> Option<Issue> {
    let number = node.get("number").and_then(|v| v.as_i64())?;
    let id = number.to_string();
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
            repo: "owner/repo".into(),
            active_states_lc: active.iter().map(|s| s.to_lowercase()).collect(),
            terminal_states_lc: terminal.iter().map(|s| s.to_lowercase()).collect(),
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
        // Useful for fetch_issue_states_by_ids: caller passes empty wanted list
        // but issue carries a known status label.
        let t = mk_tracker(&["status:todo", "status:in-progress"], &["closed"]);
        let node = json!({"state": "OPEN", "labels": [{"name": "status:in-progress"}]});
        let known = t.known_states_lc();
        assert_eq!(t.classify_state(&node, &known), "status:in-progress");
    }

    #[test]
    fn normalizes_basic_fields() {
        let node = json!({
            "number": 42, "title": "Fix login bug",
            "body": "details here", "state": "OPEN",
            "labels": [{"name": "Status:Todo"}],
            "url": "https://github.com/owner/repo/issues/42",
            "createdAt": "2026-01-01T00:00:00Z",
        });
        let i = normalize_issue(&node, "status:todo").unwrap();
        assert_eq!(i.id, "42");
        assert_eq!(i.identifier, "#42");
        assert_eq!(i.state, "status:todo");
        assert_eq!(i.labels, vec!["status:todo".to_string()]);
        assert_eq!(i.branch_name.as_deref(), Some("issue-42-fix-login-bug"));
    }

    #[test]
    fn branch_for_handles_unicode() {
        assert_eq!(branch_for(7, "Hello 世界 World"), "issue-7-hello-world");
        assert_eq!(branch_for(8, "!!!"), "issue-8");
    }
}
