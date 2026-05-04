//! `meta.toml` schema for a page folder.
//!
//! Forward-compatible: unknown keys are ignored. `meta_version` lets future
//! migrations spot stale shapes; v1 is the only one today.

use serde::{Deserialize, Serialize};

pub const DEFAULT_META_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    /// Human-readable label shown in the sidebar / page header.
    pub title: String,

    /// Optional sidebar icon name (resolved client-side; unknown names fall
    /// back to a generic page glyph).
    #[serde(default)]
    pub icon: Option<String>,

    /// Sidebar ordering. Lower = higher in the list. Ties broken by `title`.
    #[serde(default = "default_position")]
    pub position: i64,

    #[serde(default)]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Schema version of this meta file. Bump when the shape changes.
    #[serde(default = "default_version")]
    pub meta_version: i64,
}

fn default_position() -> i64 {
    100
}

fn default_version() -> i64 {
    DEFAULT_META_VERSION
}

pub fn parse(src: &str) -> Result<Meta, String> {
    toml::from_str::<Meta>(src).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal() {
        let m = parse(r#"title = "Sales overview""#).expect("parse");
        assert_eq!(m.title, "Sales overview");
        assert_eq!(m.position, 100);
        assert_eq!(m.meta_version, DEFAULT_META_VERSION);
    }

    #[test]
    fn parses_full() {
        let m = parse(
            r#"
title = "PRs this week"
icon = "bar-chart"
position = 10
meta_version = 1
"#,
        )
        .unwrap();
        assert_eq!(m.title, "PRs this week");
        assert_eq!(m.icon.as_deref(), Some("bar-chart"));
        assert_eq!(m.position, 10);
    }

    #[test]
    fn rejects_missing_title() {
        assert!(parse("position = 1").is_err());
    }
}
