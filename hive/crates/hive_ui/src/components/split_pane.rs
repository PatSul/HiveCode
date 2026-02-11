use gpui::*;

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Direction of a split between two panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Recursive layout tree for tiling panels.
#[derive(Debug, Clone)]
pub enum PaneLayout {
    /// A single panel fills the entire area.
    Single(String),
    /// Two panes separated by a divider.
    Split {
        direction: SplitDirection,
        first: Box<PaneLayout>,
        second: Box<PaneLayout>,
        /// Fraction of space allocated to the first pane (0.0 to 1.0).
        ratio: f32,
    },
}

impl PaneLayout {
    /// Create a single-panel layout.
    pub fn single(panel: impl Into<String>) -> Self {
        Self::Single(panel.into())
    }

    /// Create a horizontal split (left | right) with the given ratio.
    pub fn horizontal(first: PaneLayout, second: PaneLayout, ratio: f32) -> Self {
        Self::Split {
            direction: SplitDirection::Horizontal,
            first: Box::new(first),
            second: Box::new(second),
            ratio: ratio.clamp(0.0, 1.0),
        }
    }

    /// Create a vertical split (top / bottom) with the given ratio.
    pub fn vertical(first: PaneLayout, second: PaneLayout, ratio: f32) -> Self {
        Self::Split {
            direction: SplitDirection::Vertical,
            first: Box::new(first),
            second: Box::new(second),
            ratio: ratio.clamp(0.0, 1.0),
        }
    }

    /// Count the total number of leaf (Single) panes in this layout.
    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Single(_) => 1,
            Self::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }

    /// Collect all panel names in this layout (depth-first order).
    pub fn panel_names(&self) -> Vec<&str> {
        match self {
            Self::Single(name) => vec![name.as_str()],
            Self::Split { first, second, .. } => {
                let mut names = first.panel_names();
                names.extend(second.panel_names());
                names
            }
        }
    }
}

/// Top-level tiling state holding the layout tree.
#[derive(Debug, Clone)]
pub struct TilingState {
    pub layout: PaneLayout,
}

impl TilingState {
    /// Create a tiling state with a single panel.
    pub fn single(panel: impl Into<String>) -> Self {
        Self {
            layout: PaneLayout::single(panel),
        }
    }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// Renders a split pane layout. For `Single` layouts, renders a placeholder
/// panel with the panel name. For `Split` layouts, renders two children
/// separated by a thin divider bar.
pub fn render_split_pane(layout: &PaneLayout, theme: &HiveTheme) -> impl IntoElement {
    match layout {
        PaneLayout::Single(name) => render_single_pane(name, theme).into_any_element(),
        PaneLayout::Split {
            direction,
            first,
            second,
            ratio,
        } => render_split(*direction, first, second, *ratio, theme).into_any_element(),
    }
}

fn render_single_pane(name: &str, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .size_full()
        .bg(theme.bg_primary)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(name.to_string()),
        )
}

fn render_split(
    direction: SplitDirection,
    first: &PaneLayout,
    second: &PaneLayout,
    ratio: f32,
    theme: &HiveTheme,
) -> Div {
    // Ratio is already clamped at construction time by horizontal()/vertical().
    match direction {
        SplitDirection::Horizontal => div()
            .flex()
            .flex_row()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .w(relative(ratio))
                    .child(render_split_pane(first, theme)),
            )
            .child(horizontal_divider(theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .w(relative(1.0 - ratio))
                    .child(render_split_pane(second, theme)),
            ),
        SplitDirection::Vertical => div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .size_full()
                    .h(relative(ratio))
                    .child(render_split_pane(first, theme)),
            )
            .child(vertical_divider(theme))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .size_full()
                    .h(relative(1.0 - ratio))
                    .child(render_split_pane(second, theme)),
            ),
    }
}

fn horizontal_divider(theme: &HiveTheme) -> Div {
    div()
        .w(px(2.0))
        .h_full()
        .bg(theme.border)
}

fn vertical_divider(theme: &HiveTheme) -> Div {
    div()
        .w_full()
        .h(px(2.0))
        .bg(theme.border)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_layout_single_leaf_count() {
        let layout = PaneLayout::single("Chat");
        assert_eq!(layout.leaf_count(), 1);
    }

    #[test]
    fn pane_layout_split_leaf_count() {
        let layout = PaneLayout::horizontal(
            PaneLayout::single("Chat"),
            PaneLayout::single("Files"),
            0.5,
        );
        assert_eq!(layout.leaf_count(), 2);
    }

    #[test]
    fn pane_layout_nested_leaf_count() {
        let layout = PaneLayout::horizontal(
            PaneLayout::single("Chat"),
            PaneLayout::vertical(
                PaneLayout::single("Files"),
                PaneLayout::single("Terminal"),
                0.6,
            ),
            0.5,
        );
        assert_eq!(layout.leaf_count(), 3);
    }

    #[test]
    fn pane_layout_panel_names() {
        let layout = PaneLayout::horizontal(
            PaneLayout::single("Chat"),
            PaneLayout::vertical(
                PaneLayout::single("Files"),
                PaneLayout::single("Terminal"),
                0.5,
            ),
            0.5,
        );
        assert_eq!(layout.panel_names(), vec!["Chat", "Files", "Terminal"]);
    }

    #[test]
    fn pane_layout_ratio_clamped() {
        let layout = PaneLayout::horizontal(
            PaneLayout::single("A"),
            PaneLayout::single("B"),
            1.5,
        );
        if let PaneLayout::Split { ratio, .. } = layout {
            assert_eq!(ratio, 1.0);
        } else {
            panic!("Expected Split variant");
        }
    }

    #[test]
    fn pane_layout_ratio_clamped_negative() {
        let layout = PaneLayout::horizontal(
            PaneLayout::single("A"),
            PaneLayout::single("B"),
            -0.5,
        );
        if let PaneLayout::Split { ratio, .. } = layout {
            assert_eq!(ratio, 0.0);
        } else {
            panic!("Expected Split variant");
        }
    }

    #[test]
    fn tiling_state_single() {
        let state = TilingState::single("Chat");
        assert_eq!(state.layout.leaf_count(), 1);
    }

    #[test]
    fn split_direction_equality() {
        assert_eq!(SplitDirection::Horizontal, SplitDirection::Horizontal);
        assert_ne!(SplitDirection::Horizontal, SplitDirection::Vertical);
    }
}
