//! Declarative automation manifest.
//!
//! Each `automations/<slug>.toml` is a single `name + schedule + source +
//! action` document. The evaluator fetches `source`, then runs `action` for
//! each item with template substitution against the item's fields. No
//! conditionals, no loops, no scripting — that's the whole grammar.

use serde::{Deserialize, Serialize};

use crate::schedule::Schedule;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub schedule: Schedule,
    pub source: Source,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Source {
    #[serde(rename = "github.issues")]
    GithubIssues(#[serde(default)] GithubFilter),
    #[serde(rename = "github.prs")]
    GithubPrs(#[serde(default)] GithubFilter),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubFilter {
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    /// When true, the evaluator passes the previous successful run's start
    /// time as `updated:>=` so reruns stay incremental.
    #[serde(default)]
    pub updated_since_last_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Action {
    #[serde(rename = "inbox.create")]
    InboxCreate {
        title: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
        dedup_key: String,
    },
    #[serde(rename = "tabs.open")]
    TabsOpen {
        url: String,
        #[serde(default)]
        title: Option<String>,
        dedup_key: String,
    },
}

pub fn parse(src: &str) -> Result<Manifest, String> {
    toml::from_str::<Manifest>(src).map_err(|e| e.to_string())
}

/// Render a `{{ liquid }}` template string against a context object. Used for
/// the per-item substitution in action fields. We re-use `liquid` (already a
/// workspace dep) so the syntax matches the workflow prompt template.
pub fn render(template: &str, ctx: &liquid::Object) -> Result<String, String> {
    let parser = liquid::ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| format!("liquid parser: {e}"))?;
    let tpl = parser.parse(template).map_err(|e| format!("template parse: {e}"))?;
    tpl.render(ctx).map_err(|e| format!("template render: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prs_to_tabs() {
        let src = r#"
name = "New PRs from acme"
schedule = { every = "1h" }

[source]
kind = "github.prs"
repos = ["acme/api", "acme/web"]
state = "open"
updated_since_last_run = true

[action]
kind = "tabs.open"
url = "{{ item.url }}"
title = "{{ item.repo }}#{{ item.number }} {{ item.title }}"
dedup_key = "{{ item.url }}"
"#;
        let m = parse(src).expect("parse ok");
        assert_eq!(m.name, "New PRs from acme");
        match m.source {
            Source::GithubPrs(f) => {
                assert_eq!(f.repos, vec!["acme/api", "acme/web"]);
                assert!(f.updated_since_last_run);
            }
            _ => panic!("wrong source variant"),
        }
        match m.action {
            Action::TabsOpen { url, dedup_key, .. } => {
                assert!(url.contains("{{ item.url }}"));
                assert!(dedup_key.contains("{{ item.url }}"));
            }
            _ => panic!("wrong action variant"),
        }
    }

    #[test]
    fn parses_issues_to_inbox() {
        let src = r#"
name = "Morning issues"
schedule = { cron = "0 9 * * *" }

[source]
kind = "github.issues"
assignee = "@me"
state = "open"

[action]
kind = "inbox.create"
title = "{{ item.title }}"
url = "{{ item.url }}"
tags = ["triage"]
dedup_key = "{{ item.url }}"
"#;
        let m = parse(src).expect("parse ok");
        match m.source {
            Source::GithubIssues(f) => {
                assert_eq!(f.assignee.as_deref(), Some("@me"));
            }
            _ => panic!("wrong source"),
        }
        match m.action {
            Action::InboxCreate { tags, .. } => assert_eq!(tags, vec!["triage"]),
            _ => panic!("wrong action"),
        }
    }

    #[test]
    fn render_substitutes_item_fields() {
        let mut obj = liquid::Object::new();
        let mut item = liquid::Object::new();
        item.insert("url".into(), liquid::model::Value::scalar("https://x/1"));
        item.insert("number".into(), liquid::model::Value::scalar(42i64));
        obj.insert("item".into(), liquid::model::Value::Object(item));
        let out = render("{{ item.url }}#{{ item.number }}", &obj).unwrap();
        assert_eq!(out, "https://x/1#42");
    }

    #[test]
    fn rejects_unknown_action_kind() {
        let src = r#"
name = "x"
schedule = { every = "1h" }
[source]
kind = "github.prs"
[action]
kind = "slack.notify"
"#;
        assert!(parse(src).is_err());
    }
}
