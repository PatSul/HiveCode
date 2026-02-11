use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;

/// Visual severity of a toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn icon(self) -> IconName {
        match self {
            Self::Info => IconName::Info,
            Self::Success => IconName::CircleCheck,
            Self::Warning => IconName::TriangleAlert,
            Self::Error => IconName::CircleX,
        }
    }

    fn color(self, theme: &HiveTheme) -> Hsla {
        match self {
            Self::Info => theme.accent_cyan,
            Self::Success => theme.accent_green,
            Self::Warning => theme.accent_yellow,
            Self::Error => theme.accent_red,
        }
    }

    fn bg(self, theme: &HiveTheme) -> Hsla {
        let mut color = self.color(theme);
        color.a = 0.12;
        color
    }
}

/// Render a toast notification bar with icon, message, and dismiss button.
pub fn render_toast(kind: ToastKind, message: &str, theme: &HiveTheme) -> impl IntoElement {
    let accent = kind.color(theme);
    let bg = kind.bg(theme);
    let icon = kind.icon();
    let message = message.to_string();

    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .px(theme.space_4)
        .py(theme.space_2)
        .bg(bg)
        .border_l_4()
        .border_color(accent)
        .rounded(theme.radius_md)
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme.space_2)
                .child(Icon::new(icon).size_4().text_color(accent))
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .child(message),
                ),
        )
        .child(
            // Dismiss button placeholder
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .px(theme.space_1)
                .rounded(theme.radius_sm)
                .child(Icon::new(IconName::Close).size_3p5()),
        )
}

// Note: GPUI component tests require a running application context and cause
// compiler stack overflow during test compilation due to deep type nesting.
// Visual components are verified via `cargo check` and manual testing.
