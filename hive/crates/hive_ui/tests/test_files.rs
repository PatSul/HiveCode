use std::path::{Path, PathBuf};
use chrono::Utc;
use hive_ui::panels::files::*;

#[test]
fn test_format_size_bytes() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(1023), "1023 B");
}

#[test]
fn test_format_size_kb() {
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(2_560), "2.5 KB");
}

#[test]
fn test_format_size_mb() {
    assert_eq!(format_size(1_048_576), "1.0 MB");
    assert_eq!(format_size(1_258_291), "1.2 MB");
}

#[test]
fn test_format_size_gb() {
    assert_eq!(format_size(1_073_741_824), "1.0 GB");
}

#[test]
fn test_sorted_entries_dirs_first() {
    let data = FilesData {
        current_path: PathBuf::from("."),
        entries: vec![
            FileEntry {
                name: "zebra.txt".into(),
                is_directory: false,
                size: 100,
                modified: Utc::now(),
                extension: "txt".into(),
            },
            FileEntry {
                name: "alpha".into(),
                is_directory: true,
                size: 0,
                modified: Utc::now(),
                extension: String::new(),
            },
            FileEntry {
                name: "beta.rs".into(),
                is_directory: false,
                size: 200,
                modified: Utc::now(),
                extension: "rs".into(),
            },
            FileEntry {
                name: "Omega".into(),
                is_directory: true,
                size: 0,
                modified: Utc::now(),
                extension: String::new(),
            },
        ],
        search_query: String::new(),
        selected_file: None,
        breadcrumbs: Vec::new(),
    };

    let sorted = data.sorted_entries();
    assert_eq!(sorted[0].name, "alpha");
    assert_eq!(sorted[1].name, "Omega");
    assert_eq!(sorted[2].name, "beta.rs");
    assert_eq!(sorted[3].name, "zebra.txt");
}

#[test]
fn test_filtered_sorted_entries() {
    let data = FilesData {
        current_path: PathBuf::from("."),
        entries: vec![
            FileEntry {
                name: "Cargo.toml".into(),
                is_directory: false,
                size: 100,
                modified: Utc::now(),
                extension: "toml".into(),
            },
            FileEntry {
                name: "src".into(),
                is_directory: true,
                size: 0,
                modified: Utc::now(),
                extension: String::new(),
            },
            FileEntry {
                name: "cargo_helper.rs".into(),
                is_directory: false,
                size: 200,
                modified: Utc::now(),
                extension: "rs".into(),
            },
        ],
        search_query: "cargo".to_string(),
        selected_file: None,
        breadcrumbs: Vec::new(),
    };

    let filtered = data.filtered_sorted_entries();
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].name, "Cargo.toml");
    assert_eq!(filtered[1].name, "cargo_helper.rs");
}

#[test]
fn test_from_path_with_tempdir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hello.txt"), "world").unwrap();
    std::fs::create_dir(dir.path().join("subdir")).unwrap();

    let data = FilesData::from_path(dir.path());
    assert_eq!(data.entries.len(), 2);

    let sorted = data.sorted_entries();
    // Directory first
    assert!(sorted[0].is_directory);
    assert_eq!(sorted[0].name, "subdir");
    // Then file
    assert!(!sorted[1].is_directory);
    assert_eq!(sorted[1].name, "hello.txt");
    assert_eq!(sorted[1].extension, "txt");
}

#[test]
fn test_default_loads_cwd() {
    let data = FilesData::default();
    // Should load the current working directory, which should exist.
    assert!(!data.current_path.as_os_str().is_empty());
}

#[test]
fn test_breadcrumbs_from_path() {
    let segments = build_breadcrumbs(Path::new("/home/user/projects"));
    // On Unix: ["home", "user", "projects"] (root `/` has empty label, skipped).
    // On Windows paths like C:\Users\foo: ["C:", "Users", "foo"].
    assert!(segments.len() >= 1);
    let last = segments.last().unwrap();
    assert_eq!(last.label, "projects");
}

#[test]
fn test_relative_time_just_now() {
    let now = Utc::now();
    let dt = now - chrono::Duration::seconds(30);
    assert_eq!(format_relative_time(&dt, &now), "just now");
}

#[test]
fn test_relative_time_minutes() {
    let now = Utc::now();
    let dt = now - chrono::Duration::minutes(5);
    assert_eq!(format_relative_time(&dt, &now), "5 mins ago");
}

#[test]
fn test_relative_time_hours() {
    let now = Utc::now();
    let dt = now - chrono::Duration::hours(1);
    assert_eq!(format_relative_time(&dt, &now), "1 hour ago");
}

#[test]
fn test_relative_time_days() {
    let now = Utc::now();
    let dt = now - chrono::Duration::days(3);
    assert_eq!(format_relative_time(&dt, &now), "3 days ago");
}

#[test]
fn test_file_icon_rs() {
    assert_eq!(file_icon("main.rs"), "\u{1F9E0}");
}

#[test]
fn test_file_icon_unknown() {
    assert_eq!(file_icon("mystery"), "\u{1F4C4}");
}
