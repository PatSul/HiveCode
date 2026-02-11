//! Agent Persistence Service — save and restore agent state snapshots to disk.
//!
//! Each agent's state is serialized to a JSON file in a configurable directory.
//! This enables crash recovery, agent migration, and historical audit trails.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use hive_core::config::HiveConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// A completed task record for an agent's history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletedTask {
    pub task_id: String,
    pub description: String,
    pub result: String,
    pub duration_secs: u64,
    pub cost: f64,
}

/// A point-in-time snapshot of an agent's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub context: Vec<String>,
    pub task_history: Vec<CompletedTask>,
    pub personality_traits: Vec<String>,
    pub saved_at: DateTime<Utc>,
}

impl AgentSnapshot {
    /// Create a new snapshot with the current timestamp.
    pub fn new(agent_id: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            role: role.into(),
            status: "idle".into(),
            context: Vec::new(),
            task_history: Vec::new(),
            personality_traits: Vec::new(),
            saved_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence Service
// ---------------------------------------------------------------------------

/// File-based persistence service for agent state snapshots.
///
/// Stores each agent as `{save_dir}/{agent_id}.json`. The directory is
/// created on first write if it does not exist.
pub struct AgentPersistenceService {
    save_dir: PathBuf,
}

impl AgentPersistenceService {
    /// Create a new persistence service using `~/.hive/agents/` as the
    /// default save directory.
    pub fn new() -> Result<Self> {
        let base = HiveConfig::base_dir()?.join("agents");
        Ok(Self { save_dir: base })
    }

    /// Create a persistence service rooted at a specific directory.
    /// Useful for testing or custom deployments.
    pub fn new_at(dir: PathBuf) -> Self {
        Self { save_dir: dir }
    }

    /// Return the file path for a given agent ID.
    fn snapshot_path(&self, agent_id: &str) -> PathBuf {
        self.save_dir.join(format!("{}.json", agent_id))
    }

    /// Ensure the save directory exists.
    fn ensure_dir(&self) -> Result<()> {
        if !self.save_dir.exists() {
            std::fs::create_dir_all(&self.save_dir).with_context(|| {
                format!(
                    "Failed to create agent snapshot directory: {}",
                    self.save_dir.display()
                )
            })?;
        }
        Ok(())
    }

    /// Save an agent snapshot to disk.
    pub fn save_snapshot(&self, snapshot: &AgentSnapshot) -> Result<()> {
        self.ensure_dir()?;
        let path = self.snapshot_path(&snapshot.agent_id);
        let content = serde_json::to_string_pretty(snapshot)
            .context("Failed to serialize agent snapshot")?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write snapshot: {}", path.display()))?;
        Ok(())
    }

    /// Load an agent snapshot from disk.
    pub fn load_snapshot(&self, agent_id: &str) -> Result<AgentSnapshot> {
        let path = self.snapshot_path(agent_id);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read snapshot: {}", path.display()))?;
        let snapshot: AgentSnapshot = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse snapshot: {}", path.display()))?;
        Ok(snapshot)
    }

    /// List all saved agent IDs (without the `.json` extension).
    pub fn list_snapshots(&self) -> Result<Vec<String>> {
        if !self.save_dir.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&self.save_dir)
            .with_context(|| format!("Failed to list snapshots in: {}", self.save_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem() {
                    ids.push(stem.to_string_lossy().into_owned());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Delete a single agent snapshot.
    pub fn delete_snapshot(&self, agent_id: &str) -> Result<()> {
        let path = self.snapshot_path(agent_id);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to delete snapshot: {}", path.display()))?;
        }
        Ok(())
    }

    /// Remove snapshots older than `max_age_days`. Returns the number of
    /// snapshots deleted.
    pub fn cleanup_old(&self, max_age_days: u64) -> Result<usize> {
        if !self.save_dir.exists() {
            return Ok(0);
        }

        let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);
        let mut removed = 0;

        for entry in std::fs::read_dir(&self.save_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.extension().is_some_and(|ext| ext == "json") {
                continue;
            }

            // Try to read and parse; skip on failure.
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let snapshot: AgentSnapshot = match serde_json::from_str(&content) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if snapshot.saved_at < cutoff {
                std::fs::remove_file(&path)?;
                removed += 1;
            }
        }

        Ok(removed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_service(dir: &TempDir) -> AgentPersistenceService {
        AgentPersistenceService::new_at(dir.path().to_path_buf())
    }

    fn make_snapshot(id: &str) -> AgentSnapshot {
        AgentSnapshot {
            agent_id: id.into(),
            role: "coder".into(),
            status: "active".into(),
            context: vec!["msg1".into(), "msg2".into()],
            task_history: vec![CompletedTask {
                task_id: "t1".into(),
                description: "Write tests".into(),
                result: "pass".into(),
                duration_secs: 120,
                cost: 0.05,
            }],
            personality_traits: vec!["thorough".into(), "pragmatic".into()],
            saved_at: Utc::now(),
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let snap = make_snapshot("agent-001");

        svc.save_snapshot(&snap).unwrap();
        let loaded = svc.load_snapshot("agent-001").unwrap();

        assert_eq!(loaded.agent_id, "agent-001");
        assert_eq!(loaded.role, "coder");
        assert_eq!(loaded.status, "active");
        assert_eq!(loaded.context, vec!["msg1", "msg2"]);
        assert_eq!(loaded.task_history.len(), 1);
        assert_eq!(loaded.task_history[0].task_id, "t1");
        assert_eq!(loaded.personality_traits, vec!["thorough", "pragmatic"]);
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        let result = svc.load_snapshot("does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn list_snapshots_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        let ids = svc.list_snapshots().unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn list_snapshots_returns_sorted_ids() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        svc.save_snapshot(&make_snapshot("charlie")).unwrap();
        svc.save_snapshot(&make_snapshot("alpha")).unwrap();
        svc.save_snapshot(&make_snapshot("bravo")).unwrap();

        let ids = svc.list_snapshots().unwrap();
        assert_eq!(ids, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn delete_snapshot_removes_file() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        svc.save_snapshot(&make_snapshot("agent-x")).unwrap();
        assert_eq!(svc.list_snapshots().unwrap().len(), 1);

        svc.delete_snapshot("agent-x").unwrap();
        assert!(svc.list_snapshots().unwrap().is_empty());
    }

    #[test]
    fn delete_nonexistent_snapshot_is_ok() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        // Should not error when the file does not exist.
        let result = svc.delete_snapshot("ghost");
        assert!(result.is_ok());
    }

    #[test]
    fn save_overwrites_existing_snapshot() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        let mut snap = make_snapshot("agent-1");
        snap.status = "idle".into();
        svc.save_snapshot(&snap).unwrap();

        snap.status = "working".into();
        svc.save_snapshot(&snap).unwrap();

        let loaded = svc.load_snapshot("agent-1").unwrap();
        assert_eq!(loaded.status, "working");
    }

    #[test]
    fn cleanup_old_removes_expired_snapshots() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        // Create a snapshot with a very old saved_at timestamp.
        let mut old_snap = make_snapshot("old-agent");
        old_snap.saved_at = Utc::now() - chrono::Duration::days(100);
        svc.save_snapshot(&old_snap).unwrap();

        // Create a recent snapshot.
        let recent_snap = make_snapshot("new-agent");
        svc.save_snapshot(&recent_snap).unwrap();

        let removed = svc.cleanup_old(30).unwrap();
        assert_eq!(removed, 1);

        let remaining = svc.list_snapshots().unwrap();
        assert_eq!(remaining, vec!["new-agent"]);
    }

    #[test]
    fn snapshot_new_helper() {
        let snap = AgentSnapshot::new("test-agent", "architect");
        assert_eq!(snap.agent_id, "test-agent");
        assert_eq!(snap.role, "architect");
        assert_eq!(snap.status, "idle");
        assert!(snap.context.is_empty());
        assert!(snap.task_history.is_empty());
        assert!(snap.personality_traits.is_empty());
    }

    #[test]
    fn completed_task_serde_round_trip() {
        let task = CompletedTask {
            task_id: "task-42".into(),
            description: "Implement feature X".into(),
            result: "success".into(),
            duration_secs: 300,
            cost: 1.25,
        };
        let json = serde_json::to_string(&task).unwrap();
        let parsed: CompletedTask = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, task);
    }

    #[test]
    fn snapshot_serde_round_trip() {
        let snap = make_snapshot("serde-test");
        let json = serde_json::to_string_pretty(&snap).unwrap();
        let parsed: AgentSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "serde-test");
        assert_eq!(parsed.role, "coder");
        assert_eq!(parsed.task_history.len(), 1);
        assert_eq!(parsed.personality_traits.len(), 2);
    }

    #[test]
    fn list_snapshots_ignores_non_json_files() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);

        // Save a real snapshot.
        svc.save_snapshot(&make_snapshot("valid")).unwrap();

        // Write a non-JSON file into the same directory.
        std::fs::write(tmp.path().join("readme.txt"), "not a snapshot").unwrap();
        std::fs::write(tmp.path().join("data.csv"), "a,b,c").unwrap();

        let ids = svc.list_snapshots().unwrap();
        assert_eq!(ids, vec!["valid"]);
    }
}
