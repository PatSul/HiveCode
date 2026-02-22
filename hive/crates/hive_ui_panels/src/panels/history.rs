use chrono::{DateTime, Datelike, Local, NaiveDateTime, Utc};
use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::{Icon, IconName};

use hive_core::ConversationSummary;
use hive_core::persistence::ConversationRow;

use hive_ui_core::HiveTheme;
use hive_ui_core::{
    HistoryClearAll, HistoryClearAllCancel, HistoryClearAllConfirm,
    HistoryDeleteConversation, HistoryLoadConversation, HistoryRefresh,
};

// ---------------------------------------------------------------------------
// HistoryData — pre-loaded conversation data fed into the panel
// ---------------------------------------------------------------------------

/// Pre-loaded history data that the panel renders.
///
/// Constructed once from either a `ConversationStore` listing or from
/// `Database::list_conversations` rows, then passed into `HistoryPanel::render`.
#[derive(Debug, Clone)]
pub struct HistoryData {
    /// All conversations, already sorted newest-first.
    pub conversations: Vec<ConversationSummary>,
    /// Currently selected conversation ID, if any.
    pub selected_id: Option<String>,
    /// Current search/filter query (empty = show all).
    pub search_query: String,
    /// Whether the "Clear All" confirmation prompt is showing.
    pub confirming_clear: bool,
}

impl HistoryData {
    /// Returns an empty dataset (no conversations, no selection, no filter).
    pub fn empty() -> Self {
        Self {
            conversations: Vec::new(),
            selected_id: None,
            search_query: String::new(),
            confirming_clear: false,
        }
    }

    /// Builds history data from raw database rows (`ConversationRow`).
    ///
    /// Each row is converted into a `ConversationSummary`. Fields that the row
    /// does not carry (`preview`, `total_cost`) are filled with defaults.
    /// Rows are assumed to already be sorted by `updated_at DESC`.
    pub fn from_conversations(rows: &[ConversationRow]) -> Self {
        let conversations = rows
            .iter()
            .map(|row| {
                let created_at = parse_sqlite_datetime(&row.created_at);
                let updated_at = parse_sqlite_datetime(&row.updated_at);

                ConversationSummary {
                    id: row.id.clone(),
                    title: row.title.clone(),
                    preview: String::new(),
                    message_count: row.message_count,
                    total_cost: 0.0,
                    model: row.model.clone(),
                    created_at,
                    updated_at,
                }
            })
            .collect();

        Self {
            conversations,
            selected_id: None,
            search_query: String::new(),
            confirming_clear: false,
        }
    }

    /// Builds history data directly from pre-built summaries (e.g. from
    /// `ConversationStore::list_summaries`).
    pub fn from_summaries(summaries: Vec<ConversationSummary>) -> Self {
        Self {
            conversations: summaries,
            selected_id: None,
            search_query: String::new(),
            confirming_clear: false,
        }
    }

    /// Sets the selected conversation ID.
    pub fn with_selected(mut self, id: impl Into<String>) -> Self {
        self.selected_id = Some(id.into());
        self
    }

    /// Sets the search/filter query.
    pub fn with_search(mut self, query: impl Into<String>) -> Self {
        self.search_query = query.into();
        self
    }

