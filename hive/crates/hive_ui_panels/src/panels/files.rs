use chrono::{DateTime, Utc};
use gpui::*;
use gpui_component::{Icon, IconName};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use hive_ui_core::HiveTheme;
use hive_ui_core::{
    FilesDeleteEntry, FilesNavigateBack, FilesNavigateTo, FilesNewFile, FilesNewFolder,
    FilesOpenEntry, FilesRefresh,
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single entry in the file listing (directory or file).
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub extension: String,
}

impl FileEntry {
    /// Build a `FileEntry` from a `std::fs::DirEntry`.
    fn from_dir_entry(entry: &std::fs::DirEntry) -> Option<Self> {
        let metadata = entry.metadata().ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let extension = Path::new(&name)
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();

        let modified = metadata
            .modified()
            .ok()
            .and_then(|st| {
                let dur = st.duration_since(SystemTime::UNIX_EPOCH).ok()?;
                DateTime::from_timestamp(dur.as_secs() as i64, dur.subsec_nanos())
            })
            .unwrap_or_else(Utc::now);

        Some(Self {
            name,
            is_directory: metadata.is_dir(),
            size: metadata.len(),
            modified,
            extension,
        })
    }
}

/// A single breadcrumb segment: display label + the full path it represents.
#[derive(Debug, Clone)]
pub struct BreadcrumbSegment {
    pub label: String,
    pub full_path: PathBuf,
}

/// Everything the file-browser panel needs to render.
#[derive(Debug, Clone)]
pub struct FilesData {
    pub current_path: PathBuf,
    pub entries: Vec<FileEntry>,
    pub search_query: String,
    pub selected_file: Option<String>,
    pub breadcrumbs: Vec<BreadcrumbSegment>,
}

impl Default for FilesData {
    /// Start in the current working directory. Falls back to an empty listing
    /// if `std::env::current_dir()` fails.
    fn default() -> Self {
        match std::env::current_dir() {
            Ok(cwd) => Self::from_path(&cwd),
            Err(_) => Self {
                current_path: PathBuf::from("."),
                entries: Vec::new(),
                search_query: String::new(),
                selected_file: None,
                breadcrumbs: Vec::new(),
            },
        }
    }
}

impl FilesData {
    /// Read a directory and build a `FilesData` snapshot.
    ///
    /// Silently skips entries that cannot be read (e.g. permission denied).
    pub fn from_path(path: &Path) -> Self {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut entries = Vec::new();

        if let Ok(read_dir) = std::fs::read_dir(&canonical) {
            for entry in read_dir.flatten() {
                if let Some(fe) = FileEntry::from_dir_entry(&entry) {
                    entries.push(fe);
                }
            }
        }

        let breadcrumbs = build_breadcrumbs(&canonical);

        Self {
            current_path: canonical,
            entries,
            search_query: String::new(),
            selected_file: None,
            breadcrumbs,
        }
    }

