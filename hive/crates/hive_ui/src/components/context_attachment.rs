use gpui::*;

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single file attached as context for the AI conversation.
#[derive(Debug, Clone)]
pub struct AttachedFile {
    pub name: String,
    pub path: String,
    pub tokens: usize,
    pub source_type: String,
}

impl AttachedFile {
    /// Format the token count for display (e.g. "1.2k" for 1200).
    pub fn tokens_display(&self) -> String {
        format_token_count(self.tokens)
    }
}

/// Collection of attached context files with aggregate token count.
#[derive(Debug, Clone)]
pub struct AttachedContext {
    pub files: Vec<AttachedFile>,
    pub total_tokens: usize,
}

impl AttachedContext {
    /// Create an empty context with no files.
    pub fn empty() -> Self {
        Self {
            files: Vec::new(),
            total_tokens: 0,
        }
    }

    /// Whether there are any attached files.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Recalculate total tokens from the file list.
    pub fn recalculate_tokens(&mut self) {
        self.total_tokens = self.files.iter().map(|f| f.tokens).sum();
    }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// Renders a horizontal bar of attached context files above the chat input.
/// Returns nothing (empty div) when there are no files attached.
pub fn render_context_attachment(ctx: &AttachedContext, theme: &HiveTheme) -> impl IntoElement {
    if ctx.is_empty() {
        return div().into_any_element();
    }

    let mut bar = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_1)
        .flex_wrap();

    for file in &ctx.files {
        bar = bar.child(render_file_chip(file, theme));
    }

    bar = bar.child(add_file_button(theme));
    bar = bar.child(div().flex_1());
    bar = bar.child(total_token_badge(ctx.total_tokens, theme));

    bar.into_any_element()
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

fn render_file_chip(file: &AttachedFile, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_1)
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_primary)
                .child(file.name.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(file.tokens_display()),
        )
        .child(remove_chip_button(theme))
        .into_any_element()
}

fn remove_chip_button(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(14.0))
        .h(px(14.0))
        .rounded(theme.radius_full)
        .text_size(theme.font_size_xs)
        .text_color(theme.text_muted)
        .child("\u{2715}")
}

fn add_file_button(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(24.0))
        .h(px(24.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_sm)
        .text_color(theme.accent_cyan)
        .child("+")
}

fn total_token_badge(total: usize, theme: &HiveTheme) -> Div {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.text_secondary)
        .child(format!("{} tokens", format_token_count(total)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format a token count for compact display.
pub fn format_token_count(count: usize) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        format!("{count}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_token_count_small() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(500), "500");
        assert_eq!(format_token_count(999), "999");
    }

    #[test]
    fn format_token_count_thousands() {
        assert_eq!(format_token_count(1_000), "1.0k");
        assert_eq!(format_token_count(1_500), "1.5k");
        assert_eq!(format_token_count(12_345), "12.3k");
    }

    #[test]
    fn format_token_count_millions() {
        assert_eq!(format_token_count(1_000_000), "1.0M");
        assert_eq!(format_token_count(2_500_000), "2.5M");
    }

    #[test]
    fn attached_file_tokens_display() {
        let file = AttachedFile {
            name: "main.rs".into(),
            path: "/src/main.rs".into(),
            tokens: 3_200,
            source_type: "file".into(),
        };
        assert_eq!(file.tokens_display(), "3.2k");
    }

    #[test]
    fn attached_context_empty() {
        let ctx = AttachedContext::empty();
        assert!(ctx.is_empty());
        assert_eq!(ctx.total_tokens, 0);
    }

    #[test]
    fn attached_context_recalculate_tokens() {
        let mut ctx = AttachedContext {
            files: vec![
                AttachedFile {
                    name: "a.rs".into(),
                    path: "/a.rs".into(),
                    tokens: 100,
                    source_type: "file".into(),
                },
                AttachedFile {
                    name: "b.rs".into(),
                    path: "/b.rs".into(),
                    tokens: 250,
                    source_type: "file".into(),
                },
            ],
            total_tokens: 0,
        };
        ctx.recalculate_tokens();
        assert_eq!(ctx.total_tokens, 350);
    }

    #[test]
    fn attached_context_is_empty_with_files() {
        let ctx = AttachedContext {
            files: vec![AttachedFile {
                name: "test.rs".into(),
                path: "/test.rs".into(),
                tokens: 50,
                source_type: "file".into(),
            }],
            total_tokens: 50,
        };
        assert!(!ctx.is_empty());
    }
}
