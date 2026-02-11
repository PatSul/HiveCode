use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// HelpPanel
// ---------------------------------------------------------------------------

pub struct HelpPanel;

impl HelpPanel {
    pub fn render(theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("help-panel")
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .p(theme.space_4)
            .gap(theme.space_4)
            .overflow_y_scroll()
            .child(render_header(theme))
            .child(render_quick_start(theme))
            .child(render_keyboard_shortcuts(theme))
            .child(render_features_overview(theme))
            .child(render_open_source_credits(theme))
            .child(render_about_section(theme))
            .child(render_support_section(theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(40.0))
                .h(px(40.0))
                .rounded(theme.radius_lg)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .child(Icon::new(IconName::Info).size_6()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .text_size(theme.font_size_2xl)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::BOLD)
                                .child("Help & Documentation"),
                        )
                        .child(version_badge(theme)),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Everything you need to get started with Hive"),
                ),
        )
        .into_any_element()
}

fn version_badge(theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_cyan)
        .child(format!("v{}", env!("CARGO_PKG_VERSION")))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn card(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
}

fn section_title(icon: &str, label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .child(icon.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(label.to_string()),
        )
        .into_any_element()
}

fn section_desc(text: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_muted)
        .child(text.to_string())
        .into_any_element()
}

fn separator(theme: &HiveTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(theme.border)
        .into_any_element()
}

/// A keyboard shortcut key rendered as a monospace pill.
fn key_pill(key: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_sm)
        .font_family(theme.font_mono.clone())
        .text_color(theme.accent_aqua)
        .child(key.to_string())
        .into_any_element()
}

/// A single shortcut row: pill on the left, description on the right.
fn shortcut_row(key: &str, desc: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .min_w(px(140.0))
                .child(key_pill(key, theme)),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(desc.to_string()),
        )
        .into_any_element()
}

/// A numbered step for the Quick Start section.
fn step_row(number: usize, title: &str, desc: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap(theme.space_3)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(28.0))
                .h(px(28.0))
                .rounded(theme.radius_full)
                .bg(theme.accent_cyan)
                .text_size(theme.font_size_sm)
                .font_weight(FontWeight::BOLD)
                .text_color(theme.text_on_accent)
                .child(format!("{}", number)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_secondary)
                        .child(desc.to_string()),
                ),
        )
        .into_any_element()
}

/// A feature card with icon, title, and description.
fn feature_card(icon: &str, title: &str, desc: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap(theme.space_3)
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_xl)
                .child(icon.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child(desc.to_string()),
                ),
        )
        .into_any_element()
}

