use hive_ui::sidebar::*;

#[test]
fn test_from_index_valid() {
    assert_eq!(Panel::from_index(0), Some(Panel::Chat));
    assert_eq!(Panel::from_index(1), Some(Panel::History));
    assert_eq!(Panel::from_index(2), Some(Panel::Files));
    assert_eq!(Panel::from_index(3), Some(Panel::Specs));
    assert_eq!(Panel::from_index(4), Some(Panel::Agents));
    assert_eq!(Panel::from_index(5), Some(Panel::Workflows));
    assert_eq!(Panel::from_index(6), Some(Panel::Channels));
    assert_eq!(Panel::from_index(7), Some(Panel::Kanban));
    assert_eq!(Panel::from_index(8), Some(Panel::Monitor));
    assert_eq!(Panel::from_index(9), Some(Panel::Logs));
    assert_eq!(Panel::from_index(10), Some(Panel::Costs));
    assert_eq!(Panel::from_index(11), Some(Panel::Review));
    assert_eq!(Panel::from_index(12), Some(Panel::Skills));
    assert_eq!(Panel::from_index(13), Some(Panel::Routing));
    assert_eq!(Panel::from_index(14), Some(Panel::Models));
    assert_eq!(Panel::from_index(15), Some(Panel::Learning));
    assert_eq!(Panel::from_index(16), Some(Panel::Shield));
    assert_eq!(Panel::from_index(17), Some(Panel::Assistant));
    assert_eq!(Panel::from_index(18), Some(Panel::TokenLaunch));
    assert_eq!(Panel::from_index(19), Some(Panel::Settings));
    assert_eq!(Panel::from_index(20), Some(Panel::Help));
}

#[test]
fn test_from_index_out_of_bounds() {
    assert_eq!(Panel::from_index(21), None);
    assert_eq!(Panel::from_index(100), None);
    assert_eq!(Panel::from_index(usize::MAX), None);
}

#[test]
fn test_from_index_matches_all_array() {
    for (i, panel) in Panel::ALL.iter().enumerate() {
        assert_eq!(Panel::from_index(i), Some(*panel));
    }
}

#[test]
fn test_sidebar_default_panel_is_chat() {
    let sidebar = Sidebar::new();
    assert_eq!(sidebar.active_panel, Panel::Chat);
}

#[test]
fn test_panel_all_count() {
    assert_eq!(Panel::ALL.len(), 21);
}

// -- Panel stored round-trip tests --------------------------------------

#[test]
fn test_panel_to_stored_round_trip() {
    for panel in Panel::ALL {
        let stored = panel.to_stored();
        let restored = Panel::from_stored(stored);
        assert_eq!(
            restored, panel,
            "Round-trip failed for {:?} -> {:?} -> {:?}",
            panel, stored, restored
        );
    }
}

#[test]
fn test_panel_from_stored_unknown_defaults_to_chat() {
    assert_eq!(Panel::from_stored(""), Panel::Chat);
    assert_eq!(Panel::from_stored("Nonexistent"), Panel::Chat);
    assert_eq!(Panel::from_stored("chat"), Panel::Chat); // lowercase
}

#[test]
fn test_panel_to_stored_stable_values() {
    assert_eq!(Panel::Chat.to_stored(), "Chat");
    assert_eq!(Panel::Assistant.to_stored(), "Assistant");
    assert_eq!(Panel::TokenLaunch.to_stored(), "TokenLaunch");
    assert_eq!(Panel::Settings.to_stored(), "Settings");
    assert_eq!(Panel::Learning.to_stored(), "Learning");
    assert_eq!(Panel::Workflows.to_stored(), "Workflows");
    assert_eq!(Panel::Channels.to_stored(), "Channels");
}
