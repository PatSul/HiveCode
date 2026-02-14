use gpui::*;
use gpui_component::{Icon, IconName};

use hive_ui_core::HiveTheme;
use hive_ui_core::KanbanAddTask;
use hive_ui_core::AgentsRunWorkflow;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Task workflow status. Each variant maps to one board column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Review,
    Done,
}

impl TaskStatus {
    /// Display label used in column headers.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Todo => "To Do",
            Self::InProgress => "In Progress",
            Self::Review => "Review",
            Self::Done => "Done",
        }
    }

    /// All variants in column order.
    pub fn all() -> [Self; 4] {
        [Self::Todo, Self::InProgress, Self::Review, Self::Done]
    }
}

/// Priority level for a task, ordered from lowest to highest urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
            Self::Medium => "Med",
            Self::High => "High",
            Self::Critical => "Crit",
        }
    }
}

/// A single task on the Kanban board.
#[derive(Debug, Clone)]
pub struct KanbanTask {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub created_at: String,
    pub assigned_model: Option<String>,
}

/// A single column on the board, holding an ordered list of tasks.
#[derive(Debug, Clone)]
pub struct KanbanColumn {
    pub status: TaskStatus,
    pub title: String,
    pub color: Hsla,
    pub tasks: Vec<KanbanTask>,
}

/// Board state: four columns, each holding its own tasks.
///
/// `next_id` is a monotonically increasing counter used to assign unique IDs
/// to newly created tasks.
#[derive(Debug, Clone)]
pub struct KanbanData {
    pub columns: Vec<KanbanColumn>,
    next_id: u64,
}

impl Default for KanbanData {
    /// Creates an empty 4-column board (Todo, InProgress, Review, Done).
    ///
    /// Column colors default to neutral grey because the theme is not available
    /// at construction time. The renderer overrides these with themed accents.
    fn default() -> Self {
        let grey = hsla(0.0, 0.0, 0.5, 1.0);

        let columns = TaskStatus::all()
            .into_iter()
            .map(|status| KanbanColumn {
                status,
                title: status.label().to_string(),
                color: grey,
                tasks: Vec::new(),
            })
            .collect();

        Self {
            columns,
            next_id: 1,
        }
    }
}

impl KanbanData {
    /// Creates a board pre-populated with sample tasks so the UI has something
    /// to display before real persistence is wired in.
    pub fn sample() -> Self {
        let mut data = Self::default();

        data.add_task(
            0,
            "Set up CI pipeline",
            "Configure GitHub Actions for build, lint, and test on every PR.",
            Priority::High,
        );
        data.add_task(
            0,
            "Write auth unit tests",
            "Cover login, token refresh, and session expiry edge cases.",
            Priority::Medium,
        );
        data.add_task(
            0,
            "Add rate-limit middleware",
            "Per-IP sliding window, 60 req/min default, configurable.",
            Priority::Low,
        );

        data.add_task(
            1,
            "Implement chat streaming",
            "Wire SSE endpoint to the AI provider and render tokens incrementally.",
            Priority::Critical,
        );
        data.add_task(
            1,
            "Refactor model router",
            "Extract fallback logic into its own module, add retry budgets.",
            Priority::Medium,
        );

        data.add_task(
            2,
            "Review security gateway",
            "Audit command allowlist and URL validation against OWASP checklist.",
            Priority::High,
        );

        data.add_task(
            3,
            "Project scaffold",
            "Workspace layout, Cargo.toml, initial crate structure.",
            Priority::High,
        );
        data.add_task(
            3,
            "Theme system",
            "Design tokens, dark palette, HiveTheme struct with all colors.",
            Priority::Medium,
        );
        data.add_task(
            3,
            "Sidebar navigation",
            "Icon buttons, active state highlight, panel switching.",
            Priority::Medium,
        );

        data
    }

    /// Adds a new task to the column at `column_idx`.
    ///
    /// Returns the assigned task ID, or `None` if `column_idx` is out of range.
    pub fn add_task(
        &mut self,
        column_idx: usize,
        title: &str,
        description: &str,
        priority: Priority,
    ) -> Option<u64> {
        let column = self.columns.get_mut(column_idx)?;
        let id = self.next_id;
        self.next_id += 1;

        column.tasks.push(KanbanTask {
            id,
            title: title.to_string(),
            description: description.to_string(),
            priority,
            created_at: "just now".to_string(),
            assigned_model: None,
        });

        Some(id)
    }

