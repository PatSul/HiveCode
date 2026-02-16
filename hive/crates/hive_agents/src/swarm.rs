//! Swarm types -- shared data structures for multi-team swarm orchestration.
//!
//! These types define how the Queen meta-coordinator plans, dispatches, and
//! tracks teams of agents executing toward a common goal.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::coordinator::CoordinatorResult;
use crate::hivemind::OrchestrationResult;

// ---------------------------------------------------------------------------
// Orchestration Mode
// ---------------------------------------------------------------------------

/// How a team should be orchestrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationMode {
    /// Use the full HiveMind multi-agent pipeline (architect, coder, reviewer, etc.)
    HiveMind,
    /// Use the Coordinator with dependency-ordered task dispatch.
    Coordinator,
    /// Use the provider's native multi-agent capability.
    NativeProvider,
    /// A single AI call -- simplest and cheapest.
    SingleShot,
}

impl OrchestrationMode {
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().replace('-', "_").as_str() {
            "hivemind" | "hive_mind" => Self::HiveMind,
            "coordinator" => Self::Coordinator,
            "native_provider" | "native" => Self::NativeProvider,
            "single_shot" | "singleshot" | "single" => Self::SingleShot,
            _ => Self::SingleShot,
        }
    }
}

// ---------------------------------------------------------------------------
// Team Objective
// ---------------------------------------------------------------------------

/// A single team's objective within the swarm plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamObjective {
    /// Unique identifier (e.g. "team-1").
    pub id: String,
    /// Short descriptive name.
    pub name: String,
    /// Detailed description of what this team should accomplish.
    pub description: String,
    /// IDs of teams that must complete before this one starts.
    pub dependencies: Vec<String>,
    /// How the team should be orchestrated.
    pub orchestration_mode: OrchestrationMode,
    /// Relevant file/directory paths for scoping.
    #[serde(default)]
    pub scope_paths: Vec<String>,
    /// Priority (0 = highest, 9 = lowest).
    #[serde(default = "default_priority")]
    pub priority: u8,
    /// Preferred model ID (if None, uses default for mode).
    #[serde(default)]
    pub preferred_model: Option<String>,
}

fn default_priority() -> u8 {
    5
}

// ---------------------------------------------------------------------------
// Swarm Config
// ---------------------------------------------------------------------------

/// Configuration for a swarm run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConfig {
    /// Model to use for the Queen's own planning/synthesis calls.
    pub queen_model: String,
    /// Maximum number of teams to run in parallel.
    pub max_parallel_teams: usize,
    /// Total cost limit in USD across all teams.
    pub total_cost_limit_usd: f64,
    /// Total time limit in seconds for the entire swarm run.
    pub total_time_limit_secs: u64,
    /// Per-team cost limit in USD.
    pub per_team_cost_limit_usd: f64,
    /// Per-team time limit in seconds.
    pub per_team_time_limit_secs: u64,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            queen_model: "claude-sonnet-4-5-20250929".into(),
            max_parallel_teams: 3,
            total_cost_limit_usd: 25.0,
            total_time_limit_secs: 1800,
            per_team_cost_limit_usd: 5.0,
            per_team_time_limit_secs: 300,
        }
    }
}

// ---------------------------------------------------------------------------
// Swarm Plan
// ---------------------------------------------------------------------------

/// The plan produced by the Queen -- a set of team objectives with dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmPlan {
    pub teams: Vec<TeamObjective>,
}

impl SwarmPlan {
    /// Validate the plan: check for missing dependencies, cycles, and empty plans.
    pub fn validate(&self) -> Result<(), String> {
        if self.teams.is_empty() {
            return Err("Swarm plan has no teams".into());
        }

        let ids: std::collections::HashSet<&str> =
            self.teams.iter().map(|t| t.id.as_str()).collect();

        // Check all dependencies reference existing teams.
        for team in &self.teams {
            for dep in &team.dependencies {
                if !ids.contains(dep.as_str()) {
                    return Err(format!(
                        "Team '{}' depends on unknown team '{dep}'",
                        team.id
                    ));
                }
            }
            if team.dependencies.contains(&team.id) {
                return Err(format!("Team '{}' depends on itself", team.id));
            }
        }

        // Cycle detection via topological sort.
        let mut in_deg: std::collections::HashMap<&str, usize> = self
            .teams
            .iter()
            .map(|t| (t.id.as_str(), t.dependencies.len()))
            .collect();

        let mut queue: Vec<&str> = in_deg
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut visited = 0;

        while let Some(current) = queue.pop() {
            visited += 1;
            for team in &self.teams {
                if team.dependencies.iter().any(|d| d == current) {
                    let deg = in_deg.get_mut(team.id.as_str())
                        .ok_or_else(|| format!("Team '{}' missing from in-degree map", team.id))?;
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(team.id.as_str());
                    }
                }
            }
        }

