use chrono::{DateTime, Utc};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;
use crate::workspace::{LogsClear, LogsSetFilter, LogsToggleAutoScroll};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Severity level for a single log entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warning,
    Info,
    Debug,
}

impl LogLevel {
    /// Short uppercase tag rendered in the level badge column.
    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warning => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        }
    }

    /// Badge background color for this level.
    fn badge_bg(self, theme: &HiveTheme) -> Hsla {
        match self {
            LogLevel::Error => theme.accent_red,
            LogLevel::Warning => theme.accent_yellow,
            LogLevel::Info => theme.accent_cyan,
            LogLevel::Debug => theme.text_muted,
        }
    }

    /// Numeric severity (lower = more severe) for filter comparisons.
    pub fn severity(self) -> u8 {
        match self {
            LogLevel::Error => 0,
            LogLevel::Warning => 1,
            LogLevel::Info => 2,
            LogLevel::Debug => 3,
        }
    }
}

/// A single log entry captured from agent execution.
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
}

/// All data needed to render the Agent Logs panel.
pub struct LogsData {
    pub entries: Vec<LogEntry>,
    pub filter: LogLevel,
    /// Text search query to filter log messages. Empty means no text filter.
    pub search_query: String,
    /// Whether auto-scroll to the bottom of the log is enabled.
    pub auto_scroll: bool,
}

