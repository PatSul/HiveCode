use hive_ui::sidebar::{Panel, Sidebar};
use hive_ui::workspace::*;

#[test]
fn test_panel_switch() {
    let mut sidebar = Sidebar::new();
    assert_eq!(sidebar.active_panel, Panel::Chat);
    sidebar.active_panel = Panel::Files;
    assert_eq!(sidebar.active_panel, Panel::Files);
}

#[test]
fn test_history_data_fallback() {
    let data = HiveWorkspace::load_history_data();
    assert!(data.search_query.is_empty());
}

#[test]
fn test_set_active_panel_all_panels() {
    let mut sidebar = Sidebar::new();
    for panel in Panel::ALL {
        sidebar.active_panel = panel;
        assert_eq!(sidebar.active_panel, panel);
    }
}

/// Verify ctrl-1..ctrl-9,ctrl-0 keyboard mapping matches expected panels.
#[test]
fn test_keyboard_panel_mapping() {
    let expected: [(usize, Panel); 10] = [
        (1, Panel::Chat),
        (2, Panel::History),
        (3, Panel::Files),
        (4, Panel::Specs),
        (5, Panel::Agents),
        (6, Panel::Kanban),
        (7, Panel::Monitor),
        (8, Panel::Logs),
        (9, Panel::Costs),
        (0, Panel::Review),
    ];
    for (key, expected_panel) in expected {
        let idx = if key == 0 { 9 } else { key - 1 };
        let panel = Panel::from_index(idx).expect("panel should exist");
        assert_eq!(panel, expected_panel, "ctrl-{key} mismatch");
    }
}

/// Compile-time check: all keyboard action types implement gpui::Action.
#[test]
fn test_action_types_implement_action_trait() {
    fn assert_action<T: gpui::Action>() {}
    assert_action::<NewConversation>();
    assert_action::<ClearChat>();
    assert_action::<SwitchToChat>();
    assert_action::<SwitchToHistory>();
    assert_action::<SwitchToFiles>();
    assert_action::<SwitchToKanban>();
    assert_action::<SwitchToMonitor>();
    assert_action::<SwitchToLogs>();
    assert_action::<SwitchToCosts>();
    assert_action::<SwitchToReview>();
    assert_action::<SwitchToSkills>();
    assert_action::<SwitchToRouting>();
    assert_action::<SwitchToLearning>();
}
