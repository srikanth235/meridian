use liquid::ParserBuilder;
use serde_json::{json, Value};
use meridian_core::Issue;

use crate::error::ConfigError;

const FALLBACK_PROMPT: &str = "You are working on a GitHub issue.";

/// Render the workflow prompt template with strict checking (spec §12.2).
/// Empty templates fall back to a minimal default per spec §5.4.
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
    let globals_json = json!({
        "issue": issue_json,
        "attempt": attempt,
    });
    let globals = json_to_liquid_object(&globals_json)
        .map_err(|e| ConfigError::TemplateRenderError(e))?;

    tpl.render(&globals)
        .map_err(|e| ConfigError::TemplateRenderError(e.to_string()))
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
}
