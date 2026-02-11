use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Which view the specs panel is currently displaying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecViewMode {
    List,
    Detail,
    Edit,
}

/// Summary of a single specification for the list view.
#[derive(Debug, Clone)]
pub struct SpecSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub entries_total: usize,
    pub entries_checked: usize,
    pub updated_at: String,
}

impl SpecSummary {
    /// Progress as a fraction from 0.0 to 1.0.
    pub fn progress(&self) -> f32 {
        if self.entries_total == 0 {
            return 0.0;
        }
        self.entries_checked as f32 / self.entries_total as f32
    }
}

/// All data needed to render the specifications panel.
#[derive(Debug, Clone)]
pub struct SpecPanelData {
    pub specs: Vec<SpecSummary>,
    pub active_spec_id: Option<String>,
    pub view_mode: SpecViewMode,
}

impl SpecPanelData {
    /// Create an empty state with no specifications.
    pub fn empty() -> Self {
        Self {
            specs: Vec::new(),
            active_spec_id: None,
            view_mode: SpecViewMode::List,
        }
    }

    /// Return a sample dataset for preview / testing.
    pub fn sample() -> Self {
        Self {
            specs: vec![
                SpecSummary {
                    id: "spec-001".into(),
                    title: "Authentication Overhaul".into(),
                    status: "In Progress".into(),
                    entries_total: 12,
                    entries_checked: 7,
                    updated_at: "2 hours ago".into(),
                },
                SpecSummary {
                    id: "spec-002".into(),
                    title: "API Rate Limiting".into(),
                    status: "Draft".into(),
                    entries_total: 8,
                    entries_checked: 0,
                    updated_at: "1 day ago".into(),
                },
                SpecSummary {
                    id: "spec-003".into(),
                    title: "Database Migration v2".into(),
                    status: "Complete".into(),
                    entries_total: 5,
                    entries_checked: 5,
                    updated_at: "3 days ago".into(),
                },
            ],
            active_spec_id: None,
            view_mode: SpecViewMode::List,
        }
    }

    /// Find the active spec by ID.
    pub fn active_spec(&self) -> Option<&SpecSummary> {
        let id = self.active_spec_id.as_ref()?;
        self.specs.iter().find(|s| &s.id == id)
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Live specifications panel: lists specs with progress, allows drill-down
/// into detail/edit views.
pub struct SpecsPanel;

impl SpecsPanel {
    pub fn render(data: &SpecPanelData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("specs-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(data.specs.len(), theme))
            .child(render_body(data, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(spec_count: usize, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(header_icon(theme))
        .child(header_title(theme))
        .child(div().flex_1())
        .child(spec_count_badge(spec_count, theme))
        .child(new_spec_button(theme))
        .into_any_element()
}

fn header_icon(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(40.0))
        .h(px(40.0))
        .rounded(theme.radius_lg)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(Icon::new(IconName::File).size_4())
}

fn header_title(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(theme.font_size_xl)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("Specifications"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Track requirements, plans, and progress"),
        )
}

fn spec_count_badge(count: usize, theme: &HiveTheme) -> Div {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.text_secondary)
        .child(format!("{count} specs"))
}

fn new_spec_button(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.accent_cyan)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_on_accent)
        .child("+ New Spec")
}

// ---------------------------------------------------------------------------
// Body router
// ---------------------------------------------------------------------------

fn render_body(data: &SpecPanelData, theme: &HiveTheme) -> AnyElement {
    match data.view_mode {
        SpecViewMode::List => render_list_view(data, theme),
        SpecViewMode::Detail | SpecViewMode::Edit => render_detail_view(data, theme),
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn render_list_view(data: &SpecPanelData, theme: &HiveTheme) -> AnyElement {
    if data.specs.is_empty() {
        return render_empty_state(theme);
    }

    let mut list = div().flex().flex_col().gap(theme.space_3);

    for spec in &data.specs {
        list = list.child(render_spec_card(spec, theme));
    }

    list.into_any_element()
}

fn render_spec_card(spec: &SpecSummary, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(spec_card_top_row(spec, theme))
        .child(spec_progress_bar(spec, theme))
        .child(spec_card_footer(spec, theme))
        .into_any_element()
}

fn spec_card_top_row(spec: &SpecSummary, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(spec.title.clone()),
        )
        .child(div().flex_1())
        .child(status_badge(&spec.status, theme))
}

fn status_badge(status: &str, theme: &HiveTheme) -> Div {
    let color = match status {
        "Complete" => theme.accent_green,
        "In Progress" => theme.accent_aqua,
        "Draft" => theme.text_muted,
        _ => theme.text_secondary,
    };

    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .child(status.to_string())
}

fn spec_progress_bar(spec: &SpecSummary, theme: &HiveTheme) -> Div {
    let progress = spec.progress();
    let bar_color = if progress >= 1.0 {
        theme.accent_green
    } else if progress > 0.0 {
        theme.accent_aqua
    } else {
        theme.text_muted
    };

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!(
                            "{}/{} entries",
                            spec.entries_checked, spec.entries_total
                        )),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(bar_color)
                        .child(format!("{}%", (progress * 100.0) as u32)),
                ),
        )
        .child(
            // Track
            div()
                .w_full()
                .h(px(6.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .child(
                    // Fill
                    div()
                        .h(px(6.0))
                        .rounded(theme.radius_full)
                        .bg(bar_color)
                        .w(relative(progress)),
                ),
        )
}

fn spec_card_footer(spec: &SpecSummary, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("Updated {}", spec.updated_at)),
        )
}

// ---------------------------------------------------------------------------
// Detail view
// ---------------------------------------------------------------------------

fn render_detail_view(data: &SpecPanelData, theme: &HiveTheme) -> AnyElement {
    let spec = match data.active_spec() {
        Some(s) => s,
        None => {
            return div()
                .flex()
                .items_center()
                .justify_center()
                .flex_1()
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Select a specification to view details."),
                )
                .into_any_element();
        }
    };