    /// Returns conversations filtered by the current `search_query`.
    /// An empty query returns all conversations unmodified.
    pub fn filtered(&self) -> Vec<&ConversationSummary> {
        if self.search_query.is_empty() {
            return self.conversations.iter().collect();
        }
        let query_lower = self.search_query.to_lowercase();
        self.conversations
            .iter()
            .filter(|c| {
                c.title.to_lowercase().contains(&query_lower)
                    || c.model.to_lowercase().contains(&query_lower)
                    || c.preview.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Total number of conversations (before any filtering).
    pub fn total_count(&self) -> usize {
        self.conversations.len()
    }

    /// Returns a sample dataset with 2 pre-built conversations for testing.
    pub fn sample() -> Self {
        let now = chrono::Utc::now();
        Self {
            conversations: vec![
                ConversationSummary {
                    id: "conv-1".into(),
                    title: "Debugging auth flow".into(),
                    preview: "Let me check the login handler".into(),
                    message_count: 12,
                    total_cost: 0.05,
                    model: "claude-sonnet-4-5".into(),
                    created_at: now - chrono::Duration::hours(2),
                    updated_at: now - chrono::Duration::minutes(30),
                },
                ConversationSummary {
                    id: "conv-2".into(),
                    title: "Refactor database layer".into(),
                    preview: "We should use connection pooling".into(),
                    message_count: 8,
                    total_cost: 0.03,
                    model: "gpt-4o".into(),
                    created_at: now - chrono::Duration::days(1),
                    updated_at: now - chrono::Duration::hours(6),
                },
            ],
            selected_id: None,
            search_query: String::new(),
            confirming_clear: false,
        }
    }
}

// ---------------------------------------------------------------------------
// HistoryPanel
// ---------------------------------------------------------------------------

/// History panel: scrollable conversation browser (side panel, 280px).
pub struct HistoryPanel;

impl HistoryPanel {
    /// Renders the full history panel from pre-loaded `HistoryData`.
    pub fn render(data: &HistoryData, theme: &HiveTheme) -> impl IntoElement {
        let filtered = data.filtered();
        let filtered_count = filtered.len();
        let total = data.total_count();

        div()
            .id("history-panel")
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .bg(theme.bg_primary)
            .p(theme.space_4)
            .child(
                div()
                    .w_full()
                    .max_w(px(1080.0))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .rounded(theme.radius_lg)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(render_header(&data.search_query, data.confirming_clear, !data.conversations.is_empty(), theme))
                    .child(render_conversation_list(
                        &filtered,
                        data.selected_id.as_deref(),
                        theme,
                    ))
                    .child(render_stats_footer(filtered_count, total, theme)),
            )
    }
}

// ---------------------------------------------------------------------------
// Header with title and search field
// ---------------------------------------------------------------------------

fn render_header(
    search_query: &str,
    confirming_clear: bool,
    has_conversations: bool,
    theme: &HiveTheme,
) -> impl IntoElement {
    let bg_tertiary = theme.bg_tertiary;
    let text_muted = theme.text_muted;

    let mut header = div()
        .flex()
        .flex_col()
        .p(theme.space_3)
        .gap(theme.space_2)
        .border_b_1()
        .border_color(theme.border)
        // Title row with clear-all + refresh buttons
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(
                    div()
                        .text_size(theme.font_size_lg)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child("History"),
                )
                .child(div().flex_1())
                // Clear All button (only show when conversations exist)
                .when(has_conversations && !confirming_clear, |this: Div| {
                    this.child(
                        div()
                            .id("history-clear-all")
                            .cursor_pointer()
                            .p(theme.space_1)
                            .rounded(theme.radius_sm)
                            .hover(move |style: StyleRefinement| style.bg(bg_tertiary))
                            .child(
                                Icon::new(IconName::Delete)
                                    .size_3p5()
                                    .text_color(text_muted),
                            )
                            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                                window.dispatch_action(Box::new(HistoryClearAll), cx);
                            }),
                    )
                })
                .child(
                    div()
                        .id("history-refresh")
                        .cursor_pointer()
                        .p(theme.space_1)
                        .rounded(theme.radius_sm)
                        .hover(move |style: StyleRefinement| style.bg(bg_tertiary))
                        .child(
                            Icon::new(IconName::Redo2)
                                .size_3p5()
                                .text_color(text_muted),
                        )
                        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                            window.dispatch_action(Box::new(HistoryRefresh), cx);
                        }),
                ),
        )
        // Search input
        .child(render_search_field(search_query, theme));

    // Confirmation bar
    if confirming_clear {
        header = header.child(render_clear_confirmation(theme));
    }

    header
}

fn render_clear_confirmation(theme: &HiveTheme) -> impl IntoElement {
    let accent_red = theme.accent_red;
    let bg_tertiary = theme.bg_tertiary;

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_2)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.accent_red)
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child("Delete all conversations?"),
        )
        .child(
            div()
                .id("history-clear-confirm")
                .cursor_pointer()
                .px(theme.space_2)
                .py(theme.space_1)
                .rounded(theme.radius_sm)
                .bg(accent_red)
                .text_size(theme.font_size_xs)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_primary)
                .hover(move |style: StyleRefinement| {
                    style.bg(accent_red).opacity(0.8)
                })
                .child("Yes, delete all")
                .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                    window.dispatch_action(Box::new(HistoryClearAllConfirm), cx);
                }),
        )
        .child(
            div()
                .id("history-clear-cancel")
                .cursor_pointer()
                .px(theme.space_2)
                .py(theme.space_1)
                .rounded(theme.radius_sm)
                .bg(bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .hover(move |style: StyleRefinement| style.bg(bg_tertiary))
                .child("Cancel")
                .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                    window.dispatch_action(Box::new(HistoryClearAllCancel), cx);
                }),
        )
}

