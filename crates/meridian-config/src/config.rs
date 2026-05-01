use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::ConfigError;

const DEFAULT_LINEAR_ENDPOINT: &str = "https://api.linear.app/graphql";
const DEFAULT_POLL_INTERVAL_MS: u64 = 30_000;
const DEFAULT_HOOK_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_MAX_CONCURRENT_AGENTS: u32 = 10;
const DEFAULT_MAX_TURNS: u32 = 20;
const DEFAULT_MAX_RETRY_BACKOFF_MS: u64 = 300_000;
const DEFAULT_TURN_TIMEOUT_MS: u64 = 3_600_000;
const DEFAULT_READ_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_STALL_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_CODEX_COMMAND: &str = "codex app-server";
const DEFAULT_APPROVAL_POLICY: &str = "never";
const DEFAULT_THREAD_SANDBOX: &str = "danger-full-access";
const DEFAULT_TURN_SANDBOX_TYPE: &str = "dangerFullAccess";
const DEFAULT_SERVER_PORT: u16 = 7878;

/// Typed view over the YAML front matter (spec §5.3 + §6.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub tracker: TrackerConfig,
    pub polling: PollingConfig,
    pub workspace: WorkspaceConfig,
    pub hooks: HooksConfig,
    pub agent: AgentConfig,
    pub codex: CodexConfig,
    pub worker: WorkerConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerConfig {
    pub kind: String,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub project_slug: Option<String>,
    pub active_states: Vec<String>,
    pub terminal_states: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    pub interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    pub after_create: Option<String>,
    pub before_run: Option<String>,
    pub after_run: Option<String>,
    pub before_remove: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub max_concurrent_agents: u32,
    pub max_turns: u32,
    pub max_retry_backoff_ms: u64,
    pub max_concurrent_agents_by_state: HashMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexConfig {
    pub command: String,
    pub approval_policy: String,
    pub thread_sandbox: String,
    pub turn_sandbox_policy: Value,
    pub turn_timeout_ms: u64,
    pub read_timeout_ms: u64,
    pub stall_timeout_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub ssh_hosts: Vec<String>,
    pub max_concurrent_agents_per_host: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

impl ServiceConfig {
    /// Build a typed view from the raw YAML front matter root object.
    pub fn from_raw(raw: &Value) -> Self {
        let root = raw.as_object().cloned().unwrap_or_default();

        let tracker_obj = obj(&root, "tracker");
        let tracker_kind = string(&tracker_obj, "kind").unwrap_or_default();
        let tracker = TrackerConfig {
            endpoint: string(&tracker_obj, "endpoint").unwrap_or_else(|| {
                if tracker_kind == "linear" {
                    DEFAULT_LINEAR_ENDPOINT.to_string()
                } else {
                    String::new()
                }
            }),
            api_key: string(&tracker_obj, "api_key").and_then(resolve_env),
            project_slug: string(&tracker_obj, "project_slug"),
            active_states: string_list(&tracker_obj, "active_states")
                .unwrap_or_else(default_active_states),
            terminal_states: string_list(&tracker_obj, "terminal_states")
                .unwrap_or_else(default_terminal_states),
            kind: tracker_kind,
        };

        let polling_obj = obj(&root, "polling");
        let polling = PollingConfig {
            interval_ms: int(&polling_obj, "interval_ms").unwrap_or(DEFAULT_POLL_INTERVAL_MS),
        };

        let workspace_obj = obj(&root, "workspace");
        let workspace_root = string(&workspace_obj, "root")
            .map(|s| expand_path(&s))
            .unwrap_or_else(default_workspace_root);
        let workspace = WorkspaceConfig {
            root: workspace_root,
        };

        let hooks_obj = obj(&root, "hooks");
        let mut timeout_ms = int(&hooks_obj, "timeout_ms").unwrap_or(DEFAULT_HOOK_TIMEOUT_MS);
        if timeout_ms == 0 {
            timeout_ms = DEFAULT_HOOK_TIMEOUT_MS;
        }
        let hooks = HooksConfig {
            after_create: string(&hooks_obj, "after_create"),
            before_run: string(&hooks_obj, "before_run"),
            after_run: string(&hooks_obj, "after_run"),
            before_remove: string(&hooks_obj, "before_remove"),
            timeout_ms,
        };

        let agent_obj = obj(&root, "agent");
        let agent = AgentConfig {
            max_concurrent_agents: int(&agent_obj, "max_concurrent_agents")
                .map(|v| v as u32)
                .unwrap_or(DEFAULT_MAX_CONCURRENT_AGENTS),
            max_turns: int(&agent_obj, "max_turns")
                .map(|v| v as u32)
                .unwrap_or(DEFAULT_MAX_TURNS),
            max_retry_backoff_ms: int(&agent_obj, "max_retry_backoff_ms")
                .unwrap_or(DEFAULT_MAX_RETRY_BACKOFF_MS),
            max_concurrent_agents_by_state: parse_state_map(&agent_obj, "max_concurrent_agents_by_state"),
        };

        let codex_obj = obj(&root, "codex");
        let codex = CodexConfig {
            command: string(&codex_obj, "command")
                .unwrap_or_else(|| DEFAULT_CODEX_COMMAND.to_string()),
            approval_policy: string(&codex_obj, "approval_policy")
                .unwrap_or_else(|| DEFAULT_APPROVAL_POLICY.to_string()),
            thread_sandbox: string(&codex_obj, "thread_sandbox")
                .unwrap_or_else(|| DEFAULT_THREAD_SANDBOX.to_string()),
            turn_sandbox_policy: codex_obj
                .get("turn_sandbox_policy")
                .cloned()
                .unwrap_or_else(default_turn_sandbox_policy),
            turn_timeout_ms: int(&codex_obj, "turn_timeout_ms").unwrap_or(DEFAULT_TURN_TIMEOUT_MS),
            read_timeout_ms: int(&codex_obj, "read_timeout_ms").unwrap_or(DEFAULT_READ_TIMEOUT_MS),
            stall_timeout_ms: codex_obj
                .get("stall_timeout_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(DEFAULT_STALL_TIMEOUT_MS as i64),
        };

        let worker_obj = obj(&root, "worker");
        let worker = WorkerConfig {
            ssh_hosts: string_list(&worker_obj, "ssh_hosts").unwrap_or_default(),
            max_concurrent_agents_per_host: int(&worker_obj, "max_concurrent_agents_per_host")
                .and_then(|v| if v > 0 { Some(v as u32) } else { None }),
        };

        let server_obj = obj(&root, "server");
        let server = ServerConfig {
            port: int(&server_obj, "port")
                .map(|v| v as u16)
                .unwrap_or(DEFAULT_SERVER_PORT),
        };

        Self {
            tracker,
            polling,
            workspace,
            hooks,
            agent,
            codex,
            worker,
            server,
        }
    }

    /// Dispatch preflight (spec §6.3).
    pub fn preflight(&self) -> Result<(), ConfigError> {
        if self.tracker.kind.is_empty() {
            return Err(ConfigError::PreflightFailed(
                "tracker.kind is required".into(),
            ));
        }
        if self.tracker.kind == "linear" {
            if self
                .tracker
                .api_key
                .as_deref()
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                return Err(ConfigError::PreflightFailed(
                    "tracker.api_key is required for linear (set LINEAR_API_KEY)".into(),
                ));
            }
            if self
                .tracker
                .project_slug
                .as_deref()
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                return Err(ConfigError::PreflightFailed(
                    "tracker.project_slug is required for linear".into(),
                ));
            }
        } else {
            return Err(ConfigError::PreflightFailed(format!(
                "unsupported tracker.kind: {}",
                self.tracker.kind
            )));
        }
        if self.codex.command.trim().is_empty() {
            return Err(ConfigError::PreflightFailed(
                "codex.command is required".into(),
            ));
        }
        Ok(())
    }
}

/* ----------- helpers ----------- */

fn obj(root: &serde_json::Map<String, Value>, key: &str) -> serde_json::Map<String, Value> {
    root.get(key)
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn string(o: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    o.get(key).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

fn int(o: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    o.get(key).and_then(|v| match v {
        Value::Number(n) => n.as_u64().or_else(|| n.as_i64().map(|i| i.max(0) as u64)),
        Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    })
}

fn string_list(o: &serde_json::Map<String, Value>, key: &str) -> Option<Vec<String>> {
    o.get(key).and_then(|v| match v {
        Value::Array(arr) => Some(
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect(),
        ),
        _ => None,
    })
}

fn parse_state_map(
    o: &serde_json::Map<String, Value>,
    key: &str,
) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    let Some(Value::Object(m)) = o.get(key) else {
        return out;
    };
    for (k, v) in m {
        let Some(n) = v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        out.insert(k.to_lowercase(), n as u32);
    }
    out
}

/// Resolve `$VAR_NAME` env indirection. Empty resolved values are treated as
/// missing (spec §5.3.1).
fn resolve_env(value: String) -> Option<String> {
    if let Some(name) = value.strip_prefix('$') {
        match std::env::var(name) {
            Ok(v) if !v.is_empty() => Some(v),
            _ => None,
        }
    } else if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Expand `~`, `$VAR` for path-like values. Bare strings without separators are
/// kept as-is per spec §5.3.3.
fn expand_path(input: &str) -> PathBuf {
    let mut s = input.to_string();
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            s = home.join(stripped).to_string_lossy().into_owned();
        }
    } else if s == "~" {
        if let Some(home) = dirs::home_dir() {
            s = home.to_string_lossy().into_owned();
        }
    }
    if let Some(name) = s.strip_prefix('$') {
        if let Ok(v) = std::env::var(name) {
            s = v;
        }
    }
    PathBuf::from(s)
}

fn default_workspace_root() -> PathBuf {
    std::env::temp_dir().join("meridian_workspaces")
}

fn default_active_states() -> Vec<String> {
    vec!["Todo".into(), "In Progress".into()]
}

fn default_terminal_states() -> Vec<String> {
    vec![
        "Closed".into(),
        "Cancelled".into(),
        "Canceled".into(),
        "Duplicate".into(),
        "Done".into(),
    ]
}

fn default_turn_sandbox_policy() -> Value {
    serde_json::json!({ "type": DEFAULT_TURN_SANDBOX_TYPE })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_apply_for_empty_root() {
        let cfg = ServiceConfig::from_raw(&json!({}));
        assert_eq!(cfg.polling.interval_ms, DEFAULT_POLL_INTERVAL_MS);
        assert_eq!(cfg.agent.max_concurrent_agents, DEFAULT_MAX_CONCURRENT_AGENTS);
        assert_eq!(cfg.agent.max_turns, DEFAULT_MAX_TURNS);
        assert_eq!(cfg.codex.command, DEFAULT_CODEX_COMMAND);
        assert_eq!(cfg.codex.approval_policy, DEFAULT_APPROVAL_POLICY);
        assert_eq!(cfg.tracker.active_states, default_active_states());
    }

    #[test]
    fn linear_endpoint_default() {
        let cfg = ServiceConfig::from_raw(&json!({"tracker": {"kind": "linear"}}));
        assert_eq!(cfg.tracker.endpoint, DEFAULT_LINEAR_ENDPOINT);
    }

    #[test]
    fn preflight_requires_linear_keys() {
        let cfg = ServiceConfig::from_raw(&json!({"tracker": {"kind": "linear"}}));
        assert!(cfg.preflight().is_err());
    }

    #[test]
    fn env_indirection_resolves() {
        std::env::set_var("__SYM_TEST_KEY", "abc");
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {"kind": "linear", "api_key": "$__SYM_TEST_KEY", "project_slug": "p"}
        }));
        assert_eq!(cfg.tracker.api_key.as_deref(), Some("abc"));
        assert!(cfg.preflight().is_ok());
        std::env::remove_var("__SYM_TEST_KEY");
    }

    #[test]
    fn invalid_hook_timeout_falls_back() {
        let cfg = ServiceConfig::from_raw(&json!({"hooks": {"timeout_ms": 0}}));
        assert_eq!(cfg.hooks.timeout_ms, DEFAULT_HOOK_TIMEOUT_MS);
    }
}
