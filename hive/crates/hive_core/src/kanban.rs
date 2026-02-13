use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Task workflow column. Each variant maps to one board column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KanbanColumn {
    Todo,
    InProgress,
    Review,
    Done,
    Blocked,
}

impl KanbanColumn {
    /// Display label used in column headers.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Todo => "To Do",
            Self::InProgress => "In Progress",
            Self::Review => "Review",
            Self::Done => "Done",
            Self::Blocked => "Blocked",
        }
    }

    /// All variants in display order.
    pub fn all() -> [Self; 5] {
        [
            Self::Todo,
            Self::InProgress,
            Self::Review,
            Self::Done,
            Self::Blocked,
        ]
    }
}

/// Priority level for a task, ordered from lowest to highest urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

impl Priority {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single task on the Kanban board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanTask {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub column: KanbanColumn,
    pub priority: Priority,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub subtasks: Vec<Subtask>,
    pub comments: Vec<TaskComment>,
    /// IDs of tasks that this task depends on.
    pub dependencies: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A checklist item within a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub completed: bool,
}

/// A comment on a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComment {
    pub id: String,
    pub author: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Aggregate statistics for the board.
#[derive(Debug, Clone)]
pub struct BoardMetrics {
    pub total_tasks: usize,
    pub by_column: HashMap<KanbanColumn, usize>,
    pub by_priority: HashMap<Priority, usize>,
    pub overdue_count: usize,
    pub completion_rate: f64,
}

// ---------------------------------------------------------------------------
// KanbanBoard  --  in-memory store (same pattern as NotificationStore)
// ---------------------------------------------------------------------------

/// In-memory Kanban board with WIP-limit enforcement.
pub struct KanbanBoard {
    tasks: Vec<KanbanTask>,
    wip_limits: HashMap<KanbanColumn, usize>,
}

impl KanbanBoard {
    /// Creates a new, empty board with no WIP limits.
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            wip_limits: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Task CRUD
    // -----------------------------------------------------------------------

    /// Creates a new task in the `Todo` column and returns a clone of it.
    pub fn add_task(
        &mut self,
        title: impl Into<String>,
        description: Option<String>,
        priority: Priority,
    ) -> KanbanTask {
        let now = Utc::now();
        let task = KanbanTask {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            description,
            column: KanbanColumn::Todo,
            priority,
            assignee: None,
            labels: Vec::new(),
            due_date: None,
            subtasks: Vec::new(),
            comments: Vec::new(),
            dependencies: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        let cloned = task.clone();
        self.tasks.push(task);
        cloned
    }

    /// Moves a task to a new column, respecting WIP limits.
    pub fn move_task(&mut self, id: &str, column: KanbanColumn) -> Result<()> {
        // Check WIP limit for target column before moving.
        if let Some(&limit) = self.wip_limits.get(&column) {
            let current_count = self.tasks.iter().filter(|t| t.column == column).count();
            // If the task is already in the target column it does not increase the count.
            let task_already_in_column = self
                .tasks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.column == column)
                .unwrap_or(false);
            if !task_already_in_column && current_count >= limit {
                bail!(
                    "WIP limit reached for column {} (limit: {})",
                    column.label(),
                    limit
                );
            }
        }

        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;

        task.column = column;
        task.updated_at = Utc::now();
        Ok(())
    }

    /// Deletes a task by ID.
    pub fn delete_task(&mut self, id: &str) -> Result<()> {
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;
        self.tasks.remove(pos);
        Ok(())
    }

    /// Updates mutable fields on a task. Pass `None` to leave a field unchanged.
    pub fn update_task(
        &mut self,
        id: &str,
        title: Option<String>,
        description: Option<Option<String>>,
        priority: Option<Priority>,
        assignee: Option<Option<String>>,
        labels: Option<Vec<String>>,
        due_date: Option<Option<DateTime<Utc>>>,
    ) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;

        if let Some(t) = title {
            task.title = t;
        }
        if let Some(d) = description {
            task.description = d;
        }
        if let Some(p) = priority {
            task.priority = p;
        }
        if let Some(a) = assignee {
            task.assignee = a;
        }
        if let Some(l) = labels {
            task.labels = l;
        }
        if let Some(dd) = due_date {
            task.due_date = dd;
        }
        task.updated_at = Utc::now();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Subtasks
    // -----------------------------------------------------------------------

    /// Adds a new subtask to a task.
    pub fn add_subtask(&mut self, task_id: &str, title: impl Into<String>) -> Result<String> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        let subtask_id = Uuid::new_v4().to_string();
        task.subtasks.push(Subtask {
            id: subtask_id.clone(),
            title: title.into(),
            completed: false,
        });
        task.updated_at = Utc::now();
        Ok(subtask_id)
    }

