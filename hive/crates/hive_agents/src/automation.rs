//! Automation Workflows — define, manage, and simulate event-driven workflows.
//!
//! Mirrors the Electron app's `automation-service.ts` with trigger-based
//! workflows containing conditional steps, lifecycle management, simulated
//! execution, and run-history tracking.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The event that initiates a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TriggerType {
    Schedule { cron: String },
    FileChange { path: String },
    WebhookReceived { event: String },
    ManualTrigger,
    OnMessage { pattern: String },
    OnError { source: String },
}

/// Comparison operators for workflow step conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOp {
    Equals,
    NotEquals,
    Contains,
    GreaterThan,
    LessThan,
    Matches,
}

/// The action a workflow step performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ActionType {
    RunCommand { command: String },
    SendMessage { channel: String, content: String },
    CallApi { url: String, method: String },
    CreateTask { title: String },
    SendNotification { title: String, body: String },
    ExecuteSkill { skill_trigger: String, input: String },
}

/// Lifecycle status of a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Draft,
    Active,
    Paused,
    Completed,
    Failed,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// A predicate that must be satisfied before a step executes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub field: String,
    pub operator: ConditionOp,
    pub value: String,
}

/// A single step within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub name: String,
    pub action: ActionType,
    pub conditions: Vec<Condition>,
    pub timeout_secs: Option<u64>,
    pub retry_count: u32,
}

/// A complete automation workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: TriggerType,
    pub steps: Vec<WorkflowStep>,
    pub status: WorkflowStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u32,
}

/// The result of executing (or simulating) a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunResult {
    pub workflow_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub success: bool,
    pub steps_completed: usize,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// AutomationService
// ---------------------------------------------------------------------------

/// In-memory service for creating, managing, and simulating automation workflows.
pub struct AutomationService {
    workflows: Vec<Workflow>,
    run_history: Vec<WorkflowRunResult>,
}

impl AutomationService {
    /// Create a new automation service with no workflows.
    pub fn new() -> Self {
        Self {
            workflows: Vec::new(),
            run_history: Vec::new(),
        }
    }

    /// Create a new workflow in `Draft` status.
    pub fn create_workflow(
        &mut self,
        name: &str,
        description: &str,
        trigger: TriggerType,
    ) -> Workflow {
        let now = Utc::now();
        let workflow = Workflow {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            trigger,
            steps: Vec::new(),
            status: WorkflowStatus::Draft,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        };
        debug!(name, id = %workflow.id, "Created workflow");
        self.workflows.push(workflow.clone());
        workflow
    }

    /// Add a step to an existing workflow.
    pub fn add_step(
        &mut self,
        workflow_id: &str,
        name: &str,
        action: ActionType,
    ) -> Result<WorkflowStep> {
        self.add_step_with_conditions(workflow_id, name, action, Vec::new())
    }