    /// Sort entries: directories first, then files, both alphabetically
    /// (case-insensitive).
    pub fn sorted_entries(&self) -> Vec<&FileEntry> {
        let mut sorted: Vec<&FileEntry> = self.entries.iter().collect();
        sorted.sort_by(|a, b| {
            b.is_directory
                .cmp(&a.is_directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        sorted
    }

    /// Return sorted entries filtered by the current `search_query`.
    /// An empty query matches everything.
    pub fn filtered_sorted_entries(&self) -> Vec<&FileEntry> {
        let query = self.search_query.to_lowercase();
        let mut filtered: Vec<&FileEntry> = if query.is_empty() {
            self.entries.iter().collect()
        } else {
            self.entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&query))
                .collect()
        };

        filtered.sort_by(|a, b| {
            b.is_directory
                .cmp(&a.is_directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        filtered
    }
}

/// Build breadcrumb segments from an absolute path.
pub fn build_breadcrumbs(path: &Path) -> Vec<BreadcrumbSegment> {
    let mut segments = Vec::new();
    let mut accumulated = PathBuf::new();

    for component in path.components() {
        accumulated.push(component);
        let label = component.as_os_str().to_string_lossy().into_owned();
        // Skip empty labels (e.g. from the root `/` on Unix which gives "")
        if label.is_empty() {
            continue;
        }
        segments.push(BreadcrumbSegment {
            label,
            full_path: accumulated.clone(),
        });
    }

    segments
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Human-readable file size.
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Relative-time string for a `DateTime<Utc>` compared to `now`.
pub fn format_relative_time(dt: &DateTime<Utc>, now: &DateTime<Utc>) -> String {
    let secs = now.signed_duration_since(dt).num_seconds().max(0);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        let mins = secs / 60;
        if mins == 1 {
            "1 min ago".to_string()
        } else {
            format!("{mins} mins ago")
        }
    } else if secs < 86400 {
        let hours = secs / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{hours} hours ago")
        }
    } else if secs < 2_592_000 {
        let days = secs / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{days} days ago")
        }
    } else if secs < 31_536_000 {
        let months = secs / 2_592_000;
        if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{months} months ago")
        }
    } else {
        let years = secs / 31_536_000;
        if years == 1 {
            "1 year ago".to_string()
        } else {
            format!("{years} years ago")
        }
    }
}

/// Pick an icon for a file based on its extension.
pub fn file_icon(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("rs") => "\u{1F9E0}",   // brain
        Some("toml") => "\u{2699}",  // gear
        Some("md") => "\u{1F4DD}",   // memo
        Some("json") => "\u{1F4CB}", // clipboard
        Some("yaml" | "yml") => "\u{1F4CB}",
        Some("ts" | "tsx" | "js" | "jsx") => "\u{1F4DC}", // scroll
        Some("html" | "css") => "\u{1F3A8}",              // palette
        Some("py") => "\u{1F40D}",                        // snake
        Some("lock") => "\u{1F512}",                      // lock
        Some("png" | "jpg" | "jpeg" | "svg" | "gif" | "ico") => "\u{1F5BC}", // frame
        Some("sh" | "bash" | "zsh") => "\u{1F4DF}",       // pager
        Some("log" | "txt") => "\u{1F4C3}",               // page
        Some("zip" | "tar" | "gz") => "\u{1F4E6}",        // package
        _ => "\u{1F4C4}",                                 // document
    }
}

/// Format the current path as a displayable string, replacing the user's home
/// directory with `~` if possible.
fn display_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    if let Some(home) = dirs_hint() {
        let home_str = home.to_string_lossy();
        if path_str.starts_with(home_str.as_ref()) {
            return format!("~{}", &path_str[home_str.len()..]);
        }
    }
    path_str.into_owned()
}

/// Best-effort retrieval of the user home directory.
fn dirs_hint() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Files panel: side-panel file browser with breadcrumb navigation,
/// search bar, sortable file tree, and action bar.
pub struct FilesPanel;

