use gpui::*;

use crate::theme::HiveTheme;

/// A single line in a unified diff view.
#[derive(Debug, Clone)]
pub enum DiffLine {
    Added(String),
    Removed(String),
    Context(String),
}

/// Render a unified diff view with colored lines and gutter symbols.
pub fn render_diff(lines: &[DiffLine], theme: &HiveTheme) -> impl IntoElement {
    let lines: Vec<DiffLine> = lines.to_vec();

    div()
        .id("diff-viewer")
        .w_full()
        .overflow_y_scroll()
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .py(theme.space_2)
        .children(
            lines
                .into_iter()
                .enumerate()
                .map(|(i, line)| render_diff_line(i, line, theme)),
        )
}

/// Render a single diff line with gutter symbol and appropriate coloring.
fn render_diff_line(index: usize, line: DiffLine, theme: &HiveTheme) -> impl IntoElement {
    let (gutter, text, bg, text_color) = match line {
        DiffLine::Added(t) => {
            let mut added_bg = theme.accent_green;
            added_bg.a = 0.10;
            ("+", t, added_bg, theme.accent_green)
        }
        DiffLine::Removed(t) => {
            let mut removed_bg = theme.accent_red;
            removed_bg.a = 0.10;
            ("-", t, removed_bg, theme.accent_red)
        }
        DiffLine::Context(t) => {
            let transparent = hsla(0.0, 0.0, 0.0, 0.0);
            (" ", t, transparent, theme.text_secondary)
        }
    };

    let line_num = format!("{:>3}", index + 1);

    div()
        .flex()
        .items_start()
        .w_full()
        .bg(bg)
        .px(theme.space_3)
        .child(
            // Line number
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .min_w(px(28.0))
                .flex_shrink_0()
                .child(line_num),
        )
        .child(
            // Gutter symbol
            div()
                .text_size(theme.font_size_sm)
                .text_color(text_color)
                .min_w(px(16.0))
                .flex_shrink_0()
                .child(gutter),
        )
        .child(
            // Line content
            div()
                .text_size(theme.font_size_sm)
                .text_color(text_color)
                .child(text),
        )
}
