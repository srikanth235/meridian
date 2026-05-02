//! Linear-shaped record types + Symphony runtime/progress records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub url_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub workspace_id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub private: bool,
    pub cycles_enabled: bool,
    pub cycle_duration: Option<i64>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStateType {
    Triage,
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
}

impl WorkflowStateType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowStateType::Triage => "triage",
            WorkflowStateType::Backlog => "backlog",
            WorkflowStateType::Unstarted => "unstarted",
            WorkflowStateType::Started => "started",
            WorkflowStateType::Completed => "completed",
            WorkflowStateType::Canceled => "canceled",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "triage" => Self::Triage,
            "backlog" => Self::Backlog,
            "unstarted" => Self::Unstarted,
            "started" => Self::Started,
            "completed" => Self::Completed,
            "canceled" => Self::Canceled,
            _ => return None,
        })
    }
    /// True if Symphony should dispatch agents on issues in this state.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Unstarted | Self::Started)
    }
    /// True if Symphony should treat this as terminal (cleanup workspaces).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub id: String,
    pub team_id: String,
    pub name: String,
    pub r#type: WorkflowStateType,
    pub position: f64,
    pub color: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectState {
    Planned,
    Started,
    Paused,
    Completed,
    Canceled,
}
impl ProjectState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Started => "started",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "planned" => Self::Planned,
            "started" => Self::Started,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "canceled" => Self::Canceled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub state: ProjectState,
    pub lead_id: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub target_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub sort_order: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cycle {
    pub id: String,
    pub team_id: String,
    pub number: i64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub workspace_id: String,
    pub team_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

/// Full row in `issue`, surfaced as a domain record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueRecord {
    pub id: String,
    pub team_id: String,
    pub team_key: String,
    pub number: i64,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: i32,
    pub estimate: Option<f64>,
    pub state_id: String,
    pub state_name: String,
    pub state_type: WorkflowStateType,
    pub project_id: Option<String>,
    pub project_milestone_id: Option<String>,
    pub cycle_id: Option<String>,
    pub parent_id: Option<String>,
    pub assignee_id: Option<String>,
    pub creator_id: Option<String>,
    pub branch_name: Option<String>,
    pub url: Option<String>,
    pub sort_order: f64,
    pub sub_issue_sort_order: Option<f64>,
    pub due_date: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub trashed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub labels: Vec<String>,
    pub blocked_by: Vec<BlockerRef>,
    /// Task kind — `"issue"` (default) or `"pr_review"`.
    pub kind: String,
    /// Upstream author login (currently only set for PR-review rows).
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerRef {
    pub id: String,
    pub identifier: String,
    pub state_name: String,
    pub state_type: WorkflowStateType,
}

#[derive(Debug, Clone, Default)]
pub struct NewIssue {
    pub id: Option<String>,
    pub team_id: String,
    pub state_id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: i32,
    pub estimate: Option<f64>,
    pub project_id: Option<String>,
    pub cycle_id: Option<String>,
    pub parent_id: Option<String>,
    pub assignee_id: Option<String>,
    pub creator_id: Option<String>,
    pub label_ids: Vec<String>,
    pub branch_name: Option<String>,
    pub url: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    /// Task kind — empty string is treated as `"issue"`.
    pub kind: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueRelationType {
    Blocks,
    Duplicate,
    Related,
}
impl IssueRelationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Blocks => "blocks",
            Self::Duplicate => "duplicate",
            Self::Related => "related",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "blocks" => Self::Blocks,
            "duplicate" => Self::Duplicate,
            "related" => Self::Related,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueRelation {
    pub id: String,
    pub issue_id: String,
    pub related_issue_id: String,
    pub r#type: IssueRelationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub body: String,
    pub edited_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub issue_id: String,
    pub creator_id: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub url: String,
    pub source: Option<String>,
    pub metadata_json: Option<String>,
}

// ============================================================================
// Symphony runtime / progress records
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunAttemptStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Timeout,
    Canceled,
}
impl RunAttemptStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
            Self::Canceled => "canceled",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "timeout" => Self::Timeout,
            "canceled" => Self::Canceled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunAttemptRecord {
    pub id: String,
    pub issue_id: String,
    pub issue_identifier: String,
    pub attempt_no: i64,
    pub workspace_path: Option<String>,
    pub status: RunAttemptStatus,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSessionRecord {
    pub run_attempt_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub codex_pid: Option<i64>,
    pub last_event: Option<String>,
    pub last_event_at: Option<DateTime<Utc>>,
    pub last_message: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub last_reported_input_tokens: i64,
    pub last_reported_output_tokens: i64,
    pub last_reported_total_tokens: i64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryEntryRecord {
    pub issue_id: String,
    pub identifier: String,
    pub attempt: i64,
    pub due_at_ms: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEventRecord {
    pub id: i64,
    pub run_attempt_id: String,
    pub ts: DateTime<Utc>,
    pub event_type: String,
    pub payload_json: Option<String>,
}
