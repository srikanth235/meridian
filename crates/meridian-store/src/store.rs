//! Async-friendly handle around a single SQLite connection.
//!
//! Calls dispatch into `spawn_blocking` so the rest of the system can keep its
//! `tokio` runtime. A single connection is sufficient — SQLite serializes
//! writers anyway, and WAL mode lets reads proceed concurrently inside the
//! same connection's read transactions.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{params_from_iter, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::StoreError;
use crate::models::{
    Attachment, BlockerRef, Comment, Cycle, HarnessRecord, IssueRecord, IssueRelation,
    IssueRelationType, Label, LiveSessionRecord, NewIssue, Project, ProjectState, RepoRecord,
    RetryEntryRecord, RunAttemptRecord, RunAttemptStatus, SessionEventRecord, Team, User,
    Workspace, WorkflowState, WorkflowStateType,
};
use crate::schema;

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Store {
    /// Open (or create) the database file and apply pending migrations.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| StoreError::Open(e.to_string()))?;
            }
        }
        let path_clone = path.clone();
        let conn = tokio::task::spawn_blocking(move || -> Result<Connection, StoreError> {
            let mut conn = Connection::open(&path_clone).map_err(|e| StoreError::Open(e.to_string()))?;
            schema::apply_all(&mut conn)?;
            Ok(conn)
        })
        .await
        .map_err(|e| StoreError::Join(e.to_string()))??;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    /// Open an in-memory store; test-only, but exposed for embedded use.
    pub async fn open_in_memory() -> Result<Self, StoreError> {
        let conn = tokio::task::spawn_blocking(|| -> Result<Connection, StoreError> {
            let mut conn = Connection::open_in_memory().map_err(|e| StoreError::Open(e.to_string()))?;
            schema::apply_all(&mut conn)?;
            Ok(conn)
        })
        .await
        .map_err(|e| StoreError::Join(e.to_string()))??;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    async fn run<F, T>(&self, f: F) -> Result<T, StoreError>
    where
        F: FnOnce(&mut Connection) -> Result<T, StoreError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock();
            f(&mut guard)
        })
        .await
        .map_err(|e| StoreError::Join(e.to_string()))?
    }

    // -------------------- Workspace / User / Team --------------------

    pub async fn create_workspace(&self, name: &str, url_key: &str) -> Result<Workspace, StoreError> {
        let id = new_id();
        let name = name.to_string();
        let url_key = url_key.to_string();
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO workspace (id, name, url_key) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, name, url_key],
            )?;
            row_workspace(conn, &id)
        })
        .await
    }

    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        self.run(|conn| {
            let mut stmt = conn.prepare("SELECT id, name, url_key, created_at, updated_at FROM workspace ORDER BY created_at")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(Workspace {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        url_key: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn create_user(
        &self,
        workspace_id: &str,
        name: &str,
        email: Option<&str>,
    ) -> Result<User, StoreError> {
        let id = new_id();
        let workspace_id = workspace_id.to_string();
        let name = name.to_string();
        let email = email.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO user (id, workspace_id, name, email) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, workspace_id, name, email],
            )?;
            row_user(conn, &id)
        })
        .await
    }

    pub async fn create_team(
        &self,
        workspace_id: &str,
        key: &str,
        name: &str,
    ) -> Result<Team, StoreError> {
        let id = new_id();
        let workspace_id = workspace_id.to_string();
        let key = key.to_string();
        let name = name.to_string();
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO team (id, workspace_id, key, name) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, workspace_id, key, name],
            )?;
            row_team(conn, &id)
        })
        .await
    }

    pub async fn list_teams(&self, workspace_id: &str) -> Result<Vec<Team>, StoreError> {
        let workspace_id = workspace_id.to_string();
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, workspace_id, key, name, description, icon, color, private,
                        cycles_enabled, cycle_duration, timezone
                 FROM team WHERE workspace_id = ?1 ORDER BY key",
            )?;
            let rows = stmt
                .query_map([workspace_id], |row| {
                    Ok(Team {
                        id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        key: row.get(2)?,
                        name: row.get(3)?,
                        description: row.get(4)?,
                        icon: row.get(5)?,
                        color: row.get(6)?,
                        private: row.get::<_, i64>(7)? != 0,
                        cycles_enabled: row.get::<_, i64>(8)? != 0,
                        cycle_duration: row.get(9)?,
                        timezone: row.get(10)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    // -------------------- Workflow states --------------------

    pub async fn create_workflow_state(
        &self,
        team_id: &str,
        name: &str,
        ty: WorkflowStateType,
        position: f64,
        color: Option<&str>,
    ) -> Result<WorkflowState, StoreError> {
        let id = new_id();
        let team_id = team_id.to_string();
        let name = name.to_string();
        let color = color.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO workflow_state (id, team_id, name, type, position, color)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![id, team_id, name, ty.as_str(), position, color],
            )?;
            row_workflow_state(conn, &id)
        })
        .await
    }

    pub async fn list_workflow_states(&self, team_id: &str) -> Result<Vec<WorkflowState>, StoreError> {
        let team_id = team_id.to_string();
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, team_id, name, type, position, color, description
                 FROM workflow_state WHERE team_id = ?1 ORDER BY position",
            )?;
            let rows = stmt
                .query_map([team_id], |row| {
                    let ty: String = row.get(3)?;
                    Ok(WorkflowState {
                        id: row.get(0)?,
                        team_id: row.get(1)?,
                        name: row.get(2)?,
                        r#type: WorkflowStateType::parse(&ty).unwrap_or(WorkflowStateType::Backlog),
                        position: row.get(4)?,
                        color: row.get(5)?,
                        description: row.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    /// Seed Linear's default 5-state workflow for a team. Returns
    /// `(backlog, todo, in_progress, done, canceled)` ids.
    pub async fn seed_default_workflow_states(
        &self,
        team_id: &str,
    ) -> Result<DefaultStates, StoreError> {
        let backlog = self
            .create_workflow_state(team_id, "Backlog", WorkflowStateType::Backlog, 0.0, Some("#bec2c8"))
            .await?;
        let todo = self
            .create_workflow_state(team_id, "Todo", WorkflowStateType::Unstarted, 1.0, Some("#e2e2e2"))
            .await?;
        let in_progress = self
            .create_workflow_state(team_id, "In Progress", WorkflowStateType::Started, 2.0, Some("#f2c94c"))
            .await?;
        let done = self
            .create_workflow_state(team_id, "Done", WorkflowStateType::Completed, 3.0, Some("#5e6ad2"))
            .await?;
        let canceled = self
            .create_workflow_state(team_id, "Canceled", WorkflowStateType::Canceled, 4.0, Some("#95a2b3"))
            .await?;
        Ok(DefaultStates {
            backlog,
            todo,
            in_progress,
            done,
            canceled,
        })
    }

    // -------------------- Labels / Projects / Cycles --------------------

    pub async fn create_label(
        &self,
        workspace_id: &str,
        team_id: Option<&str>,
        name: &str,
        color: Option<&str>,
    ) -> Result<Label, StoreError> {
        let id = new_id();
        let workspace_id = workspace_id.to_string();
        let team_id = team_id.map(|s| s.to_string());
        let name = name.to_string();
        let color = color.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO label (id, workspace_id, team_id, name, color) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, workspace_id, team_id, name, color],
            )?;
            row_label(conn, &id)
        })
        .await
    }

    pub async fn create_project(
        &self,
        workspace_id: &str,
        name: &str,
        state: ProjectState,
    ) -> Result<Project, StoreError> {
        let id = new_id();
        let workspace_id = workspace_id.to_string();
        let name = name.to_string();
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO project (id, workspace_id, name, state) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, workspace_id, name, state.as_str()],
            )?;
            row_project(conn, &id)
        })
        .await
    }

    pub async fn link_project_team(&self, project_id: &str, team_id: &str) -> Result<(), StoreError> {
        let project_id = project_id.to_string();
        let team_id = team_id.to_string();
        self.run(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO project_team (project_id, team_id) VALUES (?1, ?2)",
                rusqlite::params![project_id, team_id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn create_cycle(
        &self,
        team_id: &str,
        number: i64,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Cycle, StoreError> {
        let id = new_id();
        let team_id = team_id.to_string();
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO cycle (id, team_id, number, starts_at, ends_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, team_id, number, starts_at, ends_at],
            )?;
            row_cycle(conn, &id)
        })
        .await
    }

    // -------------------- Issues --------------------

    pub async fn create_issue(&self, new: NewIssue) -> Result<IssueRecord, StoreError> {
        self.run(move |conn| {
            let tx = conn.transaction()?;

            let id = new.id.unwrap_or_else(new_id);

            // Per-team monotonic number.
            let number: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(number), 0) + 1 FROM issue WHERE team_id = ?1",
                    [&new.team_id],
                    |row| row.get(0),
                )?;
            let team_key: String = tx.query_row(
                "SELECT key FROM team WHERE id = ?1",
                [&new.team_id],
                |row| row.get(0),
            )?;
            let identifier = format!("{}-{}", team_key, number);
            let branch_name = new.branch_name.clone().unwrap_or_else(|| {
                format!("{}/{}", team_key.to_lowercase(), slugify(&new.title, number))
            });

            let kind = new
                .kind
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("issue")
                .to_string();
            tx.execute(
                "INSERT INTO issue (
                    id, team_id, number, identifier, title, description,
                    priority, estimate, state_id, project_id, cycle_id,
                    parent_id, assignee_id, creator_id, branch_name, url, due_date,
                    kind, author
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10, ?11,
                    ?12, ?13, ?14, ?15, ?16, ?17,
                    ?18, ?19
                 )",
                rusqlite::params![
                    id,
                    new.team_id,
                    number,
                    identifier,
                    new.title,
                    new.description,
                    new.priority,
                    new.estimate,
                    new.state_id,
                    new.project_id,
                    new.cycle_id,
                    new.parent_id,
                    new.assignee_id,
                    new.creator_id,
                    branch_name,
                    new.url,
                    new.due_date,
                    kind,
                    new.author,
                ],
            )?;

            for label_id in &new.label_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO issue_label (issue_id, label_id) VALUES (?1, ?2)",
                    rusqlite::params![id, label_id],
                )?;
            }

            tx.execute(
                "UPDATE team SET issue_count = issue_count + 1 WHERE id = ?1",
                [&new.team_id],
            )?;
            tx.commit()?;
            fetch_issue_by_id(conn, &id)?.ok_or_else(|| StoreError::NotFound(format!("issue {id}")))
        })
        .await
    }

    pub async fn set_issue_state(&self, issue_id: &str, state_id: &str) -> Result<(), StoreError> {
        let issue_id = issue_id.to_string();
        let state_id = state_id.to_string();
        self.run(move |conn| {
            let tx = conn.transaction()?;
            let from_state: Option<String> = tx
                .query_row("SELECT state_id FROM issue WHERE id = ?1", [&issue_id], |r| {
                    r.get(0)
                })
                .optional()?;
            let to_type: String = tx.query_row(
                "SELECT type FROM workflow_state WHERE id = ?1",
                [&state_id],
                |r| r.get(0),
            )?;
            let now: DateTime<Utc> = Utc::now();
            let started_clause = if to_type == "started" {
                "started_at = COALESCE(started_at, ?3),"
            } else {
                ""
            };
            let completed_clause = if to_type == "completed" {
                "completed_at = ?3,"
            } else {
                ""
            };
            let canceled_clause = if to_type == "canceled" {
                "canceled_at = ?3,"
            } else {
                ""
            };
            let sql = format!(
                "UPDATE issue SET state_id = ?2, {started_clause} {completed_clause} {canceled_clause}
                 updated_at = ?3 WHERE id = ?1"
            );
            tx.execute(&sql, rusqlite::params![issue_id, state_id, now])?;
            tx.execute(
                "INSERT INTO issue_history (id, issue_id, from_state_id, to_state_id) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![new_id(), issue_id, from_state, state_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn add_relation(
        &self,
        issue_id: &str,
        related_issue_id: &str,
        ty: IssueRelationType,
    ) -> Result<IssueRelation, StoreError> {
        let id = new_id();
        let issue_id = issue_id.to_string();
        let related_issue_id = related_issue_id.to_string();
        self.run(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO issue_relation (id, issue_id, related_issue_id, type) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, issue_id, related_issue_id, ty.as_str()],
            )?;
            Ok(IssueRelation {
                id,
                issue_id,
                related_issue_id,
                r#type: ty,
            })
        })
        .await
    }

    /// Read all non-archived, non-trashed issues whose state name matches one
    /// of `state_names` (case-insensitive). Labels and blockers are populated.
    pub async fn fetch_issues_by_state_names(
        &self,
        state_names: &[String],
    ) -> Result<Vec<IssueRecord>, StoreError> {
        let names_lc: Vec<String> = state_names.iter().map(|s| s.to_lowercase()).collect();
        self.run(move |conn| {
            if names_lc.is_empty() {
                return Ok(Vec::new());
            }
            let placeholders = (1..=names_lc.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT i.id FROM issue i
                 JOIN workflow_state s ON s.id = i.state_id
                 WHERE LOWER(s.name) IN ({placeholders})
                   AND i.archived_at IS NULL AND i.trashed = 0
                 ORDER BY i.priority DESC, i.sort_order, i.created_at"
            );
            let mut stmt = conn.prepare(&sql)?;
            let ids: Vec<String> = stmt
                .query_map(params_from_iter(names_lc.iter()), |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut out = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(rec) = fetch_issue_by_id(conn, &id)? {
                    out.push(rec);
                }
            }
            Ok(out)
        })
        .await
    }

    pub async fn fetch_issues_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<IssueRecord>, StoreError> {
        let ids = ids.to_vec();
        self.run(move |conn| {
            let mut out = Vec::with_capacity(ids.len());
            for id in &ids {
                if let Some(rec) = fetch_issue_by_id(conn, id)? {
                    out.push(rec);
                }
            }
            Ok(out)
        })
        .await
    }

    pub async fn get_issue(&self, id: &str) -> Result<Option<IssueRecord>, StoreError> {
        let id = id.to_string();
        self.run(move |conn| fetch_issue_by_id(conn, &id)).await
    }

    // -------------------- Comments / Attachments --------------------

    pub async fn add_comment(
        &self,
        issue_id: &str,
        user_id: Option<&str>,
        body: &str,
    ) -> Result<Comment, StoreError> {
        let id = new_id();
        let issue_id = issue_id.to_string();
        let user_id = user_id.map(|s| s.to_string());
        let body = body.to_string();
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO comment (id, issue_id, user_id, body) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, issue_id, user_id, body],
            )?;
            row_comment(conn, &id)
        })
        .await
    }

    pub async fn add_attachment(
        &self,
        issue_id: &str,
        title: &str,
        url: &str,
        source: Option<&str>,
    ) -> Result<Attachment, StoreError> {
        let id = new_id();
        let issue_id = issue_id.to_string();
        let title = title.to_string();
        let url = url.to_string();
        let source = source.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO attachment (id, issue_id, title, url, source) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, issue_id, title, url, source],
            )?;
            row_attachment(conn, &id)
        })
        .await
    }

    // -------------------- Run attempts / sessions / retries --------------------

    pub async fn create_run_attempt(
        &self,
        issue_id: &str,
        issue_identifier: &str,
        attempt_no: i64,
        workspace_path: Option<&str>,
    ) -> Result<RunAttemptRecord, StoreError> {
        let id = new_id();
        let issue_id = issue_id.to_string();
        let issue_identifier = issue_identifier.to_string();
        let workspace_path = workspace_path.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO run_attempt (id, issue_id, issue_identifier, attempt_no, workspace_path, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
                rusqlite::params![id, issue_id, issue_identifier, attempt_no, workspace_path],
            )?;
            row_run_attempt(conn, &id)
        })
        .await
    }

    pub async fn finish_run_attempt(
        &self,
        run_attempt_id: &str,
        status: RunAttemptStatus,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let run_attempt_id = run_attempt_id.to_string();
        let error = error.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "UPDATE run_attempt SET status = ?2, error = ?3, ended_at = ?4 WHERE id = ?1",
                rusqlite::params![run_attempt_id, status.as_str(), error, Utc::now()],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn upsert_live_session(&self, session: LiveSessionRecord) -> Result<(), StoreError> {
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO live_session (
                    run_attempt_id, session_id, thread_id, turn_id, codex_pid,
                    last_event, last_event_at, last_message,
                    input_tokens, output_tokens, total_tokens,
                    last_reported_input_tokens, last_reported_output_tokens, last_reported_total_tokens,
                    turn_count
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                    ?9, ?10, ?11, ?12, ?13, ?14, ?15
                 )
                 ON CONFLICT(run_attempt_id) DO UPDATE SET
                    session_id = excluded.session_id,
                    thread_id = excluded.thread_id,
                    turn_id = excluded.turn_id,
                    codex_pid = excluded.codex_pid,
                    last_event = excluded.last_event,
                    last_event_at = excluded.last_event_at,
                    last_message = excluded.last_message,
                    input_tokens = excluded.input_tokens,
                    output_tokens = excluded.output_tokens,
                    total_tokens = excluded.total_tokens,
                    last_reported_input_tokens = excluded.last_reported_input_tokens,
                    last_reported_output_tokens = excluded.last_reported_output_tokens,
                    last_reported_total_tokens = excluded.last_reported_total_tokens,
                    turn_count = excluded.turn_count",
                rusqlite::params![
                    session.run_attempt_id,
                    session.session_id,
                    session.thread_id,
                    session.turn_id,
                    session.codex_pid,
                    session.last_event,
                    session.last_event_at,
                    session.last_message,
                    session.input_tokens,
                    session.output_tokens,
                    session.total_tokens,
                    session.last_reported_input_tokens,
                    session.last_reported_output_tokens,
                    session.last_reported_total_tokens,
                    session.turn_count,
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn upsert_retry(&self, entry: RetryEntryRecord) -> Result<(), StoreError> {
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO retry_entry (issue_id, identifier, attempt, due_at_ms, error)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(issue_id) DO UPDATE SET
                    identifier = excluded.identifier,
                    attempt = excluded.attempt,
                    due_at_ms = excluded.due_at_ms,
                    error = excluded.error",
                rusqlite::params![entry.issue_id, entry.identifier, entry.attempt, entry.due_at_ms, entry.error],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn delete_retry(&self, issue_id: &str) -> Result<(), StoreError> {
        let issue_id = issue_id.to_string();
        self.run(move |conn| {
            conn.execute("DELETE FROM retry_entry WHERE issue_id = ?1", [issue_id])?;
            Ok(())
        })
        .await
    }

    pub async fn list_retries(&self) -> Result<Vec<RetryEntryRecord>, StoreError> {
        self.run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT issue_id, identifier, attempt, due_at_ms, error FROM retry_entry ORDER BY due_at_ms",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(RetryEntryRecord {
                        issue_id: row.get(0)?,
                        identifier: row.get(1)?,
                        attempt: row.get(2)?,
                        due_at_ms: row.get(3)?,
                        error: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn append_session_event(
        &self,
        run_attempt_id: &str,
        event_type: &str,
        payload_json: Option<&str>,
    ) -> Result<i64, StoreError> {
        let run_attempt_id = run_attempt_id.to_string();
        let event_type = event_type.to_string();
        let payload_json = payload_json.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO session_event (run_attempt_id, event_type, payload_json) VALUES (?1, ?2, ?3)",
                rusqlite::params![run_attempt_id, event_type, payload_json],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    pub async fn list_session_events(
        &self,
        run_attempt_id: &str,
        limit: i64,
    ) -> Result<Vec<SessionEventRecord>, StoreError> {
        let run_attempt_id = run_attempt_id.to_string();
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, run_attempt_id, ts, event_type, payload_json
                 FROM session_event WHERE run_attempt_id = ?1 ORDER BY ts DESC, id DESC LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![run_attempt_id, limit], |row| {
                    Ok(SessionEventRecord {
                        id: row.get(0)?,
                        run_attempt_id: row.get(1)?,
                        ts: row.get(2)?,
                        event_type: row.get(3)?,
                        payload_json: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    // -------------------- Harnesses --------------------

    /// Upsert one harness's catalog/probe metadata. Existing `concurrency` is
    /// preserved across upserts so the user's slider value isn't clobbered by
    /// a refresh.
    pub async fn upsert_harness_probe(
        &self,
        id: &str,
        name: &str,
        binary: &str,
        color: &str,
        default_concurrency: i64,
        available: bool,
        version: Option<&str>,
        last_seen_at: Option<DateTime<Utc>>,
    ) -> Result<(), StoreError> {
        let id = id.to_string();
        let name = name.to_string();
        let binary = binary.to_string();
        let color = color.to_string();
        let version = version.map(|s| s.to_string());
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO harness (id, name, binary, color, concurrency, available, version, last_seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    binary = excluded.binary,
                    color = excluded.color,
                    available = excluded.available,
                    version = excluded.version,
                    last_seen_at = excluded.last_seen_at",
                rusqlite::params![
                    id,
                    name,
                    binary,
                    color,
                    default_concurrency,
                    available as i64,
                    version,
                    last_seen_at,
                ],
            )?;
            Ok(())
        })
        .await
    }

    /// Mark every harness whose id is NOT in `present_ids` as unavailable.
    /// Called after a refresh sweep to flip stale rows offline.
    pub async fn mark_missing_harnesses_unavailable(
        &self,
        present_ids: &[String],
    ) -> Result<(), StoreError> {
        let present_ids = present_ids.to_vec();
        self.run(move |conn| {
            if present_ids.is_empty() {
                conn.execute("UPDATE harness SET available = 0", [])?;
                return Ok(());
            }
            let placeholders = (1..=present_ids.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!("UPDATE harness SET available = 0 WHERE id NOT IN ({placeholders})");
            conn.execute(&sql, params_from_iter(present_ids.iter()))?;
            Ok(())
        })
        .await
    }

    pub async fn list_harnesses(&self) -> Result<Vec<HarnessRecord>, StoreError> {
        self.run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, binary, color, concurrency, available, version, last_seen_at
                 FROM harness ORDER BY name",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(HarnessRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        binary: row.get(2)?,
                        color: row.get(3)?,
                        concurrency: row.get(4)?,
                        available: row.get::<_, i64>(5)? != 0,
                        version: row.get(6)?,
                        last_seen_at: row.get(7)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn set_harness_concurrency(
        &self,
        id: &str,
        concurrency: i64,
    ) -> Result<(), StoreError> {
        let id = id.to_string();
        self.run(move |conn| {
            let n = conn.execute(
                "UPDATE harness SET concurrency = ?2 WHERE id = ?1",
                rusqlite::params![id, concurrency.max(0)],
            )?;
            if n == 0 {
                return Err(StoreError::NotFound(format!("harness {id}")));
            }
            Ok(())
        })
        .await
    }

    // -------------------- Repos --------------------

    /// Upsert one repo's metadata from a `gh repo list` row. `connected` /
    /// `connected_at` are NEVER touched here — those are user state.
    pub async fn upsert_repo_metadata(
        &self,
        rec: &RepoRecord,
    ) -> Result<(), StoreError> {
        let rec = rec.clone();
        self.run(move |conn| {
            conn.execute(
                "INSERT INTO repo (
                    slug, description, url, default_branch, primary_language,
                    is_private, is_archived, updated_at, connected, connected_at, last_synced_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(slug) DO UPDATE SET
                    description = excluded.description,
                    url = excluded.url,
                    default_branch = excluded.default_branch,
                    primary_language = excluded.primary_language,
                    is_private = excluded.is_private,
                    is_archived = excluded.is_archived,
                    updated_at = excluded.updated_at,
                    last_synced_at = excluded.last_synced_at",
                rusqlite::params![
                    rec.slug,
                    rec.description,
                    rec.url,
                    rec.default_branch,
                    rec.primary_language,
                    rec.is_private as i64,
                    rec.is_archived as i64,
                    rec.updated_at,
                    rec.connected as i64,
                    rec.connected_at,
                    rec.last_synced_at,
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn list_repos(&self) -> Result<Vec<RepoRecord>, StoreError> {
        self.run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT slug, description, url, default_branch, primary_language,
                        is_private, is_archived, updated_at, connected, connected_at, last_synced_at
                 FROM repo
                 ORDER BY connected DESC, slug",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(RepoRecord {
                        slug: row.get(0)?,
                        description: row.get(1)?,
                        url: row.get(2)?,
                        default_branch: row.get(3)?,
                        primary_language: row.get(4)?,
                        is_private: row.get::<_, i64>(5)? != 0,
                        is_archived: row.get::<_, i64>(6)? != 0,
                        updated_at: row.get(7)?,
                        connected: row.get::<_, i64>(8)? != 0,
                        connected_at: row.get(9)?,
                        last_synced_at: row.get(10)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_connected_repo_slugs(&self) -> Result<Vec<String>, StoreError> {
        self.run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT slug FROM repo WHERE connected = 1 ORDER BY slug",
            )?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn set_repo_connected(
        &self,
        slug: &str,
        connected: bool,
    ) -> Result<(), StoreError> {
        let slug = slug.to_string();
        self.run(move |conn| {
            // Insert a stub row if this slug isn't known yet — lets the user
            // pre-connect a repo from WORKFLOW.md seeding before the first
            // gh refresh has populated metadata.
            conn.execute(
                "INSERT INTO repo (slug, connected, connected_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(slug) DO UPDATE SET
                    connected = excluded.connected,
                    connected_at = CASE
                        WHEN excluded.connected = 1 THEN excluded.connected_at
                        ELSE NULL
                    END",
                rusqlite::params![
                    slug,
                    connected as i64,
                    if connected { Some(Utc::now()) } else { None },
                ],
            )?;
            Ok(())
        })
        .await
    }
}

#[derive(Debug, Clone)]
pub struct DefaultStates {
    pub backlog: WorkflowState,
    pub todo: WorkflowState,
    pub in_progress: WorkflowState,
    pub done: WorkflowState,
    pub canceled: WorkflowState,
}

// ============================================================================
// Row helpers (synchronous, called inside spawn_blocking)
// ============================================================================

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn slugify(title: &str, number: i64) -> String {
    let slug: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let collapsed = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        format!("issue-{number}")
    } else {
        format!("{number}-{collapsed}")
    }
}

fn row_workspace(conn: &Connection, id: &str) -> Result<Workspace, StoreError> {
    conn.query_row(
        "SELECT id, name, url_key, created_at, updated_at FROM workspace WHERE id = ?1",
        [id],
        |row| {
            Ok(Workspace {
                id: row.get(0)?,
                name: row.get(1)?,
                url_key: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_user(conn: &Connection, id: &str) -> Result<User, StoreError> {
    conn.query_row(
        "SELECT id, workspace_id, name, display_name, email, avatar_url, is_active, is_admin
         FROM user WHERE id = ?1",
        [id],
        |row| {
            Ok(User {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                name: row.get(2)?,
                display_name: row.get(3)?,
                email: row.get(4)?,
                avatar_url: row.get(5)?,
                is_active: row.get::<_, i64>(6)? != 0,
                is_admin: row.get::<_, i64>(7)? != 0,
            })
        },
    )
    .map_err(Into::into)
}

fn row_team(conn: &Connection, id: &str) -> Result<Team, StoreError> {
    conn.query_row(
        "SELECT id, workspace_id, key, name, description, icon, color, private,
                cycles_enabled, cycle_duration, timezone
         FROM team WHERE id = ?1",
        [id],
        |row| {
            Ok(Team {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                key: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                icon: row.get(5)?,
                color: row.get(6)?,
                private: row.get::<_, i64>(7)? != 0,
                cycles_enabled: row.get::<_, i64>(8)? != 0,
                cycle_duration: row.get(9)?,
                timezone: row.get(10)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_workflow_state(conn: &Connection, id: &str) -> Result<WorkflowState, StoreError> {
    conn.query_row(
        "SELECT id, team_id, name, type, position, color, description
         FROM workflow_state WHERE id = ?1",
        [id],
        |row| {
            let ty: String = row.get(3)?;
            Ok(WorkflowState {
                id: row.get(0)?,
                team_id: row.get(1)?,
                name: row.get(2)?,
                r#type: WorkflowStateType::parse(&ty).unwrap_or(WorkflowStateType::Backlog),
                position: row.get(4)?,
                color: row.get(5)?,
                description: row.get(6)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_label(conn: &Connection, id: &str) -> Result<Label, StoreError> {
    conn.query_row(
        "SELECT id, workspace_id, team_id, parent_id, name, description, color
         FROM label WHERE id = ?1",
        [id],
        |row| {
            Ok(Label {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                team_id: row.get(2)?,
                parent_id: row.get(3)?,
                name: row.get(4)?,
                description: row.get(5)?,
                color: row.get(6)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_project(conn: &Connection, id: &str) -> Result<Project, StoreError> {
    conn.query_row(
        "SELECT id, workspace_id, name, slug, description, icon, color, state,
                lead_id, start_date, target_date, completed_at, canceled_at, sort_order
         FROM project WHERE id = ?1",
        [id],
        |row| {
            let st: String = row.get(7)?;
            Ok(Project {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                name: row.get(2)?,
                slug: row.get(3)?,
                description: row.get(4)?,
                icon: row.get(5)?,
                color: row.get(6)?,
                state: ProjectState::parse(&st).unwrap_or(ProjectState::Planned),
                lead_id: row.get(8)?,
                start_date: row.get(9)?,
                target_date: row.get(10)?,
                completed_at: row.get(11)?,
                canceled_at: row.get(12)?,
                sort_order: row.get(13)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_cycle(conn: &Connection, id: &str) -> Result<Cycle, StoreError> {
    conn.query_row(
        "SELECT id, team_id, number, name, description, starts_at, ends_at, completed_at
         FROM cycle WHERE id = ?1",
        [id],
        |row| {
            Ok(Cycle {
                id: row.get(0)?,
                team_id: row.get(1)?,
                number: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                starts_at: row.get(5)?,
                ends_at: row.get(6)?,
                completed_at: row.get(7)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_comment(conn: &Connection, id: &str) -> Result<Comment, StoreError> {
    conn.query_row(
        "SELECT id, issue_id, user_id, parent_id, body, edited_at, created_at, updated_at
         FROM comment WHERE id = ?1",
        [id],
        |row| {
            Ok(Comment {
                id: row.get(0)?,
                issue_id: row.get(1)?,
                user_id: row.get(2)?,
                parent_id: row.get(3)?,
                body: row.get(4)?,
                edited_at: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_attachment(conn: &Connection, id: &str) -> Result<Attachment, StoreError> {
    conn.query_row(
        "SELECT id, issue_id, creator_id, title, subtitle, url, source, metadata_json
         FROM attachment WHERE id = ?1",
        [id],
        |row| {
            Ok(Attachment {
                id: row.get(0)?,
                issue_id: row.get(1)?,
                creator_id: row.get(2)?,
                title: row.get(3)?,
                subtitle: row.get(4)?,
                url: row.get(5)?,
                source: row.get(6)?,
                metadata_json: row.get(7)?,
            })
        },
    )
    .map_err(Into::into)
}

fn row_run_attempt(conn: &Connection, id: &str) -> Result<RunAttemptRecord, StoreError> {
    conn.query_row(
        "SELECT id, issue_id, issue_identifier, attempt_no, workspace_path,
                status, error, started_at, ended_at
         FROM run_attempt WHERE id = ?1",
        [id],
        |row| {
            let st: String = row.get(5)?;
            Ok(RunAttemptRecord {
                id: row.get(0)?,
                issue_id: row.get(1)?,
                issue_identifier: row.get(2)?,
                attempt_no: row.get(3)?,
                workspace_path: row.get(4)?,
                status: RunAttemptStatus::parse(&st).unwrap_or(RunAttemptStatus::Pending),
                error: row.get(6)?,
                started_at: row.get(7)?,
                ended_at: row.get(8)?,
            })
        },
    )
    .map_err(Into::into)
}

/// Joined fetch: issue row + state name + team key + labels + blockers.
fn fetch_issue_by_id(conn: &Connection, id: &str) -> Result<Option<IssueRecord>, StoreError> {
    let row = conn
        .query_row(
            "SELECT i.id, i.team_id, t.key, i.number, i.identifier, i.title, i.description,
                    i.priority, i.estimate, i.state_id, s.name, s.type,
                    i.project_id, i.project_milestone_id, i.cycle_id, i.parent_id,
                    i.assignee_id, i.creator_id, i.branch_name, i.url,
                    i.sort_order, i.sub_issue_sort_order, i.due_date,
                    i.started_at, i.completed_at, i.canceled_at, i.archived_at, i.snoozed_until,
                    i.trashed, i.created_at, i.updated_at, i.kind, i.author
             FROM issue i
             JOIN workflow_state s ON s.id = i.state_id
             JOIN team t ON t.id = i.team_id
             WHERE i.id = ?1",
            [id],
            |row| {
                let state_type: String = row.get(11)?;
                Ok(IssueRecord {
                    id: row.get(0)?,
                    team_id: row.get(1)?,
                    team_key: row.get(2)?,
                    number: row.get(3)?,
                    identifier: row.get(4)?,
                    title: row.get(5)?,
                    description: row.get(6)?,
                    priority: row.get(7)?,
                    estimate: row.get(8)?,
                    state_id: row.get(9)?,
                    state_name: row.get(10)?,
                    state_type: WorkflowStateType::parse(&state_type)
                        .unwrap_or(WorkflowStateType::Backlog),
                    project_id: row.get(12)?,
                    project_milestone_id: row.get(13)?,
                    cycle_id: row.get(14)?,
                    parent_id: row.get(15)?,
                    assignee_id: row.get(16)?,
                    creator_id: row.get(17)?,
                    branch_name: row.get(18)?,
                    url: row.get(19)?,
                    sort_order: row.get(20)?,
                    sub_issue_sort_order: row.get(21)?,
                    due_date: row.get(22)?,
                    started_at: row.get(23)?,
                    completed_at: row.get(24)?,
                    canceled_at: row.get(25)?,
                    archived_at: row.get(26)?,
                    snoozed_until: row.get(27)?,
                    trashed: row.get::<_, i64>(28)? != 0,
                    created_at: row.get(29)?,
                    updated_at: row.get(30)?,
                    kind: row.get(31)?,
                    author: row.get(32)?,
                    labels: Vec::new(),
                    blocked_by: Vec::new(),
                })
            },
        )
        .optional()?;

    let Some(mut rec) = row else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT l.name FROM label l
         JOIN issue_label il ON il.label_id = l.id
         WHERE il.issue_id = ?1 ORDER BY l.name",
    )?;
    rec.labels = stmt
        .query_map([id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut stmt = conn.prepare(
        "SELECT i2.id, i2.identifier, s2.name, s2.type
         FROM issue_relation r
         JOIN issue i2 ON i2.id = r.issue_id
         JOIN workflow_state s2 ON s2.id = i2.state_id
         WHERE r.related_issue_id = ?1 AND r.type = 'blocks'",
    )?;
    rec.blocked_by = stmt
        .query_map([id], |row| {
            let ty: String = row.get(3)?;
            Ok(BlockerRef {
                id: row.get(0)?,
                identifier: row.get(1)?,
                state_name: row.get(2)?,
                state_type: WorkflowStateType::parse(&ty).unwrap_or(WorkflowStateType::Backlog),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(rec))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> Store {
        Store::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn migrations_apply_idempotently() {
        let s = fixture().await;
        // Re-run schema apply on the same connection.
        s.run(|conn| {
            schema::apply_all(conn)?;
            schema::apply_all(conn)?;
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn create_workspace_team_states_and_issue() {
        let s = fixture().await;
        let ws = s.create_workspace("Acme", "acme").await.unwrap();
        let team = s.create_team(&ws.id, "ENG", "Engineering").await.unwrap();
        let states = s.seed_default_workflow_states(&team.id).await.unwrap();
        let bug = s
            .create_label(&ws.id, Some(&team.id), "bug", Some("#f00"))
            .await
            .unwrap();

        let issue = s
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.todo.id.clone(),
                title: "Login is broken".to_string(),
                priority: 2,
                label_ids: vec![bug.id.clone()],
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(issue.identifier, "ENG-1");
        assert_eq!(issue.state_name, "Todo");
        assert_eq!(issue.state_type, WorkflowStateType::Unstarted);
        assert_eq!(issue.labels, vec!["bug".to_string()]);
        assert!(issue.branch_name.as_deref().unwrap().starts_with("eng/1-"));
        // Default kind is `issue`; author empty.
        assert_eq!(issue.kind, "issue");
        assert!(issue.author.is_none());

        let by_state = s
            .fetch_issues_by_state_names(&["Todo".to_string()])
            .await
            .unwrap();
        assert_eq!(by_state.len(), 1);
        assert_eq!(by_state[0].id, issue.id);

        let none_for_done = s
            .fetch_issues_by_state_names(&["Done".to_string()])
            .await
            .unwrap();
        assert!(none_for_done.is_empty());

        s.set_issue_state(&issue.id, &states.in_progress.id).await.unwrap();
        let after = s.get_issue(&issue.id).await.unwrap().unwrap();
        assert_eq!(after.state_name, "In Progress");
        assert_eq!(after.state_type, WorkflowStateType::Started);
        assert!(after.started_at.is_some());

        s.set_issue_state(&issue.id, &states.done.id).await.unwrap();
        let after = s.get_issue(&issue.id).await.unwrap().unwrap();
        assert!(after.completed_at.is_some());
    }

    #[tokio::test]
    async fn blockers_are_populated() {
        let s = fixture().await;
        let ws = s.create_workspace("Acme", "acme").await.unwrap();
        let team = s.create_team(&ws.id, "ENG", "Engineering").await.unwrap();
        let states = s.seed_default_workflow_states(&team.id).await.unwrap();
        let blocker = s
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.todo.id.clone(),
                title: "Add DB".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let blocked = s
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.todo.id.clone(),
                title: "Wire reads".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        s.add_relation(&blocker.id, &blocked.id, IssueRelationType::Blocks)
            .await
            .unwrap();
        let blocked_full = s.get_issue(&blocked.id).await.unwrap().unwrap();
        assert_eq!(blocked_full.blocked_by.len(), 1);
        assert_eq!(blocked_full.blocked_by[0].id, blocker.id);
        assert_eq!(blocked_full.blocked_by[0].identifier, "ENG-1");
    }

    #[tokio::test]
    async fn create_pr_review_row_persists_kind_and_author() {
        let s = fixture().await;
        let ws = s.create_workspace("Acme", "acme").await.unwrap();
        let team = s.create_team(&ws.id, "ENG", "Engineering").await.unwrap();
        let states = s.seed_default_workflow_states(&team.id).await.unwrap();

        let pr = s
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.todo.id.clone(),
                title: "Add feature X".into(),
                kind: Some("pr_review".into()),
                author: Some("alice".into()),
                url: Some("https://github.com/o/r/pull/7".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(pr.kind, "pr_review");
        assert_eq!(pr.author.as_deref(), Some("alice"));

        let fetched = s.get_issue(&pr.id).await.unwrap().unwrap();
        assert_eq!(fetched.kind, "pr_review");
        assert_eq!(fetched.author.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn harness_upsert_preserves_concurrency() {
        let s = fixture().await;
        s.upsert_harness_probe("codex", "Codex", "codex", "#10b981", 2, true, Some("0.18.4"), Some(Utc::now()))
            .await
            .unwrap();
        s.set_harness_concurrency("codex", 5).await.unwrap();
        // A subsequent probe must NOT clobber the user's concurrency value.
        s.upsert_harness_probe("codex", "Codex", "codex", "#10b981", 2, true, Some("0.19.0"), Some(Utc::now()))
            .await
            .unwrap();
        let rows = s.list_harnesses().await.unwrap();
        let codex = rows.iter().find(|h| h.id == "codex").unwrap();
        assert_eq!(codex.concurrency, 5);
        assert_eq!(codex.version.as_deref(), Some("0.19.0"));

        // mark_missing flips to unavailable rather than deleting.
        s.mark_missing_harnesses_unavailable(&[]).await.unwrap();
        let rows = s.list_harnesses().await.unwrap();
        let codex = rows.iter().find(|h| h.id == "codex").unwrap();
        assert!(!codex.available);
        assert_eq!(codex.concurrency, 5);
    }

    #[tokio::test]
    async fn repo_upsert_preserves_connect_state() {
        let s = fixture().await;
        let r = RepoRecord {
            slug: "owner/name".into(),
            description: Some("first".into()),
            url: None,
            default_branch: Some("main".into()),
            primary_language: Some("Rust".into()),
            is_private: false,
            is_archived: false,
            updated_at: None,
            connected: false,
            connected_at: None,
            last_synced_at: Some(Utc::now()),
        };
        s.upsert_repo_metadata(&r).await.unwrap();
        s.set_repo_connected("owner/name", true).await.unwrap();
        // Refreshed metadata from gh shouldn't drop the user's connect flag.
        let r2 = RepoRecord {
            description: Some("second".into()),
            ..r.clone()
        };
        s.upsert_repo_metadata(&r2).await.unwrap();
        let rows = s.list_repos().await.unwrap();
        let row = rows.iter().find(|r| r.slug == "owner/name").unwrap();
        assert!(row.connected);
        assert_eq!(row.description.as_deref(), Some("second"));

        let connected = s.list_connected_repo_slugs().await.unwrap();
        assert_eq!(connected, vec!["owner/name".to_string()]);

        s.set_repo_connected("owner/name", false).await.unwrap();
        let rows = s.list_repos().await.unwrap();
        let row = rows.iter().find(|r| r.slug == "owner/name").unwrap();
        assert!(!row.connected);
        assert!(row.connected_at.is_none());
    }

    #[tokio::test]
    async fn run_attempt_session_and_retry_persist() {
        let s = fixture().await;
        let ws = s.create_workspace("Acme", "acme").await.unwrap();
        let team = s.create_team(&ws.id, "ENG", "Engineering").await.unwrap();
        let states = s.seed_default_workflow_states(&team.id).await.unwrap();
        let issue = s
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.todo.id.clone(),
                title: "do thing".into(),
                ..Default::default()
            })
            .await
            .unwrap();

        let run = s
            .create_run_attempt(&issue.id, &issue.identifier, 0, Some("/tmp/ws"))
            .await
            .unwrap();
        assert_eq!(run.status, RunAttemptStatus::Running);

        s.upsert_live_session(LiveSessionRecord {
            run_attempt_id: run.id.clone(),
            session_id: "thread-turn".into(),
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            codex_pid: Some(42),
            last_event: Some("started".into()),
            last_event_at: Some(Utc::now()),
            last_message: None,
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            last_reported_input_tokens: 0,
            last_reported_output_tokens: 0,
            last_reported_total_tokens: 0,
            turn_count: 1,
        })
        .await
        .unwrap();

        s.append_session_event(&run.id, "turn_started", Some(r#"{"ok":true}"#))
            .await
            .unwrap();
        let evs = s.list_session_events(&run.id, 10).await.unwrap();
        assert_eq!(evs.len(), 1);

        s.upsert_retry(RetryEntryRecord {
            issue_id: issue.id.clone(),
            identifier: issue.identifier.clone(),
            attempt: 1,
            due_at_ms: 1000,
            error: Some("boom".into()),
        })
        .await
        .unwrap();
        let retries = s.list_retries().await.unwrap();
        assert_eq!(retries.len(), 1);
        s.delete_retry(&issue.id).await.unwrap();
        assert!(s.list_retries().await.unwrap().is_empty());

        s.finish_run_attempt(&run.id, RunAttemptStatus::Succeeded, None)
            .await
            .unwrap();
    }
}
