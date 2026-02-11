pub mod agents;
pub mod assistant;
pub mod chat;
pub mod costs;
pub mod files;
pub mod help;
pub mod history;
pub mod kanban;
pub mod learning;
pub mod logs;
pub mod monitor;
pub mod review;
pub mod routing;
pub mod settings;
pub mod shield;
pub mod skills;
pub mod specs;
pub mod token_launch;

use gpui::*;
use crate::theme::HiveTheme;

/// Shared stub renderer for panels not yet implemented.
pub fn panel_stub(title: &str, icon: &str, desc: &str, phase: &str, theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_1()
        .size_full()
        .gap(theme.space_4)
        .child(div().text_size(px(48.0)).child(icon.to_string()))
        .child(
            div()
                .text_size(theme.font_size_xl)
                .text_color(theme.text_primary)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_muted)
                .child(desc.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .mt(theme.space_4)
                .child(format!("Coming in {phase}")),
        )
}
