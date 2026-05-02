//! Detect coding-harness CLIs (codex, claude, gemini, pi, opencode, …) that
//! are installed on the host. Detection results are persisted to sqlite via
//! [`meridian_store::Store`] so user settings (concurrency) survive a binary
//! reinstall, and the snapshot can render immediately at startup without
//! waiting for the first probe.

use chrono::{DateTime, Utc};
use meridian_store::HarnessRecord;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Directories that aren't always on `$PATH` — particularly when the desktop
/// app is launched from Finder/Dock on macOS, which gives child processes a
/// stripped `/usr/bin:/bin:/usr/sbin:/sbin`. We probe these too so user-local
/// installs (homebrew, cargo, nvm, pnpm, ~/.local/bin) still resolve.
fn extra_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);

    for p in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
    ] {
        dirs.push(PathBuf::from(p));
    }
    if let Some(h) = home.as_ref() {
        for sub in [
            ".local/bin",
            ".cargo/bin",
            ".deno/bin",
            "go/bin",
            "Library/pnpm",
            ".bun/bin",
            ".volta/bin",
            ".asdf/shims",
        ] {
            dirs.push(h.join(sub));
        }
        // nvm: enumerate installed node versions and add their bin/ dir.
        if let Ok(read) = std::fs::read_dir(h.join(".nvm/versions/node")) {
            for entry in read.flatten() {
                let bin = entry.path().join("bin");
                if bin.is_dir() {
                    dirs.push(bin);
                }
            }
        }
    }
    dirs
}

/// `$PATH` with our extra search dirs appended, suitable as the env for
/// child processes that need to find runtime tools (e.g. `codex` is a Node
/// shebang script and needs `node` on PATH). Order: original PATH first so
/// user overrides win, then extras as a fallback.
pub fn augmented_path_env() -> std::ffi::OsString {
    let mut entries: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    let mut seen: std::collections::HashSet<PathBuf> = entries.iter().cloned().collect();
    for dir in extra_search_dirs() {
        if seen.insert(dir.clone()) {
            entries.push(dir);
        }
    }
    std::env::join_paths(entries).unwrap_or_default()
}

/// Per-binary additional directories where a harness commonly self-installs
/// outside any PATH entry (e.g. opencode drops itself into ~/.opencode/bin).
fn extra_dirs_for(binary: &str) -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    match binary {
        "opencode" => vec![home.join(".opencode/bin")],
        "claude" => vec![home.join(".claude/local/bin"), home.join(".claude/local")],
        "codex" => vec![home.join(".codex/bin")],
        "gemini" => vec![home.join(".gemini/bin")],
        _ => Vec::new(),
    }
}

/// Wire shape sent to the UI. Built from the sqlite row at snapshot time;
/// `in_flight` and `capabilities` are runtime fields the store doesn't track.
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

impl From<HarnessRecord> for Harness {
    fn from(r: HarnessRecord) -> Self {
        Harness {
            id: r.id,
            name: r.name,
            binary: r.binary,
            color: r.color,
            concurrency: r.concurrency.max(0) as u32,
            in_flight: 0,
            capabilities: Vec::new(),
            available: r.available,
            version: r.version,
            last_seen_at: r.last_seen_at,
        }
    }
}

pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub binary: &'static str,
    pub color: &'static str,
    pub default_concurrency: u32,
}

pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry { id: "codex",         name: "Codex",         binary: "codex",        color: "#10b981", default_concurrency: 2 },
    CatalogEntry { id: "claude-code",   name: "Claude Code",   binary: "claude",       color: "#a855f7", default_concurrency: 2 },
    CatalogEntry { id: "gemini",        name: "Gemini",        binary: "gemini",       color: "#3b82f6", default_concurrency: 2 },
    CatalogEntry { id: "pi-mono",       name: "pi-mono",       binary: "pi",           color: "#f59e0b", default_concurrency: 2 },
    CatalogEntry { id: "opencode",      name: "opencode",      binary: "opencode",     color: "#ef4444", default_concurrency: 2 },
    // Cursor's installer drops symlinks `agent` and `cursor-agent` into
    // ~/.local/bin. We probe `cursor-agent` (the unique one — `agent` is too
    // generic and would collide with future tools).
    CatalogEntry { id: "cursor-agent",  name: "Cursor Agent",  binary: "cursor-agent", color: "#64748b", default_concurrency: 2 },
    // GitHub Copilot CLI installs as `copilot` via `npm i -g @github/copilot`.
    CatalogEntry { id: "github-copilot", name: "GitHub Copilot", binary: "copilot",    color: "#06b6d4", default_concurrency: 2 },
];

/// Result of probing one catalog entry — what the store needs to upsert.
pub struct HarnessProbe {
    pub id: &'static str,
    pub name: &'static str,
    pub binary: &'static str,
    pub color: &'static str,
    pub default_concurrency: u32,
    pub available: bool,
    pub version: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

pub async fn probe_all() -> Vec<HarnessProbe> {
    let mut out = Vec::with_capacity(CATALOG.len());
    for entry in CATALOG {
        let resolved = which_on_path(entry.binary);
        let (available, version, last_seen_at) = match resolved {
            Some(p) => (true, probe_version(&p).await, Some(Utc::now())),
            None => (false, None, None),
        };
        out.push(HarnessProbe {
            id: entry.id,
            name: entry.name,
            binary: entry.binary,
            color: entry.color,
            default_concurrency: entry.default_concurrency,
            available,
            version,
            last_seen_at,
        });
    }
    out
}

fn which_on_path(binary: &str) -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let dirs = path_dirs
        .into_iter()
        .chain(extra_search_dirs())
        .chain(extra_dirs_for(binary))
        .filter(|d| seen.insert(d.clone()));

    for dir in dirs {
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
            .env("PATH", augmented_path_env())
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

    /// Live probe of the host. Skipped by default; run with:
    ///   cargo test -p meridian-orchestrator --lib -- --ignored --nocapture probe_host
    #[tokio::test]
    #[ignore]
    async fn probe_host() {
        let out = super::probe_all().await;
        for h in out {
            println!(
                "{:>12}  available={}  version={:?}",
                h.binary, h.available, h.version
            );
        }
    }
}
