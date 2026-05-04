//! Schema migrations.
//!
//! Each migration is `(version, sql)` where `sql` is a single transactional
//! script. Migrations are applied in order and recorded in `schema_migration`.

use rusqlite::Connection;

use crate::error::StoreError;

/// Bumped whenever a migration is appended below.
pub const CURRENT_VERSION: u32 = 5;

/// All migrations, in order. Append-only.
pub const MIGRATIONS: &[(u32, &str)] = &[
    (1, MIGRATION_001_INITIAL),
    (2, MIGRATION_002_ISSUE_KIND),
    (3, MIGRATION_003_HARNESS_AND_REPO),
    (4, MIGRATION_004_AUTOMATIONS),
    (5, MIGRATION_005_PAGES),
];

pub fn apply_all(conn: &mut Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;
         CREATE TABLE IF NOT EXISTS schema_migration (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         );",
    )?;

    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migration",
            [],
            |row| row.get::<_, i64>(0).map(|v| v as u32),
        )
        .unwrap_or(0);

    for (version, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }
        let tx = conn.transaction().map_err(|e| StoreError::Migration {
            version: *version,
            source: e,
        })?;
        tx.execute_batch(sql).map_err(|e| StoreError::Migration {
            version: *version,
            source: e,
        })?;
        tx.execute(
            "INSERT INTO schema_migration (version) VALUES (?1)",
            [*version],
        )
        .map_err(|e| StoreError::Migration {
            version: *version,
            source: e,
        })?;
        tx.commit().map_err(|e| StoreError::Migration {
            version: *version,
            source: e,
        })?;
        tracing::info!(%version, "applied store migration");
    }
    Ok(())
}

const MIGRATION_001_INITIAL: &str = r#"
-- ============================================================================
-- Linear domain
-- ============================================================================

