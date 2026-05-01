use once_cell::sync::Lazy;
use regex::Regex;

static WORKSPACE_KEY_INVALID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^A-Za-z0-9._-]").expect("static regex compiles"));

/// Sanitize a tracker `identifier` (e.g. `ABC-123`) into a directory-safe key
/// per spec §4.2: characters outside `[A-Za-z0-9._-]` become `_`.
pub fn sanitize_workspace_key(identifier: &str) -> String {
    WORKSPACE_KEY_INVALID.replace_all(identifier, "_").into_owned()
}

/// Compose `<thread_id>-<turn_id>` per spec §4.2.
pub fn session_id(thread_id: &str, turn_id: &str) -> String {
    format!("{thread_id}-{turn_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_path_separators_and_special_chars() {
        assert_eq!(sanitize_workspace_key("ABC-123"), "ABC-123");
        assert_eq!(sanitize_workspace_key("A B/C"), "A_B_C");
        assert_eq!(sanitize_workspace_key("../etc/passwd"), ".._etc_passwd");
        assert_eq!(sanitize_workspace_key("ENG-1.0_beta"), "ENG-1.0_beta");
    }

    #[test]
    fn composes_session_id() {
        assert_eq!(session_id("t1", "u9"), "t1-u9");
    }
}
