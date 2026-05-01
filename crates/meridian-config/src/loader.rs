use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::config::ServiceConfig;
use crate::error::ConfigError;

/// Parsed `WORKFLOW.md` payload (spec §4.1.2).
#[derive(Debug, Clone)]
pub struct WorkflowDefinition {
    pub source_path: PathBuf,
    pub raw_config: Value,
    pub prompt_template: String,
    pub config: ServiceConfig,
}

/// Read and parse `WORKFLOW.md` from disk.
pub async fn load_workflow(path: &Path) -> Result<WorkflowDefinition, ConfigError> {
    let body = match tokio::fs::read_to_string(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ConfigError::MissingWorkflowFile {
                path: path.display().to_string(),
            })
        }
        Err(e) => return Err(ConfigError::Io(e)),
    };
    parse_workflow(path, &body)
}

/// Parse a workflow string. Front matter is optional (spec §5.2).
pub fn parse_workflow(source: &Path, body: &str) -> Result<WorkflowDefinition, ConfigError> {
    let (raw_config, prompt_body) = split_front_matter(body)?;
    let config = ServiceConfig::from_raw(&raw_config);
    Ok(WorkflowDefinition {
        source_path: source.to_path_buf(),
        raw_config,
        prompt_template: prompt_body.trim().to_string(),
        config,
    })
}

fn split_front_matter(body: &str) -> Result<(Value, String), ConfigError> {
    // Spec §5.2: only front matter when the file *starts* with `---`.
    let trimmed = body.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return Ok((Value::Object(Default::default()), trimmed.to_string()));
    }
    // Find the closing `---` on its own line.
    let after_first = &trimmed[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);
    let close_idx = find_closing_marker(after_first).ok_or_else(|| {
        ConfigError::WorkflowParseError("unterminated YAML front matter".into())
    })?;
    let yaml_text = &after_first[..close_idx];
    let after_close = &after_first[close_idx..];
    let after_close = after_close.trim_start_matches("---");
    let after_close = after_close.strip_prefix('\n').unwrap_or(after_close);

    let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_text)
        .map_err(|e| ConfigError::WorkflowParseError(e.to_string()))?;
    let json: Value = serde_json::to_value(&yaml)
        .map_err(|e| ConfigError::WorkflowParseError(e.to_string()))?;
    if !json.is_object() && !json.is_null() {
        return Err(ConfigError::WorkflowFrontMatterNotAMap);
    }
    let json = if json.is_null() {
        Value::Object(Default::default())
    } else {
        json
    };
    Ok((json, after_close.to_string()))
}

fn find_closing_marker(s: &str) -> Option<usize> {
    let mut start = 0usize;
    while start < s.len() {
        let line_end = s[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(s.len());
        let line = &s[start..line_end];
        if line.trim_end() == "---" {
            return Some(start);
        }
        start = line_end + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_front_matter() {
        let src = "---\ntracker:\n  kind: github\n  repo: o/r\n---\nHello {{ issue.identifier }}";
        let wf = parse_workflow(&PathBuf::from("WORKFLOW.md"), src).unwrap();
        assert_eq!(wf.config.tracker.kind, "github");
        assert_eq!(wf.prompt_template, "Hello {{ issue.identifier }}");
    }

    #[test]
    fn handles_missing_front_matter() {
        let src = "Just a prompt body";
        let wf = parse_workflow(&PathBuf::from("WORKFLOW.md"), src).unwrap();
        assert_eq!(wf.prompt_template, "Just a prompt body");
        assert_eq!(wf.config.tracker.kind, ""); // no kind set
    }

    #[test]
    fn rejects_non_map_front_matter() {
        let src = "---\n- a\n- b\n---\nbody";
        let err = parse_workflow(&PathBuf::from("WORKFLOW.md"), src).unwrap_err();
        assert!(matches!(err, ConfigError::WorkflowFrontMatterNotAMap));
    }
}
