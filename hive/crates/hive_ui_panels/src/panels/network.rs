use gpui::*;
use gpui_component::{Icon, IconName};

use hive_ui_core::HiveTheme;
use hive_ui_core::NetworkRefresh;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Display-ready peer information for the Network panel.
#[derive(Debug, Clone)]
pub struct PeerDisplayInfo {
    pub name: String,
    pub status: String,
    pub address: String,
    pub latency_ms: Option<u64>,
    pub last_seen: String,
}

/// All data needed to render the Network panel.
#[derive(Debug, Clone, Default)]
pub struct NetworkPeerData {
    pub our_peer_id: String,
    pub peers: Vec<PeerDisplayInfo>,
}

impl NetworkPeerData {
    /// Number of connected peers.
    pub fn connected_count(&self) -> usize {
        self.peers
            .iter()
            .filter(|p| p.status == "Connected")
            .count()
    }

    /// Total number of known peers.
    pub fn total_count(&self) -> usize {
        self.peers.len()
    }

    /// Returns a sample dataset with 3 peers (2 Connected, 1 Discovered) for testing.
    pub fn sample() -> Self {
        Self {
            our_peer_id: "peer-abc123".into(),
            peers: vec![
                PeerDisplayInfo {
                    name: "Alice".into(),
                    status: "Connected".into(),
                    address: "192.168.1.10:9000".into(),
                    latency_ms: Some(12),
                    last_seen: "Just now".into(),
                },
                PeerDisplayInfo {
                    name: "Bob".into(),
                    status: "Connected".into(),
                    address: "192.168.1.11:9000".into(),
                    latency_ms: Some(45),
                    last_seen: "2 min ago".into(),
                },
                PeerDisplayInfo {
                    name: "Charlie".into(),
                    status: "Discovered".into(),
                    address: "192.168.1.12:9000".into(),
                    latency_ms: None,
                    last_seen: "5 min ago".into(),
                },
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// P2P Network panel — displays our peer identity and discovered/connected
/// peers with status, address, latency, and last-seen timestamps.
pub struct NetworkPanel;

impl NetworkPanel {
    /// Main entry point — renders the full network panel.
    pub fn render(data: &NetworkPeerData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("network-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(Self::header(data, theme))
            .child(Self::identity_section(data, theme))
            .child(Self::stats_row(data, theme))
            .child(Self::peers_section(data, theme))
    }

    // ------------------------------------------------------------------
    // Header
    // ------------------------------------------------------------------

    fn header(data: &NetworkPeerData, theme: &HiveTheme) -> impl IntoElement {
        let connected = data.connected_count();
        let total = data.total_count();
        let (badge_label, badge_color) = if connected > 0 {
            ("Online", theme.accent_green)
        } else if total > 0 {
            ("Discovering", theme.accent_yellow)
        } else {
            ("No Peers", theme.text_muted)
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(Icon::new(IconName::Globe).size_6())
            .child(
                div()
                    .text_size(theme.font_size_2xl)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::BOLD)
                    .child("P2P Network".to_string()),
            )
            .child(div().flex_1())
            .child(Self::refresh_btn(theme))
            .child(Self::status_badge(badge_label, badge_color, theme))
    }

    /// Refresh button — dispatches `NetworkRefresh`.
    fn refresh_btn(theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("network-refresh-btn")
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_secondary)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.dispatch_action(&NetworkRefresh);
            })
            .child("Refresh".to_string())
    }

    fn status_badge(label: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .px(theme.space_3)
            .py(theme.space_1)
            .rounded(theme.radius_full)
            .bg(theme.bg_tertiary)
            .child(Self::dot(px(6.0), color, theme))
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Identity Section
    // ------------------------------------------------------------------

    fn identity_section(data: &NetworkPeerData, theme: &HiveTheme) -> impl IntoElement {
        Self::section("Our Identity", theme).child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_3)
                .p(theme.space_3)
                .bg(theme.bg_tertiary)
                .rounded(theme.radius_md)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(36.0))
                        .h(px(36.0))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_cyan)
                        .child(
                            Icon::new(IconName::User)
                                .size_5()
                                .text_color(theme.text_on_accent),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child("Peer ID"),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::MEDIUM)
                                .font_family(theme.font_mono.clone())
                                .child(if data.our_peer_id.is_empty() {
                                    "Not initialized".to_string()
                                } else {
                                    data.our_peer_id.clone()
                                }),
                        ),
                ),
        )
    }

    // ------------------------------------------------------------------
    // Stats Row
    // ------------------------------------------------------------------

    fn stats_row(data: &NetworkPeerData, theme: &HiveTheme) -> impl IntoElement {
        let connected = data.connected_count();
        let total = data.total_count();
        let discovered = total.saturating_sub(connected);

        div()
            .flex()
            .flex_row()
            .gap(theme.space_3)
            .child(Self::stat_card(
                "Connected",
                &connected.to_string(),
                theme.accent_green,
                theme,
            ))
            .child(Self::stat_card(
                "Discovered",
                &discovered.to_string(),
                theme.accent_yellow,
                theme,
            ))
            .child(Self::stat_card(
                "Total Peers",
                &total.to_string(),
                theme.accent_cyan,
                theme,
            ))
    }

    fn stat_card(
        label: &str,
        value: &str,
        accent: Hsla,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .p(theme.space_3)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .gap(theme.space_1)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(px(20.0))
                    .text_color(accent)
                    .font_weight(FontWeight::BOLD)
                    .child(value.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Peers Table
    // ------------------------------------------------------------------

    fn peers_section(data: &NetworkPeerData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = Self::section("Peers", theme).child(Self::peers_header(theme));

        if data.peers.is_empty() {
            container = container.child(Self::empty_state(
                "No peers discovered yet. Other Hive instances on your LAN will appear here.",
                theme,
            ));
        } else {
            for peer in &data.peers {
                container = container.child(Self::peer_row(peer, theme));
            }
        }
        container
    }

    fn peers_header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .pb(theme.space_1)
            .border_b_1()
            .border_color(theme.border)
            .child(Self::col_hdr("Name", px(160.0), theme))
            .child(Self::col_hdr("Status", px(100.0), theme))
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Address".to_string()),
            )
            .child(Self::col_hdr("Latency", px(80.0), theme))
            .child(Self::col_hdr("Last Seen", px(120.0), theme))
    }

    fn col_hdr(label: &str, width: Pixels, theme: &HiveTheme) -> impl IntoElement {
        div()
            .w(width)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .font_weight(FontWeight::SEMIBOLD)
            .child(label.to_string())
    }

    fn peer_row(peer: &PeerDisplayInfo, theme: &HiveTheme) -> impl IntoElement {
        let (dot_color, status_color) = match peer.status.as_str() {
            "Connected" => (theme.accent_green, theme.accent_green),
            "Discovered" | "Connecting" => (theme.accent_yellow, theme.accent_yellow),
            "Disconnected" => (theme.accent_red, theme.accent_red),
            _ => (theme.text_muted, theme.text_muted),
        };

        let latency_text = peer
            .latency_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "--".to_string());

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            // Name
            .child(
                div()
                    .w(px(160.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .child(Self::dot(px(8.0), dot_color, theme))
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_primary)
                            .font_weight(FontWeight::MEDIUM)
                            .child(peer.name.clone()),
                    ),
            )
            // Status
            .child(
                div()
                    .w(px(100.0))
                    .text_size(theme.font_size_xs)
                    .text_color(status_color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(peer.status.clone()),
            )
            // Address
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_secondary)
                    .font_family(theme.font_mono.clone())
                    .child(peer.address.clone()),
            )
            // Latency
            .child(
                div()
                    .w(px(80.0))
                    .text_size(theme.font_size_sm)
                    .text_color(if peer.latency_ms.is_some() {
                        theme.accent_aqua
                    } else {
                        theme.text_muted
                    })
                    .child(latency_text),
            )
            // Last Seen
            .child(
                div()
                    .w(px(120.0))
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child(peer.last_seen.clone()),
            )
    }

    // ------------------------------------------------------------------
    // Shared helpers
    // ------------------------------------------------------------------

    fn section(title: &str, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_string()),
            )
    }

    fn dot(size: Pixels, color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        div().w(size).h(size).rounded(theme.radius_full).bg(color)
    }

    fn empty_state(message: &str, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_center()
            .py(theme.space_6)
            .child(
                div()
                    .text_size(theme.font_size_base)
                    .text_color(theme.text_muted)
                    .child(message.to_string()),
            )
    }
}
