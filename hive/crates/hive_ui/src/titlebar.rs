use gpui::*;
use gpui_component::{Icon, IconName, Sizable as _};

use hive_ui_core::HiveTheme;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const TITLE_BAR_HEIGHT: Pixels = px(34.0);

/// Custom titlebar with app branding, version badge, and platform window controls.
///
/// Uses `occlude()` to prevent the workspace's `track_focus` handler from
/// calling `prevent_default()` on titlebar clicks. Without this, ALL Win32 NC
/// behavior (drag, button actions) is blocked because the workspace's focus
/// handler runs on every click and sets `default_prevented = true`.
///
/// Window drag and window controls are handled by the OS via
/// `window_control_area` → WM_NCHITTEST → DefWindowProcW.
pub struct Titlebar;

impl Titlebar {
    /// Render the full titlebar: left-side branding + window control buttons.
    ///
    /// Requires `window` to check maximized state for the correct restore/maximize icon.
    pub fn render(theme: &HiveTheme, window: &Window) -> impl IntoElement {
        let is_maximized = window.is_maximized();

        div()
            .id("hive-title-bar")
            // Occlude prevents the workspace's track_focus handler from
            // seeing titlebar clicks, which would call prevent_default() and
            // kill all Win32 non-client (NC) behavior.
            .occlude()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(TITLE_BAR_HEIGHT)
            .pl(px(12.0))
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.bg_primary)
            .child(
                // Drag region: NCHITTEST → HTCAPTION → DefWindowProcW handles drag.
                // Double-click maximize is also handled natively by Windows via
                // WM_NCLBUTTONDBLCLK on HTCAPTION areas.
                div()
                    .id("titlebar-drag")
                    .window_control_area(WindowControlArea::Drag)
                    .flex()
                    .flex_1()
                    .h_full()
                    .items_center()
                    .child(branding(theme)),
            )
            .child(window_controls(theme, is_maximized))
    }
}

/// Bee icon + "Hive" + version badge.
fn branding(theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .child(
            svg()
                .path("icons/hive-bee.svg")
                .size(px(20.0))
                .text_color(theme.accent_aqua),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Hive"),
        )
        .child(version_badge(theme))
}

/// Compact version badge styled consistently with other badges in the app.
fn version_badge(theme: &HiveTheme) -> impl IntoElement {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_cyan)
        .child(format!("v{VERSION}"))
}

/// Minimize / Maximize-or-Restore / Close buttons.
///
/// All buttons use `window_control_area` for native NC behavior (correct
/// maximize/restore toggle via the Win32 NC handler).
fn window_controls(theme: &HiveTheme, is_maximized: bool) -> impl IntoElement {
    let fg = theme.text_primary;
    let hover_bg = hsla(0.0, 0.0, 1.0, 0.1);
    let active_bg = hsla(0.0, 0.0, 1.0, 0.05);
    let close_hover_bg = theme.accent_red;

    div()
        .flex()
        .items_center()
        .h_full()
        .flex_shrink_0()
        // Minimize
        .child(
            div()
                .id("minimize")
                .flex()
                .w(TITLE_BAR_HEIGHT)
                .h_full()
                .flex_shrink_0()
                .justify_center()
                .content_center()
                .items_center()
                .text_color(fg)
                .hover(|s| s.bg(hover_bg))
                .active(|s| s.bg(active_bg))
                .window_control_area(WindowControlArea::Min)
                .on_click(|_, window, cx| {
                    cx.stop_propagation();
                    window.minimize_window();
                })
                .child(Icon::new(IconName::WindowMinimize).small()),
        )
        // Maximize / Restore — no on_click; NC handler toggles correctly.
        // zoom_window() only maximizes (SW_MAXIMIZE), so an on_click fallback
        // would conflict with the NC handler's SW_NORMAL restore path.
        .child(
            div()
                .id("maximize")
                .flex()
                .w(TITLE_BAR_HEIGHT)
                .h_full()
                .flex_shrink_0()
                .justify_center()
                .content_center()
                .items_center()
                .text_color(fg)
                .hover(|s| s.bg(hover_bg))
                .active(|s| s.bg(active_bg))
                .window_control_area(WindowControlArea::Max)
                .child(
                    Icon::new(if is_maximized {
                        IconName::WindowRestore
                    } else {
                        IconName::WindowMaximize
                    })
                    .small(),
                ),
        )
        // Close
        .child(
            div()
                .id("close")
                .flex()
                .w(TITLE_BAR_HEIGHT)
                .h_full()
                .flex_shrink_0()
                .justify_center()
                .content_center()
                .items_center()
                .text_color(fg)
                .hover(|s| s.bg(close_hover_bg).text_color(hsla(0.0, 0.0, 1.0, 1.0)))
                .active(|s| s.bg(close_hover_bg))
                .window_control_area(WindowControlArea::Close)
                .child(Icon::new(IconName::WindowClose).small()),
        )
}
