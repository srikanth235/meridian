//! Side-effect surface that the evaluator calls directly (in-process). All
//! mutating verbs route through here so dry-run, dedup, and audit live in one
//! place.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use meridian_store::Store;

use crate::manifest::GithubFilter;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRow {
    pub title: String,
    pub url: String,
    pub repo: String,
    pub number: i64,
    pub labels: Vec<String>,
    pub author: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct InboxCreate {
    pub title: String,
    pub url: Option<String>,
    pub body: Option<String>,
    pub tags: Vec<String>,
    pub dedup_key: String,
}

#[derive(Debug, Clone)]
pub struct TabsOpen {
    pub url: String,
    pub title: Option<String>,
    pub dedup_key: String,
}

/// Identity passed into each side effect. Evaluator builds this per run.
#[derive(Debug, Clone)]
pub struct RunCtx {
    pub automation_id: String,
    pub run_id: i64,
    pub dry_run: bool,
    pub last_run_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct SdkSurface {
    store: Arc<Store>,
}

impl SdkSurface {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// `github.issues` / `github.prs` source. Honors `updated_since_last_run`
    /// by passing `ctx.last_run_at` as the GitHub `updated:>=` query filter.
    pub async fn github_search(
        &self,
        prs: bool,
        filter: &GithubFilter,
        ctx: &RunCtx,
    ) -> Result<Vec<IssueRow>, String> {
        let updated_since = if filter.updated_since_last_run {
            ctx.last_run_at.map(|d| d.to_rfc3339())
        } else {
            None
        };
        gh_search(prs, filter, updated_since.as_deref()).await
    }

    /// `inbox.create` action. Returns `Some(id)` if a new entry was inserted,
    /// `None` if the dedup key was already seen.
    pub async fn inbox_create(
        &self,
        ctx: &RunCtx,
        entry: InboxCreate,
    ) -> Result<Option<String>, String> {
        if ctx.dry_run {
            return Ok(None);
        }
        if !self
            .store
            .check_and_mark_seen(&ctx.automation_id, &entry.dedup_key)
            .await
            .map_err(|e| e.to_string())?
        {
            return Ok(None);
        }
        let mut tags = entry.tags.clone();
        if !tags.iter().any(|t| t == "automation") {
            tags.push("automation".into());
        }
        let id = self
            .store
            .insert_inbox_entry(
                "automation-result",
                &entry.title,
                entry.body.as_deref(),
                entry.url.as_deref(),
                &tags,
                Some(&format!("automation:{}", ctx.automation_id)),
                Some(&entry.dedup_key),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Some(id))
    }

    /// `tabs.open` action. We don't have an in-app tab system yet — surface
    /// each tab as an inbox entry tagged `tab` so the user has one place to
    /// act on automation output.
    pub async fn tabs_open(
        &self,
        ctx: &RunCtx,
        tab: TabsOpen,
    ) -> Result<Option<String>, String> {
        if ctx.dry_run {
            return Ok(None);
        }
        if !self
            .store
            .check_and_mark_seen(&ctx.automation_id, &tab.dedup_key)
            .await
            .map_err(|e| e.to_string())?
        {
            return Ok(None);
        }
        let title = tab.title.clone().unwrap_or_else(|| tab.url.clone());
        let id = self
            .store
            .insert_inbox_entry(
                "automation-tab",
                &title,
                None,
                Some(&tab.url),
                &["automation".into(), "tab".into()],
                Some(&format!("automation:{}", ctx.automation_id)),
                Some(&tab.dedup_key),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Some(id))
    }
}

/// Invoke `gh search issues` / `gh search prs` against the user's
/// authenticated CLI.
async fn gh_search(
    prs: bool,
    filter: &GithubFilter,
    updated_since: Option<&str>,
) -> Result<Vec<IssueRow>, String> {
    let mut q_parts: Vec<String> = Vec::new();
    q_parts.push(if prs { "is:pr".into() } else { "is:issue".into() });
    match filter.state.as_deref() {
        Some("open") | None => q_parts.push("is:open".into()),
        Some("closed") => q_parts.push("is:closed".into()),
        Some("any") => {}
        Some(other) => return Err(format!("unsupported state: {other}")),
    }
    if let Some(a) = &filter.assignee {
        q_parts.push(format!("assignee:{a}"));
    }
    for l in &filter.labels {
        q_parts.push(format!("label:\"{}\"", l.replace('"', "")));
    }
    for r in &filter.repos {
        q_parts.push(format!("repo:{r}"));
    }
    if let Some(since) = updated_since {
        let date = since.split('T').next().unwrap_or(since);
        q_parts.push(format!("updated:>={date}"));
    }
    let q = q_parts.join(" ");

    let kind = if prs { "prs" } else { "issues" };
    let mut cmd = Command::new("gh");
    cmd.args([
        "search",
        kind,
        &q,
        "--limit",
        "100",
        "--json",
        "title,url,repository,number,labels,author,updatedAt",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let output = match timeout(Duration::from_secs(15), cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(format!("gh invocation failed: {e}")),
        Err(_) => return Err("gh search timed out after 15s".into()),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("gh search failed: {stderr}"));
    }
    let raw: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("gh JSON parse error: {e}"))?;
    let arr = raw.as_array().ok_or("gh JSON was not an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let repo = item
            .get("repository")
            .and_then(|r| r.get("nameWithOwner"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let number = item.get("number").and_then(|v| v.as_i64()).unwrap_or(0);
        let labels = item
            .get("labels")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let author = item
            .get("author")
            .and_then(|a| a.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let updated_at = item
            .get("updatedAt")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        out.push(IssueRow {
            title: title.into(),
            url: url.into(),
            repo: repo.into(),
            number,
            labels,
            author,
            updated_at,
        });
    }
    Ok(out)
}