    /// Add a step with conditions to an existing workflow.
    pub fn add_step_with_conditions(
        &mut self,
        workflow_id: &str,
        name: &str,
        action: ActionType,
        conditions: Vec<Condition>,
    ) -> Result<WorkflowStep> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_id))?;

        let step = WorkflowStep {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            action,
            conditions,
            timeout_secs: None,
            retry_count: 0,
        };

        workflow.steps.push(step.clone());
        workflow.updated_at = Utc::now();
        debug!(workflow_id, step_name = name, "Added step to workflow");
        Ok(step)
    }

    /// Activate a workflow so it can be triggered.
    pub fn activate_workflow(&mut self, id: &str) -> Result<()> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", id))?;

        workflow.status = WorkflowStatus::Active;
        workflow.updated_at = Utc::now();
        debug!(id, "Activated workflow");
        Ok(())
    }

    /// Pause an active workflow.
    pub fn pause_workflow(&mut self, id: &str) -> Result<()> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", id))?;

        workflow.status = WorkflowStatus::Paused;
        workflow.updated_at = Utc::now();
        debug!(id, "Paused workflow");
        Ok(())
    }

    /// Delete a workflow by ID.
    pub fn delete_workflow(&mut self, id: &str) -> Result<()> {
        let before = self.workflows.len();
        self.workflows.retain(|w| w.id != id);
        if self.workflows.len() == before {
            bail!("Workflow '{}' not found", id);
        }
        debug!(id, "Deleted workflow");
        Ok(())
    }

    /// Look up a workflow by ID.
    pub fn get_workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|w| w.id == id)
    }

    /// Return all workflows.
    pub fn list_workflows(&self) -> &[Workflow] {
        &self.workflows
    }

    /// Return only workflows with `Active` status.
    pub fn list_active_workflows(&self) -> Vec<&Workflow> {
        self.workflows
            .iter()
            .filter(|w| w.status == WorkflowStatus::Active)
            .collect()
    }

    /// Simulate executing a workflow. All steps are "run" in order and a
    /// `WorkflowRunResult` is produced. The workflow's `run_count` and
    /// `last_run` are updated.
    pub fn simulate_run(&mut self, workflow_id: &str) -> Result<WorkflowRunResult> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_id))?;

        if workflow.status != WorkflowStatus::Active {
            bail!(
                "Cannot run workflow '{}': status is {:?}, expected Active",
                workflow_id,
                workflow.status
            );
        }

        let started_at = Utc::now();
        let steps_completed = workflow.steps.len();

        workflow.run_count += 1;
        workflow.last_run = Some(Utc::now());
        workflow.updated_at = Utc::now();

        let result = WorkflowRunResult {
            workflow_id: workflow_id.to_string(),
            started_at,
            completed_at: Utc::now(),
            success: true,
            steps_completed,
            error: None,
        };

        self.run_history.push(result.clone());
        debug!(workflow_id, steps_completed, "Simulated workflow run");
        Ok(result)
    }

    /// Record an external run result for a workflow.
    pub fn record_run(
        &mut self,
        workflow_id: &str,
        success: bool,
        steps_completed: usize,
        error: Option<String>,
    ) -> Result<WorkflowRunResult> {
        let workflow = self
            .workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_id))?;

        workflow.run_count += 1;
        workflow.last_run = Some(Utc::now());
        workflow.updated_at = Utc::now();

        if !success {
            workflow.status = WorkflowStatus::Failed;
        }

        let result = WorkflowRunResult {
            workflow_id: workflow_id.to_string(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            success,
            steps_completed,
            error,
        };

        self.run_history.push(result.clone());
        debug!(workflow_id, success, steps_completed, "Recorded workflow run");
        Ok(result)
    }

    /// Return the most recent `limit` run results for a workflow (newest last).
    pub fn get_run_history(&self, workflow_id: &str, limit: usize) -> Vec<&WorkflowRunResult> {
        let all: Vec<&WorkflowRunResult> = self
            .run_history
            .iter()
            .filter(|r| r.workflow_id == workflow_id)
            .collect();
        let start = all.len().saturating_sub(limit);
        all[start..].to_vec()
    }

    /// Return the total number of workflows.
    pub fn workflow_count(&self) -> usize {
        self.workflows.len()
    }

    /// Return the number of active workflows.
    pub fn active_count(&self) -> usize {
        self.workflows
            .iter()
            .filter(|w| w.status == WorkflowStatus::Active)
            .count()
    }

    /// Evaluate a single condition against an actual value.
    ///
    /// For `GreaterThan` and `LessThan`, both values are parsed as `f64`.
    /// For `Matches`, the condition value is compiled as a regex.
    pub fn check_condition(condition: &Condition, actual_value: &str) -> bool {
        match condition.operator {
            ConditionOp::Equals => actual_value == condition.value,
            ConditionOp::NotEquals => actual_value != condition.value,
            ConditionOp::Contains => actual_value.contains(&condition.value),
            ConditionOp::GreaterThan => {
                let actual = actual_value.parse::<f64>().unwrap_or(f64::NAN);
                let expected = condition.value.parse::<f64>().unwrap_or(f64::NAN);
                actual > expected
            }
            ConditionOp::LessThan => {
                let actual = actual_value.parse::<f64>().unwrap_or(f64::NAN);
                let expected = condition.value.parse::<f64>().unwrap_or(f64::NAN);
                actual < expected
            }
            ConditionOp::Matches => Regex::new(&condition.value)
                .map(|re| re.is_match(actual_value))
                .unwrap_or(false),
        }
    }
}

impl Default for AutomationService {
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

    // -- helpers ------------------------------------------------------------

    fn make_service_with_active_workflow() -> (AutomationService, String) {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow(
            "Deploy Pipeline",
            "Automated deployment",
            TriggerType::ManualTrigger,
        );
        let id = wf.id.clone();
        svc.add_step(
            &id,
            "Build",
            ActionType::RunCommand {
                command: "cargo build --release".into(),
            },
        )
        .unwrap();
        svc.activate_workflow(&id).unwrap();
        (svc, id)
    }

