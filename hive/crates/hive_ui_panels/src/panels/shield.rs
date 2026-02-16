use gpui::*;
use gpui_component::{Icon, IconName};

use hive_ui_core::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single security event detected by the privacy shield.
#[derive(Debug, Clone)]
pub struct ShieldEvent {
    pub timestamp: String,
    pub event_type: String,
    pub severity: String,
    pub detail: String,
}

impl ShieldEvent {
    /// Map severity string to a color from the theme.
    pub fn severity_color(&self, theme: &HiveTheme) -> Hsla {
        match self.severity.as_str() {
            "critical" | "high" => theme.accent_red,
            "medium" | "warning" => theme.accent_yellow,
            "low" | "info" => theme.accent_cyan,
            _ => theme.text_muted,
        }
    }
}

/// Access policy for a specific AI provider.
#[derive(Debug, Clone)]
pub struct PolicyDisplay {
    pub provider: String,
    pub trust_level: String,
    pub max_classification: String,
    pub pii_cloaking: bool,
}

/// All data needed to render the privacy shield panel.
#[derive(Debug, Clone)]
pub struct ShieldPanelData {
    pub enabled: bool,
    pub pii_detections: usize,
    pub secrets_blocked: usize,
    pub threats_caught: usize,
    pub recent_events: Vec<ShieldEvent>,
    pub policies: Vec<PolicyDisplay>,
}

impl ShieldPanelData {
    /// Create a default state with shield enabled but no events.
    pub fn empty() -> Self {
        Self {
            enabled: true,
            pii_detections: 0,
            secrets_blocked: 0,
            threats_caught: 0,
            recent_events: Vec::new(),
            policies: Vec::new(),
        }
    }

