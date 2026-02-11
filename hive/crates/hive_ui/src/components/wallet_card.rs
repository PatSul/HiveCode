use gpui::*;

use crate::theme::HiveTheme;

/// Render a wallet card showing chain, truncated address, and balance.
pub fn render_wallet_card(
    chain: &str,
    address: &str,
    balance: f64,
    theme: &HiveTheme,
) -> impl IntoElement {
    let chain = chain.to_string();
    let truncated = truncate_address(address);
    let balance_str = format!("{:.4}", balance);
    let chain_color = chain_accent(&chain, theme);

    div()
        .flex()
        .items_center()
        .gap(theme.space_3)
        .w_full()
        .px(theme.space_4)
        .py(theme.space_3)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius_lg)
        .child(
            // Chain icon placeholder (colored circle)
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(36.0))
                .h(px(36.0))
                .rounded(theme.radius_full)
                .bg(chain_color)
                .text_size(theme.font_size_sm)
                .text_color(theme.text_on_accent)
                .child(chain_initial(&chain)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .child(chain),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(truncated),
                ),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .child(balance_str),
        )
}

/// Truncate a blockchain address to `0x1234...abcd` form.
fn truncate_address(address: &str) -> String {
    if address.len() <= 12 {
        return address.to_string();
    }
    let prefix = &address[..6];
    let suffix = &address[address.len() - 4..];
    format!("{}...{}", prefix, suffix)
}

/// First character of the chain name, uppercased.
fn chain_initial(chain: &str) -> String {
    chain
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

/// Map chain name to an accent color.
fn chain_accent(chain: &str, theme: &HiveTheme) -> Hsla {
    match chain.to_lowercase().as_str() {
        "solana" => theme.accent_pink,
        "ethereum" => theme.accent_cyan,
        "base" => theme.accent_aqua,
        _ => theme.accent_powder,
    }
}
