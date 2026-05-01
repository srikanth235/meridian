use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A filesystem workspace bound to a single issue identifier (spec §4.1.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub path: PathBuf,
    pub workspace_key: String,
    pub created_now: bool,
}
