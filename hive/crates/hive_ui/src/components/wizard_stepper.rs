use gpui::*;

use crate::theme::HiveTheme;

/// Render a horizontal wizard stepper with numbered circles and connecting lines.
///
/// Steps before `current` are completed (accent), the step at `current` is active (bright),
/// and later steps are dimmed.
pub fn render_wizard_stepper(
    steps: &[&str],
    current: usize,
    theme: &HiveTheme,
) -> impl IntoElement {
    let step_count = steps.len();
    let steps_owned: Vec<String> = steps.iter().map(|s| s.to_string()).collect();

    div()
        .flex()
        .items_center()
        .w_full()
        .px(theme.space_4)
        .py(theme.space_3)
        .children((0..step_count).map(|i| {
            let label = steps_owned[i].clone();
            let is_last = i == step_count - 1;
            render_step(i, label, current, is_last, theme)
        }))
}

/// Render a single step circle + label, with an optional connecting line after it.
fn render_step(
    index: usize,
    label: String,
    current: usize,
    is_last: bool,
    theme: &HiveTheme,
) -> impl IntoElement {
    let number = format!("{}", index + 1);
    let is_done = index < current;
    let is_active = index == current;

    let circle_bg = if is_done || is_active {
        theme.accent_cyan
    } else {
        theme.bg_tertiary
    };
    let circle_text = if is_done || is_active {
        theme.text_on_accent
    } else {
        theme.text_muted
    };
    let label_color = if is_active {
        theme.text_primary
    } else if is_done {
        theme.accent_cyan
    } else {
        theme.text_muted
    };
    let line_color = if is_done {
        theme.accent_cyan
    } else {
        theme.border
    };

    let step = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(theme.space_1)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(28.0))
                .h(px(28.0))
                .rounded(theme.radius_full)
                .bg(circle_bg)
                .text_size(theme.font_size_sm)
                .text_color(circle_text)
                .child(if is_done {
                    "\u{2713}".to_string()
                } else {
                    number
                }),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(label_color)
                .child(label),
        );

    if is_last {
        div().flex().items_center().child(step)
    } else {
        div()
            .flex()
            .items_center()
            .gap(theme.space_1)
            .child(step)
            .child(
                // Connecting line
                div().h(px(2.0)).w(px(32.0)).bg(line_color).mt(px(-14.0)), // Visually align with circle center
            )
    }
}