fn render_search_field(search_query: &str, theme: &HiveTheme) -> impl IntoElement {
    let placeholder = if search_query.is_empty() {
        "Search conversations..."
    } else {
        search_query
    };

    let text_color = if search_query.is_empty() {
        theme.text_muted
    } else {
        theme.text_primary
    };

    div()
        .flex()
        .items_center()
        .px(theme.space_2)
        .py(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .hover(|style: StyleRefinement| style.border_color(theme.border_focus))
        .child(
            // Magnifying glass icon
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

// ---------------------------------------------------------------------------
// Conversation list
// ---------------------------------------------------------------------------

fn render_conversation_list(
    conversations: &[&ConversationSummary],
    selected_id: Option<&str>,
    theme: &HiveTheme,
) -> AnyElement {
    if conversations.is_empty() {
        return render_empty_state(theme).into_any_element();
    }

    let mut list = div()
        .id("history-list")
        .flex()
        .flex_col()
        .flex_1()
        .overflow_y_scroll()
        .p(theme.space_2)
        .gap(theme.space_1);

    for summary in conversations {
        let is_selected = selected_id == Some(summary.id.as_str());
        list = list.child(render_conversation_card(summary, is_selected, theme));
    }

    list.into_any_element()
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

fn render_empty_state(theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_1()
        .gap(theme.space_2)
        .p(theme.space_4)
        .child(Icon::new(IconName::Calendar).size_4())
        .child(
            div()
                .text_size(theme.font_size_base)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child("No conversations yet"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Start chatting!"),
        )
}

// ---------------------------------------------------------------------------
// Conversation card
// ---------------------------------------------------------------------------

fn render_conversation_card(
    summary: &ConversationSummary,
    is_selected: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let title = truncate_title(&summary.title, 50);
    let date = format_relative_time(&summary.updated_at);
    let msg_count = format_message_count(summary.message_count);

    let left_border_color = if is_selected {
        theme.accent_aqua
    } else {
        Hsla::transparent_black()
    };

    let bg = if is_selected {
        theme.bg_tertiary
    } else {
        theme.bg_surface
    };

    let load_id = summary.id.clone();
    let delete_id = summary.id.clone();
    let delete_btn_element_id = SharedString::from(format!("conv-delete-{}", &summary.id));

    let mut card = div()
        .id(ElementId::Name(summary.id.clone().into()))
        .flex()
        .flex_col()
        .p(theme.space_2)
        .rounded(theme.radius_md)
        .bg(bg)
        .border_l_3()
        .border_color(left_border_color)
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.bg(theme.bg_tertiary))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(HistoryLoadConversation {
                    conversation_id: load_id.clone(),
                }),
                cx,
            );
        })
        .gap(theme.space_1)
        // Title row with delete button
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .flex_1()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::MEDIUM)
                        .overflow_hidden()
                        .child(title),
                )
                .child(
                    div()
                        .id(delete_btn_element_id)
                        .cursor_pointer()
                        .p(theme.space_1)
                        .rounded(theme.radius_sm)
                        .hover(|style: StyleRefinement| style.bg(theme.accent_red))
                        .child(
                            Icon::new(IconName::Delete)
                                .size_3()
                                .text_color(theme.text_muted),
                        )
                        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                            window.dispatch_action(
                                Box::new(HistoryDeleteConversation {
                                    conversation_id: delete_id.clone(),
                                }),
                                cx,
                            );
                        }),
                ),
        )
        // Metadata row: date + message count + model
        .child(render_card_metadata(
            &date,
            &msg_count,
            &summary.model,
            theme,
        ));

    // Preview snippet (only when non-empty)
    if !summary.preview.is_empty() {
        card = card.child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .overflow_hidden()
                .child(truncate_title(&summary.preview, 80)),
        );
    }

    card.into_any_element()
}

fn render_card_metadata(
    date: &str,
    msg_count: &str,
    model: &str,
    theme: &HiveTheme,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        // Relative timestamp
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(date.to_string()),
        )
        // Message count badge
        .child(
            div()
                .px(theme.space_1)
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(msg_count.to_string()),
        );

    // Model badge (only when non-empty)
    if !model.is_empty() {
        row = row.child(
            div()
                .px(theme.space_1)
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_cyan)
                .child(model.to_string()),
        );
    }

    row
}

// ---------------------------------------------------------------------------
// Stats footer
// ---------------------------------------------------------------------------

fn render_stats_footer(filtered: usize, total: usize, theme: &HiveTheme) -> impl IntoElement {
    let label = if filtered == total {
        // No filter active (or filter matches everything)
        if total == 1 {
            "1 conversation total".to_string()
        } else {
            format!("{total} conversations total")
        }
    } else {
        // Filter is active, show "X of Y"
        format!("{filtered} of {total} conversations")
    };

    div()
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_2)
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(label),
        )
}