    /// Moves a task identified by `task_id` from `from_col` to `to_col`.
    ///
    /// Returns `true` if the move succeeded, `false` if the task was not found
    /// or the column indices are out of range.
    pub fn move_task(&mut self, task_id: u64, from_col: usize, to_col: usize) -> bool {
        if from_col == to_col {
            return false;
        }
        if from_col >= self.columns.len() || to_col >= self.columns.len() {
            return false;
        }

        let pos = self.columns[from_col]
            .tasks
            .iter()
            .position(|t| t.id == task_id);

        let Some(idx) = pos else {
            return false;
        };

        let task = self.columns[from_col].tasks.remove(idx);
        self.columns[to_col].tasks.push(task);
        true
    }

    /// Deletes the task with the given `task_id` from whichever column contains it.
    ///
    /// Returns `true` if a task was removed.
    pub fn delete_task(&mut self, task_id: u64) -> bool {
        for column in &mut self.columns {
            if let Some(idx) = column.tasks.iter().position(|t| t.id == task_id) {
                column.tasks.remove(idx);
                return true;
            }
        }
        false
    }

    /// Total number of tasks across all columns.
    pub fn task_count(&self) -> usize {
        self.columns.iter().map(|c| c.tasks.len()).sum()
    }

    /// Number of tasks in the column at `idx`, or 0 if out of range.
    pub fn column_count(&self, idx: usize) -> usize {
        self.columns.get(idx).map(|c| c.tasks.len()).unwrap_or(0)
    }

