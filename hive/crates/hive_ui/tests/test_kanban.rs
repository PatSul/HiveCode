use hive_ui::panels::kanban::*;

// ------------------------------------------------------------------
// KanbanData default / construction
// ------------------------------------------------------------------

#[test]
fn test_default_creates_four_empty_columns() {
    let data = KanbanData::default();
    assert_eq!(data.columns.len(), 4);
    assert_eq!(data.columns[0].title, "To Do");
    assert_eq!(data.columns[1].title, "In Progress");
    assert_eq!(data.columns[2].title, "Review");
    assert_eq!(data.columns[3].title, "Done");
    assert_eq!(data.task_count(), 0);
}

#[test]
fn test_default_columns_have_correct_statuses() {
    let data = KanbanData::default();
    assert_eq!(data.columns[0].status, TaskStatus::Todo);
    assert_eq!(data.columns[1].status, TaskStatus::InProgress);
    assert_eq!(data.columns[2].status, TaskStatus::Review);
    assert_eq!(data.columns[3].status, TaskStatus::Done);
}

// ------------------------------------------------------------------
// add_task
// ------------------------------------------------------------------

#[test]
fn test_add_task_returns_unique_ids() {
    let mut data = KanbanData::default();
    let id1 = data.add_task(0, "Task A", "Desc A", Priority::Low);
    let id2 = data.add_task(0, "Task B", "Desc B", Priority::High);
    let id3 = data.add_task(1, "Task C", "Desc C", Priority::Medium);

    assert!(id1.is_some());
    assert!(id2.is_some());
    assert!(id3.is_some());

    let ids = [id1.unwrap(), id2.unwrap(), id3.unwrap()];
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[1], ids[2]);
    assert_ne!(ids[0], ids[2]);
}

#[test]
fn test_add_task_places_in_correct_column() {
    let mut data = KanbanData::default();
    data.add_task(0, "In Todo", "Desc", Priority::Low);
    data.add_task(2, "In Review", "Desc", Priority::High);

    assert_eq!(data.column_count(0), 1);
    assert_eq!(data.column_count(1), 0);
    assert_eq!(data.column_count(2), 1);
    assert_eq!(data.column_count(3), 0);
    assert_eq!(data.columns[0].tasks[0].title, "In Todo");
    assert_eq!(data.columns[2].tasks[0].title, "In Review");
}

#[test]
fn test_add_task_out_of_range_returns_none() {
    let mut data = KanbanData::default();
    let result = data.add_task(99, "Nowhere", "Desc", Priority::Low);
    assert!(result.is_none());
    assert_eq!(data.task_count(), 0);
}

#[test]
fn test_add_task_stores_priority_and_description() {
    let mut data = KanbanData::default();
    data.add_task(0, "Important", "Very detailed description", Priority::Critical);

    let task = &data.columns[0].tasks[0];
    assert_eq!(task.title, "Important");
    assert_eq!(task.description, "Very detailed description");
    assert_eq!(task.priority, Priority::Critical);
}

// ------------------------------------------------------------------
// move_task
// ------------------------------------------------------------------

#[test]
fn test_move_task_between_columns() {
    let mut data = KanbanData::default();
    let id = data.add_task(0, "Movable", "Desc", Priority::Medium).unwrap();

    assert_eq!(data.column_count(0), 1);
    assert_eq!(data.column_count(1), 0);

    let moved = data.move_task(id, 0, 1);
    assert!(moved);
    assert_eq!(data.column_count(0), 0);
    assert_eq!(data.column_count(1), 1);
    assert_eq!(data.columns[1].tasks[0].title, "Movable");
}

#[test]
fn test_move_task_same_column_returns_false() {
    let mut data = KanbanData::default();
    let id = data.add_task(0, "Stuck", "Desc", Priority::Low).unwrap();
    let moved = data.move_task(id, 0, 0);
    assert!(!moved);
    assert_eq!(data.column_count(0), 1);
}

#[test]
fn test_move_task_wrong_source_column_returns_false() {
    let mut data = KanbanData::default();
    let id = data.add_task(0, "Here", "Desc", Priority::Low).unwrap();
    let moved = data.move_task(id, 1, 2);
    assert!(!moved);
    assert_eq!(data.column_count(0), 1);
}

#[test]
fn test_move_task_invalid_column_returns_false() {
    let mut data = KanbanData::default();
    let id = data.add_task(0, "Here", "Desc", Priority::Low).unwrap();
    assert!(!data.move_task(id, 0, 99));
    assert!(!data.move_task(id, 99, 0));
}

#[test]
fn test_move_task_nonexistent_id_returns_false() {
    let mut data = KanbanData::default();
    data.add_task(0, "Real", "Desc", Priority::Low);
    let moved = data.move_task(999, 0, 1);
    assert!(!moved);
}

// ------------------------------------------------------------------
// delete_task
// ------------------------------------------------------------------

#[test]
fn test_delete_task_removes_from_column() {
    let mut data = KanbanData::default();
    let id = data.add_task(0, "Delete me", "Desc", Priority::High).unwrap();
    data.add_task(0, "Keep me", "Desc", Priority::Low);

    assert_eq!(data.task_count(), 2);
    let deleted = data.delete_task(id);
    assert!(deleted);
    assert_eq!(data.task_count(), 1);
    assert_eq!(data.columns[0].tasks[0].title, "Keep me");
}

