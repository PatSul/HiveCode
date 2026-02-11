use chrono::Utc;
use hive_ui::panels::logs::*;

#[test]
fn logs_data_empty_has_no_entries() {
    let data = LogsData::empty();
    assert!(data.entries.is_empty());
    assert_eq!(data.filter, LogLevel::Debug);
    assert!(data.search_query.is_empty());
    assert!(data.auto_scroll);
}

#[test]
fn logs_data_sample_has_entries() {
    let data = LogsData::sample();
    assert!(!data.entries.is_empty());
    assert_eq!(data.filter, LogLevel::Debug);
    assert!(data.entries.len() >= 10);
    assert!(data.auto_scroll);
}

#[test]
fn add_entry_appends_with_current_timestamp() {
    let mut data = LogsData::empty();
    let before = Utc::now();
    data.add_entry(LogLevel::Info, "test-source", "hello world");
    let after = Utc::now();

    assert_eq!(data.entries.len(), 1);
    let entry = &data.entries[0];
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.source, "test-source");
    assert_eq!(entry.message, "hello world");
    assert!(entry.timestamp >= before);
    assert!(entry.timestamp <= after);
}

#[test]
fn filtered_entries_returns_all_when_filter_is_debug() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Error, "a", "err");
    data.add_entry(LogLevel::Warning, "b", "warn");
    data.add_entry(LogLevel::Info, "c", "info");
    data.add_entry(LogLevel::Debug, "d", "debug");
    data.filter = LogLevel::Debug;

    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 4);
}

#[test]
fn filtered_entries_respects_error_filter() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Error, "a", "err");
    data.add_entry(LogLevel::Warning, "b", "warn");
    data.add_entry(LogLevel::Info, "c", "info");
    data.add_entry(LogLevel::Debug, "d", "debug");
    data.filter = LogLevel::Error;

    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].level, LogLevel::Error);
}

#[test]
fn filtered_entries_respects_warning_filter() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Error, "a", "err");
    data.add_entry(LogLevel::Warning, "b", "warn");
    data.add_entry(LogLevel::Info, "c", "info");
    data.add_entry(LogLevel::Debug, "d", "debug");
    data.filter = LogLevel::Warning;

    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|e| e.level == LogLevel::Error || e.level == LogLevel::Warning));
}

#[test]
fn filtered_entries_respects_info_filter() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Error, "a", "err");
    data.add_entry(LogLevel::Warning, "b", "warn");
    data.add_entry(LogLevel::Info, "c", "info");
    data.add_entry(LogLevel::Debug, "d", "debug");
    data.filter = LogLevel::Info;

    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 3);
    assert!(filtered.iter().all(|e| e.level != LogLevel::Debug));
}

#[test]
fn log_level_severity_ordering() {
    assert!(LogLevel::Error.severity() < LogLevel::Warning.severity());
    assert!(LogLevel::Warning.severity() < LogLevel::Info.severity());
    assert!(LogLevel::Info.severity() < LogLevel::Debug.severity());
}

#[test]
fn log_level_labels_are_correct() {
    assert_eq!(LogLevel::Error.label(), "ERROR");
    assert_eq!(LogLevel::Warning.label(), "WARN");
    assert_eq!(LogLevel::Info.label(), "INFO");
    assert_eq!(LogLevel::Debug.label(), "DEBUG");
}

#[test]
fn add_multiple_entries_preserves_order() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Info, "first", "message 1");
    data.add_entry(LogLevel::Warning, "second", "message 2");
    data.add_entry(LogLevel::Error, "third", "message 3");

    assert_eq!(data.entries.len(), 3);
    assert_eq!(data.entries[0].source, "first");
    assert_eq!(data.entries[1].source, "second");
    assert_eq!(data.entries[2].source, "third");
}

#[test]
fn filtered_entries_on_empty_data_returns_empty() {
    let data = LogsData::empty();
    assert!(data.filtered_entries().is_empty());
}

// -- New search and auto-scroll tests --

#[test]
fn search_filters_by_message() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Info, "app", "Server started on port 3000");
    data.add_entry(LogLevel::Error, "db", "Connection refused");
    data.add_entry(LogLevel::Info, "app", "Request received");

    data.set_search("connection");
    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].source, "db");
}

#[test]
fn search_filters_by_source() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Info, "router", "Routing request");
    data.add_entry(LogLevel::Info, "auth", "Token validated");
    data.add_entry(LogLevel::Info, "router", "Response sent");

    data.set_search("router");
    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 2);
}

#[test]
fn search_is_case_insensitive() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Error, "app", "CRITICAL ERROR occurred");
    data.add_entry(LogLevel::Info, "app", "Normal operation");

    data.set_search("critical");
    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 1);
}

#[test]
fn search_combined_with_level_filter() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Error, "db", "Connection error");
    data.add_entry(LogLevel::Warning, "db", "Connection slow");
    data.add_entry(LogLevel::Info, "db", "Connection established");

    data.filter = LogLevel::Warning; // show Error + Warning only
    data.set_search("connection");
    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 2);
}

#[test]
fn empty_search_shows_all_matching_level() {
    let mut data = LogsData::empty();
    data.add_entry(LogLevel::Info, "a", "one");
    data.add_entry(LogLevel::Info, "b", "two");
    data.set_search("");
    let filtered = data.filtered_entries();
    assert_eq!(filtered.len(), 2);
}

#[test]
fn toggle_auto_scroll() {
    let mut data = LogsData::empty();
    assert!(data.auto_scroll);
    data.toggle_auto_scroll();
    assert!(!data.auto_scroll);
    data.toggle_auto_scroll();
    assert!(data.auto_scroll);
}

#[test]
fn set_search_updates_query() {
    let mut data = LogsData::empty();
    assert!(data.search_query.is_empty());
    data.set_search("hello");
    assert_eq!(data.search_query, "hello");
    data.set_search("");
    assert!(data.search_query.is_empty());
}