    div()
        .flex()
        .flex_col()
        .gap(theme.space_4)
        .child(detail_header(spec, theme))
        .child(collapsible_section("Requirements", theme))
        .child(collapsible_section("Plan", theme))
        .child(collapsible_section("Progress", theme))
        .child(collapsible_section("Notes", theme))
        .into_any_element()
}

fn detail_header(spec: &SpecSummary, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(back_button(theme))
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(spec.title.clone()),
        )
        .child(div().flex_1())
        .child(status_badge(&spec.status, theme))
        .into_any_element()
}

fn back_button(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_2)
        .py(theme.space_1)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_sm)
        .text_color(theme.text_secondary)
        .child("\u{2190} Back")
}

fn collapsible_section(title: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("\u{25B6}"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .py(theme.space_2)
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("No entries yet."),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

fn render_empty_state(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_1()
        .gap(theme.space_2)
        .p(theme.space_8)
        .child(
            div()
                .text_size(px(32.0))
                .text_color(theme.text_muted)
                .child("\u{1F4CB}"),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child("No specifications yet"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Create one to get started."),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_summary_progress_empty() {
        let spec = SpecSummary {
            id: "s1".into(),
            title: "Test".into(),
            status: "Draft".into(),
            entries_total: 0,
            entries_checked: 0,
            updated_at: "now".into(),
        };
        assert_eq!(spec.progress(), 0.0);
    }

    #[test]
    fn spec_summary_progress_partial() {
        let spec = SpecSummary {
            id: "s2".into(),
            title: "Test".into(),
            status: "In Progress".into(),
            entries_total: 10,
            entries_checked: 3,
            updated_at: "now".into(),
        };
        let p = spec.progress();
        assert!((p - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn spec_summary_progress_complete() {
        let spec = SpecSummary {
            id: "s3".into(),
            title: "Test".into(),
            status: "Complete".into(),
            entries_total: 5,
            entries_checked: 5,
            updated_at: "now".into(),
        };
        assert_eq!(spec.progress(), 1.0);
    }

    #[test]
    fn spec_panel_data_empty() {
        let data = SpecPanelData::empty();
        assert!(data.specs.is_empty());
        assert_eq!(data.view_mode, SpecViewMode::List);
        assert!(data.active_spec_id.is_none());
    }

    #[test]
    fn spec_panel_data_sample_has_specs() {
        let data = SpecPanelData::sample();
        assert_eq!(data.specs.len(), 3);
    }

    #[test]
    fn spec_panel_data_active_spec_lookup() {
        let mut data = SpecPanelData::sample();
        assert!(data.active_spec().is_none());

        data.active_spec_id = Some("spec-002".into());
        let spec = data.active_spec().expect("should find spec-002");
        assert_eq!(spec.title, "API Rate Limiting");
    }

    #[test]
    fn spec_panel_data_active_spec_missing() {
        let mut data = SpecPanelData::sample();
        data.active_spec_id = Some("nonexistent".into());
        assert!(data.active_spec().is_none());
    }
}
