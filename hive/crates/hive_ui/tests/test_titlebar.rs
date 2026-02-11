use hive_ui::titlebar::*;

#[test]
fn version_string_is_valid_semver() {
    // CARGO_PKG_VERSION must be a non-empty semver string.
    assert!(!VERSION.is_empty(), "VERSION must not be empty");

    let parts: Vec<&str> = VERSION.split('.').collect();
    assert!(
        parts.len() >= 2,
        "VERSION should have at least major.minor: got {VERSION}"
    );

    for part in &parts {
        assert!(
            part.parse::<u32>().is_ok(),
            "Each version segment should be numeric: got '{part}' in {VERSION}"
        );
    }
}

#[test]
fn version_badge_format_matches_convention() {
    // The version badge should produce a "v0.1.0" style string.
    let formatted = format!("v{VERSION}");
    assert!(formatted.starts_with('v'), "Badge must start with 'v'");
    assert!(
        formatted.len() > 1,
        "Badge must contain more than just 'v'"
    );
    assert!(
        formatted.contains('.'),
        "Badge must contain a dot separator for semver"
    );
}

#[test]
fn titlebar_struct_is_zero_sized() {
    // Titlebar is a namespace struct with no fields -- it should be ZST.
    assert_eq!(
        std::mem::size_of::<Titlebar>(),
        0,
        "Titlebar should be a zero-sized type"
    );
}