// ---------------------------------------------------------------------------
// SQLite datetime parsing
// ---------------------------------------------------------------------------

/// Parses a SQLite datetime string (`"YYYY-MM-DD HH:MM:SS"`) into `DateTime<Utc>`.
/// Returns `Utc::now()` on parse failure so the panel never panics.
fn parse_sqlite_datetime(s: &str) -> DateTime<Utc> {
    // SQLite default format: "2026-02-08 14:30:00"
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|ndt| ndt.and_utc())
        .unwrap_or_else(|_| Utc::now())
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Truncates a title to `max_len` characters, appending "..." if needed.
/// Respects UTF-8 character boundaries.
fn truncate_title(title: &str, max_len: usize) -> String {
    let trimmed = title.trim();
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }
    let boundary = trimmed
        .char_indices()
        .take_while(|(i, _)| *i < max_len)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max_len);
    format!("{}...", &trimmed[..boundary])
}

/// Formats a UTC timestamp as a human-friendly relative time string:
/// - < 1 minute:  "Just now"
/// - < 60 minutes: "X minutes ago"
/// - < 24 hours:   "X hours ago"
/// - < 7 days:     "Yesterday" or "X days ago"
/// - Same year:    "Jan 5"
/// - Older:        "Jan 5, 2024"
fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);

    let total_seconds = duration.num_seconds();
    if total_seconds < 0 {
        // Future timestamp -- fall back to absolute date
        return format_absolute_date(dt);
    }

    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if minutes < 1 {
        return "Just now".to_string();
    }
    if minutes == 1 {
        return "1 minute ago".to_string();
    }
    if minutes < 60 {
        return format!("{minutes} minutes ago");
    }
    if hours == 1 {
        return "1 hour ago".to_string();
    }
    if hours < 24 {
        return format!("{hours} hours ago");
    }
    if days == 1 {
        return "Yesterday".to_string();
    }
    if days < 7 {
        return format!("{days} days ago");
    }

    format_absolute_date(dt)
}

/// Formats an absolute date: "Jan 5" (same year) or "Jan 5, 2024" (different year).
fn format_absolute_date(dt: &DateTime<Utc>) -> String {
    let local: DateTime<Local> = dt.with_timezone(&Local);
    let now = Local::now();

    if local.year() == now.year() {
        local.format("%b %-d").to_string()
    } else {
        local.format("%b %-d, %Y").to_string()
    }
}

fn format_message_count(count: usize) -> String {
    if count == 1 {
        "1 msg".to_string()
    } else {
        format!("{count} msgs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- parse_sqlite_datetime ----

    #[test]
    fn parse_valid_sqlite_datetime() {
        let dt = parse_sqlite_datetime("2026-02-08 14:30:00");
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 2);
        assert_eq!(dt.day(), 8);
    }

    #[test]
    fn parse_invalid_returns_now() {
        let before = Utc::now();
        let dt = parse_sqlite_datetime("not-a-date");
        let after = Utc::now();
        assert!(dt >= before && dt <= after);
    }

    // ---- truncate_title ----

    #[test]
    fn short_title_unchanged() {
        assert_eq!(truncate_title("Hello", 10), "Hello");
    }

    #[test]
    fn exact_length_title_unchanged() {
        assert_eq!(truncate_title("12345", 5), "12345");
    }

    #[test]
    fn over_limit_gets_ellipsis() {
        assert_eq!(truncate_title("Hello World!", 5), "Hello...");
    }

    #[test]
    fn unicode_safe_truncation() {
        // 4-byte UTF-8 chars should not break mid-character
        let title = "Hello \u{1F600} World";
        let result = truncate_title(title, 8);
        assert!(result.ends_with("..."));
        assert!(result.is_char_boundary(result.len()));
    }

    // ---- format_message_count ----

    #[test]
    fn singular_message_count() {
        assert_eq!(format_message_count(1), "1 msg");
    }

    #[test]
    fn plural_message_count() {
        assert_eq!(format_message_count(5), "5 msgs");
    }

    #[test]
    fn zero_message_count() {
        assert_eq!(format_message_count(0), "0 msgs");
    }

    // ---- format_relative_time ----

    #[test]
    fn just_now() {
        let now = Utc::now();
        assert_eq!(format_relative_time(&now), "Just now");
    }

    #[test]
    fn minutes_ago() {
        let dt = Utc::now() - chrono::Duration::minutes(5);
        assert_eq!(format_relative_time(&dt), "5 minutes ago");
    }

    #[test]
    fn hours_ago() {
        let dt = Utc::now() - chrono::Duration::hours(3);
        assert_eq!(format_relative_time(&dt), "3 hours ago");
    }
}
