use gpui::*;

use crate::theme::HiveTheme;

/// Render a syntax-highlighted-style code block with line numbers and a language label.
pub fn render_code_block(code: &str, language: &str, theme: &HiveTheme) -> impl IntoElement {
    let language = language.to_string();
    let lines: Vec<String> = code.lines().map(String::from).collect();
    let line_count = lines.len();

    let mut code_bg = theme.bg_primary;
    code_bg.a = 0.85;

    div()
        .w_full()
        .rounded(theme.radius_md)
        .bg(code_bg)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(
            // Header: language label + copy placeholder
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(theme.space_3)
                .py(theme.space_1)
                .bg(theme.bg_secondary)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(language),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .px(theme.space_2)
                        .rounded(theme.radius_sm)
                        .child("Copy"),
                ),
        )
        .child(
            // Code body with line numbers
            div()
                .id("code-block-body")
                .overflow_y_scroll()
                .px(theme.space_3)
                .py(theme.space_2)
                .children((0..line_count).map(|i| {
                    let line_num = format!("{:>3}", i + 1);
                    let line_text = lines[i].clone();
                    render_code_line(line_num, line_text, theme)
                })),
        )
}

/// Render a single line of code with its line number.
fn render_code_line(line_num: String, line_text: String, theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap(theme.space_3)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .min_w(px(28.0))
                .flex_shrink_0()
                .child(line_num),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(line_text),
        )
}
