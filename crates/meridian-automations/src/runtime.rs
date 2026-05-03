//! Runtime probe: pick the JS runtime that will execute automations.
//!
//! Preference order: `MERIDIAN_NODE_BIN` (explicit override) → `bun` (zero-flag
//! TypeScript) → `node`. We capture the version so the UI can tell the user
//! whether `.ts` files will work — Bun runs them natively; Node needs ≥22.6
//! for `--experimental-strip-types`.

use serde::Serialize;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Bun,
    Node,
    /// Configured but unrecognized (custom MERIDIAN_NODE_BIN). Conservatively
    /// assume Node-style flags.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfo {
    pub command: String,
    pub kind: RuntimeKind,
    /// Best-effort version (e.g. `v22.8.0`, `1.1.34`). Empty if `--version`
    /// failed.
    pub version: String,
    /// True if this runtime can execute `.ts`/`.mts` files. Bun: always.
    /// Node: ≥22.6.
    pub supports_ts: bool,
    /// Where the choice came from — surfaced in the UI.
    pub source: &'static str,
    /// Set when no runtime was found. Other fields are placeholders.
    pub missing: bool,
    /// Optional human-readable note for the UI (e.g. "upgrade to Node 22.6+").
    pub hint: Option<String>,
}

impl RuntimeInfo {
    pub fn missing() -> Self {
        Self {
            command: String::new(),
            kind: RuntimeKind::Unknown,
            version: String::new(),
            supports_ts: false,
            source: "missing",
            missing: true,
            hint: Some(
                "Install Bun (https://bun.sh) or Node.js 22.6+ to run automations.".into(),
            ),
        }
    }

    /// Flags to prepend before the runner script for the given automation file.
    pub fn flags_for(&self, file: &Path) -> Vec<&'static str> {
        let needs_ts = file
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e == "ts" || e == "mts")
            .unwrap_or(false);
        match (self.kind, needs_ts) {
            (RuntimeKind::Bun, _) => Vec::new(),
            (RuntimeKind::Node | RuntimeKind::Unknown, true) => {
                vec!["--experimental-strip-types", "--no-warnings"]
            }
            _ => Vec::new(),
        }
    }
}

/// Probe the host for an executable JS runtime.
///
/// Honors `MERIDIAN_NODE_BIN` as an explicit override (used in tests + when
/// the user's PATH is unusual). Otherwise tries `bun` then `node`.
pub async fn detect() -> RuntimeInfo {
    if let Ok(custom) = std::env::var("MERIDIAN_NODE_BIN") {
        let custom = custom.trim();
        if !custom.is_empty() {
            return classify(custom, "MERIDIAN_NODE_BIN").await;
        }
    }
    if probe_exists("bun").await {
        return classify("bun", "auto").await;
    }
    if probe_exists("node").await {
        return classify("node", "auto").await;
    }
    let info = RuntimeInfo::missing();
    warn!("no JS runtime found on PATH (neither bun nor node)");
    info
}

async fn probe_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn classify(cmd: &str, source: &'static str) -> RuntimeInfo {
    let raw = read_version(cmd).await;
    let kind = guess_kind(cmd);
    let version = raw.trim().to_string();
    // If the binary couldn't even report a version, treat it as missing —
    // every spawn will fail otherwise. This catches typos in
    // MERIDIAN_NODE_BIN and uninstalled binaries on PATH.
    if version.is_empty() {
        warn!(cmd, "configured runtime did not respond to --version");
        let mut info = RuntimeInfo::missing();
        info.command = cmd.to_string();
        info.source = source;
        info.kind = kind;
        info.hint = Some(match kind {
            RuntimeKind::Bun => format!("`{cmd}` did not respond to --version. Reinstall Bun or check the path."),
            RuntimeKind::Node => format!("`{cmd}` did not respond to --version. Reinstall Node or check the path."),
            RuntimeKind::Unknown => format!("`{cmd}` did not respond to --version. Check that the binary exists and is executable."),
        });
        return info;
    }
    let (supports_ts, hint) = match kind {
        RuntimeKind::Bun => (true, None),
        RuntimeKind::Node | RuntimeKind::Unknown => {
            let (ok, hint) = node_supports_strip_types(&version);
            (ok, hint)
        }
    };
    let info = RuntimeInfo {
        command: cmd.to_string(),
        kind,
        version,
        supports_ts,
        source,
        missing: false,
        hint,
    };
    info!(
        runtime = ?info.kind,
        cmd = %info.command,
        version = %info.version,
        supports_ts = info.supports_ts,
        source = info.source,
        "automations runtime detected"
    );
    info
}

