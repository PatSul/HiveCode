use gpui::*;
use std::collections::HashMap;

use hive_ai::CostTracker;

use hive_ui_core::HiveTheme;
use hive_ui_core::{CostsClearHistory, CostsExportCsv, CostsResetToday};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single model's aggregated cost entry for the dashboard table.
#[derive(Debug, Clone)]
pub struct ModelCostEntry {
    pub model_id: String,
    pub requests: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost: f64,
}

/// All data needed to render the cost dashboard.
///
/// Built from `CostTracker` via `from_tracker()` each time the panel renders,
/// so it always reflects the latest state.
#[derive(Debug, Clone)]
pub struct CostData {
    /// Total cost incurred today (UTC).
    pub today_cost: f64,
    /// Total cost across all recorded history.
    pub all_time_cost: f64,
    /// Total API calls (requests) recorded.
    pub total_requests: usize,
    /// Total input tokens across all requests.
    pub total_input_tokens: usize,
    /// Total output tokens across all requests.
    pub total_output_tokens: usize,
    /// Per-model breakdown, sorted descending by cost.
    pub by_model: Vec<ModelCostEntry>,
}

impl CostData {
    /// Build a snapshot of all cost data from the live tracker.
    pub fn from_tracker(tracker: &CostTracker) -> Self {
        // Aggregate per-model stats from individual records.
        let mut model_map: HashMap<String, (usize, usize, usize, f64)> = HashMap::new();
        for record in tracker.records() {
            let entry = model_map
                .entry(record.model_id.clone())
                .or_insert((0, 0, 0, 0.0));
            entry.0 += 1; // requests
            entry.1 += record.input_tokens;
            entry.2 += record.output_tokens;
            entry.3 += record.cost;
        }

        let mut by_model: Vec<ModelCostEntry> = model_map
            .into_iter()
            .map(
                |(model_id, (requests, input_tokens, output_tokens, cost))| ModelCostEntry {
                    model_id,
                    requests,
                    input_tokens,
                    output_tokens,
                    cost,
                },
            )
            .collect();

        // Sort descending by cost so the most expensive model is first.
        by_model.sort_by(|a, b| {
            b.cost
                .partial_cmp(&a.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self {
            today_cost: tracker.today_cost(),
            all_time_cost: tracker.total_cost(),
            total_requests: tracker.total_calls(),
            total_input_tokens: tracker.total_input_tokens(),
            total_output_tokens: tracker.total_output_tokens(),
            by_model,
        }
    }

    /// An empty snapshot for fresh state (no usage recorded yet).
    pub fn empty() -> Self {
        Self {
            today_cost: 0.0,
            all_time_cost: 0.0,
            total_requests: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            by_model: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Cost dashboard with summary cards, model breakdown table, and action buttons.
pub struct CostsPanel;

impl CostsPanel {
    /// Main entry point -- renders the full dashboard from live cost data.
    pub fn render(data: &CostData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("costs-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(Self::header(theme))
            .child(Self::summary_cards(data, theme))
            .child(Self::model_table(data, theme))
            .child(Self::action_buttons(theme))
    }

    // ------------------------------------------------------------------
    // Header
    // ------------------------------------------------------------------

    fn header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_2xl)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::BOLD)
                    .child("Cost Dashboard".to_string()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("Live".to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Summary cards (4 across)
    // ------------------------------------------------------------------

    fn summary_cards(data: &CostData, theme: &HiveTheme) -> impl IntoElement {
        let total_tokens = data.total_input_tokens + data.total_output_tokens;

        div()
            .flex()
            .flex_row()
            .gap(theme.space_3)
            .child(Self::card(
                "Today",
                &format!("${:.2}", data.today_cost),
                "spent today",
                theme.accent_aqua,
                theme,
            ))
            .child(Self::card(
                "All Time",
                &format!("${:.2}", data.all_time_cost),
                "total spend",
                theme.accent_cyan,
                theme,
            ))
            .child(Self::card(
                "API Calls",
                &Self::fmt_number(data.total_requests),
                "total requests",
                theme.accent_powder,
                theme,
            ))
            .child(Self::card(
                "Tokens",
                &Self::fmt_number(total_tokens),
                &format!(
                    "{}in + {}out",
                    Self::fmt_compact(data.total_input_tokens),
                    Self::fmt_compact(data.total_output_tokens),
                ),
                theme.accent_green,
                theme,
            ))
    }

    /// A single summary card with label, big value, subtitle, and accent color.
    fn card(
        label: &str,
        value: &str,
        subtitle: &str,
        accent: Hsla,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .p(theme.space_3)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .gap(theme.space_1)
            // Label row
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(label.to_string()),
            )
            // Big value
            .child(
                div()
                    .text_size(theme.font_size_xl)
                    .text_color(accent)
                    .font_weight(FontWeight::BOLD)
                    .child(value.to_string()),
            )
            // Subtitle
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(subtitle.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Usage by Model (table)
    // ------------------------------------------------------------------

    fn model_table(data: &CostData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            // Section title
            .child(
                div()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb(theme.space_2)
                    .child("Usage by Model".to_string()),
            )
            // Column headers
            .child(Self::table_header(theme));

        // Data rows (already sorted descending by cost from CostData::from_tracker)
        for (idx, entry) in data.by_model.iter().enumerate() {
            let dot_color = Self::provider_color(idx, theme);
            container = container.child(Self::table_row(entry, dot_color, theme));
        }

        // Empty-state hint if no models
        if data.by_model.is_empty() {
            container = container.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(theme.space_6)
                    .child(
                        div()
                            .text_size(theme.font_size_base)
                            .text_color(theme.text_muted)
                            .child(
                                "No usage data yet -- start chatting to see breakdown".to_string(),
                            ),
                    ),
            );
        }

        container
    }

    /// Header row for the model table.
    fn table_header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .pb(theme.space_1)
            .border_b_1()
            .border_color(theme.border)
            // Model column (flex-1)
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Model".to_string()),
            )
            // Requests column
            .child(
                div()
                    .w(px(72.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Requests".to_string()),
            )
            // Input Tokens column
            .child(
                div()
                    .w(px(96.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Input Tok".to_string()),
            )
            // Output Tokens column
            .child(
                div()
                    .w(px(96.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Output Tok".to_string()),
            )
            // Cost column
            .child(
                div()
                    .w(px(72.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Cost".to_string()),
            )
    }

    /// A single data row in the model table.
    fn table_row(entry: &ModelCostEntry, dot_color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            // Model name with colored dot
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        // Colored dot
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(theme.radius_full)
                            .bg(dot_color),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_primary)
                            .child(entry.model_id.clone()),
                    ),
            )
            // Requests
            .child(
                div()
                    .w(px(72.0))
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_secondary)
                    .child(Self::fmt_number(entry.requests)),
            )
            // Input tokens
            .child(
                div()
                    .w(px(96.0))
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_secondary)
                    .child(Self::fmt_number(entry.input_tokens)),
            )
            // Output tokens
            .child(
                div()
                    .w(px(96.0))
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_secondary)
                    .child(Self::fmt_number(entry.output_tokens)),
            )
            // Cost
            .child(
                div()
                    .w(px(72.0))
                    .text_size(theme.font_size_sm)
                    .text_color(if entry.cost > 0.0 {
                        theme.accent_aqua
                    } else {
                        theme.accent_green
                    })
                    .font_weight(FontWeight::MEDIUM)
                    .child(if entry.cost > 0.0 {
                        format!("${:.4}", entry.cost)
                    } else {
                        "Free".to_string()
                    }),
            )
    }

    // ------------------------------------------------------------------
    // Action buttons row
    // ------------------------------------------------------------------

    fn action_buttons(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .gap(theme.space_3)
            .child(
                Self::action_btn("Export CSV", "costs-export-csv", theme.accent_cyan, theme)
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.dispatch_action(&CostsExportCsv);
                    }),
            )
            .child(
                Self::action_btn(
                    "Reset Today",
                    "costs-reset-today",
                    theme.accent_yellow,
                    theme,
                )
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.dispatch_action(&CostsResetToday);
                }),
            )
            .child(div().flex_1()) // spacer pushes Clear to the right
            .child(
                Self::action_btn(
                    "Clear History",
                    "costs-clear-history",
                    theme.accent_red,
                    theme,
                )
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.dispatch_action(&CostsClearHistory);
                }),
            )
    }

    /// A styled text button with accent color and unique ID for click handling.
    fn action_btn(label: &str, id: &str, color: Hsla, theme: &HiveTheme) -> Stateful<Div> {
        div()
            .id(SharedString::from(id.to_string()))
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_sm)
            .text_color(color)
            .cursor_pointer()
            .child(label.to_string())
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Pick a dot color per model index to distinguish providers visually.
    fn provider_color(index: usize, theme: &HiveTheme) -> Hsla {
        let palette = [
            theme.accent_aqua,
            theme.accent_cyan,
            theme.accent_pink,
            theme.accent_yellow,
            theme.accent_green,
            theme.accent_powder,
            theme.accent_red,
        ];
        palette[index % palette.len()]
    }

    /// Format a number with comma separators (e.g. 1,234,567).
    fn fmt_number(n: usize) -> String {
        if n == 0 {
            return "0".to_string();
        }
        let s = n.to_string();
        let mut result = String::with_capacity(s.len() + s.len() / 3);
        for (i, ch) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(ch);
        }
        result.chars().rev().collect()
    }