CREATE TABLE workspace (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    url_key     TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE user (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    display_name  TEXT,
    email         TEXT,
    avatar_url    TEXT,
    is_active     INTEGER NOT NULL DEFAULT 1,
    is_admin      INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_user_workspace ON user(workspace_id);

CREATE TABLE team (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    key             TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    icon            TEXT,
    color           TEXT,
    private         INTEGER NOT NULL DEFAULT 0,
    cycles_enabled  INTEGER NOT NULL DEFAULT 0,
    cycle_duration  INTEGER,
    issue_count     INTEGER NOT NULL DEFAULT 0,
    timezone        TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(workspace_id, key)
);
CREATE INDEX idx_team_workspace ON team(workspace_id);

CREATE TABLE workflow_state (
    id           TEXT PRIMARY KEY,
    team_id      TEXT NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    type         TEXT NOT NULL CHECK (type IN ('triage','backlog','unstarted','started','completed','canceled')),
    position     REAL NOT NULL,
    color        TEXT,
    description  TEXT,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(team_id, name)
);
CREATE INDEX idx_workflow_state_team_type ON workflow_state(team_id, type);

CREATE TABLE project (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    slug          TEXT,
    description   TEXT,
    icon          TEXT,
    color         TEXT,
    state         TEXT NOT NULL DEFAULT 'planned'
                  CHECK (state IN ('planned','started','paused','completed','canceled')),
    lead_id       TEXT REFERENCES user(id) ON DELETE SET NULL,
    start_date    TEXT,
    target_date   TEXT,
    completed_at  TEXT,
    canceled_at   TEXT,
    sort_order    REAL NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_project_workspace ON project(workspace_id);

CREATE TABLE project_team (
    project_id  TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    team_id     TEXT NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    PRIMARY KEY (project_id, team_id)
);

CREATE TABLE project_milestone (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    description  TEXT,
    target_date  TEXT,
    sort_order   REAL NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE cycle (
    id            TEXT PRIMARY KEY,
    team_id       TEXT NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    number        INTEGER NOT NULL,
    name          TEXT,
    description   TEXT,
    starts_at     TEXT NOT NULL,
    ends_at       TEXT NOT NULL,
    completed_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(team_id, number)
);

CREATE TABLE label (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    team_id       TEXT REFERENCES team(id) ON DELETE CASCADE,
    parent_id     TEXT REFERENCES label(id) ON DELETE SET NULL,
    name          TEXT NOT NULL,
    description   TEXT,
    color         TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_label_team ON label(team_id);
CREATE INDEX idx_label_workspace ON label(workspace_id);

CREATE TABLE issue (
    id                    TEXT PRIMARY KEY,
    team_id               TEXT NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    number                INTEGER NOT NULL,
    identifier            TEXT NOT NULL,
    title                 TEXT NOT NULL,
    description           TEXT,
    priority              INTEGER NOT NULL DEFAULT 0,
    estimate              REAL,
    state_id              TEXT NOT NULL REFERENCES workflow_state(id),
    project_id            TEXT REFERENCES project(id) ON DELETE SET NULL,
    project_milestone_id  TEXT REFERENCES project_milestone(id) ON DELETE SET NULL,
    cycle_id              TEXT REFERENCES cycle(id) ON DELETE SET NULL,
    parent_id             TEXT REFERENCES issue(id) ON DELETE SET NULL,
    assignee_id           TEXT REFERENCES user(id) ON DELETE SET NULL,
    creator_id            TEXT REFERENCES user(id) ON DELETE SET NULL,
    branch_name           TEXT,
    url                   TEXT,
    sort_order            REAL NOT NULL DEFAULT 0,
    sub_issue_sort_order  REAL,
    due_date              TEXT,
    started_at            TEXT,
    completed_at          TEXT,
    canceled_at           TEXT,
    archived_at           TEXT,
    snoozed_until         TEXT,
    trashed               INTEGER NOT NULL DEFAULT 0,
    created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(team_id, number),
    UNIQUE(identifier)
);
CREATE INDEX idx_issue_state    ON issue(state_id);
CREATE INDEX idx_issue_team     ON issue(team_id);
CREATE INDEX idx_issue_project  ON issue(project_id);
CREATE INDEX idx_issue_cycle    ON issue(cycle_id);
CREATE INDEX idx_issue_parent   ON issue(parent_id);
CREATE INDEX idx_issue_assignee ON issue(assignee_id);

CREATE TABLE issue_label (
    issue_id  TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    label_id  TEXT NOT NULL REFERENCES label(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id, label_id)
);
CREATE INDEX idx_issue_label_label ON issue_label(label_id);

-- Directional: row (issue_id=A, related_issue_id=B, type='blocks') means A blocks B.
CREATE TABLE issue_relation (
    id                TEXT PRIMARY KEY,
    issue_id          TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    related_issue_id  TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    type              TEXT NOT NULL CHECK (type IN ('blocks','duplicate','related')),
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(issue_id, related_issue_id, type)
);
CREATE INDEX idx_issue_rel_related ON issue_relation(related_issue_id);

CREATE TABLE issue_subscriber (
    issue_id  TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    user_id   TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id, user_id)
);

CREATE TABLE comment (
    id          TEXT PRIMARY KEY,
    issue_id    TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    user_id     TEXT REFERENCES user(id) ON DELETE SET NULL,
    parent_id   TEXT REFERENCES comment(id) ON DELETE CASCADE,
    body        TEXT NOT NULL,
    edited_at   TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_comment_issue ON comment(issue_id);

CREATE TABLE comment_reaction (
    id          TEXT PRIMARY KEY,
    comment_id  TEXT NOT NULL REFERENCES comment(id) ON DELETE CASCADE,
    user_id     TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    emoji       TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(comment_id, user_id, emoji)
);

CREATE TABLE attachment (
    id             TEXT PRIMARY KEY,
    issue_id       TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    creator_id     TEXT REFERENCES user(id) ON DELETE SET NULL,
    title          TEXT NOT NULL,
    subtitle       TEXT,
    url            TEXT NOT NULL,
    source         TEXT,
    metadata_json  TEXT,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_attachment_issue ON attachment(issue_id);

CREATE TABLE issue_history (
    id             TEXT PRIMARY KEY,
    issue_id       TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    actor_id       TEXT REFERENCES user(id) ON DELETE SET NULL,
    from_state_id  TEXT REFERENCES workflow_state(id),
    to_state_id    TEXT REFERENCES workflow_state(id),
    changes_json   TEXT,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_issue_history_issue ON issue_history(issue_id);

-- ============================================================================
-- Symphony runtime / progress (spec §4.1.5–§4.1.8 — persisted instead of in-mem)
-- ============================================================================

CREATE TABLE run_attempt (
    id                TEXT PRIMARY KEY,
    issue_id          TEXT NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    issue_identifier  TEXT NOT NULL,
    attempt_no        INTEGER NOT NULL,
    workspace_path    TEXT,
    status            TEXT NOT NULL CHECK (status IN ('pending','running','succeeded','failed','timeout','canceled')),
    error             TEXT,
    started_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    ended_at          TEXT
);
CREATE INDEX idx_run_attempt_issue ON run_attempt(issue_id);
CREATE INDEX idx_run_attempt_status ON run_attempt(status);

CREATE TABLE live_session (
    run_attempt_id              TEXT PRIMARY KEY REFERENCES run_attempt(id) ON DELETE CASCADE,
    session_id                  TEXT NOT NULL UNIQUE,
    thread_id                   TEXT NOT NULL,
    turn_id                     TEXT NOT NULL,
    codex_pid                   INTEGER,
    last_event                  TEXT,
    last_event_at               TEXT,
    last_message                TEXT,
    input_tokens                INTEGER NOT NULL DEFAULT 0,
    output_tokens               INTEGER NOT NULL DEFAULT 0,
    total_tokens                INTEGER NOT NULL DEFAULT 0,
    last_reported_input_tokens  INTEGER NOT NULL DEFAULT 0,
    last_reported_output_tokens INTEGER NOT NULL DEFAULT 0,
    last_reported_total_tokens  INTEGER NOT NULL DEFAULT 0,
    turn_count                  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE retry_entry (
    issue_id    TEXT PRIMARY KEY REFERENCES issue(id) ON DELETE CASCADE,
    identifier  TEXT NOT NULL,
    attempt     INTEGER NOT NULL,
    due_at_ms   INTEGER NOT NULL,
    error       TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE session_event (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_attempt_id  TEXT NOT NULL REFERENCES run_attempt(id) ON DELETE CASCADE,
    ts              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    event_type      TEXT NOT NULL,
    payload_json    TEXT
);
CREATE INDEX idx_session_event_attempt ON session_event(run_attempt_id, ts);
"#;

/// Adds `kind` and `author` columns to `issue` so PR-review tasks can be
/// stored alongside regular issues. `kind` is one of {"issue","pr_review"};
/// `author` carries the upstream PR author login (NULL for issues).
const MIGRATION_002_ISSUE_KIND: &str = r#"
ALTER TABLE issue ADD COLUMN kind   TEXT NOT NULL DEFAULT 'issue';
ALTER TABLE issue ADD COLUMN author TEXT;
CREATE INDEX idx_issue_kind ON issue(kind);
"#;

/// Local desktop state: detected harnesses + GitHub repos discoverable via gh.
/// `harness` rows persist across uninstalls (so settings like concurrency
/// survive a reinstall) — `available` flips to 0 when the binary disappears.
/// `repo` rows are upserted from `gh repo list`; `connected` is the user
/// toggle and is preserved across refreshes.
const MIGRATION_003_HARNESS_AND_REPO: &str = r#"
CREATE TABLE harness (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    binary        TEXT NOT NULL,
    color         TEXT NOT NULL,
    concurrency   INTEGER NOT NULL DEFAULT 2,
    available     INTEGER NOT NULL DEFAULT 0,
    version       TEXT,
    last_seen_at  TEXT
);

CREATE TABLE repo (
    slug              TEXT PRIMARY KEY,
    description       TEXT,
    url               TEXT,
    default_branch    TEXT,
    primary_language  TEXT,
    is_private        INTEGER NOT NULL DEFAULT 0,
    is_archived       INTEGER NOT NULL DEFAULT 0,
    updated_at        TEXT,
    connected         INTEGER NOT NULL DEFAULT 0,
    connected_at      TEXT,
    last_synced_at    TEXT
);
CREATE INDEX idx_repo_connected ON repo(connected);
"#;

/// Automations: filesystem-watched scripts under `<workflow_dir>/automations/`.
/// `automation` rows track loaded scripts; `automation_run` is the run
/// history; `automation_seen_key` deduplicates SDK side-effects per
/// automation. `inbox_entry` holds custom inbox surface area used by
/// automations (NL request specs, error surfacing) — distinct from issues.
const MIGRATION_004_AUTOMATIONS: &str = r#"
CREATE TABLE automation (
    id             TEXT PRIMARY KEY,
    file_path      TEXT NOT NULL UNIQUE,
    name           TEXT NOT NULL,
    schedule_json  TEXT NOT NULL,
    enabled        INTEGER NOT NULL DEFAULT 1,
    last_run_at    TEXT,
    next_run_at    TEXT,
    running_since  TEXT,
    last_error     TEXT,
    source_hash    TEXT,
    failure_count  INTEGER NOT NULL DEFAULT 0,
    parse_error    TEXT,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_automation_next ON automation(next_run_at);

CREATE TABLE automation_run (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    automation_id  TEXT NOT NULL REFERENCES automation(id) ON DELETE CASCADE,
    started_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    ended_at       TEXT,
    status         TEXT NOT NULL CHECK (status IN ('running','succeeded','failed')),
    dry_run        INTEGER NOT NULL DEFAULT 0,
    error          TEXT,
    log            TEXT
);
CREATE INDEX idx_automation_run_aid ON automation_run(automation_id, started_at DESC);

CREATE TABLE automation_seen_key (
    automation_id  TEXT NOT NULL REFERENCES automation(id) ON DELETE CASCADE,
    dedup_key      TEXT NOT NULL,
    seen_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (automation_id, dedup_key)
);
CREATE INDEX idx_automation_seen_key_seen_at ON automation_seen_key(seen_at);

CREATE TABLE inbox_entry (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL,
    title         TEXT NOT NULL,
    body          TEXT,
    url           TEXT,
    tags_json     TEXT,
    source        TEXT,
    dedup_key     TEXT,
    dismissed_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_inbox_entry_kind    ON inbox_entry(kind, created_at DESC);
CREATE UNIQUE INDEX idx_inbox_entry_dedup
    ON inbox_entry(source, dedup_key)
    WHERE source IS NOT NULL AND dedup_key IS NOT NULL;
"#;

/// Pages: filesystem-watched LLM-authored TSX modules under
/// `<workflow_dir>/pages/<slug>/{page.tsx,meta.toml}`. The slug (folder name)
/// is the immutable identity; the title is mutable. The registry is a thin
/// shadow of disk — disk is the source of truth, just like automations.
const MIGRATION_005_PAGES: &str = r#"
CREATE TABLE page (
    slug          TEXT PRIMARY KEY,
    folder_path   TEXT NOT NULL UNIQUE,
    title         TEXT NOT NULL,
    icon          TEXT,
    position      INTEGER NOT NULL DEFAULT 100,
    meta_version  INTEGER NOT NULL DEFAULT 1,
    parse_error   TEXT,
    last_error    TEXT,
    last_opened_at TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_page_position ON page(position, title);
"#;
