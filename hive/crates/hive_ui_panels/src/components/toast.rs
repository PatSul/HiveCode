use gpui::*;
use gpui_component::{Icon, IconName};

use hive_ui_core::HiveTheme;

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

// Note: GPUI rendering tests require a running application context and cause
// compiler stack overflow during test compilation due to deep type nesting.
// Visual components are verified via `cargo check` and manual testing.
// The tests below cover pure data/logic (no GPUI element construction).

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> HiveTheme {
        HiveTheme::dark()
    }

    // ---- icon mapping ----

    #[test]
    fn info_icon_is_info() {
        assert!(matches!(ToastKind::Info.icon(), IconName::Info));
    }

    #[test]
    fn success_icon_is_circle_check() {
        assert!(matches!(ToastKind::Success.icon(), IconName::CircleCheck));
    }

    #[test]
    fn warning_icon_is_triangle_alert() {
        assert!(matches!(ToastKind::Warning.icon(), IconName::TriangleAlert));
    }

    #[test]
    fn error_icon_is_circle_x() {
        assert!(matches!(ToastKind::Error.icon(), IconName::CircleX));
    }

    // ---- color mapping ----

    #[test]
    fn info_color_is_accent_cyan() {
        let t = theme();
        assert_eq!(ToastKind::Info.color(&t), t.accent_cyan);
    }

    #[test]
    fn success_color_is_accent_green() {
        let t = theme();
        assert_eq!(ToastKind::Success.color(&t), t.accent_green);
    }

    #[test]
    fn warning_color_is_accent_yellow() {
        let t = theme();
        assert_eq!(ToastKind::Warning.color(&t), t.accent_yellow);
    }

    #[test]
    fn error_color_is_accent_red() {
        let t = theme();
        assert_eq!(ToastKind::Error.color(&t), t.accent_red);
    }

    // ---- bg alpha ----

    #[test]
    fn bg_alpha_is_012_for_info() {
        let t = theme();
        let bg = ToastKind::Info.bg(&t);
        assert!((bg.a - 0.12).abs() < f32::EPSILON);
    }

    #[test]
    fn bg_alpha_is_012_for_error() {
        let t = theme();
        let bg = ToastKind::Error.bg(&t);
        assert!((bg.a - 0.12).abs() < f32::EPSILON);
    }

    #[test]
    fn bg_preserves_hue_of_color() {
        let t = theme();
        let color = ToastKind::Warning.color(&t);
        let bg = ToastKind::Warning.bg(&t);
        assert_eq!(bg.h, color.h);
        assert_eq!(bg.s, color.s);
        assert_eq!(bg.l, color.l);
    }
}
