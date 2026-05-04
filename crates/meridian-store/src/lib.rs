//! SQLite-backed store with a Linear-shaped data model.
//!
//! Two layers live here:
//!
//! - The Linear domain (workspace, team, project, cycle, workflow_state, label,
//!   user, issue, issue_label, issue_relation, issue_subscriber, comment,
//!   comment_reaction, attachment, issue_history) — the source of truth that
//!   replaces the GitHub-issues backend.
//! - The Symphony runtime/progress tables (run_attempt, live_session,
//!   retry_entry, session_event) — what the spec keeps in-memory; we persist
//!   them so progress survives restart.
//!
//! All async methods wrap a sync [`rusqlite::Connection`] guarded by a
//! `parking_lot::Mutex` and dispatched through `spawn_blocking`. WAL mode and
//! foreign-key enforcement are enabled at open time.

pub mod error;
pub mod models;
pub mod schema;
pub mod store;

pub use error::StoreError;
pub use models::{
    Attachment, AutomationRecord, AutomationRunRecord, Comment, Cycle, HarnessRecord,
    InboxEntryRecord, IssueRecord, IssueRelation, IssueRelationType, Label, LiveSessionRecord,
    NewIssue, PageRecord, Project, ProjectState, RepoRecord, RetryEntryRecord, RunAttemptRecord,
    RunAttemptStatus, SessionEventRecord, Team, User, Workspace, WorkflowState, WorkflowStateType,
};
pub use store::{ReadOnlyQueryResult, Store};