    /// Toggles the completion state of a subtask.
    pub fn toggle_subtask(&mut self, task_id: &str, subtask_id: &str) -> Result<bool> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        let subtask = task
            .subtasks
            .iter_mut()
            .find(|s| s.id == subtask_id)
            .ok_or_else(|| anyhow::anyhow!("Subtask not found: {}", subtask_id))?;

        subtask.completed = !subtask.completed;
        task.updated_at = Utc::now();
        Ok(subtask.completed)
    }

    // -----------------------------------------------------------------------
    // Comments
    // -----------------------------------------------------------------------

    /// Adds a comment to a task.
    pub fn add_comment(
        &mut self,
        task_id: &str,
        author: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<String> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        let comment_id = Uuid::new_v4().to_string();
        task.comments.push(TaskComment {
            id: comment_id.clone(),
            author: author.into(),
            content: content.into(),
            created_at: Utc::now(),
        });
        task.updated_at = Utc::now();
        Ok(comment_id)
    }

    // -----------------------------------------------------------------------
    // Dependencies
    // -----------------------------------------------------------------------

    /// Adds a dependency: `task_id` depends on `depends_on_id`.
    ///
    /// Validates that both tasks exist and prevents self-references and
    /// duplicate dependencies.
    pub fn add_dependency(&mut self, task_id: &str, depends_on_id: &str) -> Result<()> {
        if task_id == depends_on_id {
            bail!("A task cannot depend on itself");
        }

        // Verify the dependency target exists.
        if !self.tasks.iter().any(|t| t.id == depends_on_id) {
            bail!("Dependency target not found: {}", depends_on_id);
        }

        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        if task.dependencies.contains(&depends_on_id.to_string()) {
            bail!("Dependency already exists");
        }

        task.dependencies.push(depends_on_id.to_string());
        task.updated_at = Utc::now();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // WIP limits
    // -----------------------------------------------------------------------

    /// Sets the maximum number of tasks allowed in a column.
    /// Pass `0` to remove the limit.
    pub fn set_wip_limit(&mut self, column: KanbanColumn, limit: usize) {
        if limit == 0 {
            self.wip_limits.remove(&column);
        } else {
            self.wip_limits.insert(column, limit);
        }
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Returns references to all tasks in a given column, ordered by priority
    /// descending then creation time ascending.
    pub fn tasks_in_column(&self, column: KanbanColumn) -> Vec<&KanbanTask> {
        let mut tasks: Vec<&KanbanTask> =
            self.tasks.iter().filter(|t| t.column == column).collect();
        tasks.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.created_at.cmp(&b.created_at))
        });
        tasks
    }

    /// Looks up a single task by ID.
    pub fn get_task(&self, id: &str) -> Option<&KanbanTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Returns a slice of all tasks (unordered).
    pub fn all_tasks(&self) -> &[KanbanTask] {
        &self.tasks
    }

    /// Computes aggregate board metrics.
    pub fn metrics(&self) -> BoardMetrics {
        let total_tasks = self.tasks.len();
        let now = Utc::now();

        let mut by_column: HashMap<KanbanColumn, usize> = HashMap::new();
        let mut by_priority: HashMap<Priority, usize> = HashMap::new();
        let mut overdue_count = 0usize;
        let mut done_count = 0usize;

        for task in &self.tasks {
            *by_column.entry(task.column).or_insert(0) += 1;
            *by_priority.entry(task.priority).or_insert(0) += 1;

            if task.column == KanbanColumn::Done {
                done_count += 1;
            }

            if let Some(due) = task.due_date {
                if due < now && task.column != KanbanColumn::Done {
                    overdue_count += 1;
                }
            }
        }

        let completion_rate = if total_tasks == 0 {
            0.0
        } else {
            done_count as f64 / total_tasks as f64
        };

        BoardMetrics {
            total_tasks,
            by_column,
            by_priority,
            overdue_count,
            completion_rate,
        }
    }

    /// Returns tasks that are past their due date and not in the Done column.
    pub fn overdue_tasks(&self) -> Vec<&KanbanTask> {
        let now = Utc::now();
        self.tasks
            .iter()
            .filter(|t| {
                t.column != KanbanColumn::Done && t.due_date.map(|d| d < now).unwrap_or(false)
            })
            .collect()
    }
}

impl Default for KanbanBoard {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // -----------------------------------------------------------------------
    // 1. add_task
    // -----------------------------------------------------------------------

