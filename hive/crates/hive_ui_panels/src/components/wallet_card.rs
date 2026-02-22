use gpui::*;

use hive_ui_core::HiveTheme;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> HiveTheme {
        HiveTheme::dark()
    }

    // ---- truncate_address ----

    #[test]
    fn short_address_unchanged() {
        assert_eq!(truncate_address("0x1234"), "0x1234");
    }

    #[test]
    fn long_address_truncated() {
        let addr = "0x1234567890abcdef1234567890abcdef12345678";
        let result = truncate_address(addr);
        assert_eq!(result, "0x1234...5678");
    }

    #[test]
    fn exactly_12_chars_unchanged() {
        assert_eq!(truncate_address("123456789012"), "123456789012");
    }

    #[test]
    fn thirteen_chars_gets_truncated() {
        let result = truncate_address("1234567890123");
        assert_eq!(result, "123456...0123");
    }

    #[test]
    fn empty_address() {
        assert_eq!(truncate_address(""), "");
    }

    // ---- chain_initial ----

    #[test]
    fn ethereum_initial() {
        assert_eq!(chain_initial("ethereum"), "E");
    }

    #[test]
    fn solana_initial() {
        assert_eq!(chain_initial("solana"), "S");
    }

    #[test]
    fn empty_chain_initial() {
        assert_eq!(chain_initial(""), "");
    }

    // ---- chain_accent ----

    #[test]
    fn solana_accent_is_pink() {
        let t = theme();
        assert_eq!(chain_accent("solana", &t), t.accent_pink);
    }

    #[test]
    fn ethereum_accent_is_cyan() {
        let t = theme();
        assert_eq!(chain_accent("ethereum", &t), t.accent_cyan);
    }

    #[test]
    fn base_accent_is_aqua() {
        let t = theme();
        assert_eq!(chain_accent("base", &t), t.accent_aqua);
    }

    #[test]
    fn unknown_chain_accent_is_powder() {
        let t = theme();
        assert_eq!(chain_accent("polygon", &t), t.accent_powder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> HiveTheme {
        HiveTheme::dark()
    }

    // ---- truncate_address ----

    #[test]
    fn short_address_unchanged() {
        assert_eq!(truncate_address("0x1234"), "0x1234");
    }

    #[test]
    fn twelve_char_address_unchanged() {
        assert_eq!(truncate_address("0x12345abcde"), "0x12345abcde");
    }

    #[test]
    fn long_address_truncated() {
        let addr = "0x1234567890abcdef1234567890abcdef12345678";
        let result = truncate_address(addr);
        assert_eq!(result, "0x1234...5678");
    }

    #[test]
    fn empty_address() {
        assert_eq!(truncate_address(""), "");
    }

    // ---- chain_initial ----

    #[test]
    fn ethereum_initial() {
        assert_eq!(chain_initial("ethereum"), "E");
    }

    #[test]
    fn solana_initial() {
        assert_eq!(chain_initial("solana"), "S");
    }

    #[test]
    fn empty_chain_initial() {
        assert_eq!(chain_initial(""), "");
    }

    // ---- chain_accent ----

    #[test]
    fn solana_accent_is_pink() {
        let t = theme();
        assert_eq!(chain_accent("solana", &t), t.accent_pink);
    }

    #[test]
    fn ethereum_accent_is_cyan() {
        let t = theme();
        assert_eq!(chain_accent("ethereum", &t), t.accent_cyan);
    }

    #[test]
    fn base_accent_is_aqua() {
        let t = theme();
        assert_eq!(chain_accent("base", &t), t.accent_aqua);
    }

    #[test]
    fn unknown_chain_accent_is_powder() {
        let t = theme();
        assert_eq!(chain_accent("polygon", &t), t.accent_powder);
    }
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