    /// Compact format for large token counts (e.g. "519K", "1.2M").
    fn fmt_compact(n: usize) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M ", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{}K ", n / 1_000)
        } else {
            format!("{} ", n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> HiveTheme {
        HiveTheme::dark()
    }

    // ---- fmt_number ----

    #[test]
    fn fmt_number_zero() {
        assert_eq!(CostsPanel::fmt_number(0), "0");
    }

    #[test]
    fn fmt_number_single_digit() {
        assert_eq!(CostsPanel::fmt_number(5), "5");
    }

    #[test]
    fn fmt_number_three_digits() {
        assert_eq!(CostsPanel::fmt_number(999), "999");
    }

    #[test]
    fn fmt_number_thousand() {
        assert_eq!(CostsPanel::fmt_number(1000), "1,000");
    }

    #[test]
    fn fmt_number_millions() {
        assert_eq!(CostsPanel::fmt_number(1_234_567), "1,234,567");
    }

    // ---- fmt_compact ----

    #[test]
    fn fmt_compact_small() {
        assert_eq!(CostsPanel::fmt_compact(42), "42 ");
    }

    #[test]
    fn fmt_compact_thousands() {
        assert_eq!(CostsPanel::fmt_compact(519_000), "519K ");
    }

    #[test]
    fn fmt_compact_millions() {
        assert_eq!(CostsPanel::fmt_compact(1_200_000), "1.2M ");
    }

    // ---- provider_color ----

    #[test]
    fn provider_color_cycles_at_palette_length() {
        let t = theme();
        // Palette has 7 entries, so index 0 and 7 should be the same
        assert_eq!(CostsPanel::provider_color(0, &t), CostsPanel::provider_color(7, &t));
        assert_eq!(CostsPanel::provider_color(1, &t), CostsPanel::provider_color(8, &t));
    }
}