    // -- creation -----------------------------------------------------------

    #[test]
    fn create_workflow_defaults_to_draft() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow(
            "Test Workflow",
            "A test",
            TriggerType::Schedule {
                cron: "0 * * * *".into(),
            },
        );

        assert_eq!(wf.name, "Test Workflow");
        assert_eq!(wf.description, "A test");
        assert_eq!(wf.status, WorkflowStatus::Draft);
        assert!(wf.steps.is_empty());
        assert_eq!(wf.run_count, 0);
        assert!(wf.last_run.is_none());
        assert_eq!(svc.workflow_count(), 1);
    }

    #[test]
    fn create_multiple_workflows() {
        let mut svc = AutomationService::new();
        svc.create_workflow("A", "first", TriggerType::ManualTrigger);
        svc.create_workflow("B", "second", TriggerType::ManualTrigger);
        svc.create_workflow("C", "third", TriggerType::ManualTrigger);

        assert_eq!(svc.workflow_count(), 3);
        assert_eq!(svc.list_workflows().len(), 3);
    }

    // -- steps --------------------------------------------------------------

    #[test]
    fn add_step_to_workflow() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Build", "CI", TriggerType::ManualTrigger);

        let step = svc
            .add_step(
                &wf.id,
                "Compile",
                ActionType::RunCommand {
                    command: "make build".into(),
                },
            )
            .unwrap();

        assert_eq!(step.name, "Compile");
        assert!(step.conditions.is_empty());
        assert_eq!(step.retry_count, 0);
        assert!(step.timeout_secs.is_none());

        let updated = svc.get_workflow(&wf.id).unwrap();
        assert_eq!(updated.steps.len(), 1);
    }

    #[test]
    fn add_step_with_conditions() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Conditional", "Test", TriggerType::ManualTrigger);

        let conditions = vec![Condition {
            field: "branch".into(),
            operator: ConditionOp::Equals,
            value: "main".into(),
        }];

        let step = svc
            .add_step_with_conditions(
                &wf.id,
                "Deploy to prod",
                ActionType::RunCommand {
                    command: "deploy.sh".into(),
                },
                conditions,
            )
            .unwrap();

        assert_eq!(step.conditions.len(), 1);
        assert_eq!(step.conditions[0].field, "branch");
    }

    #[test]
    fn add_step_to_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        let result = svc.add_step(
            "no-such-id",
            "Step",
            ActionType::RunCommand {
                command: "echo".into(),
            },
        );
        assert!(result.is_err());
    }

    // -- lifecycle ----------------------------------------------------------

    #[test]
    fn activate_and_pause_workflow() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Lifecycle", "Test", TriggerType::ManualTrigger);

        assert_eq!(wf.status, WorkflowStatus::Draft);

        svc.activate_workflow(&wf.id).unwrap();
        assert_eq!(
            svc.get_workflow(&wf.id).unwrap().status,
            WorkflowStatus::Active
        );

        svc.pause_workflow(&wf.id).unwrap();
        assert_eq!(
            svc.get_workflow(&wf.id).unwrap().status,
            WorkflowStatus::Paused
        );
    }

    #[test]
    fn activate_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.activate_workflow("ghost").is_err());
    }

    #[test]
    fn pause_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.pause_workflow("ghost").is_err());
    }

    // -- delete -------------------------------------------------------------

    #[test]
    fn delete_workflow_removes_it() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Ephemeral", "Will be deleted", TriggerType::ManualTrigger);

        assert_eq!(svc.workflow_count(), 1);
        svc.delete_workflow(&wf.id).unwrap();
        assert_eq!(svc.workflow_count(), 0);
    }

    #[test]
    fn delete_nonexistent_workflow_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.delete_workflow("no-such-id").is_err());
    }

    // -- lookup -------------------------------------------------------------

    #[test]
    fn get_workflow_returns_none_for_unknown() {
        let svc = AutomationService::new();
        assert!(svc.get_workflow("unknown").is_none());
    }

    #[test]
    fn list_active_workflows_filters_correctly() {
        let mut svc = AutomationService::new();
        let wf1 = svc.create_workflow("Active1", "a", TriggerType::ManualTrigger);
        let wf2 = svc.create_workflow("Active2", "b", TriggerType::ManualTrigger);
        let _wf3 = svc.create_workflow("Draft", "c", TriggerType::ManualTrigger);

        svc.activate_workflow(&wf1.id).unwrap();
        svc.activate_workflow(&wf2.id).unwrap();

        assert_eq!(svc.active_count(), 2);
        assert_eq!(svc.list_active_workflows().len(), 2);
    }

    // -- simulate run -------------------------------------------------------

    #[test]
    fn simulate_run_succeeds_for_active_workflow() {
        let (mut svc, id) = make_service_with_active_workflow();

        let result = svc.simulate_run(&id).unwrap();
        assert!(result.success);
        assert_eq!(result.steps_completed, 1);
        assert!(result.error.is_none());

        let wf = svc.get_workflow(&id).unwrap();
        assert_eq!(wf.run_count, 1);
        assert!(wf.last_run.is_some());
    }

    #[test]
    fn simulate_run_fails_for_draft_workflow() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow("Draft", "Not active", TriggerType::ManualTrigger);

        let result = svc.simulate_run(&wf.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Active"));
    }

    #[test]
    fn simulate_run_nonexistent_fails() {
        let mut svc = AutomationService::new();
        assert!(svc.simulate_run("no-such-id").is_err());
    }

    // -- record run ---------------------------------------------------------

    #[test]
    fn record_successful_run() {
        let (mut svc, id) = make_service_with_active_workflow();

        let result = svc.record_run(&id, true, 1, None).unwrap();
        assert!(result.success);
        assert_eq!(result.steps_completed, 1);
        assert!(result.error.is_none());

        let wf = svc.get_workflow(&id).unwrap();
        assert_eq!(wf.run_count, 1);
        assert_eq!(wf.status, WorkflowStatus::Active);
    }

    #[test]
    fn record_failed_run_sets_status() {
        let (mut svc, id) = make_service_with_active_workflow();

        let result = svc
            .record_run(&id, false, 0, Some("Timeout".into()))
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Timeout"));

        let wf = svc.get_workflow(&id).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Failed);
    }

    // -- run history --------------------------------------------------------

    #[test]
    fn get_run_history_returns_limited_results() {
        let (mut svc, id) = make_service_with_active_workflow();

        for _ in 0..5 {
            svc.simulate_run(&id).unwrap();
        }

        let history = svc.get_run_history(&id, 3);
        assert_eq!(history.len(), 3);

        let all = svc.get_run_history(&id, 100);
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn get_run_history_empty_for_unknown_workflow() {
        let svc = AutomationService::new();
        let history = svc.get_run_history("unknown", 10);
        assert!(history.is_empty());
    }

    // -- conditions ---------------------------------------------------------

    #[test]
    fn check_condition_equals() {
        let cond = Condition {
            field: "status".into(),
            operator: ConditionOp::Equals,
            value: "ready".into(),
        };
        assert!(AutomationService::check_condition(&cond, "ready"));
        assert!(!AutomationService::check_condition(&cond, "pending"));
    }

    #[test]
    fn check_condition_not_equals() {
        let cond = Condition {
            field: "env".into(),
            operator: ConditionOp::NotEquals,
            value: "production".into(),
        };
        assert!(AutomationService::check_condition(&cond, "staging"));
        assert!(!AutomationService::check_condition(&cond, "production"));
    }

    #[test]
    fn check_condition_contains() {
        let cond = Condition {
            field: "message".into(),
            operator: ConditionOp::Contains,
            value: "error".into(),
        };
        assert!(AutomationService::check_condition(
            &cond,
            "An error occurred"
        ));
        assert!(!AutomationService::check_condition(&cond, "All good"));
    }

    #[test]
    fn check_condition_greater_than() {
        let cond = Condition {
            field: "score".into(),
            operator: ConditionOp::GreaterThan,
            value: "50".into(),
        };
        assert!(AutomationService::check_condition(&cond, "75"));
        assert!(!AutomationService::check_condition(&cond, "25"));
        assert!(!AutomationService::check_condition(&cond, "50"));
    }

    #[test]
    fn check_condition_less_than() {
        let cond = Condition {
            field: "latency".into(),
            operator: ConditionOp::LessThan,
            value: "100".into(),
        };
        assert!(AutomationService::check_condition(&cond, "42"));
        assert!(!AutomationService::check_condition(&cond, "200"));
        assert!(!AutomationService::check_condition(&cond, "100"));
    }

    #[test]
    fn check_condition_matches_regex() {
        let cond = Condition {
            field: "version".into(),
            operator: ConditionOp::Matches,
            value: r"^v\d+\.\d+\.\d+$".into(),
        };
        assert!(AutomationService::check_condition(&cond, "v1.2.3"));
        assert!(!AutomationService::check_condition(&cond, "1.2.3"));
        assert!(!AutomationService::check_condition(&cond, "v1.2"));
    }

    #[test]
    fn check_condition_matches_invalid_regex_returns_false() {
        let cond = Condition {
            field: "x".into(),
            operator: ConditionOp::Matches,
            value: r"[invalid".into(),
        };
        assert!(!AutomationService::check_condition(&cond, "anything"));
    }

    #[test]
    fn check_condition_greater_than_non_numeric_returns_false() {
        let cond = Condition {
            field: "x".into(),
            operator: ConditionOp::GreaterThan,
            value: "50".into(),
        };
        assert!(!AutomationService::check_condition(&cond, "not-a-number"));
    }

    // -- serde round trip ---------------------------------------------------

    #[test]
    fn workflow_serde_round_trip() {
        let mut svc = AutomationService::new();
        let wf = svc.create_workflow(
            "Serde Test",
            "Testing serialization",
            TriggerType::FileChange {
                path: "/src".into(),
            },
        );
        svc.add_step_with_conditions(
            &wf.id,
            "Notify",
            ActionType::SendNotification {
                title: "Changed".into(),
                body: "File changed".into(),
            },
            vec![Condition {
                field: "ext".into(),
                operator: ConditionOp::Equals,
                value: ".rs".into(),
            }],
        )
        .unwrap();

        let workflow = svc.get_workflow(&wf.id).unwrap();
        let json = serde_json::to_string_pretty(workflow).unwrap();
        let parsed: Workflow = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "Serde Test");
        assert_eq!(parsed.steps.len(), 1);
        assert_eq!(parsed.steps[0].conditions.len(), 1);
    }

    #[test]
    fn run_result_serde_round_trip() {
        let result = WorkflowRunResult {
            workflow_id: "test-wf".into(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            success: true,
            steps_completed: 3,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: WorkflowRunResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.workflow_id, "test-wf");
        assert!(parsed.success);
        assert_eq!(parsed.steps_completed, 3);
    }

    // -- counts -------------------------------------------------------------

    #[test]
    fn workflow_count_and_active_count() {
        let mut svc = AutomationService::new();
        assert_eq!(svc.workflow_count(), 0);
        assert_eq!(svc.active_count(), 0);

        let wf1 = svc.create_workflow("A", "", TriggerType::ManualTrigger);
        let wf2 = svc.create_workflow("B", "", TriggerType::ManualTrigger);
        svc.create_workflow("C", "", TriggerType::ManualTrigger);

        svc.activate_workflow(&wf1.id).unwrap();
        svc.activate_workflow(&wf2.id).unwrap();

        assert_eq!(svc.workflow_count(), 3);
        assert_eq!(svc.active_count(), 2);
    }

    // -- trigger types ------------------------------------------------------

    #[test]
    fn all_trigger_types_serialize() {
        let triggers = vec![
            TriggerType::Schedule {
                cron: "* * * * *".into(),
            },
            TriggerType::FileChange {
                path: "/tmp".into(),
            },
            TriggerType::WebhookReceived {
                event: "push".into(),
            },
            TriggerType::ManualTrigger,
            TriggerType::OnMessage {
                pattern: "deploy".into(),
            },
            TriggerType::OnError {
                source: "build".into(),
            },
        ];

        for trigger in &triggers {
            let json = serde_json::to_string(trigger).unwrap();
            assert!(!json.is_empty());
        }
    }

    // -- action types -------------------------------------------------------

    #[test]
    fn all_action_types_serialize() {
        let actions = vec![
            ActionType::RunCommand {
                command: "ls".into(),
            },
            ActionType::SendMessage {
                channel: "#general".into(),
                content: "Hello".into(),
            },
            ActionType::CallApi {
                url: "https://api.example.com".into(),
                method: "POST".into(),
            },
            ActionType::CreateTask {
                title: "Fix bug".into(),
            },
            ActionType::SendNotification {
                title: "Alert".into(),
                body: "Something happened".into(),
            },
            ActionType::ExecuteSkill {
                skill_trigger: "/test".into(),
                input: "run all".into(),
            },
        ];

        for action in &actions {
            let json = serde_json::to_string(action).unwrap();
            assert!(!json.is_empty());
        }
    }

    // -- default impl -------------------------------------------------------

    #[test]
    fn default_creates_empty_service() {
        let svc = AutomationService::default();
        assert_eq!(svc.workflow_count(), 0);
        assert_eq!(svc.active_count(), 0);
    }
}
