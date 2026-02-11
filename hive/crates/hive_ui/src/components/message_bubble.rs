use gpui::*;

use crate::theme::HiveTheme;

/// Render a user chat message bubble (right-aligned, accent background).
pub fn render_user_message(content: &str, theme: &HiveTheme) -> impl IntoElement {
    let content = content.to_string();
    let mut accent_bg = theme.accent_cyan;
    accent_bg.a = 0.18;

    div()
        .flex()
        .flex_row_reverse()
        .w_full()
        .py(theme.space_1)
        .child(
            div()
                .max_w(px(560.0))
                .px(theme.space_4)
                .py(theme.space_2)
                .bg(accent_bg)
                .border_1()
                .border_color(theme.accent_cyan)
                .rounded(theme.radius_lg)
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .child(content),
        )
}

/// Render an AI response bubble (left-aligned, surface background) with model badge and optional cost.
pub fn render_ai_message(
    content: &str,
    model: &str,
    cost: Option<f64>,
    theme: &HiveTheme,
) -> impl IntoElement {
    let content = content.to_string();
    let model = model.to_string();

    let cost_label = match cost {
        Some(c) => format!(" \u{00B7} ${:.4}", c),
        None => String::new(),
    };
    let meta = format!("{}{}", model, cost_label);

    div().flex().flex_col().w_full().py(theme.space_1).child(
        div()
            .max_w(px(560.0))
            .px(theme.space_4)
            .py(theme.space_2)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_lg)
            .child(
                div()
                    .text_size(theme.font_size_base)
                    .text_color(theme.text_primary)
                    .child(content),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .pt(theme.space_1)
                    .child(meta),
            ),
    )
}

/// Render an error message bubble with red tint.
pub fn render_error_message(error: &str, theme: &HiveTheme) -> impl IntoElement {
    let error = error.to_string();
    let mut red_bg = theme.accent_red;
    red_bg.a = 0.12;

    div().flex().w_full().py(theme.space_1).child(
        div()
            .max_w(px(560.0))
            .px(theme.space_4)
            .py(theme.space_2)
            .bg(red_bg)
            .border_1()
            .border_color(theme.accent_red)
            .rounded(theme.radius_lg)
            .text_size(theme.font_size_sm)
            .text_color(theme.accent_red)
            .child(error),
    )
}