impl FilesPanel {
    /// Top-level render. Accepts `FilesData` (real filesystem data) and the theme.
    pub fn render(data: &FilesData, theme: &HiveTheme) -> impl IntoElement {
        let entries = data.filtered_sorted_entries();
        let dir_count = entries.iter().filter(|e| e.is_directory).count();
        let file_count = entries.len() - dir_count;
        let now = Utc::now();

        div()
            .id("files-panel")
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_primary)
            .p(theme.space_4)
            .child(
                div()
                    .w_full()
                    .max_w(px(1260.0))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .rounded(theme.radius_lg)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    // 1. Header (title + breadcrumb)
                    .child(Self::header(data, theme))
                    // 2. Search bar
                    .child(Self::search_bar(&data.search_query, theme))
                    // 3. File tree (scrollable)
                    .child(Self::file_tree(&entries, &data.selected_file, &now, theme))
                    // 4. Action bar
                    .child(Self::action_bar(dir_count, file_count, theme)),
            )
    }

    // ------------------------------------------------------------------
    // 1. Header: title row + breadcrumb
    // ------------------------------------------------------------------

    fn header(data: &FilesData, theme: &HiveTheme) -> impl IntoElement {
        let path_display = display_path(&data.current_path);

        div()
            .flex()
            .flex_col()
            .p(theme.space_3)
            .gap(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            // Title row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        Icon::new(IconName::Folder)
                            .size_4()
                            .text_color(theme.text_primary),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_lg)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child("Files".to_string()),
                    )
                    // Current path as subtitle
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .overflow_hidden()
                            .child(path_display),
                    ),
            )
            // Breadcrumb row
            .child(Self::breadcrumb(&data.breadcrumbs, theme))
    }

    fn breadcrumb(segments: &[BreadcrumbSegment], theme: &HiveTheme) -> impl IntoElement {
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .overflow_hidden()
            // Back button
            .child(
                div()
                    .id("files-back-btn")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(24.0))
                    .h(px(24.0))
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_secondary)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(Box::new(FilesNavigateBack), cx);
                    })
                    .child(Icon::new(IconName::ArrowLeft).size_3p5()),
            );

        // Only show the last few segments to avoid overflow.
        let max_visible = 4;
        let start = if segments.len() > max_visible {
            segments.len() - max_visible
        } else {
            0
        };
        let visible = &segments[start..];

        // If we truncated, show an ellipsis first.
        if start > 0 {
            row = row.child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child("\u{2026}".to_string()), // ellipsis char
            );
        }

        for (i, segment) in visible.iter().enumerate() {
            if i > 0 || start > 0 {
                row = row.child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("/".to_string()),
                );
            }

            let is_last = i == visible.len() - 1;
            let text_color = if is_last {
                theme.accent_aqua
            } else {
                theme.text_secondary
            };

            let nav_path = segment.full_path.to_string_lossy().to_string();
            row = row.child(
                div()
                    .id(("files-crumb", start + i))
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .text_size(theme.font_size_sm)
                    .text_color(text_color)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(
                            Box::new(FilesNavigateTo {
                                path: nav_path.clone(),
                            }),
                            cx,
                        );
                    })
                    .child(segment.label.clone()),
            );
        }

        row
    }

    // ------------------------------------------------------------------
    // 2. Search bar
    // ------------------------------------------------------------------

    fn search_bar(query: &str, theme: &HiveTheme) -> impl IntoElement {
        let placeholder = if query.is_empty() {
            "Search files...".to_string()
        } else {
            query.to_string()
        };
        let text_color = if query.is_empty() {
            theme.text_muted
        } else {
            theme.text_primary
        };

        div()
            .px(theme.space_3)
            .py(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_md)
                    .bg(theme.bg_primary)
                    .border_1()
                    .border_color(theme.border)
                    .gap(theme.space_2)
                    .child(
                        Icon::new(IconName::Search)
                            .size_3p5()
                            .text_color(theme.text_muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme.font_size_sm)
                            .text_color(text_color)
                            .child(placeholder),
                    ),
            )
    }

    // ------------------------------------------------------------------
    // 3. File tree (scrollable list)
    // ------------------------------------------------------------------

    fn file_tree(
        entries: &[&FileEntry],
        selected_file: &Option<String>,
        now: &DateTime<Utc>,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        let mut list = div()
            .id("files-tree")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .py(theme.space_2);

        if entries.is_empty() {
            list = list.child(Self::empty_state(theme));
        } else {
            for entry in entries {
                let is_selected = selected_file.as_ref().is_some_and(|s| s == &entry.name);
                list = list.child(Self::entry_row(entry, is_selected, now, theme));
            }
        }

        list
    }

    fn entry_row(
        entry: &FileEntry,
        is_selected: bool,
        now: &DateTime<Utc>,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        let icon = if entry.is_directory {
            "\u{1F4C1}" // folder
        } else {
            file_icon(&entry.name)
        };

        let name_color = if entry.is_directory {
            theme.accent_yellow
        } else {
            theme.text_primary
        };

        let name_weight = if entry.is_directory {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        };

        let row_bg = if is_selected {
            theme.bg_tertiary
        } else {
            Hsla::transparent_black()
        };

        let relative_time = format_relative_time(&entry.modified, now);

        let row_click_name = entry.name.clone();
        let row_click_is_dir = entry.is_directory;
        let mut row = div()
            .id(ElementId::Name(SharedString::new(format!(
                "files-entry-{}",
                entry.name
            ))))
            .flex()
            .flex_row()
            .items_center()
            .px(theme.space_3)
            .py(theme.space_2)
            .mx(theme.space_2)
            .rounded(theme.radius_sm)
            .bg(row_bg)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                window.dispatch_action(
                    Box::new(FilesOpenEntry {
                        name: row_click_name.clone(),
                        is_directory: row_click_is_dir,
                    }),
                    cx,
                );
            })
            .gap(theme.space_2)
            // Icon
            .child(
                div()
                    .w(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(theme.font_size_base)
                    .child(icon.to_string()),
            )
            // Name
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_base)
                    .text_color(name_color)
                    .font_weight(name_weight)
                    .overflow_hidden()
                    .child(entry.name.clone()),
            );

        // Size column (files only)
        if !entry.is_directory {
            let size_str = format_size(entry.size);
            row = row.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .flex_shrink_0()
                    .child(size_str),
            );
        }

        // Relative time column
        row = row.child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .flex_shrink_0()
                .min_w(px(72.0))
                .child(relative_time),
        );

        // Action buttons: Open + Delete
        row = row.child(Self::entry_action_buttons(
            &entry.name,
            entry.is_directory,
            theme,
        ));

        row
    }

    /// Inline action buttons for a single entry (open / delete).
    fn entry_action_buttons(name: &str, is_directory: bool, theme: &HiveTheme) -> impl IntoElement {
        let open_icon = if is_directory {
            IconName::FolderOpen
        } else {
            IconName::ExternalLink
        };

        let open_name = name.to_string();
        let delete_name = name.to_string();

        div()
            .flex()
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .gap(theme.space_1)
            // Open / Enter button
            .child(
                div()
                    .id(ElementId::Name(SharedString::new(format!(
                        "files-open-{}",
                        name
                    ))))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(26.0))
                    .h(px(24.0))
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(
                            Box::new(FilesOpenEntry {
                                name: open_name.clone(),
                                is_directory,
                            }),
                            cx,
                        );
                    })
                    .child(
                        Icon::new(open_icon)
                            .size_3p5()
                            .text_color(theme.text_secondary),
                    ),
            )
            // Delete button
            .child(
                div()
                    .id(ElementId::Name(SharedString::new(format!(
                        "files-del-{}",
                        name
                    ))))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(26.0))
                    .h(px(24.0))
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(
                            Box::new(FilesDeleteEntry {
                                name: delete_name.clone(),
                            }),
                            cx,
                        );
                    })
                    .child(
                        Icon::new(IconName::Delete)
                            .size_3p5()
                            .text_color(theme.accent_red),
                    ),
            )
    }

    fn empty_state(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .flex_1()
            .py(theme.space_8)
            .gap(theme.space_3)
            .child(
                Icon::new(IconName::FolderOpen)
                    .size_6()
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .text_size(theme.font_size_base)
                    .text_color(theme.text_muted)
                    .child("Directory is empty".to_string()),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child("No files or folders found in this directory".to_string()),
            )
    }

    // ------------------------------------------------------------------
    // 4. Action bar (bottom)
    // ------------------------------------------------------------------

    fn action_bar(dir_count: usize, file_count: usize, theme: &HiveTheme) -> impl IntoElement {
        let total = dir_count + file_count;
        let summary = if total == 0 {
            "Empty directory".to_string()
        } else {
            let mut parts = Vec::new();
            if dir_count > 0 {
                parts.push(if dir_count == 1 {
                    "1 folder".to_string()
                } else {
                    format!("{dir_count} folders")
                });
            }
            if file_count > 0 {
                parts.push(if file_count == 1 {
                    "1 file".to_string()
                } else {
                    format!("{file_count} files")
                });
            }
            format!("{total} items \u{2022} {}", parts.join(", "))
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .px(theme.space_3)
            .py(theme.space_2)
            .gap(theme.space_2)
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.bg_surface)
            // New File button
            .child(
                Self::bottom_action_btn(IconName::File, "New File", "files-new-file-btn", theme)
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(Box::new(FilesNewFile), cx);
                    }),
            )
            // New Folder button
            .child(
                Self::bottom_action_btn(
                    IconName::FolderOpen,
                    "New Folder",
                    "files-new-folder-btn",
                    theme,
                )
                .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                    window.dispatch_action(Box::new(FilesNewFolder), cx);
                }),
            )
            // Refresh button
            .child(
                Self::bottom_action_btn(IconName::Redo, "Refresh", "files-refresh-btn", theme)
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        window.dispatch_action(Box::new(FilesRefresh), cx);
                    }),
            )
            // Spacer
            .child(div().flex_1())
            // Item count
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(summary),
            )
    }

    fn bottom_action_btn(
        icon: IconName,
        label: &str,
        id: &'static str,
        theme: &HiveTheme,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .cursor_pointer()
            .child(Icon::new(icon).size_3p5().text_color(theme.text_secondary))
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_secondary)
                    .child(label.to_string()),
            )
    }
}
