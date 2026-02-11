use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;

/// Welcome screen shown before the first message.
pub struct WelcomeScreen;

impl WelcomeScreen {
    pub fn render(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .flex_1()
            .gap(theme.space_6)
            .child(
                // Logo icon
                Icon::new(IconName::Bot)
                    .size_6()
                    .text_color(theme.accent_aqua),
            )
            .child(
                div()
                    .text_size(theme.font_size_2xl)
                    .text_color(theme.text_primary)
                    .child("Welcome to Hive"),
            )
            .child(
                div()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_secondary)
                    .child("Your AI-powered development companion"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_2)
                    .mt(theme.space_4)
                    .items_center()
                    .child(
                        div()
                            .text_size(theme.font_size_base)
                            .text_color(theme.text_muted)
                            .child("Start by typing a message below, or:"),
                    )
                    .child(hint_row(theme, IconName::Settings, "Configure API keys in Settings"))
                    .child(hint_row(theme, IconName::Folder, "Open a project folder in Files"))
                    .child(hint_row(theme, IconName::Map, "Explore AI models in Routing")),
            )
    }
}

fn hint_row(theme: &HiveTheme, icon: IconName, text: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .text_size(theme.font_size_sm)
        .text_color(theme.text_secondary)
        .child(Icon::new(icon).size_4().text_color(theme.accent_cyan))
        .child(div().child(text.to_string()))
}