impl LogsData {
    /// Returns an empty log state with no entries and the filter set to Debug
    /// (show everything).
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            filter: LogLevel::Debug,
            search_query: String::new(),
            auto_scroll: true,
        }
    }

    /// Sample data so the layout is visible before real log streaming is wired in.
    pub fn sample() -> Self {
        use chrono::TimeZone;

        let base = Utc.with_ymd_and_hms(2026, 2, 8, 14, 32, 0).unwrap();

        let entries = vec![
            LogEntry {
                timestamp: base,
                level: LogLevel::Info,
                source: "app".into(),
                message: "Hive v0.4.2 started -- loading configuration".into(),
            },
            LogEntry {
                timestamp: base,
                level: LogLevel::Debug,
                source: "config".into(),
                message: "Loaded config from ~/.hive/config.json (1.2 KB)".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(1),
                level: LogLevel::Info,
                source: "providers".into(),
                message: "Probing 7 local AI providers...".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(1),
                level: LogLevel::Info,
                source: "providers".into(),
                message: "Detected Ollama at localhost:11434 (3 models available)".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(1),
                level: LogLevel::Warning,
                source: "providers".into(),
                message: "LM Studio not detected at localhost:1234 -- skipping".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(2),
                level: LogLevel::Info,
                source: "router".into(),
                message: "Model router initialized: auto-routing enabled".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(2),
                level: LogLevel::Debug,
                source: "router".into(),
                message: "Fallback chains: Premium -> Mid -> Budget -> Free".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(3),
                level: LogLevel::Info,
                source: "hivemind".into(),
                message: "Starting orchestration for task \"Implement auth middleware\"".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(3),
                level: LogLevel::Debug,
                source: "router".into(),
                message: "Complexity classified as Premium (score: 0.87)".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(4),
                level: LogLevel::Info,
                source: "architect".into(),
                message: "Decomposing task into 4 subtasks".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(5),
                level: LogLevel::Debug,
                source: "architect".into(),
                message: "Subtask 1: Create JWT validation module".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(6),
                level: LogLevel::Info,
                source: "coder".into(),
                message: "Generating code for subtask 1 using claude-opus-4-6".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(8),
                level: LogLevel::Warning,
                source: "coder".into(),
                message: "Rate limit approaching for claude-opus-4-6 (82% of quota)".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(10),
                level: LogLevel::Info,
                source: "reviewer".into(),
                message: "Code review passed -- no issues found in auth middleware".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(11),
                level: LogLevel::Error,
                source: "security".into(),
                message: "Blocked dangerous command: rm -rf /".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(12),
                level: LogLevel::Info,
                source: "security".into(),
                message: "Security scan complete -- 1 threat blocked, 0 vulnerabilities".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(14),
                level: LogLevel::Debug,
                source: "costs".into(),
                message: "Token usage: 12,400 in / 8,200 out ($0.42)".into(),
            },
            LogEntry {
                timestamp: base + chrono::Duration::seconds(15),
                level: LogLevel::Info,
                source: "hivemind".into(),
                message: "Task complete -- 4/4 subtasks finished in 12.1s".into(),
            },
        ];

        Self {
            entries,
            filter: LogLevel::Debug,
            search_query: String::new(),
            auto_scroll: true,
        }
    }

    /// Return entries that pass both the level filter and the text search query.
    ///
    /// Level filter: entries with severity <= filter severity are included.
    /// Text search: case-insensitive substring match on source + message.
    pub fn filtered_entries(&self) -> Vec<&LogEntry> {
        let max_severity = self.filter.severity();
        let query_lower = self.search_query.to_lowercase();
        let has_query = !query_lower.is_empty();

        self.entries
            .iter()
            .filter(|e| e.level.severity() <= max_severity)
            .filter(|e| {
                if !has_query {
                    return true;
                }
                e.message.to_lowercase().contains(&query_lower)
                    || e.source.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Append a new log entry with the current UTC timestamp.
    pub fn add_entry(&mut self, level: LogLevel, source: impl Into<String>, message: impl Into<String>) {
        self.entries.push(LogEntry {
            timestamp: Utc::now(),
            level,
            source: source.into(),
            message: message.into(),
        });
    }

    /// Set the text search filter.
    pub fn set_search(&mut self, query: impl Into<String>) {
        self.search_query = query.into();
    }

    /// Toggle auto-scroll mode.
    pub fn toggle_auto_scroll(&mut self) {
        self.auto_scroll = !self.auto_scroll;
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Agent Logs panel with level filtering, text search, auto-scroll toggle,
/// and terminal-style display.
pub struct LogsPanel;

impl LogsPanel {
    /// Main entry point -- renders the full panel.
    pub fn render(data: &LogsData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("logs-panel")
            .flex()
            .flex_col()
            .size_full()
            .child(Self::header(data, theme))
            .child(Self::filter_bar(data, theme))
            .child(Self::search_bar(data, theme))
            .child(Self::log_container(data, theme))
    }

    // ------------------------------------------------------------------
    // Header
    // ------------------------------------------------------------------

    fn header(data: &LogsData, theme: &HiveTheme) -> impl IntoElement {
        let count = data.entries.len();
        let filtered_count = data.filtered_entries().len();

        let count_label = if filtered_count == count {
            count.to_string()
        } else {
            format!("{filtered_count}/{count}")
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .px(theme.space_4)
            .py(theme.space_3)
            .gap(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .child(Self::header_title(theme))
            .child(Self::count_pill(&count_label, theme))
            .child(div().flex_1())
            .child(Self::auto_scroll_toggle(data, theme))
            .child(Self::header_btn("Refresh", theme))
            .child(Self::clear_btn(theme))
    }

    fn header_title(theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_lg)
            .text_color(theme.text_primary)
            .font_weight(FontWeight::BOLD)
            .child("Agent Logs".to_string())
    }

    fn count_pill(label: &str, theme: &HiveTheme) -> Div {
        div()
            .px(theme.space_2)
            .py(px(2.0))
            .rounded(theme.radius_full)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .child(label.to_string())
    }

    /// Small bordered button used in the header row (no action wired).
    fn header_btn(label: &str, theme: &HiveTheme) -> impl IntoElement {
        div()
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_secondary)
            .cursor_pointer()
            .child(label.to_string())
    }

    /// Clear button -- dispatches `LogsClear`.
    fn clear_btn(theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("logs-clear-btn")
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_secondary)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.dispatch_action(&LogsClear);
            })
            .child("Clear".to_string())
    }

    /// Auto-scroll toggle indicator in the header -- dispatches `LogsToggleAutoScroll`.
    fn auto_scroll_toggle(data: &LogsData, theme: &HiveTheme) -> impl IntoElement {
        let (bg, text_color, label) = if data.auto_scroll {
            (theme.accent_aqua, theme.text_on_accent, "Auto-scroll ON")
        } else {
            (theme.bg_surface, theme.text_secondary, "Auto-scroll OFF")
        };

        div()
            .id("logs-auto-scroll-toggle")
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(bg)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_xs)
            .text_color(text_color)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.dispatch_action(&LogsToggleAutoScroll);
            })
            .child(label.to_string())
    }

    // ------------------------------------------------------------------
    // Filter bar
    // ------------------------------------------------------------------

    fn filter_bar(data: &LogsData, theme: &HiveTheme) -> impl IntoElement {
        let is_show_all = data.filter == LogLevel::Debug;
        let filters: &[(&str, LogLevel, bool)] = &[
            ("All", LogLevel::Debug, true),
            ("Error", LogLevel::Error, false),
            ("Warning", LogLevel::Warning, false),
            ("Info", LogLevel::Info, false),
            ("Debug", LogLevel::Debug, false),
        ];

        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .px(theme.space_4)
            .py(theme.space_2)
            .gap(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.bg_secondary);

        for (label, level, is_all) in filters {
            let active = if *is_all {
                is_show_all
            } else {
                !is_show_all && data.filter == *level
            };
            let filter_level = match level {
                LogLevel::Error => "error",
                LogLevel::Warning => "warning",
                LogLevel::Info => "info",
                LogLevel::Debug => "debug",
            };
            row = row.child(Self::filter_pill(label, filter_level, active, theme));
        }

        row
    }

    /// A single pill button in the filter bar -- dispatches `LogsSetFilter`.
    fn filter_pill(label: &str, level: &str, active: bool, theme: &HiveTheme) -> impl IntoElement {
        let (bg, text_color) = if active {
            (theme.accent_aqua, theme.text_on_accent)
        } else {
            (theme.bg_surface, theme.text_secondary)
        };

        let id = SharedString::from(format!("logs-filter-{level}"));
        let level = level.to_string();

        div()
            .id(id)
            .px(theme.space_3)
            .py(theme.space_1)
            .rounded(theme.radius_full)
            .bg(bg)
            .text_size(theme.font_size_xs)
            .text_color(text_color)
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                cx.dispatch_action(&LogsSetFilter {
                    level: level.clone(),
                });
            })
            .child(label.to_string())
    }

    // ------------------------------------------------------------------
    // Search bar
    // ------------------------------------------------------------------

    fn search_bar(data: &LogsData, theme: &HiveTheme) -> impl IntoElement {
        let placeholder = if data.search_query.is_empty() {
            "Search logs by message or source..."
        } else {
            &data.search_query
        };

        let text_color = if data.search_query.is_empty() {
            theme.text_muted
        } else {
            theme.text_primary
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .px(theme.space_4)
            .py(theme.space_2)
            .gap(theme.space_2)
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.bg_secondary)
            .child(Self::search_input_field(placeholder, text_color, theme))
            // Show match count when searching
            .when(!data.search_query.is_empty(), |el: Div| {
                let filtered = data.filtered_entries();
                el.child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("{} matches", filtered.len())),
                )
            })
    }

    fn search_input_field(placeholder: &str, text_color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .flex_1()
            .items_center()
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_md)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .hover(|style: StyleRefinement| style.border_color(theme.border_focus))
            .child(
                div()
                    .mr(theme.space_1)
                    .child(Icon::new(IconName::Search).size_3p5()),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(text_color)
                    .child(placeholder.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Log container (terminal-style)
    // ------------------------------------------------------------------

    fn log_container(data: &LogsData, theme: &HiveTheme) -> impl IntoElement {
        let visible = data.filtered_entries();

        if visible.is_empty() {
            return div()
                .id("logs-scroll")
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .bg(theme.bg_primary)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_muted)
                        .child(if data.search_query.is_empty() {
                            "No logs yet".to_string()
                        } else {
                            format!("No logs matching \"{}\"", data.search_query)
                        }),
                );
        }

        let mut container = div()
            .id("logs-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .bg(theme.bg_primary)
            .p(theme.space_3)
            .gap(px(1.0))
            .font_family(theme.font_mono.clone());

        for entry in &visible {
            container = container.child(Self::log_row(entry, &data.search_query, theme));
        }

        container
    }

    /// A single log row rendered in terminal style.
    fn log_row(entry: &LogEntry, search_query: &str, theme: &HiveTheme) -> impl IntoElement {
        let time_str = entry.timestamp.format("%H:%M:%S").to_string();

        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_2)
            .py(px(3.0))
            .rounded(theme.radius_sm)
            .hover(|s| s.bg(theme.bg_surface))
            .child(Self::log_timestamp(&time_str, theme))
            .child(Self::level_badge(entry.level, theme))
            .child(Self::log_source(&entry.source, theme))
            .child(Self::log_message(&entry.message, entry.level, theme));

        // Highlight matching text visually by adding a subtle background
        // when a search query is active and matches this entry.
        if !search_query.is_empty() {
            let query_lower = search_query.to_lowercase();
            let matches = entry.message.to_lowercase().contains(&query_lower)
                || entry.source.to_lowercase().contains(&query_lower);
            if matches {
                return row.bg(hsla(174.0 / 360.0, 0.5, 0.15, 0.2));
            }
        }

        row
    }

    fn log_timestamp(time_str: &str, theme: &HiveTheme) -> Div {
        div()
            .flex_shrink_0()
            .w(px(68.0))
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .child(format!("[{}]", time_str))
    }

    fn log_source(source: &str, theme: &HiveTheme) -> Div {
        div()
            .flex_shrink_0()
            .w(px(80.0))
            .text_size(theme.font_size_xs)
            .text_color(theme.text_secondary)
            .child(source.to_string())
    }

    fn log_message(message: &str, level: LogLevel, theme: &HiveTheme) -> Div {
        div()
            .flex_1()
            .text_size(theme.font_size_sm)
            .text_color(Self::message_color(level, theme))
            .child(message.to_string())
    }

    /// Colored badge showing the log level (e.g. ERROR, WARN, INFO, DEBUG).
    fn level_badge(level: LogLevel, theme: &HiveTheme) -> impl IntoElement {
        let bg = level.badge_bg(theme);
        let text_color = match level {
            LogLevel::Debug => theme.text_primary,
            _ => theme.text_on_accent,
        };

        div()
            .flex_shrink_0()
            .w(px(48.0))
            .flex()
            .items_center()
            .justify_center()
            .px(theme.space_1)
            .py(px(1.0))
            .rounded(theme.radius_sm)
            .bg(bg)
            .text_size(theme.font_size_xs)
            .text_color(text_color)
            .font_weight(FontWeight::BOLD)
            .child(level.label().to_string())
    }

    /// Message text color varies by severity to draw attention to problems.
    fn message_color(level: LogLevel, theme: &HiveTheme) -> Hsla {
        match level {
            LogLevel::Error => theme.accent_red,
            LogLevel::Warning => theme.accent_yellow,
            _ => theme.text_primary,
        }
    }
}
