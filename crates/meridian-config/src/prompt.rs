use liquid::ParserBuilder;
use serde_json::{json, Value};
use meridian_core::issue::kind as task_kind;
use meridian_core::Issue;

use crate::error::ConfigError;

const FALLBACK_PROMPT: &str = "You are working on a GitHub issue.";

/// Render the workflow prompt template with strict checking (spec §12.2).
/// Empty templates fall back to a minimal default per spec §5.4.
///
/// Bound globals:
///   * `issue`    — the full [`Issue`] record (always).
///   * `task.kind` — `"issue"` or `"pr_review"`. Templates branch on this.
///   * `pr`       — synthesized PR view when `task.kind == "pr_review"`. Carries
///                  `identifier`, `title`, `body`, `repo`, `number`, `author`,
///                  `labels`, `url`. Absent for issue tasks.
///   * `attempt`  — retry/continuation attempt number (Option).
pub fn render_prompt(
    template: &str,
    issue: &Issue,
    attempt: Option<u32>,
) -> Result<String, ConfigError> {
    if template.trim().is_empty() {
        return Ok(FALLBACK_PROMPT.to_string());
    }
    let parser = ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| ConfigError::TemplateParseError(e.to_string()))?;
    let tpl = parser
        .parse(template)
        .map_err(|e| ConfigError::TemplateParseError(e.to_string()))?;

    let issue_json = serde_json::to_value(issue)
        .map_err(|e| ConfigError::TemplateRenderError(e.to_string()))?;
    let kind = if issue.kind.is_empty() {
        task_kind::ISSUE
    } else {
        issue.kind.as_str()
    };
    let pr_json = if kind == task_kind::PR_REVIEW {
        pr_view(issue)
    } else {
        Value::Null
    };
    let globals_json = json!({
        "issue": issue_json,
        "task": { "kind": kind },
        "pr": pr_json,
        "attempt": attempt,
    });
    let globals = json_to_liquid_object(&globals_json)
        .map_err(|e| ConfigError::TemplateRenderError(e))?;

    tpl.render(&globals)
        .map_err(|e| ConfigError::TemplateRenderError(e.to_string()))
}

/// Build the `pr.*` Liquid namespace from a PR-review [`Issue`].
///
/// Issue.id for PR-review tasks is `<owner>/<name>/pr/<number>`. The PR
/// number is the trailing segment.
fn pr_view(issue: &Issue) -> Value {
    let number = issue
        .id
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<i64>().ok());
    json!({
        "identifier": issue.identifier,
        "title": issue.title,
        "body": issue.description,
        "repo": issue.repo,
        "number": number,
        "author": issue.author,
        "labels": issue.labels,
        "url": issue.url,
        "branch": issue.branch_name,
    })
}

/// Helper used by the agent runner to build continuation guidance for turns
/// after the first one (spec §10.3 + §12.3).
pub fn continuation_prompt(issue: &Issue, attempt: u32) -> String {
    format!(
        "Continuing work on {} ({}). Pick up from the prior turn (attempt {}). \
        Re-check the tracker state if needed before making further edits.",
        issue.identifier, issue.title, attempt
    )
}

fn json_to_liquid_object(v: &Value) -> Result<liquid::Object, String> {
    let lv = json_to_liquid_value(v);
    match lv {
        liquid_core::Value::Object(o) => Ok(o),
        _ => Err("globals must be a JSON object".into()),
    }
}

