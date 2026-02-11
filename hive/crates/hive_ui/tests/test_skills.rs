use hive_ui::panels::skills::*;

#[test]
fn empty_data_has_no_content() {
    let data = SkillsData::empty();
    assert!(data.installed.is_empty());
    assert!(data.directory.is_empty());
    assert!(data.sources.is_empty());
    assert!(data.search_query.is_empty());
    assert_eq!(data.active_tab, SkillsTab::Installed);
}

#[test]
fn sample_data_has_content() {
    let data = SkillsData::sample();
    assert!(!data.installed.is_empty());
    assert!(!data.directory.is_empty());
    assert!(!data.sources.is_empty());
    assert_eq!(data.active_tab, SkillsTab::Installed);
}

#[test]
fn filtered_installed_returns_all_when_no_query() {
    let data = SkillsData::sample();
    let filtered = data.filtered_installed();
    assert_eq!(filtered.len(), data.installed.len());
}

#[test]
fn filtered_installed_matches_name() {
    let mut data = SkillsData::sample();
    data.search_query = "code_review".into();
    let filtered = data.filtered_installed();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "code_review");
}

#[test]
fn filtered_installed_matches_description() {
    let mut data = SkillsData::sample();
    data.search_query = "vulnerabilities".into();
    let filtered = data.filtered_installed();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "security_scan");
}

#[test]
fn filtered_installed_is_case_insensitive() {
    let mut data = SkillsData::sample();
    data.search_query = "CODE_REVIEW".into();
    let filtered = data.filtered_installed();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "code_review");
}

#[test]
fn filtered_directory_returns_all_when_no_query() {
    let data = SkillsData::sample();
    let filtered = data.filtered_directory();
    assert_eq!(filtered.len(), data.directory.len());
}

#[test]
fn filtered_directory_matches_name() {
    let mut data = SkillsData::sample();
    data.search_query = "api_designer".into();
    let filtered = data.filtered_directory();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "api_designer");
}

#[test]
fn filtered_directory_matches_author() {
    let mut data = SkillsData::sample();
    data.search_query = "PerfLabs".into();
    let filtered = data.filtered_directory();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|s| s.author == "PerfLabs"));
}

#[test]
fn filtered_directory_matches_description() {
    let mut data = SkillsData::sample();
    data.search_query = "CVE".into();
    let filtered = data.filtered_directory();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "dependency_audit");
}

#[test]
fn filtered_returns_empty_for_no_match() {
    let mut data = SkillsData::sample();
    data.search_query = "zzz_nonexistent_zzz".into();
    assert!(data.filtered_installed().is_empty());
    assert!(data.filtered_directory().is_empty());
}

#[test]
fn sample_installed_skills_have_integrity_hashes() {
    let data = SkillsData::sample();
    for skill in &data.installed {
        assert!(
            !skill.integrity_hash.is_empty(),
            "Skill {} should have an integrity hash",
            skill.name
        );
        assert!(
            skill.integrity_hash.starts_with("sha256:"),
            "Hash for {} should start with sha256:",
            skill.name
        );
    }
}

#[test]
fn sample_directory_skills_have_ids() {
    let data = SkillsData::sample();
    for skill in &data.directory {
        assert!(
            !skill.id.is_empty(),
            "Directory skill {} should have an id",
            skill.name
        );
    }
}

#[test]
fn format_download_count_formats_correctly() {
    assert_eq!(format_download_count(500), "500");
    assert_eq!(format_download_count(1_000), "1.0k");
    assert_eq!(format_download_count(12_400), "12.4k");
    assert_eq!(format_download_count(0), "0");
}

#[test]
fn enabled_count_matches_sample_data() {
    let data = SkillsData::sample();
    let enabled = data.installed.iter().filter(|s| s.enabled).count();
    // code_review, test_gen, doc_writer, refactor are enabled; security_scan is disabled
    assert_eq!(enabled, 4);
}

// -- Category filtering tests --

#[test]
fn filtered_directory_by_category() {
    let mut data = SkillsData::sample();
    data.selected_category = Some(SkillCategory::Testing);
    let filtered = data.filtered_directory();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|s| s.category == SkillCategory::Testing));
}

#[test]
fn filtered_directory_by_category_and_query() {
    let mut data = SkillsData::sample();
    data.selected_category = Some(SkillCategory::Testing);
    data.search_query = "perf_profiler".into();
    let filtered = data.filtered_directory();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "perf_profiler");
}

#[test]
fn filtered_directory_no_category_returns_all() {
    let data = SkillsData::sample();
    assert!(data.selected_category.is_none());
    let filtered = data.filtered_directory();
    assert_eq!(filtered.len(), data.directory.len());
}

#[test]
fn filtered_directory_empty_category_returns_empty() {
    let mut data = SkillsData::sample();
    // No skills have the Other category in sample data
    data.selected_category = Some(SkillCategory::Other);
    let filtered = data.filtered_directory();
    assert!(filtered.is_empty());
}

// -- Directory skill new fields tests --

#[test]
fn sample_directory_skills_have_ratings() {
    let data = SkillsData::sample();
    for skill in &data.directory {
        assert!(
            skill.rating >= 0.0 && skill.rating <= 5.0,
            "Rating for {} out of range: {}",
            skill.name,
            skill.rating
        );
    }
}

#[test]
fn sample_directory_skills_have_versions() {
    let data = SkillsData::sample();
    for skill in &data.directory {
        assert!(
            !skill.version.is_empty(),
            "Directory skill {} should have a version",
            skill.name
        );
    }
}

// -- CreateSkillDraft tests --

#[test]
fn empty_draft_is_not_valid() {
    let draft = CreateSkillDraft::empty();
    assert!(!draft.is_valid());
}

#[test]
fn draft_with_all_fields_is_valid() {
    let draft = CreateSkillDraft {
        name: "my_skill".into(),
        description: "A test skill".into(),
        version: "0.1.0".into(),
        instructions: "You are a helpful assistant.".into(),
    };
    assert!(draft.is_valid());
}

#[test]
fn draft_missing_name_is_not_valid() {
    let draft = CreateSkillDraft {
        name: "".into(),
        description: "A test skill".into(),
        version: "0.1.0".into(),
        instructions: "You are a helpful assistant.".into(),
    };
    assert!(!draft.is_valid());
}

#[test]
fn draft_missing_instructions_is_not_valid() {
    let draft = CreateSkillDraft {
        name: "my_skill".into(),
        description: "A test skill".into(),
        version: "0.1.0".into(),
        instructions: "".into(),
    };
    assert!(!draft.is_valid());
}

#[test]
fn draft_whitespace_only_name_is_not_valid() {
    let draft = CreateSkillDraft {
        name: "   ".into(),
        description: "A test skill".into(),
        version: "0.1.0".into(),
        instructions: "You are a helpful assistant.".into(),
    };
    assert!(!draft.is_valid());
}

// -- SkillCategory tests --

#[test]
fn category_labels_are_non_empty() {
    for cat in &SkillCategory::ALL {
        assert!(!cat.label().is_empty());
    }
}

#[test]
fn category_all_has_correct_count() {
    assert_eq!(SkillCategory::ALL.len(), 8);
}

// -- Tab enum coverage --

#[test]
fn skills_tab_has_create() {
    let tab = SkillsTab::Create;
    assert_eq!(tab, SkillsTab::Create);
}
