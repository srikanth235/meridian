//! HTTP-shaped surface that the SDK shim calls into. The transport (axum
//! routes) lives in `meridian-server`; this module owns the data model and
//! the actual side-effect implementations so the runtime is testable without
//! a server.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::timeout;

use meridian_store::Store;

use crate::tokens::TokenContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueFilter {
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub updated_since: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxCreate {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub dedup_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabsOpen {
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
    pub dedup_key: String,
}

/// Payloads accepted on the SDK HTTP surface.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SdkRequest {
    GithubIssues { filter: IssueFilter },
    GithubPrs { filter: IssueFilter },
    InboxCreate { entry: InboxCreate },
    TabsOpen { tab: TabsOpen },
}

#[derive(Debug, Clone, Serialize)]
pub enum SdkResponse {
    #[serde(rename = "items")]
    Items(Vec<IssueRow>),
    #[serde(rename = "ok")]
    Ok(Value),
}

/// Side-effect plumbing for the SDK HTTP surface. Owns the store handle plus
/// optional callbacks (e.g. tabs.open uses `shell.openExternal` via Electron;
/// in headless mode it's a no-op that just records the would-open URL).
#[derive(Clone)]
pub struct SdkSurface {
    store: Arc<Store>,
    log_tx: Option<mpsc::UnboundedSender<RunLog>>,
}

#[derive(Debug, Clone)]
pub struct RunLog {
    pub run_id: i64,
    pub line: String,
}

impl SdkSurface {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store, log_tx: None }
    }

    pub fn with_log_sink(mut self, tx: mpsc::UnboundedSender<RunLog>) -> Self {
        self.log_tx = Some(tx);
        self
    }

    pub async fn handle(
        &self,
        ctx: &TokenContext,
        req: SdkRequest,
    ) -> Result<SdkResponse, String> {
        match req {
            SdkRequest::GithubIssues { filter } => {
                let items = gh_search(false, &filter).await?;
                Ok(SdkResponse::Items(items))
            }
            SdkRequest::GithubPrs { filter } => {
                let items = gh_search(true, &filter).await?;
                Ok(SdkResponse::Items(items))
            }
            SdkRequest::InboxCreate { entry } => {
                self.log(ctx.run_id, format!("inbox.create dedupKey={}", entry.dedup_key));
                if ctx.dry_run {
                    // Honor dedup state without consuming it — a dry-run
                    // should be idempotent and shouldn't block the next real
                    // run from acting on the same key.
                    return Ok(SdkResponse::Ok(serde_json::json!({"wouldCreate": entry})));
                }
                if !self
                    .store
                    .check_and_mark_seen(&ctx.automation_id, &entry.dedup_key)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Ok(SdkResponse::Ok(serde_json::json!({"deduped": true})));
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
                Ok(SdkResponse::Ok(serde_json::json!({"id": id})))
            }
            SdkRequest::TabsOpen { tab } => {
                self.log(ctx.run_id, format!("tabs.open dedupKey={}", tab.dedup_key));
                if ctx.dry_run {
                    return Ok(SdkResponse::Ok(serde_json::json!({"wouldOpen": tab})));
                }
                if !self
                    .store
                    .check_and_mark_seen(&ctx.automation_id, &tab.dedup_key)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Ok(SdkResponse::Ok(serde_json::json!({"deduped": true})));
                }
                // We don't have an in-app tab system yet — surface it as an
                // inbox entry tagged `tab` so the user has a single place to
                // act on automation output. The Electron shell can later
                // intercept this and open in a real tab.
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
                Ok(SdkResponse::Ok(serde_json::json!({"id": id})))
            }
        }
    }

    fn log(&self, run_id: i64, line: String) {
        if let Some(tx) = &self.log_tx {
            let _ = tx.send(RunLog { run_id, line });
        }
    }
}

/// Invoke `gh search issues` / `gh search prs` against the user's
/// authenticated CLI. We use the search endpoint rather than `gh issue list`
/// because it natively supports cross-repo + multi-label filtering with one
/// call.
async fn gh_search(prs: bool, filter: &IssueFilter) -> Result<Vec<IssueRow>, String> {
    let mut q_parts: Vec<String> = Vec::new();
    q_parts.push(if prs { "is:pr".into() } else { "is:issue".into() });
    match filter.state.as_deref() {
        Some("open") | None => q_parts.push("is:open".into()),
        Some("closed") => q_parts.push("is:closed".into()),
        Some("any") => {}
        Some(other) => return Err(format!("unsupported state: {other}")),
    }
    if let Some(a) = &filter.assignee {
        q_parts.push(format!("assignee:{}", if a == "@me" { "@me".into() } else { a.clone() }));
    }
    for l in &filter.labels {
        q_parts.push(format!("label:\"{}\"", l.replace('"', "")));
    }
    for r in &filter.repos {
        q_parts.push(format!("repo:{r}"));
    }
    if let Some(since) = &filter.updated_since {
        // GitHub search needs YYYY-MM-DD.
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