    /// Return a sample dataset for preview / testing.
    #[allow(dead_code)]
    pub fn sample() -> Self {
        Self {
            enabled: true,
            pii_detections: 14,
            secrets_blocked: 3,
            threats_caught: 1,
            recent_events: vec![
                ShieldEvent {
                    timestamp: "2 min ago".into(),
                    event_type: "PII Detected".into(),
                    severity: "medium".into(),
                    detail: "Email address cloaked in prompt to Anthropic".into(),
                },
                ShieldEvent {
                    timestamp: "15 min ago".into(),
                    event_type: "Secret Blocked".into(),
                    severity: "high".into(),
                    detail: "AWS access key removed from code context".into(),
                },
                ShieldEvent {
                    timestamp: "1 hour ago".into(),
                    event_type: "Threat Detected".into(),
                    severity: "critical".into(),
                    detail: "Prompt injection attempt blocked in skill instructions".into(),
                },
                ShieldEvent {
                    timestamp: "3 hours ago".into(),
                    event_type: "PII Detected".into(),
                    severity: "low".into(),
                    detail: "Phone number cloaked in chat message".into(),
                },
            ],
            policies: vec![
                PolicyDisplay {
                    provider: "Anthropic".into(),
                    trust_level: "High".into(),
                    max_classification: "Confidential".into(),
                    pii_cloaking: true,
                },
                PolicyDisplay {
                    provider: "OpenAI".into(),
                    trust_level: "Medium".into(),
                    max_classification: "Internal".into(),
                    pii_cloaking: true,
                },
                PolicyDisplay {
                    provider: "OpenRouter".into(),
                    trust_level: "Low".into(),
                    max_classification: "Public".into(),
                    pii_cloaking: true,
                },
                PolicyDisplay {
                    provider: "Ollama (Local)".into(),
                    trust_level: "Full".into(),
                    max_classification: "Secret".into(),
                    pii_cloaking: false,
                },
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Privacy shield panel: stats, recent security events, access policies.
pub struct ShieldPanel;

impl ShieldPanel {
    pub fn render(data: &ShieldPanelData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("shield-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(data.enabled, theme))
            .child(render_content(data, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(enabled: bool, theme: &HiveTheme) -> AnyElement {
    let status_color = if enabled {
        theme.accent_green
    } else {
        theme.accent_red
    };
    let status_text = if enabled { "Active" } else { "Disabled" };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(header_icon(theme))
        .child(header_title(theme))
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(theme.radius_full)
                        .bg(status_color),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(status_color)
                        .font_weight(FontWeight::MEDIUM)
                        .child(status_text),
                ),
        )
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
        .child(Icon::new(IconName::EyeOff).size_4())
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
                .child("Privacy Shield"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("PII detection, secret scanning, and threat prevention"),
        )
}

// ---------------------------------------------------------------------------
// Content router
// ---------------------------------------------------------------------------

fn render_content(data: &ShieldPanelData, theme: &HiveTheme) -> AnyElement {
    if !data.enabled {
        return render_disabled_state(theme);
    }

    div()
        .flex()
        .flex_col()
        .gap(theme.space_4)
        .child(render_stats_bar(data, theme))
        .child(render_recent_activity(&data.recent_events, theme))
        .child(render_policies_section(&data.policies, theme))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Stats bar
// ---------------------------------------------------------------------------

fn render_stats_bar(data: &ShieldPanelData, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(theme.space_3)
        .child(stat_card(
            "PII Detections",
            data.pii_detections,
            theme.accent_yellow,
            theme,
        ))
        .child(stat_card(
            "Secrets Blocked",
            data.secrets_blocked,
            theme.accent_red,
            theme,
        ))
        .child(stat_card(
            "Threats Caught",
            data.threats_caught,
            theme.accent_cyan,
            theme,
        ))
        .into_any_element()
}

fn stat_card(label: &str, count: usize, accent: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .p(theme.space_3)
        .gap(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_2xl)
                .text_color(accent)
                .font_weight(FontWeight::BOLD)
                .child(format!("{count}")),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
}

// ---------------------------------------------------------------------------
// Recent activity
// ---------------------------------------------------------------------------

fn render_recent_activity(events: &[ShieldEvent], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
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
                        .text_size(theme.font_size_lg)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Recent Activity"),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_tertiary)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child(format!("{}", events.len())),
                ),
        );

    if events.is_empty() {
        section = section.child(
            div()
                .py(theme.space_4)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("No recent security events."),
                ),
        );
    } else {
        // Separator
        section = section.child(div().w_full().h(px(1.0)).bg(theme.border));

        for event in events {
            section = section.child(render_event_row(event, theme));
        }
    }

    section.into_any_element()
}

fn render_event_row(event: &ShieldEvent, theme: &HiveTheme) -> AnyElement {
    let severity_color = event.severity_color(theme);

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .py(theme.space_1)
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(theme.radius_full)
                .bg(severity_color),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(event.timestamp.clone()),
        )
        .child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(severity_color)
                .font_weight(FontWeight::MEDIUM)
                .child(event.event_type.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .overflow_hidden()
                .child(event.detail.clone()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Access policies
// ---------------------------------------------------------------------------

fn render_policies_section(policies: &[PolicyDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Access Policies"),
        );

    if policies.is_empty() {
        section = section.child(
            div()
                .py(theme.space_4)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("No access policies configured."),
                ),
        );
    } else {
        // Column headers
        section = section.child(policy_table_header(theme));

        // Separator
        section = section.child(div().w_full().h(px(1.0)).bg(theme.border));

        for policy in policies {
            section = section.child(render_policy_row(policy, theme));
        }
    }

    section.into_any_element()
}

fn policy_table_header(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .w(px(120.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Provider"),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Trust"),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Max Classification"),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("PII Cloak"),
        )
}

fn render_policy_row(policy: &PolicyDisplay, theme: &HiveTheme) -> AnyElement {
    let trust_color = match policy.trust_level.as_str() {
        "Full" => theme.accent_green,
        "High" => theme.accent_aqua,
        "Medium" => theme.accent_yellow,
        "Low" => theme.accent_red,
        _ => theme.text_muted,
    };

    let pii_color = if policy.pii_cloaking {
        theme.accent_green
    } else {
        theme.text_muted
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .py(theme.space_1)
        .child(
            div()
                .w(px(120.0))
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(policy.provider.clone()),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(trust_color)
                .font_weight(FontWeight::MEDIUM)
                .child(policy.trust_level.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(policy.max_classification.clone()),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(pii_color)
                .child(if policy.pii_cloaking {
                    "\u{2713} On"
                } else {
                    "\u{2717} Off"
                }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Disabled state
// ---------------------------------------------------------------------------

fn render_disabled_state(theme: &HiveTheme) -> AnyElement {
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
                .child("\u{1F6E1}"),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child("Privacy Shield Disabled"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Enable the shield to protect sensitive data in AI interactions."),
        )
        .into_any_element()
}