        if visited != self.teams.len() {
            return Err("Dependency cycle detected in swarm plan".into());
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Team Result
// ---------------------------------------------------------------------------

/// What kind of inner orchestration produced the team's result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InnerResult {
    HiveMind { result: OrchestrationResult },
    Coordinator { result: CoordinatorResult },
    Native { content: String, model: String },
    SingleShot { content: String, model: String },
}

/// Status of a team's execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Result of executing a single team objective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamResult {
    pub team_id: String,
    pub team_name: String,
    pub status: TeamStatus,
    pub inner: Option<InnerResult>,
    pub cost: f64,
    pub duration_ms: u64,
    pub insights: Vec<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Merge Result
// ---------------------------------------------------------------------------

/// The merged/synthesized output from all teams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub synthesized_content: String,
    pub total_cost: f64,
    pub total_duration_ms: u64,
    pub teams_completed: usize,
    pub teams_failed: usize,
    pub teams_skipped: usize,
}

// ---------------------------------------------------------------------------
// Swarm Result
// ---------------------------------------------------------------------------

/// Overall status of the swarm execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwarmStatus {
    Planning,
    Executing,
    Synthesizing,
    Complete,
    PartialSuccess,
    Failed,
    BudgetExceeded,
    TimedOut,
    TeamStarted,
    TeamCompleted,
    TeamFailed,
    CrossTeamSync,
}

/// Complete result of a swarm orchestration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmResult {
    pub run_id: String,
    pub goal: String,
    pub status: SwarmStatus,
    pub plan: SwarmPlan,
    pub team_results: Vec<TeamResult>,
    pub synthesized_output: String,
    pub total_cost: f64,
    pub total_duration_ms: u64,
    pub learnings_recorded: usize,
}

// ---------------------------------------------------------------------------
// Status Callback
// ---------------------------------------------------------------------------

