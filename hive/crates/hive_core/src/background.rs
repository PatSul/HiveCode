use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Task status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// ---------------------------------------------------------------------------
// BackgroundTask
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub id: String,
    pub name: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl BackgroundTask {
    /// Creates a new task in the `Pending` state.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }

    /// Transition the task to `Running`.
    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Transition the task to `Completed`.
    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Transition the task to `Failed` with an error message.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = TaskStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error.into());
    }

    /// Transition the task to `Cancelled`.
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Returns `true` if the task has reached a terminal state.
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

// ---------------------------------------------------------------------------
// BackgroundService
// ---------------------------------------------------------------------------

/// In-memory background task service.
///
/// Follows the same in-memory store pattern as [`crate::NotificationStore`].
/// Tasks are submitted, tracked through their lifecycle, and cleaned up when
/// no longer needed.
pub struct BackgroundService {
    tasks: Vec<BackgroundTask>,
    max_concurrent: usize,
}

impl BackgroundService {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            tasks: Vec::new(),
            max_concurrent,
        }
    }

    /// Submit a new task. Returns the task ID.
    ///
    /// The task is created in `Pending` state. If the number of currently
    /// running tasks is below `max_concurrent`, the task is immediately
    /// promoted to `Running`.
    pub fn submit(&mut self, name: impl Into<String>) -> String {
        let mut task = BackgroundTask::new(name);
        if self.running_count() < self.max_concurrent {
            task.start();
        }
        let id = task.id.clone();
        self.tasks.push(task);
        id
    }

    /// Cancel a task by ID. Returns an error if the task is not found or
    /// is already finished.
    pub fn cancel(&mut self, task_id: &str) -> anyhow::Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        if task.is_finished() {
            anyhow::bail!("Task {} is already finished", task_id);
        }
        task.cancel();
        self.promote_pending();
        Ok(())
    }

    /// Mark a task as completed. Returns an error if the task is not found
    /// or is not currently running.
    pub fn complete(&mut self, task_id: &str) -> anyhow::Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        if task.status != TaskStatus::Running {
            anyhow::bail!("Task {} is not running (status: {:?})", task_id, task.status);
        }
        task.complete();
        self.promote_pending();
        Ok(())
    }

    /// Mark a task as failed. Returns an error if the task is not found
    /// or is not currently running.
    pub fn fail(&mut self, task_id: &str, error: impl Into<String>) -> anyhow::Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        if task.status != TaskStatus::Running {
            anyhow::bail!("Task {} is not running (status: {:?})", task_id, task.status);
        }
        task.fail(error);
        self.promote_pending();
        Ok(())
    }

    /// Look up a task by ID.
    pub fn status(&self, task_id: &str) -> Option<&BackgroundTask> {
        self.tasks.iter().find(|t| t.id == task_id)
    }

    /// Returns a slice of all tasks.
    pub fn list_tasks(&self) -> &[BackgroundTask] {
        &self.tasks
    }

    /// Remove all tasks that have reached a terminal state.
    pub fn cleanup_completed(&mut self) {
        self.tasks.retain(|t| !t.is_finished());
    }

    /// Number of tasks currently in the `Running` state.
    pub fn running_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Running)
            .count()
    }

    /// Number of tasks currently in the `Pending` state.
    pub fn pending_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .count()
    }

    /// The configured concurrency limit.
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Promote pending tasks to running when there is capacity.
    fn promote_pending(&mut self) {
        let available = self.max_concurrent.saturating_sub(self.running_count());
        if available == 0 {
            return;
        }
        let mut promoted = 0;
        for task in &mut self.tasks {
            if promoted >= available {
                break;
            }
            if task.status == TaskStatus::Pending {
                task.start();
                promoted += 1;
            }
        }
    }
}

