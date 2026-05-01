use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use meridian_config::TrackerConfig;
use meridian_core::{Blocker, Issue};
use tracing::warn;

use crate::error::TrackerError;
use crate::tracker::Tracker;

const PAGE_SIZE: i64 = 50;
const NETWORK_TIMEOUT_SECS: u64 = 30;

const CANDIDATE_QUERY: &str = r#"
query Candidates($projectSlug: String!, $states: [String!]!, $first: Int!, $after: String) {
  issues(
    first: $first,
    after: $after,
    filter: {
      project: { slugId: { eq: $projectSlug } }
      state: { name: { in: $states } }
    }
  ) {
    pageInfo { hasNextPage endCursor }
    nodes {
      id
      identifier
      title
      description
      priority
      branchName
      url
      createdAt
      updatedAt
      state { name }
      labels { nodes { name } }
      inverseRelations {
        nodes {
          type
          issue { id identifier state { name } }
        }
      }
    }
  }
}
"#;

const STATES_QUERY: &str = r#"
query ByStates($projectSlug: String!, $states: [String!]!, $first: Int!, $after: String) {
  issues(
    first: $first,
    after: $after,
    filter: {
      project: { slugId: { eq: $projectSlug } }
      state: { name: { in: $states } }
    }
  ) {
    pageInfo { hasNextPage endCursor }
    nodes { id identifier title state { name } }
  }
}
"#;

const STATES_BY_IDS_QUERY: &str = r#"
query ByIds($ids: [ID!]!) {
  issues(filter: { id: { in: $ids } }) {
    nodes {
      id
      identifier
      title
      state { name }
      priority
      branchName
      url
      description
      createdAt
      updatedAt
      labels { nodes { name } }
      inverseRelations {
        nodes {
          type
          issue { id identifier state { name } }
        }
      }
    }
  }
}
"#;

#[derive(Clone)]
pub struct LinearTracker {
    endpoint: String,
    api_key: String,
    project_slug: String,
    client: reqwest::Client,
}

impl LinearTracker {
    pub fn from_config(cfg: &TrackerConfig) -> Result<Self, TrackerError> {
        if cfg.kind != "linear" {
            return Err(TrackerError::UnsupportedTrackerKind(cfg.kind.clone()));
        }
        let api_key = cfg.api_key.clone().ok_or(TrackerError::MissingApiKey)?;
        if api_key.is_empty() {
            return Err(TrackerError::MissingApiKey);
        }
        let project_slug = cfg
            .project_slug
            .clone()
            .ok_or(TrackerError::MissingProjectSlug)?;
        if project_slug.is_empty() {
            return Err(TrackerError::MissingProjectSlug);
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(NETWORK_TIMEOUT_SECS))
            .build()
            .map_err(|e| TrackerError::LinearApiRequest(e.to_string()))?;
        Ok(Self {
            endpoint: cfg.endpoint.clone(),
            api_key,
            project_slug,
            client,
        })
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, TrackerError> {
        let body = json!({"query": query, "variables": variables});
        let resp = self
            .client
            .post(&self.endpoint)
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TrackerError::LinearApiRequest(e.to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| TrackerError::LinearApiRequest(e.to_string()))?;
        if !status.is_success() {
            return Err(TrackerError::LinearApiStatus {
                status: status.as_u16(),
                body: text,
            });
        }
        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| TrackerError::LinearApiRequest(format!("invalid json: {e}")))?;
        if let Some(errs) = parsed.get("errors") {
            return Err(TrackerError::LinearGraphqlErrors(errs.to_string()));
        }
        let Some(data) = parsed.get("data").cloned() else {
            return Err(TrackerError::LinearUnknownPayload);
        };
        Ok(data)
    }

    async fn fetch_paged_issues(
        &self,
        query: &str,
        states: &[String],
    ) -> Result<Vec<Issue>, TrackerError> {
        let mut all = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let vars = json!({
                "projectSlug": self.project_slug,
                "states": states,
                "first": PAGE_SIZE,
                "after": after,
            });
            let data = self.graphql(query, vars).await?;
            let issues_obj = data.get("issues").cloned().unwrap_or(Value::Null);
            let nodes = issues_obj
                .get("nodes")
                .and_then(|n| n.as_array())
                .cloned()
                .unwrap_or_default();
            for node in nodes {
                if let Some(issue) = normalize_issue(&node) {
                    all.push(issue);
                }
            }
            let page_info = issues_obj.get("pageInfo");
            let has_next = page_info
                .and_then(|p| p.get("hasNextPage"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !has_next {
                break;
            }
            after = match page_info.and_then(|p| p.get("endCursor")).and_then(|v| v.as_str()) {
                Some(c) => Some(c.to_string()),
                None => return Err(TrackerError::LinearMissingEndCursor),
            };
        }
        Ok(all)
    }
}

#[async_trait]
impl Tracker for LinearTracker {
    async fn fetch_issues_by_states(
        &self,
        state_names: &[String],
    ) -> Result<Vec<Issue>, TrackerError> {
        // Use the rich CANDIDATE_QUERY when caller is asking for the active set
        // so that blockers/labels are populated; fall back to a leaner shape
        // otherwise.
        let q = if state_names.iter().any(|s| s.eq_ignore_ascii_case("Todo"))
            || state_names.iter().any(|s| s.eq_ignore_ascii_case("In Progress"))
        {
            CANDIDATE_QUERY
        } else {
            STATES_QUERY
        };
        self.fetch_paged_issues(q, state_names).await
    }

    async fn fetch_issue_states_by_ids(
        &self,
        issue_ids: &[String],
    ) -> Result<HashMap<String, Issue>, TrackerError> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let vars = json!({"ids": issue_ids});
        let data = self.graphql(STATES_BY_IDS_QUERY, vars).await?;
        let nodes = data
            .get("issues")
            .and_then(|i| i.get("nodes"))
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = HashMap::new();
        for node in nodes {
            if let Some(issue) = normalize_issue(&node) {
                out.insert(issue.id.clone(), issue);
            } else {
                warn!(node = %node, "skipping unparseable issue");
            }
        }
        Ok(out)
    }
}

fn normalize_issue(node: &Value) -> Option<Issue> {
    let id = node.get("id")?.as_str()?.to_string();
    let identifier = node.get("identifier")?.as_str()?.to_string();
    let title = node.get("title")?.as_str()?.to_string();
    let state = node
        .get("state")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())?
        .to_string();
    let description = node
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let priority = node.get("priority").and_then(|v| v.as_i64()).map(|n| n as i32);
    let branch_name = node
        .get("branchName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let url = node
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let labels = node
        .get("labels")
        .and_then(|l| l.get("nodes"))
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    let blocked_by = node
        .get("inverseRelations")
        .and_then(|r| r.get("nodes"))
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|rel| {
                    let rel_type = rel.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if rel_type != "blocks" {
                        return None;
                    }
                    let issue = rel.get("issue")?;
                    Some(Blocker {
                        id: issue.get("id").and_then(|v| v.as_str()).map(|s| s.into()),
                        identifier: issue
                            .get("identifier")
                            .and_then(|v| v.as_str())
                            .map(|s| s.into()),
                        state: issue
                            .get("state")
                            .and_then(|s| s.get("name"))
                            .and_then(|n| n.as_str())
                            .map(|s| s.into()),
                    })
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

    Some(Issue {
        id,
        identifier,
        title,
        description,
        priority,
        state,
        branch_name,
        url,
        labels,
        blocked_by,
        created_at,
        updated_at,
    })
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}
