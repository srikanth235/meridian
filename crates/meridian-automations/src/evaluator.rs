//! Run a parsed [`Manifest`] in-process.
//!
//! The grammar is fixed: fetch `source` → for each item, render the action's
//! template fields against `{ item, lastRunAt, now }` → dispatch to the SDK
//! surface. Each action call appends a one-line audit entry to the run log.

use chrono::Utc;

use crate::manifest::{render, Action, GithubFilter, Manifest, Source};
use crate::sdk::{InboxCreate, IssueRow, RunCtx, SdkSurface, TabsOpen};

const LOG_TAIL_BYTES: usize = 8 * 1024;

pub struct EvalOutcome {
    pub success: bool,
    pub log: String,
    pub error: Option<String>,
}

pub async fn evaluate(
    manifest: &Manifest,
    ctx: &RunCtx,
    surface: &SdkSurface,
) -> EvalOutcome {
    let mut log = String::new();
    log.push_str(&format!(
        "[start] {} (dry_run={})\n",
        manifest.name, ctx.dry_run
    ));

    let items = match fetch_source(&manifest.source, ctx, surface).await {
        Ok(v) => v,
        Err(e) => {
            return EvalOutcome {
                success: false,
                log: trim_log(log + &format!("[error] source: {e}\n")),
                error: Some(format!("source: {e}")),
            };
        }
    };
    log.push_str(&format!("[source] fetched {} item(s)\n", items.len()));

    let last_run_at_str = ctx
        .last_run_at
        .map(|d| d.to_rfc3339())
        .unwrap_or_default();
    let now_str = Utc::now().to_rfc3339();

    let mut performed = 0usize;
    let mut deduped = 0usize;
    for (idx, item) in items.iter().enumerate() {
        let mut obj = liquid::Object::new();
        obj.insert("item".into(), liquid::model::Value::Object(item_object(item)));
        obj.insert(
            "lastRunAt".into(),
            liquid::model::Value::scalar(last_run_at_str.clone()),
        );
        obj.insert("now".into(), liquid::model::Value::scalar(now_str.clone()));

        match dispatch_action(&manifest.action, &obj, ctx, surface).await {
            Ok(true) => {
                performed += 1;
                log.push_str(&format!(
                    "[{}] {}\n",
                    action_kind(&manifest.action),
                    item.url
                ));
            }
            Ok(false) => {
                deduped += 1;
            }
            Err(e) => {
                log.push_str(&format!("[error] item {idx} ({}): {e}\n", item.url));
                return EvalOutcome {
                    success: false,
                    log: trim_log(log),
                    error: Some(format!("action item {idx}: {e}")),
                };
            }
        }
    }

    log.push_str(&format!(
        "[done] performed={performed} deduped={deduped}\n"
    ));
    EvalOutcome {
        success: true,
        log: trim_log(log),
        error: None,
    }
}

async fn fetch_source(
    source: &Source,
    ctx: &RunCtx,
    surface: &SdkSurface,
) -> Result<Vec<IssueRow>, String> {
    match source {
        Source::GithubIssues(f) => fetch_github(false, f, ctx, surface).await,
        Source::GithubPrs(f) => fetch_github(true, f, ctx, surface).await,
    }
}

async fn fetch_github(
    prs: bool,
    filter: &GithubFilter,
    ctx: &RunCtx,
    surface: &SdkSurface,
) -> Result<Vec<IssueRow>, String> {
    surface.github_search(prs, filter, ctx).await
}

async fn dispatch_action(
    action: &Action,
    obj: &liquid::Object,
    ctx: &RunCtx,
    surface: &SdkSurface,
) -> Result<bool, String> {
    match action {
        Action::InboxCreate {
            title,
            body,
            url,
            tags,
            dedup_key,
        } => {
            let entry = InboxCreate {
                title: render(title, obj)?,
                body: maybe_render(body.as_deref(), obj)?,
                url: maybe_render(url.as_deref(), obj)?,
                tags: tags.clone(),
                dedup_key: render(dedup_key, obj)?,
            };
            let id = surface.inbox_create(ctx, entry).await?;
            Ok(id.is_some() || ctx.dry_run)
        }
        Action::TabsOpen {
            url,
            title,
            dedup_key,
        } => {
            let tab = TabsOpen {
                url: render(url, obj)?,
                title: maybe_render(title.as_deref(), obj)?,
                dedup_key: render(dedup_key, obj)?,
            };
            let id = surface.tabs_open(ctx, tab).await?;
            Ok(id.is_some() || ctx.dry_run)
        }
    }
}

fn maybe_render(t: Option<&str>, obj: &liquid::Object) -> Result<Option<String>, String> {
    match t {
        Some(s) if !s.is_empty() => Ok(Some(render(s, obj)?)),
        _ => Ok(None),
    }
}

fn action_kind(a: &Action) -> &'static str {
    match a {
        Action::InboxCreate { .. } => "inbox.create",
        Action::TabsOpen { .. } => "tabs.open",
    }
}

fn item_object(it: &IssueRow) -> liquid::Object {
    let mut o = liquid::Object::new();
    o.insert("title".into(), liquid::model::Value::scalar(it.title.clone()));
    o.insert("url".into(), liquid::model::Value::scalar(it.url.clone()));
    o.insert("repo".into(), liquid::model::Value::scalar(it.repo.clone()));
    o.insert("number".into(), liquid::model::Value::scalar(it.number));
    o.insert(
        "author".into(),
        liquid::model::Value::scalar(it.author.clone()),
    );
    o.insert(
        "labels".into(),
        liquid::model::Value::Array(
            it.labels
                .iter()
                .map(|s| liquid::model::Value::scalar(s.clone()))
                .collect(),
        ),
    );
    o.insert(
        "updatedAt".into(),
        liquid::model::Value::scalar(it.updated_at.to_rfc3339()),
    );
    o
}

fn trim_log(log: String) -> String {
    if log.len() <= LOG_TAIL_BYTES {
        return log;
    }
    let drop_n = log.len() - LOG_TAIL_BYTES;
    format!("…(truncated {drop_n} bytes)…\n{}", &log[drop_n..])
}
