//! Standup Service — generate daily standup reports from agent snapshots.
//!
//! Produces structured standup summaries with per-agent "yesterday / today /
//! blockers" reports, modeled after the classic agile daily standup format.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::persistence::AgentSnapshot;

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// A per-agent report within a daily standup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReport {
    pub agent_id: String,
    pub role: String,
    pub completed_yesterday: Vec<String>,
    pub working_on_today: Vec<String>,
    pub blockers: Vec<String>,
}

/// A daily standup containing reports from multiple agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStandup {
    pub id: String,
    pub date: DateTime<Utc>,
    pub agent_reports: Vec<AgentReport>,
}

impl DailyStandup {
    /// Return the total number of completed items across all agents.
    pub fn total_completed(&self) -> usize {
        self.agent_reports
            .iter()
            .map(|r| r.completed_yesterday.len())
            .sum()
    }

    /// Return the total number of blockers across all agents.
    pub fn total_blockers(&self) -> usize {
        self.agent_reports.iter().map(|r| r.blockers.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Standup Service
// ---------------------------------------------------------------------------

/// In-memory standup history and generation service.
pub struct StandupService {
    standups: Vec<DailyStandup>,
}

impl StandupService {
    /// Create a new standup service with no history.
    pub fn new() -> Self {
        Self {
            standups: Vec::new(),
        }
    }

    /// Generate a daily standup from a set of agent snapshots.
    ///
    /// Each snapshot's `task_history` contributes to "completed yesterday",
    /// and any context messages are treated as "working on today". Agents
    /// with a status of "blocked" will have their latest context entry
    /// listed as a blocker.
    ///
    /// The generated standup is appended to the internal history.
    pub fn generate_standup(&mut self, snapshots: &[AgentSnapshot]) -> DailyStandup {
        let mut reports = Vec::new();

        for snapshot in snapshots {
            let completed: Vec<String> = snapshot
                .task_history
                .iter()
                .map(|t| format!("{}: {}", t.task_id, t.description))
                .collect();

            let working_on: Vec<String> = if snapshot.context.is_empty() {
                vec!["No active tasks".into()]
            } else {
                snapshot.context.clone()
            };

            let blockers: Vec<String> = if snapshot.status == "blocked" {
                snapshot
                    .context
                    .last()
                    .map(|c| vec![c.clone()])
                    .unwrap_or_else(|| vec!["Blocked (no details)".into()])
            } else {
                Vec::new()
            };

            reports.push(AgentReport {
                agent_id: snapshot.agent_id.clone(),
                role: snapshot.role.clone(),
                completed_yesterday: completed,
                working_on_today: working_on,
                blockers,
            });
        }

        let standup = DailyStandup {
            id: Uuid::new_v4().to_string(),
            date: Utc::now(),
            agent_reports: reports,
        };

        self.standups.push(standup.clone());
        standup
    }

    /// Return the most recent `limit` standups (newest first).
    pub fn list_standups(&self, limit: usize) -> &[DailyStandup] {
        let start = self.standups.len().saturating_sub(limit);
        &self.standups[start..]
    }

    /// Look up a standup by its ID.
    pub fn get_standup(&self, id: &str) -> Option<&DailyStandup> {
        self.standups.iter().find(|s| s.id == id)
    }

    /// Return the total number of standups in history.
    pub fn count(&self) -> usize {
        self.standups.len()
    }
}

impl Default for StandupService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::CompletedTask;

    fn make_snapshot(id: &str, role: &str, status: &str) -> AgentSnapshot {
        AgentSnapshot {
            agent_id: id.into(),
            role: role.into(),
            status: status.into(),
            context: vec!["Working on feature X".into()],
            task_history: vec![CompletedTask {
                task_id: "t1".into(),
                description: "Implement module A".into(),
                result: "success".into(),
                duration_secs: 60,
                cost: 0.02,
            }],
            personality_traits: vec!["diligent".into()],
            saved_at: Utc::now(),
        }
    }

    #[test]
    fn new_service_has_no_standups() {
        let svc = StandupService::new();
        assert_eq!(svc.count(), 0);
        assert!(svc.list_standups(10).is_empty());
    }

    #[test]
    fn generate_standup_from_single_agent() {
        let mut svc = StandupService::new();
        let snapshots = vec![make_snapshot("agent-1", "coder", "active")];

        let standup = svc.generate_standup(&snapshots);

        assert_eq!(standup.agent_reports.len(), 1);
        assert_eq!(standup.agent_reports[0].agent_id, "agent-1");
        assert_eq!(standup.agent_reports[0].role, "coder");
        assert_eq!(standup.agent_reports[0].completed_yesterday.len(), 1);
        assert!(standup.agent_reports[0].blockers.is_empty());
        assert!(!standup.id.is_empty());
    }

    #[test]
    fn generate_standup_from_multiple_agents() {
        let mut svc = StandupService::new();
        let snapshots = vec![
            make_snapshot("agent-a", "architect", "active"),
            make_snapshot("agent-b", "coder", "active"),
            make_snapshot("agent-c", "tester", "active"),
        ];

        let standup = svc.generate_standup(&snapshots);

        assert_eq!(standup.agent_reports.len(), 3);
        assert_eq!(standup.total_completed(), 3);
        assert_eq!(standup.total_blockers(), 0);
    }

    #[test]
    fn blocked_agent_reports_blockers() {
        let mut svc = StandupService::new();
        let snapshots = vec![make_snapshot("agent-blocked", "debugger", "blocked")];

        let standup = svc.generate_standup(&snapshots);

        assert_eq!(standup.agent_reports[0].blockers.len(), 1);
        assert_eq!(standup.agent_reports[0].blockers[0], "Working on feature X");
    }

    #[test]
    fn agent_with_empty_context_gets_default_working_on() {
        let mut svc = StandupService::new();
        let snap = AgentSnapshot {
            agent_id: "empty-ctx".into(),
            role: "reviewer".into(),
            status: "idle".into(),
            context: Vec::new(),
            task_history: Vec::new(),
            personality_traits: Vec::new(),
            saved_at: Utc::now(),
        };

        let standup = svc.generate_standup(&[snap]);

        assert_eq!(
            standup.agent_reports[0].working_on_today,
            vec!["No active tasks"]
        );
        assert!(standup.agent_reports[0].completed_yesterday.is_empty());
    }

    #[test]
    fn list_standups_respects_limit() {
        let mut svc = StandupService::new();
        let snap = vec![make_snapshot("a", "coder", "active")];

        for _ in 0..5 {
            svc.generate_standup(&snap);
        }

        assert_eq!(svc.count(), 5);
        assert_eq!(svc.list_standups(3).len(), 3);
        assert_eq!(svc.list_standups(10).len(), 5);
        assert_eq!(svc.list_standups(0).len(), 0);
    }

    #[test]
    fn get_standup_by_id() {
        let mut svc = StandupService::new();
        let snap = vec![make_snapshot("a", "coder", "active")];

        let standup = svc.generate_standup(&snap);
        let id = standup.id.clone();

        let found = svc.get_standup(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id);
    }

    #[test]
    fn get_standup_returns_none_for_unknown_id() {
        let svc = StandupService::new();
        assert!(svc.get_standup("nonexistent").is_none());
    }

    #[test]
    fn standup_serde_round_trip() {
        let mut svc = StandupService::new();
        let snapshots = vec![
            make_snapshot("agent-1", "coder", "active"),
            make_snapshot("agent-2", "tester", "blocked"),
        ];

        let standup = svc.generate_standup(&snapshots);
        let json = serde_json::to_string_pretty(&standup).unwrap();
        let parsed: DailyStandup = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, standup.id);
        assert_eq!(parsed.agent_reports.len(), 2);
        assert_eq!(parsed.agent_reports[0].agent_id, "agent-1");
        assert_eq!(parsed.agent_reports[1].agent_id, "agent-2");
    }

    #[test]
    fn generate_standup_increments_count() {
        let mut svc = StandupService::new();
        let snap = vec![make_snapshot("a", "coder", "active")];

        assert_eq!(svc.count(), 0);
        svc.generate_standup(&snap);
        assert_eq!(svc.count(), 1);
        svc.generate_standup(&snap);
        assert_eq!(svc.count(), 2);
    }

    #[test]
    fn blocked_agent_without_context_gets_default_blocker() {
        let mut svc = StandupService::new();
        let snap = AgentSnapshot {
            agent_id: "blocked-no-ctx".into(),
            role: "security".into(),
            status: "blocked".into(),
            context: Vec::new(),
            task_history: Vec::new(),
            personality_traits: Vec::new(),
            saved_at: Utc::now(),
        };

        let standup = svc.generate_standup(&[snap]);
        assert_eq!(
            standup.agent_reports[0].blockers,
            vec!["Blocked (no details)"]
        );
    }

    #[test]
    fn daily_standup_total_helpers() {
        let standup = DailyStandup {
            id: "test".into(),
            date: Utc::now(),
            agent_reports: vec![
                AgentReport {
                    agent_id: "a".into(),
                    role: "coder".into(),
                    completed_yesterday: vec!["item1".into(), "item2".into()],
                    working_on_today: vec!["stuff".into()],
                    blockers: vec!["blocker1".into()],
                },
                AgentReport {
                    agent_id: "b".into(),
                    role: "tester".into(),
                    completed_yesterday: vec!["item3".into()],
                    working_on_today: vec![],
                    blockers: vec![],
                },
            ],
        };

        assert_eq!(standup.total_completed(), 3);
        assert_eq!(standup.total_blockers(), 1);
    }
}