#[test]
fn test_delete_task_nonexistent_returns_false() {
    let mut data = KanbanData::default();
    data.add_task(0, "Exists", "Desc", Priority::Low);
    let deleted = data.delete_task(999);
    assert!(!deleted);
    assert_eq!(data.task_count(), 1);
}

#[test]
fn test_delete_task_finds_across_columns() {
    let mut data = KanbanData::default();
    data.add_task(0, "In Todo", "Desc", Priority::Low);
    let id = data.add_task(2, "In Review", "Desc", Priority::High).unwrap();
    data.add_task(3, "In Done", "Desc", Priority::Medium);

    assert_eq!(data.task_count(), 3);
    let deleted = data.delete_task(id);
    assert!(deleted);
    assert_eq!(data.task_count(), 2);
    assert_eq!(data.column_count(2), 0);
}

// ------------------------------------------------------------------
// task_count / column_count
// ------------------------------------------------------------------

#[test]
fn test_task_count_reflects_all_columns() {
    let mut data = KanbanData::default();
    data.add_task(0, "A", "D", Priority::Low);
    data.add_task(1, "B", "D", Priority::Medium);
    data.add_task(2, "C", "D", Priority::High);
    data.add_task(3, "D", "D", Priority::Critical);
    data.add_task(3, "E", "D", Priority::Low);

    assert_eq!(data.task_count(), 5);
}

#[test]
fn test_column_count_out_of_range_returns_zero() {
    let data = KanbanData::default();
    assert_eq!(data.column_count(100), 0);
}

// ------------------------------------------------------------------
// sorted_tasks_for_column
// ------------------------------------------------------------------

#[test]
fn test_sorted_tasks_for_column_orders_by_priority_desc() {
    let mut data = KanbanData::default();
    data.add_task(0, "Low", "D", Priority::Low);
    data.add_task(0, "Critical", "D", Priority::Critical);
    data.add_task(0, "Medium", "D", Priority::Medium);
    data.add_task(0, "High", "D", Priority::High);

    let sorted = data.sorted_tasks_for_column(0);
    assert_eq!(sorted.len(), 4);
    assert_eq!(sorted[0].priority, Priority::Critical);
    assert_eq!(sorted[1].priority, Priority::High);
    assert_eq!(sorted[2].priority, Priority::Medium);
    assert_eq!(sorted[3].priority, Priority::Low);
}

#[test]
fn test_sorted_tasks_for_invalid_column_returns_empty() {
    let data = KanbanData::default();
    assert!(data.sorted_tasks_for_column(99).is_empty());
}

// ------------------------------------------------------------------
// tasks_with_priority
// ------------------------------------------------------------------

#[test]
fn test_tasks_with_priority_filters_across_columns() {
    let mut data = KanbanData::default();
    data.add_task(0, "A", "D", Priority::High);
    data.add_task(1, "B", "D", Priority::Low);
    data.add_task(2, "C", "D", Priority::High);
    data.add_task(3, "D", "D", Priority::Medium);

    let high = data.tasks_with_priority(Priority::High);
    assert_eq!(high.len(), 2);
    for t in &high {
        assert_eq!(t.priority, Priority::High);
    }
}

// ------------------------------------------------------------------
// sample board
// ------------------------------------------------------------------

#[test]
fn test_sample_board_has_tasks_in_all_columns() {
    let data = KanbanData::sample();
    for (idx, column) in data.columns.iter().enumerate() {
        assert!(
            !column.tasks.is_empty(),
            "Column {} ({}) should have tasks",
            idx,
            column.title
        );
    }
}

#[test]
fn test_sample_board_task_count_matches_sum_of_columns() {
    let data = KanbanData::sample();
    let sum: usize = (0..4).map(|i| data.column_count(i)).sum();
    assert_eq!(sum, data.task_count());
}

// ------------------------------------------------------------------
// Existing tests preserved
// ------------------------------------------------------------------

#[test]
fn test_truncate_text_short() {
    let result = truncate_text("Hello", 10);
    assert_eq!(result, "Hello");
}

#[test]
fn test_truncate_text_exact_boundary() {
    let result = truncate_text("12345", 5);
    assert_eq!(result, "12345");
}

#[test]
fn test_truncate_text_long() {
    let result = truncate_text("Hello, World!", 5);
    assert_eq!(result, "Hello\u{2026}");
}

#[test]
fn test_priority_ordering() {
    assert!(Priority::Low < Priority::Medium);
    assert!(Priority::Medium < Priority::High);
    assert!(Priority::High < Priority::Critical);
}

#[test]
fn test_status_labels() {
    assert_eq!(TaskStatus::Todo.label(), "To Do");
    assert_eq!(TaskStatus::InProgress.label(), "In Progress");
    assert_eq!(TaskStatus::Review.label(), "Review");
    assert_eq!(TaskStatus::Done.label(), "Done");
}

#[test]
fn test_priority_labels() {
    assert_eq!(Priority::Low.label(), "Low");
    assert_eq!(Priority::Medium.label(), "Med");
    assert_eq!(Priority::High.label(), "High");
    assert_eq!(Priority::Critical.label(), "Crit");
}