/// A text link rendered in accent color.
fn link_label(label: &str, url: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.accent_cyan)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(url.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section 1: Quick Start
// ---------------------------------------------------------------------------

fn render_quick_start(theme: &HiveTheme) -> AnyElement {
    card(theme)
        .child(section_title("\u{1F680}", "Quick Start", theme))
        .child(section_desc(
            "Get up and running in four simple steps.",
            theme,
        ))
        .child(separator(theme))
        .child(step_row(
            1,
            "Set an API key",
            "Open Settings (Ctrl+,) and add your Anthropic, OpenAI, or OpenRouter key. \
             Or skip this step to use local models.",
            theme,
        ))
        .child(step_row(
            2,
            "Choose a model",
            "Use the model selector in the status bar, or enable Auto Routing \
             to let Hive pick the best model per task.",
            theme,
        ))
        .child(step_row(
            3,
            "Start chatting",
            "Type a message in the chat input at the bottom and press Enter. \
             Hive streams responses with full markdown and code highlighting.",
            theme,
        ))
        .child(step_row(
            4,
            "Explore features",
            "Browse the sidebar panels: Files, Git Review, Costs, Skills, \
             Token Launch, and more.",
            theme,
        ))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section 2: Keyboard Shortcuts
// ---------------------------------------------------------------------------

fn render_keyboard_shortcuts(theme: &HiveTheme) -> AnyElement {
    card(theme)
        .child(section_title("\u{2328}", "Keyboard Shortcuts", theme))
        .child(section_desc(
            "Speed up your workflow with these shortcuts.",
            theme,
        ))
        .child(separator(theme))
        .child(shortcut_row("Ctrl + Q", "Quit Hive", theme))
        .child(shortcut_row("Ctrl + ,", "Open Settings", theme))
        .child(shortcut_row("Ctrl + P", "Toggle Privacy Mode", theme))
        .child(shortcut_row("Ctrl + F", "Search files", theme))
        .child(shortcut_row("Ctrl + G", "Git operations", theme))
        .child(shortcut_row_coming_soon("Ctrl + K", "Command palette", theme))
        .child(separator(theme))
        .child(shortcut_row("Enter", "Send message", theme))
        .child(shortcut_row("Shift + Enter", "New line in chat", theme))
        .child(shortcut_row("Escape", "Cancel streaming / close modal", theme))
        .into_any_element()
}

/// A shortcut row with a "coming soon" badge after the description.
fn shortcut_row_coming_soon(key: &str, desc: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .min_w(px(140.0))
                .child(key_pill(key, theme)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_secondary)
                        .child(desc.to_string()),
                )
                .child(
                    div()
                        .px(theme.space_1)
                        .py(px(1.0))
                        .rounded(theme.radius_sm)
                        .bg(theme.bg_tertiary)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_yellow)
                        .child("coming soon"),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section 3: Features Overview (2-column grid)
// ---------------------------------------------------------------------------

fn render_features_overview(theme: &HiveTheme) -> AnyElement {
    let features: &[(&str, &str, &str)] = &[
        ("\u{1F916}", "Multi-Provider AI", "Route to 6+ providers with smart fallback"),
        ("\u{1F3E0}", "Local-First", "Run Ollama, LM Studio, or any local AI"),
        ("\u{1F4B0}", "Cost Intelligence", "Track spending, predict costs, optimize routing"),
        ("\u{1F41D}", "Agent System", "9-role HiveMind with autonomous iteration"),
        ("\u{1FA99}", "Token Launch", "Deploy SPL and ERC-20 tokens"),
        ("\u{1F6D2}", "Skills Marketplace", "Extend with community skills"),
        ("\u{1F50D}", "Code Review", "Git-aware diff viewing and analysis"),
        ("\u{1F4C4}", "Document Generation", "Export to PDF, DOCX, XLSX, CSV"),
    ];

    let mut grid = div()
        .flex()
        .flex_col()
        .gap(theme.space_2);

    // Render features as rows of 2
    let mut i = 0;
    while i < features.len() {
        let mut row = div()
            .flex()
            .flex_row()
            .gap(theme.space_2);

        row = row.child(
            div()
                .flex_1()
                .child(feature_card(features[i].0, features[i].1, features[i].2, theme)),
        );

        if i + 1 < features.len() {
            row = row.child(
                div()
                    .flex_1()
                    .child(feature_card(features[i + 1].0, features[i + 1].1, features[i + 1].2, theme)),
            );
        }

        grid = grid.child(row);
        i += 2;
    }

    card(theme)
        .child(section_title("\u{2B50}", "Features Overview", theme))
        .child(section_desc(
            "A snapshot of what Hive can do for you.",
            theme,
        ))
        .child(separator(theme))
        .child(grid)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section 4: Open Source Credits
// ---------------------------------------------------------------------------

/// A featured (Tier-1) credit card with cyan top-border accent.
fn featured_credit_card(
    name: &str,
    tagline: &str,
    author: &str,
    license: &str,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_4)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .border_t_2()
        .border_color(theme.accent_cyan)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(name.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(tagline.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("{} \u{2022} {}", author, license)),
        )
        .into_any_element()
}

/// A compact Tier-2 credit card.
fn credit_card(name: &str, desc: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .p(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.accent_cyan)
                .font_weight(FontWeight::BOLD)
                .child(name.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(format!("\u{2014} {}", desc)),
        )
        .into_any_element()
}

/// A compact Tier-3 dependency row.
fn dep_row(name: &str, desc: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .py(px(2.0))
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(name.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(desc.to_string()),
        )
        .into_any_element()
}

fn render_open_source_credits(theme: &HiveTheme) -> AnyElement {
    // -- Tier 1: Featured projects (full-width cards) --
    let featured = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(featured_credit_card(
            "GPUI",
            "GPU-accelerated UI framework",
            "Zed Industries",
            "Apache-2.0",
            theme,
        ))
        .child(featured_credit_card(
            "Tokio",
            "Async runtime for Rust",
            "Tokio Contributors",
            "MIT",
            theme,
        ))
        .child(featured_credit_card(
            "Rust",
            "Systems programming language",
            "The Rust Project",
            "MIT / Apache-2.0",
            theme,
        ));

    // -- Tier 2: Core libraries (2-column grid, grouped) --
    let tier2_data: &[(&str, &[(&str, &str)])] = &[
        (
            "Networking & Data",
            &[
                ("reqwest", "HTTP client (seanmonstar)"),
                ("serde", "Serialization framework (David Tolnay)"),
                ("rusqlite", "SQLite database (rusqlite contributors)"),
                ("git2", "Git operations via libgit2"),
            ],
        ),
        (
            "Security",
            &[
                ("RustCrypto", "AES-256-GCM, Argon2, SHA-2 encryption"),
                ("pulldown-cmark", "Markdown parsing"),
            ],
        ),
        (
            "Async & Runtime",
            &[
                ("futures", "Async abstractions"),
                ("tracing", "Structured diagnostics (Tokio)"),
            ],
        ),
    ];

    let mut tier2 = div().flex().flex_col().gap(theme.space_3);

    for (category, libs) in tier2_data {
        let mut group = div().flex().flex_col().gap(theme.space_1).child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .font_weight(FontWeight::BOLD)
                .child(category.to_string()),
        );

        // 2-column grid
        let mut grid = div().flex().flex_col().gap(theme.space_1);
        let mut i = 0;
        while i < libs.len() {
            let mut row = div().flex().flex_row().gap(theme.space_2);
            row = row.child(div().flex_1().child(credit_card(libs[i].0, libs[i].1, theme)));
            if i + 1 < libs.len() {
                row = row.child(div().flex_1().child(credit_card(libs[i + 1].0, libs[i + 1].1, theme)));
            }
            grid = grid.child(row);
            i += 2;
        }

        group = group.child(grid);
        tier2 = tier2.child(group);
    }

    // -- Tier 3: All remaining dependencies (compact list) --
    let tier3_deps: &[(&str, &str)] = &[
        ("notify", "File system watcher"),
        ("ignore", "Gitignore-aware file walking"),
        ("regex", "Regular expressions"),
        ("chrono", "Date and time"),
        ("uuid", "Unique identifiers"),
        ("anyhow", "Flexible error handling"),
        ("thiserror", "Derive-based errors"),
        ("tray-icon", "System tray integration"),
        ("rust-embed", "Asset embedding"),
        ("image", "Image processing"),
        ("parking_lot", "Faster synchronization primitives"),
        ("dirs", "Platform directory paths"),
        ("toml", "TOML parsing"),
        ("csv", "CSV reading/writing"),
        ("rust_xlsxwriter", "Excel file generation"),
        ("docx-rs", "Word document generation"),
        ("zip", "ZIP archive support"),
        ("url", "URL parsing"),
        ("hex", "Hex encoding"),
        ("whoami", "System user info"),
        ("winrt-notification", "Windows toast notifications"),
        ("async-trait", "Async trait support"),
        ("gpui-component", "GPUI component library"),
    ];

    let mut tier3 = div().flex().flex_col().gap(px(2.0));
    for (name, desc) in tier3_deps {
        tier3 = tier3.child(dep_row(name, desc, theme));
    }

    // -- Footer --
    let footer = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(theme.space_1)
        .pt(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .child("Thank you to the entire Rust ecosystem \u{1F9E1}"),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("\u{00A9} 2025\u{2013}2026 Airglow Studios. All rights reserved."),
        );

    card(theme)
        .child(section_title("\u{1F4E6}", "Open Source Credits", theme))
        .child(section_desc(
            "Hive is built on these outstanding open source projects.",
            theme,
        ))
        .child(separator(theme))
        .child(featured)
        .child(separator(theme))
        .child(tier2)
        .child(separator(theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_secondary)
                        .font_weight(FontWeight::BOLD)
                        .child("Additional Dependencies"),
                )
                .child(tier3),
        )
        .child(separator(theme))
        .child(footer)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section 5: About
// ---------------------------------------------------------------------------

fn render_about_section(theme: &HiveTheme) -> AnyElement {
    card(theme)
        .child(section_title("\u{1F41D}", "About Hive", theme))
        .child(separator(theme))
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(
                    "Hive is an AI-powered desktop assistant for software engineering, \
                     built from the ground up in Rust with GPU-accelerated rendering via GPUI. \
                     It supports multi-provider AI routing, local-first privacy, cost intelligence, \
                     multi-agent orchestration, and a skills marketplace.",
                ),
        )
        .child(separator(theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(theme.space_2)
                .child(link_label("GitHub", "github.com/AirglowStudios/Hive", theme))
                .child(link_label("Documentation", "docs.hive.dev", theme))
                .child(link_label("Changelog", "github.com/AirglowStudios/Hive/releases", theme)),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section 6: Support
// ---------------------------------------------------------------------------

fn render_support_section(theme: &HiveTheme) -> AnyElement {
    card(theme)
        .child(section_title("\u{1F6E0}", "Support", theme))
        .child(section_desc(
            "Need help or have feedback? Reach out through these channels.",
            theme,
        ))
        .child(separator(theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(theme.space_2)
                .child(link_label(
                    "\u{1F41B} Report a Bug",
                    "github.com/AirglowStudios/Hive/issues/new",
                    theme,
                ))
                .child(link_label(
                    "\u{1F4A1} Request a Feature",
                    "github.com/AirglowStudios/Hive/discussions/new",
                    theme,
                )),
        )
        .child(separator(theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child(format!("Hive v{}", env!("CARGO_PKG_VERSION"))),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Built with GPUI \u{2014} GPU-accelerated at 120 fps"),
                ),
        )
        .into_any_element()
}