impl Default for BackgroundService {
    fn default() -> Self {
        Self::new(4)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_returns_unique_ids() {
        let mut svc = BackgroundService::new(4);
        let id1 = svc.submit("task-a");
        let id2 = svc.submit("task-b");
        assert_ne!(id1, id2);
        assert_eq!(svc.list_tasks().len(), 2);
    }

    #[test]
    fn submitted_task_starts_immediately_when_capacity() {
        let mut svc = BackgroundService::new(2);
        let id = svc.submit("task");
        let task = svc.status(&id).unwrap();
        assert_eq!(task.status, TaskStatus::Running);
        assert!(task.started_at.is_some());
    }

    #[test]
    fn task_stays_pending_when_at_capacity() {
        let mut svc = BackgroundService::new(1);
        let _id1 = svc.submit("first");
        let id2 = svc.submit("second");

        assert_eq!(svc.running_count(), 1);
        assert_eq!(svc.pending_count(), 1);
        let task2 = svc.status(&id2).unwrap();
        assert_eq!(task2.status, TaskStatus::Pending);
    }

    #[test]
    fn completing_task_promotes_pending() {
        let mut svc = BackgroundService::new(1);
        let id1 = svc.submit("first");
        let id2 = svc.submit("second");

        assert_eq!(svc.status(&id2).unwrap().status, TaskStatus::Pending);

        svc.complete(&id1).unwrap();

        assert_eq!(svc.status(&id1).unwrap().status, TaskStatus::Completed);
        assert_eq!(svc.status(&id2).unwrap().status, TaskStatus::Running);
        assert_eq!(svc.running_count(), 1);
    }

    #[test]
    fn cancel_running_task() {
        let mut svc = BackgroundService::new(4);
        let id = svc.submit("cancel-me");
        assert_eq!(svc.status(&id).unwrap().status, TaskStatus::Running);

        svc.cancel(&id).unwrap();
        let task = svc.status(&id).unwrap();
        assert_eq!(task.status, TaskStatus::Cancelled);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn cancel_pending_task() {
        let mut svc = BackgroundService::new(1);
        let _running = svc.submit("running");
        let pending = svc.submit("pending");

        svc.cancel(&pending).unwrap();
        assert_eq!(svc.status(&pending).unwrap().status, TaskStatus::Cancelled);
    }

    #[test]
    fn cancel_finished_task_errors() {
        let mut svc = BackgroundService::new(4);
        let id = svc.submit("done");
        svc.complete(&id).unwrap();

        let result = svc.cancel(&id);
        assert!(result.is_err());
    }

    #[test]
    fn cancel_nonexistent_task_errors() {
        let mut svc = BackgroundService::new(4);
        let result = svc.cancel("no-such-id");
        assert!(result.is_err());
    }

    #[test]
    fn fail_task_records_error() {
        let mut svc = BackgroundService::new(4);
        let id = svc.submit("will-fail");

        svc.fail(&id, "something went wrong").unwrap();
        let task = svc.status(&id).unwrap();
        assert_eq!(task.status, TaskStatus::Failed);
        assert_eq!(task.error.as_deref(), Some("something went wrong"));
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn fail_promotes_pending() {
        let mut svc = BackgroundService::new(1);
        let id1 = svc.submit("first");
        let id2 = svc.submit("second");

        svc.fail(&id1, "boom").unwrap();
        assert_eq!(svc.status(&id2).unwrap().status, TaskStatus::Running);
    }

    #[test]
    fn cleanup_completed_removes_finished_tasks() {
        let mut svc = BackgroundService::new(4);
        let id1 = svc.submit("done");
        let id2 = svc.submit("failed");
        let id3 = svc.submit("still-running");

        svc.complete(&id1).unwrap();
        svc.fail(&id2, "err").unwrap();

        assert_eq!(svc.list_tasks().len(), 3);
        svc.cleanup_completed();
        assert_eq!(svc.list_tasks().len(), 1);
        assert_eq!(svc.list_tasks()[0].id, id3);
    }

    #[test]
    fn running_count_and_pending_count() {
        let mut svc = BackgroundService::new(2);
        svc.submit("a");
        svc.submit("b");
        svc.submit("c");

        assert_eq!(svc.running_count(), 2);
        assert_eq!(svc.pending_count(), 1);
    }

    #[test]
    fn default_service_has_concurrency_of_four() {
        let svc = BackgroundService::default();
        assert_eq!(svc.max_concurrent(), 4);
        assert!(svc.list_tasks().is_empty());
    }

    #[test]
    fn task_serde_roundtrip() {
        let mut task = BackgroundTask::new("serde-test");
        task.start();
        task.complete();

        let json = serde_json::to_string(&task).unwrap();
        let parsed: BackgroundTask = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, task.id);
        assert_eq!(parsed.name, "serde-test");
        assert_eq!(parsed.status, TaskStatus::Completed);
        assert!(parsed.started_at.is_some());
        assert!(parsed.completed_at.is_some());
    }

    #[test]
    fn status_returns_none_for_unknown_id() {
        let svc = BackgroundService::new(4);
        assert!(svc.status("unknown").is_none());
    }

    #[test]
    fn complete_non_running_task_errors() {
        let mut svc = BackgroundService::new(1);
        let _running = svc.submit("running");
        let pending = svc.submit("pending");

        // pending is not Running, so complete should fail
        let result = svc.complete(&pending);
        assert!(result.is_err());
    }
}
