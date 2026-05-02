//! Discover GitHub repos available to the user via the `gh` CLI.
//!
//! Single GraphQL pass: `viewer.repositories(ownerAffiliations: [OWNER,
//! COLLABORATOR, ORGANIZATION_MEMBER])`. Fast and well-bounded — covers
//! the user's own repos, repos they've been added to as a collaborator,
//! and org repos they have direct team access to.
//!
//! GitHub's `ORGANIZATION_MEMBER` filter does *not* include org repos the
//! viewer only has implicit admin access to (e.g. one-person orgs with no
//! teams). Those need to be added one-by-one via [`fetch_one`] from the
//! manual-add UI path — keeping the auto-discovered list small and avoiding
//! a multi-org sweep that could enumerate hundreds of repos by default.
//!
//! The `connected` flag (whether the orchestrator should dispatch against
//! this repo) is user state and lives only in sqlite — `gh` results never
//! touch it.
//!
//! `gh` not installed or not authenticated → empty probe + a status flag the
//! UI can use to nudge the user toward `gh auth login`.

use chrono::{DateTime, Utc};
use meridian_store::RepoRecord;
use serde::Deserialize;
use std::collections::HashSet;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::warn;

use crate::harnesses::augmented_path_env;

const GH_TIMEOUT_PER_PAGE: Duration = Duration::from_secs(15);
const GH_MAX_PAGES: u32 = 30; // hard ceiling per query: 3000 repos

const VIEWER_REPOS_QUERY: &str = r#"
query($cursor: String) {
  viewer {
    repositories(
      first: 100,
      after: $cursor,
      ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER],
      orderBy: { field: UPDATED_AT, direction: DESC }
    ) {
      pageInfo { hasNextPage endCursor }
      nodes {
        nameWithOwner
        description
        url
        defaultBranchRef { name }
        primaryLanguage { name }
        isPrivate
        isArchived
        updatedAt
      }
    }
  }
}
"#;

#[derive(Debug, Deserialize)]
struct GqlResponse<T> {
    #[serde(default = "Option::default")]
    data: Option<T>,
    #[serde(default)]
    errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ViewerReposData {
    viewer: ViewerRepos,
}
#[derive(Debug, Deserialize)]
struct ViewerRepos {
    repositories: GqlRepoConn,
}

#[derive(Debug, Deserialize)]
struct GqlRepoConn {
    #[serde(rename = "pageInfo")]
    page_info: GqlPageInfo,
    nodes: Vec<GhRepo>,
}

#[derive(Debug, Deserialize)]
struct GqlPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

/// One repo node from the GraphQL response. Optional fields are tolerant of
/// older `gh` versions / scope-restricted accounts.
#[derive(Debug, Deserialize)]
struct GhRepo {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default, rename = "defaultBranchRef")]
    default_branch_ref: Option<GhBranchRef>,
    #[serde(default, rename = "primaryLanguage")]
    primary_language: Option<GhLanguage>,
    #[serde(default, rename = "isPrivate")]
    is_private: bool,
    #[serde(default, rename = "isArchived")]
    is_archived: bool,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct GhBranchRef {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GhLanguage {
    name: String,
}

/// Outcome of a single discovery sweep.
#[derive(Debug, Clone)]
pub struct DiscoveryReport {
    /// True when `gh` was invokable and returned a successful response.
    pub gh_available: bool,
    /// True when we successfully parsed a list (even an empty one) — i.e.
    /// gh is installed AND authenticated. False if `gh` returned an auth
    /// error or wasn't found at all.
    pub gh_authenticated: bool,
    /// Last error string for surfacing in the UI; None on success.
    pub error: Option<String>,
    /// Repos accessible to the viewer (own + collaborator + direct org
    /// team grants). Org repos the viewer only has implicit admin access
    /// to are not included here — use [`fetch_one`] to add them by slug.
    pub repos: Vec<RepoRecord>,
}

impl DiscoveryReport {
    fn missing(error: impl Into<String>) -> Self {
        Self {
            gh_available: false,
            gh_authenticated: false,
            error: Some(error.into()),
            repos: Vec::new(),
        }
    }
}

/// Shell out to `gh api graphql`, paginating `viewer.repositories` until
/// done. Side-effect-free; callers handle persistence and cache updates.
pub async fn discover() -> DiscoveryReport {
    let mut nodes: Vec<GhRepo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor: Option<String> = None;

    for _ in 0..GH_MAX_PAGES {
        match fetch_viewer_repos_page(cursor.as_deref()).await {
            Ok(conn) => {
                merge_nodes(&mut nodes, &mut seen, conn.nodes);
                if !conn.page_info.has_next_page {
                    break;
                }
                cursor = conn.page_info.end_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            Err(PageError::NotInvokable(msg)) => return DiscoveryReport::missing(msg),
            Err(PageError::AuthFailed(msg)) => {
                warn!(error = %msg, "gh graphql auth failed");
                return DiscoveryReport {
                    gh_available: true,
                    gh_authenticated: false,
                    error: Some(msg),
                    repos: Vec::new(),
                };
            }
            Err(PageError::Other(msg)) => {
                warn!(error = %msg, "gh graphql page failed; returning what we have");
                return DiscoveryReport {
                    gh_available: true,
                    gh_authenticated: true,
                    error: Some(msg),
                    repos: nodes_to_records(nodes),
                };
            }
        }
    }

    DiscoveryReport {
        gh_available: true,
        gh_authenticated: true,
        error: None,
        repos: nodes_to_records(nodes),
    }
}

#[derive(Debug)]
enum PageError {
    NotInvokable(String),
    AuthFailed(String),
    Other(String),
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageError::NotInvokable(s) | PageError::AuthFailed(s) | PageError::Other(s) => {
                f.write_str(s)
            }
        }
    }
}

