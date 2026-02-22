use hive_core::ConversationSummary;
use hive_ui_panels::panels::history::HistoryData;

fn make_summary(id: &str, title: &str, model: &str, preview: &str) -> ConversationSummary {
    let now = chrono::Utc::now();
    ConversationSummary {
        id: id.into(),
        title: title.into(),
        preview: preview.into(),
        message_count: 5,
        total_cost: 0.01,
        model: model.into(),
        created_at: now,
        updated_at: now,
    }
}

// ---------------------------------------------------------------------------
// HistoryData::empty
// ---------------------------------------------------------------------------

#[test]
fn empty_has_no_conversations() {
    let data = HistoryData::empty();
    assert!(data.conversations.is_empty());
    assert!(data.selected_id.is_none());
    assert!(data.search_query.is_empty());
    assert!(!data.confirming_clear);
}

// ---------------------------------------------------------------------------
// HistoryData::sample
// ---------------------------------------------------------------------------

#[test]
fn sample_has_two_conversations() {
    let data = HistoryData::sample();
    assert_eq!(data.conversations.len(), 2);
    assert_eq!(data.conversations[0].id, "conv-1");
    assert_eq!(data.conversations[1].id, "conv-2");
}

// ---------------------------------------------------------------------------
// HistoryData::from_summaries
// ---------------------------------------------------------------------------

#[test]
fn from_summaries_preserves_all() {
    let summaries = vec![
        make_summary("a", "Title A", "gpt-4o", "preview a"),
        make_summary("b", "Title B", "claude", "preview b"),
    ];
    let data = HistoryData::from_summaries(summaries);
    assert_eq!(data.total_count(), 2);
    assert_eq!(data.conversations[0].id, "a");
}

// ---------------------------------------------------------------------------
// Builder methods
// ---------------------------------------------------------------------------

#[test]
fn with_selected_sets_id() {
    let data = HistoryData::empty().with_selected("conv-42");
    assert_eq!(data.selected_id.as_deref(), Some("conv-42"));
}

#[test]
fn with_search_sets_query() {
    let data = HistoryData::empty().with_search("auth");
    assert_eq!(data.search_query, "auth");
}

// ---------------------------------------------------------------------------
// filtered()
// ---------------------------------------------------------------------------

#[test]
fn filtered_empty_query_returns_all() {
    let data = HistoryData::from_summaries(vec![
        make_summary("1", "Hello", "gpt", ""),
        make_summary("2", "World", "gpt", ""),
    ]);
    assert_eq!(data.filtered().len(), 2);
}

#[test]
fn filtered_matches_title() {
    let data = HistoryData::from_summaries(vec![
        make_summary("1", "Auth flow", "gpt", ""),
        make_summary("2", "DB queries", "gpt", ""),
    ])
    .with_search("auth");
    let results = data.filtered();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "1");
}

#[test]
fn filtered_matches_model() {
    let data = HistoryData::from_summaries(vec![
        make_summary("1", "Task", "claude-sonnet", ""),
        make_summary("2", "Task", "gpt-4o", ""),
    ])
    .with_search("sonnet");
    let results = data.filtered();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "1");
}

#[test]
fn filtered_matches_preview() {
    let data = HistoryData::from_summaries(vec![
        make_summary("1", "Task", "gpt", "checking login handler"),
        make_summary("2", "Task", "gpt", "database migration"),
    ])
    .with_search("login");
    let results = data.filtered();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "1");
}

#[test]
fn filtered_case_insensitive() {
    let data = HistoryData::from_summaries(vec![
        make_summary("1", "AUTH Flow", "gpt", ""),
    ])
    .with_search("auth");
    assert_eq!(data.filtered().len(), 1);
}

#[test]
fn filtered_no_match_returns_empty() {
    let data = HistoryData::from_summaries(vec![
        make_summary("1", "Hello", "gpt", "world"),
    ])
    .with_search("zzzzz");
    assert!(data.filtered().is_empty());
}

// ---------------------------------------------------------------------------
// total_count
// ---------------------------------------------------------------------------

#[test]
fn total_count_unaffected_by_filter() {
    let data = HistoryData::from_summaries(vec![
        make_summary("1", "Hello", "gpt", ""),
        make_summary("2", "World", "gpt", ""),
    ])
    .with_search("Hello");
    // filtered returns 1, but total_count stays 2
    assert_eq!(data.filtered().len(), 1);
    assert_eq!(data.total_count(), 2);
}
