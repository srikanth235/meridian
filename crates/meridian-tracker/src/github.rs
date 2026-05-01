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
/// Multi-repo aware: `Issue.id` is `<owner>/<name>/<number>` so it stays
/// globally unique. `Issue.repo` carries `owner/name` for UI grouping.
#[derive(Clone)]
pub struct GithubTracker {
    repos: Vec<String>,
    active_states_lc: Vec<String>,
    terminal_states_lc: Vec<String>,
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

    /// Split a global Issue.id (`owner/name/number`) back into (`owner/name`, `number`).
    fn split_id(id: &str) -> Option<(String, String)> {
        let last = id.rfind('/')?;
        let repo = &id[..last];
        let number = &id[last + 1..];
        if !repo.contains('/') || number.is_empty() {
            return None;
        }
        Some((repo.to_string(), number.to_string()))
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

        // Fan out across repos. Each repo gets up to two `gh issue list` calls
        // (open/closed). We do them sequentially per repo but parallelize across
        // repos.
        let mut tasks = Vec::new();
        for repo in &self.repos {
            let repo = repo.clone();
            tasks.push(tokio::spawn(async move {
                let mut nodes: Vec<Value> = Vec::new();
                if want_open {
                    nodes.extend(Self::list(&repo, "open", OPEN_LIMIT).await?);
                }
                if want_closed {
                    nodes.extend(Self::list(&repo, "closed", CLOSED_LIMIT).await?);
                }
                Ok::<_, TrackerError>((repo, nodes))
            }));
        }

        let mut out = Vec::new();
        for task in tasks {
            let (repo, nodes) = task
                .await
                .map_err(|e| TrackerError::GhSpawn(format!("join error: {e}")))??;
            for node in &nodes {
                let computed_state = self.classify_state(node, &wanted_lc);
                if let Some(issue) = normalize_issue(node, &repo, &computed_state) {
                    out.push(issue);
                } else {
                    warn!(node = %node, "skipping unparseable github issue");
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
            let Some((repo, number)) = Self::split_id(id) else {
                warn!(%id, "skipping unparseable issue id (expected owner/name/number)");
                continue;
            };
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
            Some(("owner/repo".to_string(), "42".to_string()))
        );
        assert_eq!(GithubTracker::split_id("just-number"), None);
        assert_eq!(GithubTracker::split_id("repo/42"), None); // owner/name needs slash
    }

    #[test]
    fn branch_for_handles_unicode() {
        assert_eq!(branch_for(7, "Hello 世界 World"), "issue-7-hello-world");
        assert_eq!(branch_for(8, "!!!"), "issue-8");
    }
}