    #[test]
    fn add_task_creates_in_todo() {
        let mut board = KanbanBoard::new();
        let task = board.add_task("My task", Some("Details".into()), Priority::Medium);

        assert_eq!(task.title, "My task");
        assert_eq!(task.description.as_deref(), Some("Details"));
        assert_eq!(task.column, KanbanColumn::Todo);
        assert_eq!(task.priority, Priority::Medium);
        assert!(!task.id.is_empty());
        assert_eq!(board.all_tasks().len(), 1);
    }

    // -----------------------------------------------------------------------
    // 2. move_task
    // -----------------------------------------------------------------------

    #[test]
    fn move_task_changes_column() {
        let mut board = KanbanBoard::new();
        let task = board.add_task("Move me", None, Priority::High);
        let id = task.id.clone();

        board.move_task(&id, KanbanColumn::InProgress).unwrap();
        assert_eq!(
            board.get_task(&id).unwrap().column,
            KanbanColumn::InProgress
        );

        board.move_task(&id, KanbanColumn::Done).unwrap();
        assert_eq!(board.get_task(&id).unwrap().column, KanbanColumn::Done);
    }

    #[test]
    fn move_task_not_found() {
        let mut board = KanbanBoard::new();
        let result = board.move_task("nonexistent", KanbanColumn::Done);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 3. WIP limits
    // -----------------------------------------------------------------------

    #[test]
    fn wip_limit_blocks_move() {
        let mut board = KanbanBoard::new();
        board.set_wip_limit(KanbanColumn::InProgress, 1);

        let t1 = board.add_task("Task 1", None, Priority::Low);
        let t2 = board.add_task("Task 2", None, Priority::Low);

        board.move_task(&t1.id, KanbanColumn::InProgress).unwrap();
        let result = board.move_task(&t2.id, KanbanColumn::InProgress);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("WIP limit reached")
        );
    }

    #[test]
    fn wip_limit_allows_move_within_same_column() {
        let mut board = KanbanBoard::new();
        board.set_wip_limit(KanbanColumn::InProgress, 1);

        let t1 = board.add_task("Task 1", None, Priority::Low);
        board.move_task(&t1.id, KanbanColumn::InProgress).unwrap();

        // Moving the same task to the same column should not violate the limit.
        board.move_task(&t1.id, KanbanColumn::InProgress).unwrap();
    }

    #[test]
    fn remove_wip_limit() {
        let mut board = KanbanBoard::new();
        board.set_wip_limit(KanbanColumn::Review, 1);

        let t1 = board.add_task("T1", None, Priority::Low);
        let t2 = board.add_task("T2", None, Priority::Low);

        board.move_task(&t1.id, KanbanColumn::Review).unwrap();
        assert!(board.move_task(&t2.id, KanbanColumn::Review).is_err());

        // Remove limit by setting to 0
        board.set_wip_limit(KanbanColumn::Review, 0);
        board.move_task(&t2.id, KanbanColumn::Review).unwrap();
    }

    // -----------------------------------------------------------------------
    // 4. delete_task
    // -----------------------------------------------------------------------

    #[test]
    fn delete_task_removes_it() {
        let mut board = KanbanBoard::new();
        let task = board.add_task("Delete me", None, Priority::Low);
        let id = task.id.clone();

        assert_eq!(board.all_tasks().len(), 1);
        board.delete_task(&id).unwrap();
        assert_eq!(board.all_tasks().len(), 0);
        assert!(board.get_task(&id).is_none());
    }

    #[test]
    fn delete_task_not_found() {
        let mut board = KanbanBoard::new();
        assert!(board.delete_task("ghost").is_err());
    }

    // -----------------------------------------------------------------------
    // 5. update_task
    // -----------------------------------------------------------------------

    #[test]
    fn update_task_partial_fields() {
        let mut board = KanbanBoard::new();
        let task = board.add_task("Original", None, Priority::Low);
        let id = task.id.clone();

        board
            .update_task(
                &id,
                Some("Updated title".into()),
                Some(Some("New desc".into())),
                Some(Priority::Critical),
                Some(Some("Alice".into())),
                Some(vec!["bug".into(), "urgent".into()]),
                None, // leave due_date unchanged
            )
            .unwrap();

        let updated = board.get_task(&id).unwrap();
        assert_eq!(updated.title, "Updated title");
        assert_eq!(updated.description.as_deref(), Some("New desc"));
        assert_eq!(updated.priority, Priority::Critical);
        assert_eq!(updated.assignee.as_deref(), Some("Alice"));
        assert_eq!(updated.labels, vec!["bug", "urgent"]);
        assert!(updated.due_date.is_none());
    }