/// Callback type for receiving swarm-level status updates.
///
/// The callback receives `(status, detail_message)`.
pub type SwarmStatusCallback = Arc<dyn Fn(SwarmStatus, &str) + Send + Sync>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> SwarmPlan {
        SwarmPlan {
            teams: vec![
                TeamObjective {
                    id: "team-1".into(),
                    name: "Research".into(),
                    description: "Investigate the codebase".into(),
                    dependencies: vec![],
                    orchestration_mode: OrchestrationMode::SingleShot,
                    scope_paths: vec!["src/".into()],
                    priority: 0,
                    preferred_model: None,
                },
                TeamObjective {
                    id: "team-2".into(),
                    name: "Implement".into(),
                    description: "Write the code".into(),
                    dependencies: vec!["team-1".into()],
                    orchestration_mode: OrchestrationMode::HiveMind,
                    scope_paths: vec!["src/".into()],
                    priority: 3,
                    preferred_model: None,
                },
                TeamObjective {
                    id: "team-3".into(),
                    name: "Review".into(),
                    description: "Review the implementation".into(),
                    dependencies: vec!["team-2".into()],
                    orchestration_mode: OrchestrationMode::Coordinator,
                    scope_paths: vec![],
                    priority: 5,
                    preferred_model: None,
                },
            ],
        }
    }

    #[test]
    fn default_config() {
        let config = SwarmConfig::default();
        assert_eq!(config.max_parallel_teams, 3);
        assert_eq!(config.total_cost_limit_usd, 25.0);
        assert_eq!(config.total_time_limit_secs, 1800);
    }

    #[test]
    fn plan_validates_valid() {
        let plan = sample_plan();
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn plan_validates_empty() {
        let plan = SwarmPlan { teams: vec![] };
        assert!(plan.validate().is_err());
    }

    #[test]
    fn plan_validates_missing_dependency() {
        let plan = SwarmPlan {
            teams: vec![TeamObjective {
                id: "team-1".into(),
                name: "Test".into(),
                description: "Do something".into(),
                dependencies: vec!["nonexistent".into()],
                orchestration_mode: OrchestrationMode::SingleShot,
                scope_paths: vec![],
                priority: 0,
                preferred_model: None,
            }],
        };
        let err = plan.validate().unwrap_err();
        assert!(err.contains("unknown team"));
    }

    #[test]
    fn plan_validates_self_dependency() {
        let plan = SwarmPlan {
            teams: vec![TeamObjective {
                id: "team-1".into(),
                name: "Test".into(),
                description: "Do something".into(),
                dependencies: vec!["team-1".into()],
                orchestration_mode: OrchestrationMode::SingleShot,
                scope_paths: vec![],
                priority: 0,
                preferred_model: None,
            }],
        };
        let err = plan.validate().unwrap_err();
        assert!(err.contains("depends on itself"));
    }

    #[test]
    fn plan_validates_cycle() {
        let plan = SwarmPlan {
            teams: vec![
                TeamObjective {
                    id: "a".into(),
                    name: "A".into(),
                    description: "A".into(),
                    dependencies: vec!["b".into()],
                    orchestration_mode: OrchestrationMode::SingleShot,
                    scope_paths: vec![],
                    priority: 0,
                    preferred_model: None,
                },
                TeamObjective {
                    id: "b".into(),
                    name: "B".into(),
                    description: "B".into(),
                    dependencies: vec!["a".into()],
                    orchestration_mode: OrchestrationMode::SingleShot,
                    scope_paths: vec![],
                    priority: 0,
                    preferred_model: None,
                },
            ],
        };
        let err = plan.validate().unwrap_err();
        assert!(err.contains("cycle"));
    }

    #[test]
    fn orchestration_mode_from_str() {
        assert_eq!(
            OrchestrationMode::from_str_loose("hivemind"),
            OrchestrationMode::HiveMind
        );
        assert_eq!(
            OrchestrationMode::from_str_loose("hive_mind"),
            OrchestrationMode::HiveMind
        );
        assert_eq!(
            OrchestrationMode::from_str_loose("coordinator"),
            OrchestrationMode::Coordinator
        );
        assert_eq!(
            OrchestrationMode::from_str_loose("native_provider"),
            OrchestrationMode::NativeProvider
        );
        assert_eq!(
            OrchestrationMode::from_str_loose("single_shot"),
            OrchestrationMode::SingleShot
        );
        assert_eq!(
            OrchestrationMode::from_str_loose("singleshot"),
            OrchestrationMode::SingleShot
        );
        assert_eq!(
            OrchestrationMode::from_str_loose("unknown"),
            OrchestrationMode::SingleShot
        );
    }

    #[test]
    fn team_objective_serialization() {
        let obj = TeamObjective {
            id: "team-1".into(),
            name: "Test Team".into(),
            description: "A test team".into(),
            dependencies: vec!["team-0".into()],
            orchestration_mode: OrchestrationMode::HiveMind,
            scope_paths: vec!["src/".into()],
            priority: 2,
            preferred_model: Some("claude-opus".into()),
        };
        let json = serde_json::to_string(&obj).unwrap();
        let parsed: TeamObjective = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "team-1");
        assert_eq!(parsed.orchestration_mode, OrchestrationMode::HiveMind);
        assert_eq!(parsed.preferred_model, Some("claude-opus".into()));
    }

    #[test]
    fn swarm_result_serialization() {
        let result = SwarmResult {
            run_id: "run-1".into(),
            goal: "Build a feature".into(),
            status: SwarmStatus::Complete,
            plan: SwarmPlan { teams: vec![] },
            team_results: vec![],
            synthesized_output: "All done.".into(),
            total_cost: 1.23,
            total_duration_ms: 5000,
            learnings_recorded: 3,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: SwarmResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.run_id, "run-1");
        assert_eq!(parsed.status, SwarmStatus::Complete);
        assert_eq!(parsed.learnings_recorded, 3);
    }
}