fn merge_nodes(out: &mut Vec<GhRepo>, seen: &mut HashSet<String>, more: Vec<GhRepo>) {
    for n in more {
        // Slugs from GitHub are case-preserving but case-insensitive at lookup.
        // Dedup case-insensitively so the same repo doesn't appear twice if the
        // two passes return slightly different casings.
        let key = n.name_with_owner.to_lowercase();
        if seen.insert(key) {
            out.push(n);
        }
    }
}

async fn fetch_viewer_repos_page(cursor: Option<&str>) -> Result<GqlRepoConn, PageError> {
    let mut args: Vec<String> = vec![
        "api".into(),
        "graphql".into(),
        "-f".into(),
        format!("query={VIEWER_REPOS_QUERY}"),
    ];
    if let Some(c) = cursor {
        args.push("-f".into());
        args.push(format!("cursor={c}"));
    }
    let resp: GqlResponse<ViewerReposData> = run_gh_graphql(&args).await?;
    let data = require_data(resp)?;
    Ok(data.viewer.repositories)
}

/// Run `gh api graphql ...` and parse the response as `GqlResponse<T>`.
async fn run_gh_graphql<T: for<'de> Deserialize<'de>>(
    args: &[String],
) -> Result<GqlResponse<T>, PageError> {
    let mut cmd = Command::new("gh");
    cmd.args(args)
        .env("PATH", augmented_path_env())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = match timeout(GH_TIMEOUT_PER_PAGE, cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(PageError::NotInvokable(format!("gh not invokable: {e}"))),
        Err(_) => return Err(PageError::Other("gh api graphql timed out".into())),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let lower = stderr.to_lowercase();
        let err = format!(
            "gh exited with {}: {stderr}",
            output.status.code().unwrap_or(-1)
        );
        if lower.contains("not logged") || lower.contains("authentication") {
            return Err(PageError::AuthFailed(err));
        }
        return Err(PageError::Other(err));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| PageError::Other(format!("gh api graphql returned malformed json: {e}")))
}

fn require_data<T>(resp: GqlResponse<T>) -> Result<T, PageError> {
    if let Some(errs) = resp.errors {
        if !errs.is_empty() {
            let msg = errs
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(PageError::Other(format!("graphql errors: {msg}")));
        }
    }
    resp.data
        .ok_or_else(|| PageError::Other("graphql response missing data".into()))
}

fn nodes_to_records(nodes: Vec<GhRepo>) -> Vec<RepoRecord> {
    let now = Utc::now();
    nodes
        .into_iter()
        .map(|r| RepoRecord {
            slug: r.name_with_owner,
            description: r.description.filter(|s| !s.is_empty()),
            url: r.url,
            default_branch: r.default_branch_ref.map(|b| b.name),
            primary_language: r.primary_language.map(|l| l.name),
            is_private: r.is_private,
            is_archived: r.is_archived,
            updated_at: r.updated_at,
            connected: false,
            connected_at: None,
            last_synced_at: Some(now),
        })
        .collect()
}

/// Look up a single repo by `owner/name` via `gh repo view`. Used by the
/// manual-add API path to enrich a stub row when discovery hasn't found it.
/// Returns `None` if `gh` isn't reachable or the repo isn't visible.
pub async fn fetch_one(slug: &str) -> Option<RepoRecord> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "repo",
        "view",
        slug,
        "--json",
        "nameWithOwner,description,url,defaultBranchRef,primaryLanguage,isPrivate,isArchived,updatedAt",
    ])
    .env("PATH", augmented_path_env())
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let output = timeout(GH_TIMEOUT_PER_PAGE, cmd.output()).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let node: GhRepo = serde_json::from_slice(&output.stdout).ok()?;
    nodes_to_records(vec![node]).into_iter().next()
}