    // -----------------------------------------------------------------------
    // 6. subtasks
    // -----------------------------------------------------------------------

    #[test]
    fn add_and_toggle_subtask() {
        let mut board = KanbanBoard::new();
        let task = board.add_task("Parent", None, Priority::Medium);
        let task_id = task.id.clone();

        let sub_id = board.add_subtask(&task_id, "Sub 1").unwrap();

        let parent = board.get_task(&task_id).unwrap();
        assert_eq!(parent.subtasks.len(), 1);
        assert!(!parent.subtasks[0].completed);

        // Toggle to completed
        let new_state = board.toggle_subtask(&task_id, &sub_id).unwrap();
        assert!(new_state);
        assert!(board.get_task(&task_id).unwrap().subtasks[0].completed);

        // Toggle back
        let new_state = board.toggle_subtask(&task_id, &sub_id).unwrap();
        assert!(!new_state);
    }

    #[test]
    fn subtask_on_missing_task() {
        let mut board = KanbanBoard::new();
        assert!(board.add_subtask("nope", "sub").is_err());
    }

    // -----------------------------------------------------------------------
    // 7. comments
    // -----------------------------------------------------------------------

    #[test]
    fn add_comment_to_task() {
        let mut board = KanbanBoard::new();
        let task = board.add_task("Commentable", None, Priority::Low);
        let task_id = task.id.clone();

        let comment_id = board.add_comment(&task_id, "Bob", "Looks good!").unwrap();
        assert!(!comment_id.is_empty());

        let t = board.get_task(&task_id).unwrap();
        assert_eq!(t.comments.len(), 1);
        assert_eq!(t.comments[0].author, "Bob");
        assert_eq!(t.comments[0].content, "Looks good!");
    }

    // -----------------------------------------------------------------------
    // 8. dependencies
    // -----------------------------------------------------------------------

    #[test]
    fn add_dependency_between_tasks() {
        let mut board = KanbanBoard::new();
        let t1 = board.add_task("Foundation", None, Priority::High);
        let t2 = board.add_task("Depends on foundation", None, Priority::Medium);

        board.add_dependency(&t2.id, &t1.id).unwrap();

        let task2 = board.get_task(&t2.id).unwrap();
        assert_eq!(task2.dependencies.len(), 1);
        assert_eq!(task2.dependencies[0], t1.id);
    }

