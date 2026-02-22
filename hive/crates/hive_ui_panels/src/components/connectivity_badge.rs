use gpui::*;

use hive_ui_core::HiveTheme;

/// Network connectivity state of the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityState {
    Online,
    LocalOnly,
    Offline,
}

impl ConnectivityState {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> HiveTheme {
        HiveTheme::dark()
    }

    #[test]
    fn online_label() {
        assert_eq!(ConnectivityState::Online.label(), "Online");
    }

    #[test]
    fn local_only_label() {
        assert_eq!(ConnectivityState::LocalOnly.label(), "Local Only");
    }

    #[test]
    fn offline_label() {
        assert_eq!(ConnectivityState::Offline.label(), "Offline");
    }

    #[test]
    fn online_color_is_green() {
        let t = theme();
        assert_eq!(ConnectivityState::Online.color(&t), t.accent_green);
    }

    #[test]
    fn local_only_color_is_yellow() {
        let t = theme();
        assert_eq!(ConnectivityState::LocalOnly.color(&t), t.accent_yellow);
    }

    #[test]
    fn offline_color_is_red() {
        let t = theme();
        assert_eq!(ConnectivityState::Offline.color(&t), t.accent_red);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> HiveTheme {
        HiveTheme::dark()
    }

    #[test]
    fn online_label() {
        assert_eq!(ConnectivityState::Online.label(), "Online");
    }

    #[test]
    fn local_only_label() {
        assert_eq!(ConnectivityState::LocalOnly.label(), "Local Only");
    }

    #[test]
    fn offline_label() {
        assert_eq!(ConnectivityState::Offline.label(), "Offline");
    }

    #[test]
    fn online_color_is_green() {
        let t = theme();
        assert_eq!(ConnectivityState::Online.color(&t), t.accent_green);
    }

    #[test]
    fn local_only_color_is_yellow() {
        let t = theme();
        assert_eq!(ConnectivityState::LocalOnly.color(&t), t.accent_yellow);
    }

    #[test]
    fn offline_color_is_red() {
        let t = theme();
        assert_eq!(ConnectivityState::Offline.color(&t), t.accent_red);
    }
}

/// Render a connectivity indicator dot with label.
pub fn render_connectivity_badge(state: ConnectivityState, theme: &HiveTheme) -> impl IntoElement {
    let color = state.color(theme);

    div()
        .flex()
        .items_center()
        .gap(theme.space_1)
        .px(theme.space_2)
        .py(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded(theme.radius_full)
                .bg(color),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(state.label()),
        )
}
