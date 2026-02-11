//! Learning panel — Continuous self-improvement dashboard.
//!
//! Displays performance metrics, learning log, preferences, prompt suggestions,
//! pattern library, routing insights, and self-evaluation reports.

use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Display data for a learning log entry.
#[derive(Debug, Clone)]
pub struct LogEntryDisplay {
    pub event_type: String,
    pub description: String,
    pub timestamp: String,
}

/// Display data for a learned preference.
#[derive(Debug, Clone)]
pub struct PreferenceDisplay {
    pub key: String,
    pub value: String,
    pub confidence: f64,
}

/// Display data for a prompt suggestion.
#[derive(Debug, Clone)]
pub struct PromptSuggestionDisplay {
    pub persona: String,
    pub reason: String,
    pub current_quality: f64,
}

/// Display data for a routing insight.
#[derive(Debug, Clone)]
pub struct RoutingInsightDisplay {
    pub task_type: String,
    pub from_tier: String,
    pub to_tier: String,
    pub confidence: f64,
}

/// Display data for quality metrics.
#[derive(Debug, Clone)]
pub struct QualityMetrics {
    pub overall_quality: f64,
    pub trend: String,
    pub total_interactions: u64,
    pub correction_rate: f64,
    pub regeneration_rate: f64,
    pub cost_efficiency: f64,
}

impl QualityMetrics {
    pub fn empty() -> Self {
        Self {
            overall_quality: 0.0,
            trend: "Stable".into(),
            total_interactions: 0,
            correction_rate: 0.0,
            regeneration_rate: 0.0,
            cost_efficiency: 0.0,
        }
    }
}

/// All data needed to render the learning panel.
#[derive(Debug, Clone)]
pub struct LearningPanelData {
    pub metrics: QualityMetrics,
    pub log_entries: Vec<LogEntryDisplay>,
    pub preferences: Vec<PreferenceDisplay>,
    pub prompt_suggestions: Vec<PromptSuggestionDisplay>,
    pub routing_insights: Vec<RoutingInsightDisplay>,
    pub weak_areas: Vec<String>,
    pub best_model: Option<String>,
    pub worst_model: Option<String>,
}

impl LearningPanelData {
    pub fn empty() -> Self {
        Self {
            metrics: QualityMetrics::empty(),
            log_entries: Vec::new(),
            preferences: Vec::new(),
            prompt_suggestions: Vec::new(),
            routing_insights: Vec::new(),
            weak_areas: Vec::new(),
            best_model: None,
            worst_model: None,
        }
    }