    #[test]
    fn dependency_self_reference_blocked() {
        let mut board = KanbanBoard::new();
        let t = board.add_task("Self", None, Priority::Low);
        let result = board.add_dependency(&t.id, &t.id);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot depend on itself")
        );
    }

    #[test]
    fn dependency_duplicate_blocked() {
        let mut board = KanbanBoard::new();
        let t1 = board.add_task("A", None, Priority::Low);
        let t2 = board.add_task("B", None, Priority::Low);

        board.add_dependency(&t2.id, &t1.id).unwrap();
        let result = board.add_dependency(&t2.id, &t1.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    // -----------------------------------------------------------------------
    // 9. tasks_in_column
    // -----------------------------------------------------------------------

    #[test]
    fn tasks_in_column_sorted_by_priority() {
        let mut board = KanbanBoard::new();
        board.add_task("Low task", None, Priority::Low);
        board.add_task("Critical task", None, Priority::Critical);
        board.add_task("Medium task", None, Priority::Medium);

        let todo_tasks = board.tasks_in_column(KanbanColumn::Todo);
        assert_eq!(todo_tasks.len(), 3);
        assert_eq!(todo_tasks[0].priority, Priority::Critical);
        assert_eq!(todo_tasks[1].priority, Priority::Medium);
        assert_eq!(todo_tasks[2].priority, Priority::Low);

        // Empty column returns empty vec.
        assert!(board.tasks_in_column(KanbanColumn::Done).is_empty());
    }

    // -----------------------------------------------------------------------
    // 10. metrics
    // -----------------------------------------------------------------------

    #[test]
    fn metrics_computation() {
        let mut board = KanbanBoard::new();
        let t1 = board.add_task("A", None, Priority::High);
        let t2 = board.add_task("B", None, Priority::Low);
        let _t3 = board.add_task("C", None, Priority::Medium);
        let _t4 = board.add_task("D", None, Priority::Low);

        board.move_task(&t1.id, KanbanColumn::Done).unwrap();
        board.move_task(&t2.id, KanbanColumn::InProgress).unwrap();

        let m = board.metrics();
        assert_eq!(m.total_tasks, 4);
        assert_eq!(*m.by_column.get(&KanbanColumn::Done).unwrap_or(&0), 1);
        assert_eq!(*m.by_column.get(&KanbanColumn::InProgress).unwrap_or(&0), 1);
        assert_eq!(*m.by_column.get(&KanbanColumn::Todo).unwrap_or(&0), 2);
        assert_eq!(*m.by_priority.get(&Priority::Low).unwrap_or(&0), 2);
        assert_eq!(*m.by_priority.get(&Priority::High).unwrap_or(&0), 1);
        // 1 out of 4 done => 0.25
        assert!((m.completion_rate - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn metrics_empty_board() {
        let board = KanbanBoard::new();
        let m = board.metrics();
        assert_eq!(m.total_tasks, 0);
        assert_eq!(m.overdue_count, 0);
        assert!((m.completion_rate - 0.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // 11. overdue detection
    // -----------------------------------------------------------------------

    #[test]
    fn overdue_tasks_detection() {
        let mut board = KanbanBoard::new();

        // Task with past due date (overdue).
        let t1 = board.add_task("Overdue", None, Priority::High);
        board
            .update_task(
                &t1.id,
                None,
                None,
                None,
                None,
                None,
                Some(Some(Utc::now() - Duration::days(1))),
            )
            .unwrap();

        // Task with future due date (not overdue).
        let t2 = board.add_task("On time", None, Priority::Low);
        board
            .update_task(
                &t2.id,
                None,
                None,
                None,
                None,
                None,
                Some(Some(Utc::now() + Duration::days(7))),
            )
            .unwrap();

        // Task with past due date but Done (not overdue).
        let t3 = board.add_task("Done overdue", None, Priority::Low);
        board
            .update_task(
                &t3.id,
                None,
                None,
                None,
                None,
                None,
                Some(Some(Utc::now() - Duration::days(3))),
            )
            .unwrap();
        board.move_task(&t3.id, KanbanColumn::Done).unwrap();

        // Task with no due date (not overdue).
        board.add_task("No date", None, Priority::Medium);

        let overdue = board.overdue_tasks();
        assert_eq!(overdue.len(), 1);
        assert_eq!(overdue[0].id, t1.id);

        // Metrics should agree.
        let m = board.metrics();
        assert_eq!(m.overdue_count, 1);
    }

    // -----------------------------------------------------------------------
    // 12. serde round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn task_serde_roundtrip() {
        let mut board = KanbanBoard::new();
        let task = board.add_task("Serialize me", Some("desc".into()), Priority::Critical);
        let task_id = task.id.clone();

        board.add_subtask(&task_id, "Sub A").unwrap();
        board.add_comment(&task_id, "Eve", "LGTM").unwrap();

        let original = board.get_task(&task_id).unwrap();
        let json = serde_json::to_string_pretty(original).unwrap();
        let parsed: KanbanTask = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.title, "Serialize me");
        assert_eq!(parsed.description.as_deref(), Some("desc"));
        assert_eq!(parsed.column, KanbanColumn::Todo);
        assert_eq!(parsed.priority, Priority::Critical);
        assert_eq!(parsed.subtasks.len(), 1);
        assert_eq!(parsed.subtasks[0].title, "Sub A");
        assert!(!parsed.subtasks[0].completed);
        assert_eq!(parsed.comments.len(), 1);
        assert_eq!(parsed.comments[0].author, "Eve");
        assert_eq!(parsed.comments[0].content, "LGTM");
    }

    // -----------------------------------------------------------------------
    // 13. all_tasks slice
    // -----------------------------------------------------------------------

    #[test]
    fn all_tasks_returns_full_slice() {
        let mut board = KanbanBoard::new();
        assert!(board.all_tasks().is_empty());

        board.add_task("A", None, Priority::Low);
        board.add_task("B", None, Priority::High);
        assert_eq!(board.all_tasks().len(), 2);
    }

    // -----------------------------------------------------------------------
    // 14. default trait
    // -----------------------------------------------------------------------

    #[test]
    fn default_creates_empty_board() {
        let board = KanbanBoard::default();
        assert!(board.all_tasks().is_empty());
        assert!(board.wip_limits.is_empty());
    }

    // -----------------------------------------------------------------------
    // 15. enum serde
    // -----------------------------------------------------------------------

    #[test]
    fn column_and_priority_serde() {
        // KanbanColumn
        let json = serde_json::to_string(&KanbanColumn::Blocked).unwrap();
        assert_eq!(json, "\"Blocked\"");
        let parsed: KanbanColumn = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, KanbanColumn::Blocked);

        // Priority
        let json = serde_json::to_string(&Priority::Critical).unwrap();
        assert_eq!(json, "\"Critical\"");
        let parsed: Priority = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Priority::Critical);
    }
}