fn guess_kind(cmd: &str) -> RuntimeKind {
    let base = std::path::Path::new(cmd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(cmd);
    if base.eq_ignore_ascii_case("bun") {
        RuntimeKind::Bun
    } else if base.eq_ignore_ascii_case("node") {
        RuntimeKind::Node
    } else {
        RuntimeKind::Unknown
    }
}

async fn read_version(cmd: &str) -> String {
    let mut c = Command::new(cmd);
    c.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    match timeout(Duration::from_secs(5), c.output()).await {
        Ok(Ok(out)) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

/// Return (supports_strip_types, hint). Node `--experimental-strip-types`
/// landed in 22.6 behind a flag and is on by default in 23.6+. Anything
/// older means `.ts` files will fail; `.mjs` keeps working.
fn node_supports_strip_types(version: &str) -> (bool, Option<String>) {
    let v = version.trim_start_matches('v');
    let mut parts = v.split('.');
    let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let supports = major > 22 || (major == 22 && minor >= 6);
    let hint = if supports {
        None
    } else if major == 0 {
        Some("Node version unknown — `.ts` automations may fail.".into())
    } else {
        Some(format!(
            "Node {major}.{minor} can't run .ts files; upgrade to Node 22.6+ or install Bun for native TypeScript support."
        ))
    };
    (supports, hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_22_8_supports_ts() {
        let (ok, _) = node_supports_strip_types("v22.8.0");
        assert!(ok);
    }

    #[test]
    fn node_20_supports_no_ts() {
        let (ok, hint) = node_supports_strip_types("v20.10.0");
        assert!(!ok);
        assert!(hint.unwrap().contains("Node 20.10"));
    }

    #[test]
    fn node_22_6_is_threshold() {
        assert!(node_supports_strip_types("v22.6.0").0);
        assert!(!node_supports_strip_types("v22.5.99").0);
    }

    #[test]
    fn bun_skips_ts_flags() {
        let info = RuntimeInfo {
            command: "bun".into(),
            kind: RuntimeKind::Bun,
            version: "1.1.34".into(),
            supports_ts: true,
            source: "auto",
            missing: false,
            hint: None,
        };
        assert!(info.flags_for(std::path::Path::new("a.ts")).is_empty());
        assert!(info.flags_for(std::path::Path::new("a.mjs")).is_empty());
    }

    #[tokio::test]
    async fn explicit_override_to_nonexistent_marks_missing() {
        std::env::set_var("MERIDIAN_NODE_BIN", "/nope/no-such-binary");
        let info = detect().await;
        std::env::remove_var("MERIDIAN_NODE_BIN");
        assert!(info.missing, "expected missing, got {:?}", info);
        assert_eq!(info.command, "/nope/no-such-binary");
        assert_eq!(info.source, "MERIDIAN_NODE_BIN");
        assert!(info.hint.unwrap().contains("did not respond"));
    }

    #[test]
    fn node_adds_strip_flags_for_ts_only() {
        let info = RuntimeInfo {
            command: "node".into(),
            kind: RuntimeKind::Node,
            version: "v22.8.0".into(),
            supports_ts: true,
            source: "auto",
            missing: false,
            hint: None,
        };
        let ts_flags = info.flags_for(std::path::Path::new("a.ts"));
        assert_eq!(ts_flags, vec!["--experimental-strip-types", "--no-warnings"]);
        assert!(info.flags_for(std::path::Path::new("a.mjs")).is_empty());
    }
}
