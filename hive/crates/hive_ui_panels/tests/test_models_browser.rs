use hive_ui_panels::components::model_selector::FetchStatus;
use hive_ui_panels::panels::models_browser::ViewMode;

// ---------------------------------------------------------------------------
// ViewMode
// ---------------------------------------------------------------------------

#[test]
fn view_mode_equality() {
    assert_eq!(ViewMode::Browse, ViewMode::Browse);
    assert_eq!(ViewMode::Project, ViewMode::Project);
}

#[test]
fn view_mode_inequality() {
    assert_ne!(ViewMode::Browse, ViewMode::Project);
}

#[test]
fn view_mode_copy() {
    let mode = ViewMode::Browse;
    let copy = mode;
    assert_eq!(mode, copy);
}

#[test]
fn view_mode_debug() {
    let debug = format!("{:?}", ViewMode::Browse);
    assert!(debug.contains("Browse"));
}

// ---------------------------------------------------------------------------
// FetchStatus
// ---------------------------------------------------------------------------

#[test]
fn fetch_status_idle_distinct() {
    assert_ne!(FetchStatus::Idle, FetchStatus::Loading);
    assert_ne!(FetchStatus::Idle, FetchStatus::Done);
    assert_ne!(FetchStatus::Idle, FetchStatus::Failed);
}

#[test]
fn fetch_status_loading_distinct() {
    assert_ne!(FetchStatus::Loading, FetchStatus::Done);
    assert_ne!(FetchStatus::Loading, FetchStatus::Failed);
}

#[test]
fn fetch_status_done_vs_failed() {
    assert_ne!(FetchStatus::Done, FetchStatus::Failed);
}

#[test]
fn fetch_status_self_equal() {
    assert_eq!(FetchStatus::Idle, FetchStatus::Idle);
    assert_eq!(FetchStatus::Loading, FetchStatus::Loading);
    assert_eq!(FetchStatus::Done, FetchStatus::Done);
    assert_eq!(FetchStatus::Failed, FetchStatus::Failed);
}