    /// Returns tasks in the given column sorted by priority descending.
    pub fn sorted_tasks_for_column(&self, idx: usize) -> Vec<&KanbanTask> {
        let Some(column) = self.columns.get(idx) else {
            return Vec::new();
        };
        let mut sorted: Vec<&KanbanTask> = column.tasks.iter().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));
        sorted
    }

    /// Returns all tasks matching a given priority across all columns.
    pub fn tasks_with_priority(&self, priority: Priority) -> Vec<&KanbanTask> {
        self.columns
            .iter()
            .flat_map(|c| c.tasks.iter())
            .filter(|t| t.priority == priority)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Kanban panel: 4-column task board with toolbar and statistics footer.
pub struct KanbanPanel;

impl KanbanPanel {
    /// Main entry point -- renders the full board from external data.
    pub fn render(data: &KanbanData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("kanban-panel")
            .flex()
            .flex_col()
            .size_full()
            .child(Self::toolbar(theme))
            .child(Self::board(data, theme))
            .child(Self::statistics_footer(data, theme))
    }

    // ------------------------------------------------------------------
    // Toolbar
    // ------------------------------------------------------------------

    fn toolbar(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .p(theme.space_4)
            .gap(theme.space_3)
            .border_b_1()
            .border_color(theme.border)
            // Icon + title
            .child(Icon::new(IconName::LayoutDashboard).size_4())
            .child(
                div()
                    .text_size(theme.font_size_xl)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::BOLD)
                    .child("Task Board".to_string()),
            )
            // Spacer
            .child(div().flex_1())
            // Filter placeholder
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child("Filter \u{25BE}".to_string()),
            )
            // Bulk actions
            .child(Self::toolbar_btn("Move Selected", theme.accent_cyan, theme))
            .child(Self::toolbar_btn(
                "Delete Selected",
                theme.accent_red,
                theme,
            ))
            // Add task
            .child(
                div()
                    .id("kanban-add-task")
                    .px(theme.space_3)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.accent_aqua)
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_on_accent)
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(Box::new(KanbanAddTask), cx);
                    })
                    .child("+ Add Task".to_string()),
            )
    }

    fn toolbar_btn(label: &str, color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        div()
            .px(theme.space_3)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_sm)
            .text_color(color)
            .child(label.to_string())
    }

    // ------------------------------------------------------------------
    // Board (4 columns)
    // ------------------------------------------------------------------

    fn board(data: &KanbanData, theme: &HiveTheme) -> impl IntoElement {
        let accent_colors = [
            theme.accent_aqua,
            theme.accent_yellow,
            theme.accent_pink,
            theme.accent_green,
        ];

        let mut board = div()
            .id("kanban-board")
            .flex()
            .flex_row()
            .flex_1()
            .p(theme.space_3)
            .gap(theme.space_3)
            .overflow_x_scroll();

        for (idx, column) in data.columns.iter().enumerate() {
            let accent = accent_colors.get(idx).copied().unwrap_or(theme.accent_cyan);
            board = board.child(Self::column(column, accent, theme));
        }

        board
    }

    fn column(column: &KanbanColumn, accent: Hsla, theme: &HiveTheme) -> impl IntoElement {
        let mut sorted: Vec<&KanbanTask> = column.tasks.iter().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let count = sorted.len();

        // Column header with accent dot, label, and count badge
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .pb(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(theme.radius_full)
                    .bg(accent),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(column.title.clone()),
            )
            .child(
                div()
                    .ml_auto()
                    .px(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(count.to_string()),
            );

        // Scrollable task list
        let mut task_list = div()
            .id(SharedString::from(format!("col-{}", column.title)))
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .gap(theme.space_2);

        if sorted.is_empty() {
            task_list = task_list.child(Self::empty_column_state(theme));
        } else {
            for task in &sorted {
                task_list = task_list.child(Self::task_card(task, theme));
            }
        }

        div()
            .flex()
            .flex_col()
            .w(px(280.0))
            .min_w(px(280.0))
            .h_full()
            .bg(theme.bg_secondary)
            .rounded(theme.radius_md)
            .border_1()
            .border_color(theme.border)
            .p(theme.space_2)
            .gap(theme.space_2)
            .child(header)
            .child(task_list)
    }

    // ------------------------------------------------------------------
    // Task card
    // ------------------------------------------------------------------

    fn task_card(task: &KanbanTask, theme: &HiveTheme) -> impl IntoElement {
        let priority_color = Self::priority_color(task.priority, theme);
        let desc_display = truncate_text(&task.description, 80);

        div()
            .flex()
            .flex_col()
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            // Top accent strip colored by priority
            .child(div().w_full().h(px(3.0)).bg(priority_color))
            // Card body
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(theme.space_2)
                    .gap(theme.space_1)
                    // Title
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(task.title.clone()),
                    )
                    // Description (truncated)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .child(desc_display),
                    )
                    // Priority badge + optional model badge + timestamp
                    .child(Self::card_footer(task, priority_color, theme)),
            )
    }

    fn card_footer(task: &KanbanTask, priority_color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        let mut footer = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .mt(theme.space_1)
            // Priority badge
            .child(
                div()
                    .px(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(priority_color)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_on_accent)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(task.priority.label().to_string()),
            );

        // Model badge (if assigned)
        if let Some(ref model) = task.assigned_model {
            footer = footer.child(
                div()
                    .px(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.accent_cyan)
                    .child(model.clone()),
            );
        }

        // Timestamp pushed to the right
        footer = footer.child(
            div()
                .ml_auto()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(task.created_at.clone()),
        );

        let task_id = task.id;
        let title = task.title.clone();
        let description = task.description.clone();

        footer = footer.child(
            div()
                .px(theme.space_2)
                .py(theme.space_1)
                .rounded(theme.radius_sm)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.accent_aqua)
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_aqua)
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                    let instruction = format!("Execute kanban task {}: {}", title, description);
                    window.dispatch_action(
                        Box::new(AgentsRunWorkflow {
                            workflow_id: "builtin:hive-dogfood-v1".into(),
                            instruction,
                            source: "kanban-task".into(),
                            source_id: task_id.to_string(),
                        }),
                        cx,
                    );
                })
                .child("Run"),
        );

        footer
    }

    // ------------------------------------------------------------------
    // Empty column state
    // ------------------------------------------------------------------

    fn empty_column_state(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .flex_1()
            .py(theme.space_6)
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(px(24.0))
                    .text_color(theme.text_muted)
                    .child("\u{1F4ED}".to_string()),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child("No tasks".to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Statistics footer
    // ------------------------------------------------------------------

    fn statistics_footer(data: &KanbanData, theme: &HiveTheme) -> impl IntoElement {
        let total = data.task_count();

        let mut footer = div()
            .flex()
            .flex_row()
            .items_center()
            .px(theme.space_4)
            .py(theme.space_2)
            .gap(theme.space_4)
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.bg_secondary)
            // Total
            .child(Self::stat_item(
                "Total",
                &total.to_string(),
                theme.text_primary,
                theme,
            ));

        let accent_colors = [
            theme.accent_aqua,
            theme.accent_yellow,
            theme.accent_pink,
            theme.accent_green,
        ];

        for (idx, column) in data.columns.iter().enumerate() {
            let accent = accent_colors.get(idx).copied().unwrap_or(theme.accent_cyan);
            footer = footer.child(Self::stat_item(
                &column.title,
                &column.tasks.len().to_string(),
                accent,
                theme,
            ));
        }

        footer
    }

    fn stat_item(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Map a priority level to its accent color.
    fn priority_color(priority: Priority, theme: &HiveTheme) -> Hsla {
        match priority {
            Priority::Low => theme.accent_cyan,
            Priority::Medium => theme.accent_yellow,
            Priority::High => theme.accent_pink,
            Priority::Critical => theme.accent_red,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Truncate a string to `max_chars`, appending an ellipsis if trimmed.
pub fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}\u{2026}")
    }
}
