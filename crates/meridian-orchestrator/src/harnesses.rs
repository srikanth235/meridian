//! Detect coding-harness CLIs (codex, claude, gemini, pi, opencode, …) that
//! are installed on the host. Resolved once at startup and refreshed on a
//! background tick so the snapshot can report what's actually available.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Harness {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub color: String,
    pub concurrency: u32,
    pub in_flight: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub available: bool,
    pub version: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    binary: &'static str,
    color: &'static str,
    default_concurrency: u32,
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry { id: "codex",       name: "Codex",       binary: "codex",    color: "#10b981", default_concurrency: 2 },
    CatalogEntry { id: "claude-code", name: "Claude Code", binary: "claude",   color: "#a855f7", default_concurrency: 2 },
    CatalogEntry { id: "gemini",      name: "Gemini",      binary: "gemini",   color: "#3b82f6", default_concurrency: 2 },
    CatalogEntry { id: "pi-mono",     name: "pi-mono",     binary: "pi",       color: "#f59e0b", default_concurrency: 2 },
    CatalogEntry { id: "opencode",    name: "opencode",    binary: "opencode", color: "#ef4444", default_concurrency: 2 },
];

pub async fn detect_harnesses() -> Vec<Harness> {
    let mut out = Vec::with_capacity(CATALOG.len());
    for entry in CATALOG {
        let resolved = which_on_path(entry.binary);
        let (available, version, last_seen_at) = match resolved {
            Some(p) => (true, probe_version(&p).await, Some(Utc::now())),
            None => (false, None, None),
        };
        out.push(Harness {
            id: entry.id.into(),
            name: entry.name.into(),
            binary: entry.binary.into(),
            color: entry.color.into(),
            concurrency: entry.default_concurrency,
            in_flight: 0,
            capabilities: Vec::new(),
            available,
            version,
            last_seen_at,
        });
    }
    out
}

fn which_on_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat", "ps1"] {
            let mut alt = candidate.clone();
            alt.set_extension(ext);
            if is_executable(&alt) {
                return Some(alt);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(md) => md.is_file() && md.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

async fn probe_version(binary_path: &Path) -> Option<String> {
    let fut = async {
        let output = Command::new(binary_path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .ok()?;
        let raw = if !output.stdout.is_empty() {
            String::from_utf8_lossy(&output.stdout).into_owned()
        } else {
            String::from_utf8_lossy(&output.stderr).into_owned()
        };
        extract_version(&raw)
    };
    timeout(Duration::from_millis(2_000), fut).await.ok().flatten()
}

fn extract_version(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Prefer the first dotted-numeric token (e.g. "1.0.62").
    for token in trimmed.split_whitespace() {
        let cleaned: String = token
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '.')
            .into();
        if cleaned.contains('.') && cleaned.chars().any(|c| c.is_ascii_digit()) {
            return Some(cleaned);
        }
    }
    // Fall back to the first non-empty line, capped to a reasonable length.
    let first_line = trimmed.lines().next().unwrap_or("").trim().to_string();
    if first_line.is_empty() {
        None
    } else {
        Some(first_line.chars().take(80).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::extract_version;

    #[test]
    fn extracts_dotted_version() {
        assert_eq!(extract_version("codex 0.18.4"), Some("0.18.4".into()));
        assert_eq!(extract_version("claude 1.0.62 (claude-code)"), Some("1.0.62".into()));
        assert_eq!(
            extract_version("gemini-cli version 0.4.0\n"),
            Some("0.4.0".into())
        );
    }

    #[test]
    fn falls_back_to_first_line() {
        let v = extract_version("opencode dev-build\n");
        assert!(v.unwrap().contains("opencode"));
    }

    #[test]
    fn empty_returns_none() {
        assert_eq!(extract_version(""), None);
        assert_eq!(extract_version("   \n  "), None);
    }
}
