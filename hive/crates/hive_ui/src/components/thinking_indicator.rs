use gpui::*;

use crate::theme::HiveTheme;

/// Current phase of an AI thinking process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingPhase {
    Thinking,
    Planning,
    Coding,
    Reviewing,
    Verifying,
    Done,
}

impl ThinkingPhase {
    fn label(self) -> &'static str {
        match self {
            Self::Thinking => "Thinking...",
            Self::Planning => "Planning...",
            Self::Coding => "Coding...",
            Self::Reviewing => "Reviewing...",
            Self::Verifying => "Verifying...",
            Self::Done => "Done",
        }
    }

    fn ordinal(self) -> usize {
        match self {
            Self::Thinking => 0,
            Self::Planning => 1,
            Self::Coding => 2,
            Self::Reviewing => 3,
            Self::Verifying => 4,
            Self::Done => 5,
        }
    }
}

/// Render a thinking/progress indicator with a pulsing dot, phase label, and step dots.
pub fn render_thinking_indicator(phase: ThinkingPhase, theme: &HiveTheme) -> impl IntoElement {
    let active = phase.ordinal();
    let total = 6;
    let is_done = phase == ThinkingPhase::Done;
    let dot_color = if is_done {
        theme.accent_green
    } else {
        theme.accent_cyan
    };

    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_2)
        .bg(theme.bg_surface)
        .rounded(theme.radius_md)
        .border_1()
        .border_color(theme.border)
        .child(
            // Pulsing dot
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded(theme.radius_full)
                .bg(dot_color),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(phase.label()),
        )
        .child(
            // Progress dots
            div()
                .flex()
                .items_center()
                .gap(theme.space_1)
                .children((0..total).map(|i| {
                    let filled = i <= active;
                    let color = if filled {
                        theme.accent_cyan
                    } else {
                        theme.text_muted
                    };
                    let mut bg = color;
                    if !filled {
                        bg.a = 0.3;
                    }
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded(theme.radius_full)
                        .bg(bg)
                })),
        )
}
