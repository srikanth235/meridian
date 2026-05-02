//! [`Tracker`] backed by [`meridian_store::Store`] (SQLite, Linear-shaped).
//!
//! State semantics:
//!   - Configured `active_states` / `terminal_states` are matched against
//!     `workflow_state.name` (case-insensitive). The same string flows back
//!     into [`Issue::state`] so existing classify/orchestration logic keeps
//!     working unchanged.
//!   - `Issue.id` = the row's UUID (`workflow_state` and `team` rows are
//!     joined for derived fields).
//!   - `Issue.repo` is set to the team key (e.g. `ENG`) so the multi-repo
//!     UI grouping treats Linear teams the same way it treated GitHub repos.

use async_trait::async_trait;
use meridian_core::{Blocker, Issue};
use meridian_store::{IssueRecord, Store};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::TrackerError;
use crate::tracker::Tracker;

#[derive(Clone)]
pub struct SqliteTracker {
    store: Arc<Store>,
}

impl SqliteTracker {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub async fn open(path: impl AsRef<std::path::Path>) -> Result<Self, TrackerError> {
        let store = Store::open(path).await?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    pub async fn open_in_memory() -> Result<Self, TrackerError> {
        let store = Store::open_in_memory().await?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    pub fn store(&self) -> Arc<Store> {
        self.store.clone()
    }
}

fn record_to_issue(rec: IssueRecord) -> Issue {
    Issue {
        id: rec.id,
        identifier: rec.identifier,
        title: rec.title,
        description: rec.description.filter(|s| !s.is_empty()),
        priority: Some(rec.priority),
        state: rec.state_name,
        branch_name: rec.branch_name,
        url: rec.url,
        labels: rec.labels,
        blocked_by: rec
            .blocked_by
            .into_iter()
            .map(|b| Blocker {
                id: Some(b.id),
                identifier: Some(b.identifier),
                state: Some(b.state_name),
            })
            .collect(),
        created_at: Some(rec.created_at),
        updated_at: Some(rec.updated_at),
        repo: Some(rec.team_key),
    }
}

#[async_trait]
impl Tracker for SqliteTracker {
    async fn fetch_issues_by_states(
        &self,
        state_names: &[String],
    ) -> Result<Vec<Issue>, TrackerError> {
        let recs = self.store.fetch_issues_by_state_names(state_names).await?;
        Ok(recs.into_iter().map(record_to_issue).collect())
    }

    async fn fetch_issue_states_by_ids(
        &self,
        issue_ids: &[String],
    ) -> Result<HashMap<String, Issue>, TrackerError> {
        let mut out = HashMap::new();
        let recs = self.store.fetch_issues_by_ids(issue_ids).await?;
        for rec in recs {
            let issue = record_to_issue(rec);
            out.insert(issue.id.clone(), issue);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_store::{NewIssue, WorkflowStateType};

    async fn fixture() -> SqliteTracker {
        SqliteTracker::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn fetch_by_state_returns_normalized_issue() {
        let t = fixture().await;
        let store = t.store();
        let ws = store.create_workspace("Acme", "acme").await.unwrap();
        let team = store.create_team(&ws.id, "ENG", "Engineering").await.unwrap();
        let states = store.seed_default_workflow_states(&team.id).await.unwrap();
        let bug = store
            .create_label(&ws.id, Some(&team.id), "bug", None)
            .await
            .unwrap();

        let _i1 = store
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.todo.id.clone(),
                title: "Login broken".into(),
                priority: 2,
                label_ids: vec![bug.id.clone()],
                ..Default::default()
            })
            .await
            .unwrap();
        let _i2 = store
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.done.id.clone(),
                title: "Already shipped".into(),
                ..Default::default()
            })
            .await
            .unwrap();

        let active = t
            .fetch_issues_by_states(&["Todo".into()])
            .await
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].state, "Todo");
        assert_eq!(active[0].labels, vec!["bug".to_string()]);
        assert_eq!(active[0].repo.as_deref(), Some("ENG"));
        assert_eq!(active[0].identifier, "ENG-1");

        // case-insensitive
        let active_lc = t
            .fetch_issues_by_states(&["todo".into()])
            .await
            .unwrap();
        assert_eq!(active_lc.len(), 1);
    }

    #[tokio::test]
    async fn fetch_by_ids_returns_map() {
        let t = fixture().await;
        let store = t.store();
        let ws = store.create_workspace("Acme", "acme").await.unwrap();
        let team = store.create_team(&ws.id, "ENG", "Engineering").await.unwrap();
        let states = store.seed_default_workflow_states(&team.id).await.unwrap();
        let issue = store
            .create_issue(NewIssue {
                team_id: team.id.clone(),
                state_id: states.todo.id.clone(),
                title: "x".into(),
                ..Default::default()
            })
            .await
            .unwrap();

        let map = t
            .fetch_issue_states_by_ids(&[issue.id.clone(), "missing".into()])
            .await
            .unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&issue.id));
        assert_eq!(map[&issue.id].state, "Todo");
    }

    #[tokio::test]
    async fn workflow_state_type_helpers() {
        // Sanity: the helpers our config preflight will rely on.
        assert!(WorkflowStateType::Started.is_active());
        assert!(WorkflowStateType::Unstarted.is_active());
        assert!(WorkflowStateType::Completed.is_terminal());
        assert!(WorkflowStateType::Canceled.is_terminal());
        assert!(!WorkflowStateType::Backlog.is_active());
    }
}
