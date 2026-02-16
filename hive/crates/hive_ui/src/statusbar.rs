use gpui::*;
use gpui_component::{Icon, IconName};

use hive_ui_core::{HiveTheme, SwitchToSettings};

/// Status bar at the bottom of the window.
/// Shows connectivity, model, privacy mode, project scope, cost, and version.
pub struct StatusBar {
    pub connectivity: ConnectivityDisplay,
    pub current_model: String,
    pub privacy_mode: bool,
    pub active_project: String,
    pub total_cost: f64,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityDisplay {
    Online,
    LocalOnly,
    Offline,
}

impl ConnectivityDisplay {
    fn label(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::LocalOnly => "Local Only",
            Self::Offline => "Offline",
        }
    }

    fn color(self, theme: &HiveTheme) -> Hsla {
        match self {
            Self::Online => theme.accent_green,
            Self::LocalOnly => theme.accent_yellow,
            Self::Offline => theme.accent_red,
        }
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            connectivity: ConnectivityDisplay::Offline,
            current_model: "Select Model".into(),
            privacy_mode: false,
            active_project: "No project".into(),
            total_cost: 0.0,
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    pub fn render(&self, theme: &HiveTheme) -> impl IntoElement {
        let conn_color = self.connectivity.color(theme);
        let conn_label = self.connectivity.label();
        let model = if self.current_model.trim().is_empty()
            || self.current_model == "(no model)"
            || self.current_model == "Select Model"
        {
            "Select Model".to_string()
        } else {
            self.current_model.clone()
        };
        let cost_str = format!("${:.2}", self.total_cost);
        let privacy = if self.privacy_mode {
            "Private Mode"
        } else {
            "Cloud Mode"
        };
        let privacy_icon = if self.privacy_mode {
            IconName::EyeOff
        } else {
            IconName::Eye
        };
        let project = self.active_project.clone();
        let version = format!("v{}", self.version);

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(32.0))
            .bg(theme.bg_secondary)
            .border_t_1()
            .border_color(theme.border)
            .px(theme.space_2)
            .text_color(theme.text_muted)
            .child(
                // Left summary area
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded(theme.radius_full)
                                    .bg(conn_color),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_secondary)
                                    .child(conn_label),
                            ),
                    )
                    .child(
                        div()
                            .id("status-model")
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_tertiary)
                            .text_color(theme.accent_cyan)
                            .text_size(theme.font_size_xs)
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                                window.dispatch_action(Box::new(SwitchToSettings), cx);
                            })
                            .child(model),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .overflow_hidden()
                            .max_w(px(230.0))
                            .child(format!("Project: {project}")),
                    ),
            )
            .child(
                // Right controls
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_size(theme.font_size_xs)
                            .child(Icon::new(privacy_icon).size_3p5())
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .child(privacy),
                            ),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_color(theme.accent_green)
                            .text_size(theme.font_size_xs)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(cost_str),
                    )
                    .child(
                        div()
                            .px(theme.space_2)
                            .py(px(2.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_surface)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .font_weight(FontWeight::MEDIUM)
                            .child(version),
                    ),
            )
    }
}