fn json_to_liquid_value(v: &Value) -> liquid_core::Value {
    use liquid_core::model::{Scalar, Value as LV};
    match v {
        Value::Null => LV::Nil,
        Value::Bool(b) => LV::scalar(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                LV::scalar(i)
            } else if let Some(f) = n.as_f64() {
                LV::scalar(f)
            } else {
                LV::scalar(Scalar::new(n.to_string()))
            }
        }
        Value::String(s) => LV::scalar(s.clone()),
        Value::Array(arr) => {
            LV::Array(arr.iter().map(json_to_liquid_value).collect())
        }
        Value::Object(obj) => {
            let mut out = liquid::Object::new();
            for (k, val) in obj {
                out.insert(
                    liquid_core::model::KString::from_string(k.clone()).into(),
                    json_to_liquid_value(val),
                );
            }
            LV::Object(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use meridian_core::Issue;

    fn sample_issue() -> Issue {
        Issue {
            id: "id1".into(),
            identifier: "ABC-1".into(),
            title: "Hello".into(),
            description: None,
            priority: Some(2),
            state: "Todo".into(),
            branch_name: None,
            url: None,
            labels: vec!["bug".into()],
            blocked_by: vec![],
            created_at: Some(Utc::now()),
            updated_at: None,
            repo: None,
            kind: "issue".into(),
            author: None,
        }
    }

    #[test]
    fn renders_basic_template() {
        let out =
            render_prompt("Hi {{ issue.identifier }}!", &sample_issue(), None).unwrap();
        assert_eq!(out, "Hi ABC-1!");
    }

    #[test]
    fn empty_template_falls_back() {
        let out = render_prompt("", &sample_issue(), None).unwrap();
        assert_eq!(out, FALLBACK_PROMPT);
    }

    #[test]
    fn unknown_filter_fails() {
        let err = render_prompt("{{ issue.title | bogus }}", &sample_issue(), None)
            .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::TemplateParseError(_) | ConfigError::TemplateRenderError(_)
        ));
    }

    fn pr_review_issue() -> Issue {
        Issue {
            id: "owner/repo/pr/42".into(),
            identifier: "PR #42".into(),
            title: "Add feature X".into(),
            description: Some("Closes #1".into()),
            priority: None,
            state: "pr:pending-review".into(),
            branch_name: Some("feat-x".into()),
            url: Some("https://github.com/owner/repo/pull/42".into()),
            labels: vec!["feature".into()],
            blocked_by: vec![],
            created_at: Some(Utc::now()),
            updated_at: None,
            repo: Some("owner/repo".into()),
            kind: "pr_review".into(),
            author: Some("alice".into()),
        }
    }

    #[test]
    fn task_kind_is_bound() {
        let out = render_prompt("kind={{ task.kind }}", &sample_issue(), None).unwrap();
        assert_eq!(out, "kind=issue");
        let out = render_prompt("kind={{ task.kind }}", &pr_review_issue(), None).unwrap();
        assert_eq!(out, "kind=pr_review");
    }

    #[test]
    fn pr_namespace_is_null_for_issue_truthy_for_pr_review() {
        // For issue tasks, `pr` is bound to null so `{% if pr %}` works.
        let tpl = "{% if pr %}has-pr{% else %}no-pr{% endif %}";
        assert_eq!(render_prompt(tpl, &sample_issue(), None).unwrap(), "no-pr");
        assert_eq!(
            render_prompt(tpl, &pr_review_issue(), None).unwrap(),
            "has-pr"
        );
        // PR-review rendering: pr.* is populated.
        let tpl = "{{ pr.identifier }} on {{ pr.repo }} #{{ pr.number }} by {{ pr.author }} branch={{ pr.branch }}";
        let out = render_prompt(tpl, &pr_review_issue(), None).unwrap();
        assert_eq!(out, "PR #42 on owner/repo #42 by alice branch=feat-x");
    }

    #[test]
    fn branched_workflow_template_round_trip() {
        let tpl = "{% assign kind = task.kind | default: \"issue\" %}\
                   {% if kind == \"pr_review\" %}review {{ pr.identifier }}\
                   {% else %}work {{ issue.identifier }}{% endif %}";
        assert_eq!(
            render_prompt(tpl, &sample_issue(), None).unwrap(),
            "work ABC-1"
        );
        assert_eq!(
            render_prompt(tpl, &pr_review_issue(), None).unwrap(),
            "review PR #42"
        );
    }
}
