use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::ConfigError;

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
    /// One or more `owner/name` repos to poll. The legacy `repo:` (singular)
    /// key is also accepted at parse time and folded into this list. Required
    /// when `kind == "github"`.
    #[serde(default)]
    pub repos: Vec<String>,
    /// Path to the SQLite database. Required when `kind == "sqlite"`. `~` is
    /// expanded; relative paths resolve against the directory containing
    /// `WORKFLOW.md`. Defaults to `~/.meridian/meridian.db` when unset.
    #[serde(default)]
    pub db_path: Option<PathBuf>,
    /// Issue states the orchestrator dispatches on. For `kind: github` these
    /// are label names like `status:todo` (case-insensitive) plus the special
    /// token `open` for label-less open issues. For `kind: sqlite` these are
    /// `workflow_state.name` values (e.g. `Todo`, `In Progress`).
    pub active_states: Vec<String>,
    /// Terminal states (cancel running workers, drop from kanban active set).
    /// For GitHub: `closed`. For SQLite: typically `Done`, `Canceled`.
    pub terminal_states: Vec<String>,
    /// Optional explicit column ordering for the UI. If empty, columns are
    /// `active_states ++ terminal_states` in declaration order.
    #[serde(default)]
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    pub interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub root: PathBuf,
    /// Whether to delete the per-issue workspace directory when the issue
    /// transitions to a terminal state (or appears in startup terminal
    /// cleanup). Spec §8.5/§8.6 lists deletion as the default behavior, but
    /// it makes the Codex desktop app unable to group sessions by cwd
    /// (vanished projects don't render). Set to `false` to keep workspaces
    /// around for browsing/debugging. Default: `false`.
    pub delete_on_terminal: bool,
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
    /// If true, the orchestrator skips new dispatches and re-queues retries
    /// without invoking workers. In-flight workers finish naturally. Toggleable
    /// at runtime via /api/control/{pause|resume}; the runtime toggle takes
    /// precedence over this default until the daemon restarts.
    /// Defaults to `true` so a fresh install doesn't auto-burn agent quota.
    pub paused: bool,
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
    /// Workaround for Codex desktop sidebar visibility (see WORKFLOW.md notes).
    /// `codex app-server` over stdio always tags sessions as `source: vscode`,
    /// which the desktop app filters out of its sidebar. When this is set,
    /// Meridian rewrites the `source` field in line 1 of the rollout file
    /// after the session ends. Common values: `cli`, `exec`. Set to `null`
    /// to disable.
    pub session_source_override: Option<String>,
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
        // Accept either `repos: [a, b]` or legacy singular `repo: a`. Dedup
        // while preserving order so the UI's project list is stable.
        let mut repos = string_list(&tracker_obj, "repos").unwrap_or_default();
        if let Some(single) = string(&tracker_obj, "repo") {
            if !repos.iter().any(|r| r == &single) {
                repos.push(single);
            }
        }
        let tracker_kind_lc = tracker_kind.to_lowercase();
        let (default_active, default_terminal): (fn() -> Vec<String>, fn() -> Vec<String>) =
            if tracker_kind_lc == "sqlite" {
                (default_active_states_linear, default_terminal_states_linear)
            } else {
                (default_active_states_github, default_terminal_states_github)
            };
        let tracker = TrackerConfig {
            repos,
            db_path: string(&tracker_obj, "db_path").map(|s| expand_path(&s)),
            active_states: string_list(&tracker_obj, "active_states").unwrap_or_else(default_active),
            terminal_states: string_list(&tracker_obj, "terminal_states")
                .unwrap_or_else(default_terminal),
            columns: string_list(&tracker_obj, "columns").unwrap_or_default(),
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
            delete_on_terminal: workspace_obj
                .get("delete_on_terminal")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
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
            paused: agent_obj
                .get("paused")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
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
            session_source_override: match codex_obj.get("session_source_override") {
                None => Some("cli".to_string()),
                Some(Value::Null) => None,
                Some(v) => v.as_str().map(|s| s.to_string()).or(None),
            },
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
        match self.tracker.kind.to_lowercase().as_str() {
            "github" => {
                if self.tracker.repos.is_empty() {
                    return Err(ConfigError::PreflightFailed(
                        "tracker.repos must list at least one \"owner/name\" entry".into(),
                    ));
                }
                for repo in &self.tracker.repos {
                    if !repo.contains('/') {
                        return Err(ConfigError::PreflightFailed(format!(
                            "tracker.repos entry must be \"owner/name\", got {repo:?}"
                        )));
                    }
                }
            }
            "sqlite" => {
                // db_path is optional; an absent value is filled in by
                // `effective_db_path()` at construction time.
            }
            other => {
                return Err(ConfigError::PreflightFailed(format!(
                    "unsupported tracker.kind: {other} (supported: \"github\", \"sqlite\")"
                )));
            }
        }
        if self.codex.command.trim().is_empty() {
            return Err(ConfigError::PreflightFailed(
                "codex.command is required".into(),
            ));
        }
        Ok(())
    }

    /// Resolve the effective SQLite path for `kind: sqlite`. Falls back to
    /// `~/.meridian/meridian.db` when `tracker.db_path` is unset.
    pub fn effective_db_path(&self) -> PathBuf {
        if let Some(p) = &self.tracker.db_path {
            return p.clone();
        }
        if let Some(home) = dirs::home_dir() {
            home.join(".meridian").join("meridian.db")
        } else {
            PathBuf::from("meridian.db")
        }
    }

    /// Effective kanban column ordering: explicit `tracker.columns`, else
    /// `active_states ++ terminal_states` minus duplicates.
    pub fn kanban_columns(&self) -> Vec<String> {
        if !self.tracker.columns.is_empty() {
            return self.tracker.columns.clone();
        }
        let mut out = Vec::new();
        for s in self
            .tracker
            .active_states
            .iter()
            .chain(self.tracker.terminal_states.iter())
        {
            if !out.iter().any(|x: &String| x.eq_ignore_ascii_case(s)) {
                out.push(s.clone());
            }
        }
        out
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

fn default_active_states_github() -> Vec<String> {
    vec!["status:todo".into(), "status:in-progress".into()]
}

fn default_terminal_states_github() -> Vec<String> {
    vec!["closed".into()]
}

fn default_active_states_linear() -> Vec<String> {
    vec!["Todo".into(), "In Progress".into()]
}

fn default_terminal_states_linear() -> Vec<String> {
    vec!["Done".into(), "Canceled".into()]
}

#[cfg(test)]
fn default_active_states() -> Vec<String> {
    default_active_states_github()
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
    fn preflight_requires_github_repo() {
        let cfg = ServiceConfig::from_raw(&json!({"tracker": {"kind": "github"}}));
        assert!(cfg.preflight().is_err());
    }

    #[test]
    fn preflight_rejects_malformed_repo() {
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {"kind": "github", "repo": "no-slash"}
        }));
        assert!(cfg.preflight().is_err());
    }

    #[test]
    fn preflight_passes_for_legacy_singular() {
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {"kind": "github", "repo": "owner/name"}
        }));
        assert!(cfg.preflight().is_ok());
        assert_eq!(cfg.tracker.repos, vec!["owner/name".to_string()]);
    }

    #[test]
    fn preflight_passes_for_repos_array() {
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {"kind": "github", "repos": ["o/a", "o/b"]}
        }));
        assert!(cfg.preflight().is_ok());
        assert_eq!(cfg.tracker.repos.len(), 2);
    }

    #[test]
    fn singular_and_array_merge_with_dedupe() {
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {"kind": "github", "repos": ["o/a", "o/b"], "repo": "o/a"}
        }));
        assert_eq!(cfg.tracker.repos, vec!["o/a".to_string(), "o/b".to_string()]);
    }

    #[test]
    fn rejects_unsupported_kind() {
        let cfg = ServiceConfig::from_raw(&json!({"tracker": {"kind": "jira"}}));
        assert!(cfg.preflight().is_err());
    }

    #[test]
    fn preflight_passes_for_sqlite_without_db_path() {
        let cfg = ServiceConfig::from_raw(&json!({"tracker": {"kind": "sqlite"}}));
        assert!(cfg.preflight().is_ok(), "sqlite kind should preflight without db_path");
        assert!(cfg.effective_db_path().to_string_lossy().ends_with("meridian.db"));
        // Linear-style defaults applied:
        assert_eq!(cfg.tracker.active_states, vec!["Todo".to_string(), "In Progress".to_string()]);
        assert_eq!(cfg.tracker.terminal_states, vec!["Done".to_string(), "Canceled".to_string()]);
    }

    #[test]
    fn preflight_passes_for_sqlite_with_db_path() {
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {"kind": "sqlite", "db_path": "/tmp/meridian-test.db"}
        }));
        assert!(cfg.preflight().is_ok());
        assert_eq!(cfg.effective_db_path(), std::path::PathBuf::from("/tmp/meridian-test.db"));
    }

    #[test]
    fn kanban_columns_default_to_active_then_terminal() {
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {
                "kind": "github", "repo": "o/r",
                "active_states": ["status:todo", "status:in-progress"],
                "terminal_states": ["closed"],
            }
        }));
        assert_eq!(cfg.kanban_columns(),
            vec!["status:todo".to_string(), "status:in-progress".into(), "closed".into()]);
    }

    #[test]
    fn kanban_columns_explicit_override() {
        let cfg = ServiceConfig::from_raw(&json!({
            "tracker": {
                "kind": "github", "repo": "o/r",
                "columns": ["status:todo", "status:done"],
            }
        }));
        assert_eq!(cfg.kanban_columns(),
            vec!["status:todo".to_string(), "status:done".into()]);
    }

    #[test]
    fn invalid_hook_timeout_falls_back() {
        let cfg = ServiceConfig::from_raw(&json!({"hooks": {"timeout_ms": 0}}));
        assert_eq!(cfg.hooks.timeout_ms, DEFAULT_HOOK_TIMEOUT_MS);
    }
}