    pub fn sample() -> Self {
        Self {
            metrics: QualityMetrics {
                overall_quality: 0.78,
                trend: "Improving".into(),
                total_interactions: 142,
                correction_rate: 0.12,
                regeneration_rate: 0.04,
                cost_efficiency: 0.032,
            },
            log_entries: vec![
                LogEntryDisplay {
                    event_type: "outcome_recorded".into(),
                    description: "Accepted response for model claude-sonnet-4 (quality: 0.90)".into(),
                    timestamp: "2m ago".into(),
                },
                LogEntryDisplay {
                    event_type: "routing_analysis".into(),
                    description: "Analyzed 50 interactions — no adjustments needed".into(),
                    timestamp: "15m ago".into(),
                },
                LogEntryDisplay {
                    event_type: "preference_learned".into(),
                    description: "Learned: code_style.naming = snake_case (confidence: 0.85)".into(),
                    timestamp: "1h ago".into(),
                },
            ],
            preferences: vec![
                PreferenceDisplay {
                    key: "code_style.naming".into(),
                    value: "snake_case".into(),
                    confidence: 0.85,
                },
                PreferenceDisplay {
                    key: "response_style.verbosity".into(),
                    value: "concise".into(),
                    confidence: 0.72,
                },
            ],
            prompt_suggestions: Vec::new(),
            routing_insights: vec![RoutingInsightDisplay {
                task_type: "debugging".into(),
                from_tier: "Budget".into(),
                to_tier: "Mid".into(),
                confidence: 0.78,
            }],
            weak_areas: vec!["regex_generation".into()],
            best_model: Some("claude-sonnet-4.5".into()),
            worst_model: Some("llama-3.1-8b".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct LearningPanel;

impl LearningPanel {
    pub fn render(data: &LearningPanelData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("learning-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(theme))
            .child(render_metrics_section(&data.metrics, theme))
            .child(render_model_performance(
                &data.best_model,
                &data.worst_model,
                &data.weak_areas,
                theme,
            ))
            .child(render_preferences_section(&data.preferences, theme))
            .child(render_routing_section(&data.routing_insights, theme))
            .child(render_log_section(&data.log_entries, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
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
                .child(Icon::new(IconName::Redo2).size_4()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_xl)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Continuous Learning"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Self-improvement through outcome tracking and adaptation"),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Metrics dashboard
// ---------------------------------------------------------------------------

fn render_metrics_section(metrics: &QualityMetrics, theme: &HiveTheme) -> AnyElement {
    let trend_color = match metrics.trend.as_str() {
        "Improving" => theme.accent_green,
        "Declining" => theme.accent_red,
        _ => theme.text_muted,
    };

    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Performance", theme))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(theme.space_3)
                .child(metric_card(
                    "Quality",
                    &format!("{:.0}%", metrics.overall_quality * 100.0),
                    theme.accent_cyan,
                    theme,
                ))
                .child(metric_card("Trend", &metrics.trend, trend_color, theme))
                .child(metric_card(
                    "Interactions",
                    &metrics.total_interactions.to_string(),
                    theme.text_secondary,
                    theme,
                ))
                .child(metric_card(
                    "Corrections",
                    &format!("{:.0}%", metrics.correction_rate * 100.0),
                    theme.accent_yellow,
                    theme,
                ))
                .child(metric_card(
                    "Regenerations",
                    &format!("{:.0}%", metrics.regeneration_rate * 100.0),
                    theme.accent_red,
                    theme,
                ))
                .child(metric_card(
                    "$/Quality",
                    &format!("${:.3}", metrics.cost_efficiency),
                    theme.accent_aqua,
                    theme,
                )),
        )
        .into_any_element()
}

fn metric_card(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w(px(120.0))
        .p(theme.space_3)
        .gap(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(color)
                .font_weight(FontWeight::BOLD)
                .child(value.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Model performance
// ---------------------------------------------------------------------------

fn render_model_performance(
    best: &Option<String>,
    worst: &Option<String>,
    weak_areas: &[String],
    theme: &HiveTheme,
) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_4)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(section_title("Model Insights", theme));

    if let Some(b) = best {
        section = section.child(insight_row("Best model", b, theme.accent_green, theme));
    }
    if let Some(w) = worst {
        section = section.child(insight_row("Worst model", w, theme.accent_red, theme));
    }

    if !weak_areas.is_empty() {
        section = section.child(insight_row(
            "Weak areas",
            &weak_areas.join(", "),
            theme.accent_yellow,
            theme,
        ));
    }

    section.into_any_element()
}

fn insight_row(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .min_w(px(90.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(color)
                .child(value.to_string()),
        )
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

fn render_preferences_section(prefs: &[PreferenceDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Learned Preferences", theme));

    if prefs.is_empty() {
        section = section.child(empty_state("No preferences learned yet", theme));
    } else {
        for pref in prefs {
            section = section.child(render_preference_row(pref, theme));
        }
    }

    section.into_any_element()
}

fn render_preference_row(pref: &PreferenceDisplay, theme: &HiveTheme) -> AnyElement {
    let conf_color = if pref.confidence > 0.8 {
        theme.accent_green
    } else if pref.confidence > 0.5 {
        theme.accent_yellow
    } else {
        theme.text_muted
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .p(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .min_w(px(160.0))
                .child(pref.key.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::MEDIUM)
                .child(pref.value.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(conf_color)
                .child(format!("{:.0}%", pref.confidence * 100.0)),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Routing insights
// ---------------------------------------------------------------------------

fn render_routing_section(insights: &[RoutingInsightDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Routing Insights", theme));

    if insights.is_empty() {
        section = section.child(empty_state("No routing adjustments yet", theme));
    } else {
        for insight in insights {
            section = section.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .p(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_primary)
                            .child(insight.task_type.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(format!("{} -> {}", insight.from_tier, insight.to_tier)),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_cyan)
                            .child(format!("{:.0}% conf", insight.confidence * 100.0)),
                    ),
            );
        }
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Learning log
// ---------------------------------------------------------------------------

fn render_log_section(entries: &[LogEntryDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Learning Log", theme));

    if entries.is_empty() {
        section = section.child(empty_state("No learning events recorded yet", theme));
    } else {
        for entry in entries {
            section = section.child(render_log_entry(entry, theme));
        }
    }

    section.into_any_element()
}

fn render_log_entry(entry: &LogEntryDisplay, theme: &HiveTheme) -> AnyElement {
    let type_color = match entry.event_type.as_str() {
        "outcome_recorded" => theme.accent_cyan,
        "routing_analysis" => theme.accent_aqua,
        "preference_learned" => theme.accent_green,
        "self_evaluation" => theme.accent_yellow,
        _ => theme.text_secondary,
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .py(theme.space_1)
        .child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(type_color)
                .min_w(px(100.0))
                .child(entry.event_type.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(entry.description.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(entry.timestamp.clone()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn section_title(title: &str, theme: &HiveTheme) -> Div {
    div()
        .text_size(theme.font_size_lg)
        .text_color(theme.text_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .child(title.to_string())
}

fn empty_state(message: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(theme.space_4)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(message.to_string()),
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
    fn learning_panel_data_empty() {
        let data = LearningPanelData::empty();
        assert!(data.log_entries.is_empty());
        assert!(data.preferences.is_empty());
        assert!(data.prompt_suggestions.is_empty());
        assert!(data.routing_insights.is_empty());
        assert!(data.weak_areas.is_empty());
        assert!(data.best_model.is_none());
        assert!(data.worst_model.is_none());
    }

    #[test]
    fn learning_panel_data_sample() {
        let data = LearningPanelData::sample();
        assert!(!data.log_entries.is_empty());
        assert!(!data.preferences.is_empty());
        assert!(!data.routing_insights.is_empty());
        assert!(data.best_model.is_some());
        assert!(data.worst_model.is_some());
    }

    #[test]
    fn quality_metrics_empty() {
        let m = QualityMetrics::empty();
        assert_eq!(m.overall_quality, 0.0);
        assert_eq!(m.trend, "Stable");
        assert_eq!(m.total_interactions, 0);
    }
}
