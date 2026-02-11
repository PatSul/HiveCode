use gpui::*;
use gpui_component::{Icon, IconName};

use crate::components::render_wallet_card;
use crate::components::render_wizard_stepper;
use crate::theme::HiveTheme;
use crate::workspace::{TokenLaunchDeploy, TokenLaunchSelectChain, TokenLaunchSetStep};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Which step of the wizard the user is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    SelectChain,
    TokenDetails,
    WalletSetup,
    Deploy,
}

impl WizardStep {
    /// Zero-based index for the stepper component.
    pub fn index(self) -> usize {
        match self {
            Self::SelectChain => 0,
            Self::TokenDetails => 1,
            Self::WalletSetup => 2,
            Self::Deploy => 3,
        }
    }

    /// The next step in the wizard, if any.
    fn next(self) -> Option<Self> {
        match self {
            Self::SelectChain => Some(Self::TokenDetails),
            Self::TokenDetails => Some(Self::WalletSetup),
            Self::WalletSetup => Some(Self::Deploy),
            Self::Deploy => None,
        }
    }

    /// The previous step in the wizard, if any.
    fn prev(self) -> Option<Self> {
        match self {
            Self::SelectChain => None,
            Self::TokenDetails => Some(Self::SelectChain),
            Self::WalletSetup => Some(Self::TokenDetails),
            Self::Deploy => Some(Self::WalletSetup),
        }
    }
}

/// Supported blockchain networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainOption {
    Solana,
    Ethereum,
    Base,
}

impl ChainOption {
    pub fn name(self) -> &'static str {
        match self {
            Self::Solana => "Solana",
            Self::Ethereum => "Ethereum",
            Self::Base => "Base",
        }
    }

    pub fn standard(self) -> &'static str {
        match self {
            Self::Solana => "SPL Token",
            Self::Ethereum => "ERC-20",
            Self::Base => "ERC-20",
        }
    }

    pub fn default_decimals(self) -> u8 {
        match self {
            Self::Solana => 9,
            Self::Ethereum | Self::Base => 18,
        }
    }
}

/// Current deploy status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployStatus {
    NotStarted,
    Deploying,
    Success(String),
    Failed(String),
}

/// All data needed to render and drive the Token Launch Wizard.
#[derive(Debug, Clone)]
pub struct TokenLaunchData {
    pub current_step: WizardStep,
    pub selected_chain: Option<ChainOption>,
    pub token_name: String,
    pub token_symbol: String,
    pub total_supply: String,
    pub decimals: u8,
    pub wallet_address: Option<String>,
    pub wallet_balance: Option<f64>,
    pub estimated_cost: Option<f64>,
    pub deploy_status: DeployStatus,
}

impl TokenLaunchData {
    /// Create a new wizard state starting at step 1 with all fields at their defaults.
    pub fn new() -> Self {
        Self {
            current_step: WizardStep::SelectChain,
            selected_chain: None,
            token_name: String::new(),
            token_symbol: String::new(),
            total_supply: String::new(),
            decimals: 9,
            wallet_address: None,
            wallet_balance: None,
            estimated_cost: None,
            deploy_status: DeployStatus::NotStarted,
        }
    }

    /// Advance to the next wizard step if validation passes and a next step exists.
    /// Returns `true` if the step actually changed.
    pub fn advance_step(&mut self) -> bool {
        if !self.can_advance() {
            return false;
        }
        if let Some(next) = self.current_step.next() {
            self.current_step = next;
            return true;
        }
        false
    }

    /// Go back to the previous wizard step.
    /// Returns `true` if the step actually changed.
    pub fn go_back(&mut self) -> bool {
        if let Some(prev) = self.current_step.prev() {
            self.current_step = prev;
            return true;
        }
        false
    }

    /// Whether the current step's required fields are filled, allowing advancement.
    pub fn can_advance(&self) -> bool {
        match self.current_step {
            WizardStep::SelectChain => self.selected_chain.is_some(),
            WizardStep::TokenDetails => {
                !self.token_name.is_empty()
                    && !self.token_symbol.is_empty()
                    && !self.total_supply.is_empty()
            }
            WizardStep::WalletSetup => self.wallet_address.is_some(),
            WizardStep::Deploy => false,
        }
    }

    /// Reset the wizard back to step 1 with all fields cleared.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Whether the connected wallet has enough funds for the estimated deployment cost.
    pub fn has_sufficient_funds(&self) -> bool {
        match (self.wallet_balance, self.estimated_cost) {
            (Some(balance), Some(cost)) => balance >= cost,
            _ => false,
        }
    }
}

impl Default for TokenLaunchData {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Token Launch Wizard: 4-step process for deploying tokens.
pub struct TokenLaunchPanel;

impl TokenLaunchPanel {
    /// Render the wizard with the given data snapshot.
    pub fn render(data: &TokenLaunchData, theme: &HiveTheme) -> impl IntoElement {
        let step_content: AnyElement = match data.current_step {
            WizardStep::SelectChain => render_chain_selection(data, theme),
            WizardStep::TokenDetails => render_token_details(data, theme),
            WizardStep::WalletSetup => render_wallet_step(data, theme),
            WizardStep::Deploy => render_deploy_step(data, theme),
        };

        div()
            .id("token-launch-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(theme))
            .child(render_stepper(data, theme))
            .child(step_content)
            .child(render_navigation(data, theme))
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
                .child(Icon::new(IconName::Globe).size_4()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_2xl)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Token Launch Wizard"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Deploy your own token in four simple steps"),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Stepper
// ---------------------------------------------------------------------------

fn render_stepper(data: &TokenLaunchData, theme: &HiveTheme) -> AnyElement {
    let steps = &["Chain", "Details", "Wallet", "Deploy"];
    render_wizard_stepper(steps, data.current_step.index(), theme).into_any_element()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// A card container matching the settings panel pattern.
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

/// Section title with icon.
fn section_title(icon: &str, label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(div().text_size(theme.font_size_lg).child(icon.to_string()))
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(label.to_string()),
        )
        .into_any_element()
}

/// Section description text.
fn section_desc(text: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_muted)
        .child(text.to_string())
        .into_any_element()
}

/// Horizontal separator line.
fn separator(theme: &HiveTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(theme.border)
        .into_any_element()
}

/// A styled button with accent background.
fn accent_button(label: &str, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_4)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(color)
        .text_size(theme.font_size_base)
        .text_color(theme.text_on_accent)
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.to_string())
        .into_any_element()
}

/// A styled secondary button with border.
fn secondary_button(label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_4)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_tertiary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_base)
        .text_color(theme.text_secondary)
        .child(label.to_string())
        .into_any_element()
}

/// A disabled button (muted appearance, no cursor pointer).
fn disabled_button(label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_4)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_base)
        .text_color(theme.text_muted)
        .child(label.to_string())
        .into_any_element()
}

/// A read-only input placeholder row (label + value box).
fn input_row(label: &str, value: &str, placeholder: &str, theme: &HiveTheme) -> AnyElement {
    let display = if value.is_empty() { placeholder } else { value };
    let text_color = if value.is_empty() {
        theme.text_muted
    } else {
        theme.text_primary
    };

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .font_weight(FontWeight::MEDIUM)
                .child(label.to_string()),
        )
        .child(
            div()
                .px(theme.space_3)
                .py(theme.space_2)
                .w_full()
                .rounded(theme.radius_sm)
                .bg(theme.bg_primary)
                .border_1()
                .border_color(theme.border)
                .text_size(theme.font_size_base)
                .text_color(text_color)
                .child(display.to_string()),
        )
        .into_any_element()
}

/// A key-value row for the summary display.
fn summary_row(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(color)
                .font_weight(FontWeight::MEDIUM)
                .child(value.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Step 1: Chain Selection
// ---------------------------------------------------------------------------

fn render_chain_selection(data: &TokenLaunchData, theme: &HiveTheme) -> AnyElement {
    let selected = data.selected_chain;

    card(theme)
        .child(section_title("\u{1F310}", "Select Blockchain", theme))
        .child(section_desc(
            "Choose which network to deploy your token on",
            theme,
        ))
        .child(separator(theme))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(theme.space_3)
                .child(render_chain_card(
                    "chain-solana",
                    "solana",
                    "\u{1F680}",
                    "Solana",
                    "SPL Token",
                    "Fast & cheap",
                    "~0.05 SOL",
                    "~10 seconds",
                    selected == Some(ChainOption::Solana),
                    theme,
                ))
                .child(render_chain_card(
                    "chain-ethereum",
                    "ethereum",
                    "\u{1F48E}",
                    "Ethereum",
                    "ERC-20",
                    "Most established",
                    "~0.05 ETH + gas",
                    "~30 seconds",
                    selected == Some(ChainOption::Ethereum),
                    theme,
                ))
                .child(render_chain_card(
                    "chain-base",
                    "base",
                    "\u{26A1}",
                    "Base",
                    "ERC-20",
                    "Low gas L2",
                    "~0.001 ETH + gas",
                    "~5 seconds",
                    selected == Some(ChainOption::Base),
                    theme,
                )),
        )
        .into_any_element()
}

fn render_chain_card(
    element_id: &str,
    chain_key: &str,
    icon: &str,
    name: &str,
    standard: &str,
    description: &str,
    cost: &str,
    speed: &str,
    selected: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let border_color = if selected {
        theme.accent_aqua
    } else {
        theme.border
    };
    let bg = if selected {
        theme.bg_tertiary
    } else {
        theme.bg_secondary
    };

    let chain = chain_key.to_string();
    div()
        .id(SharedString::from(element_id.to_string()))
        .flex()
        .flex_col()
        .flex_1()
        .items_center()
        .gap(theme.space_2)
        .p(theme.space_4)
        .bg(bg)
        .border_2()
        .border_color(border_color)
        .rounded(theme.radius_md)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(TokenLaunchSelectChain {
                    chain: chain.clone(),
                }),
                cx,
            );
        })
        // Icon
        .child(div().text_size(px(32.0)).child(icon.to_string()))
        // Name
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(name.to_string()),
        )
        // Standard badge
        .child(
            div()
                .px(theme.space_2)
                .py(theme.space_1)
                .rounded(theme.radius_sm)
                .bg(theme.bg_surface)
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_cyan)
                .child(standard.to_string()),
        )
        // Description
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(description.to_string()),
        )
        // Cost estimate
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .mt(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Cost:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_yellow)
                        .font_weight(FontWeight::MEDIUM)
                        .child(cost.to_string()),
                ),
        )
        // Speed indicator
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Speed:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_green)
                        .font_weight(FontWeight::MEDIUM)
                        .child(speed.to_string()),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Step 2: Token Details
// ---------------------------------------------------------------------------

fn render_token_details(data: &TokenLaunchData, theme: &HiveTheme) -> AnyElement {
    let chain_label = data
        .selected_chain
        .map(|c| format!("{} ({})", c.name(), c.standard()))
        .unwrap_or_else(|| "Not selected".to_string());

    card(theme)
        .child(section_title("\u{270F}", "Token Details", theme))
        .child(section_desc(
            "Configure your token's name, symbol, and supply",
            theme,
        ))
        .child(separator(theme))
        // Chain reminder
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .px(theme.space_3)
                .py(theme.space_2)
                .rounded(theme.radius_sm)
                .bg(theme.bg_primary)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Deploying on:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_cyan)
                        .font_weight(FontWeight::MEDIUM)
                        .child(chain_label),
                ),
        )
        // Input fields
        .child(input_row(
            "Token Name",
            &data.token_name,
            "e.g. My Awesome Token",
            theme,
        ))
        .child(input_row(
            "Token Symbol (3-5 chars)",
            &data.token_symbol,
            "e.g. MAT",
            theme,
        ))
        .child(input_row(
            "Total Supply",
            &data.total_supply,
            "e.g. 1000000000",
            theme,
        ))
        .child(input_row(
            "Decimals",
            &data.decimals.to_string(),
            "9",
            theme,
        ))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Step 3: Wallet
// ---------------------------------------------------------------------------

fn render_wallet_step(data: &TokenLaunchData, theme: &HiveTheme) -> AnyElement {
    let chain_name = data
        .selected_chain
        .map(|c| c.name())
        .unwrap_or("Unknown");

    let wallet_display: AnyElement = match &data.wallet_address {
        Some(address) => {
            let balance = data.wallet_balance.unwrap_or(0.0);
            render_wallet_card(chain_name, address, balance, theme).into_any_element()
        }
        None => {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .py(theme.space_6)
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(px(32.0))
                        .child("\u{1F4B3}"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_muted)
                        .child("No wallet connected"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Create or import a wallet to continue"),
                )
                .into_any_element()
        }
    };

    let fund_status: AnyElement = if data.wallet_address.is_some() {
        let sufficient = data.has_sufficient_funds();
        let (label, color) = if sufficient {
            ("Sufficient funds for deployment", theme.accent_green)
        } else {
            ("Insufficient funds -- please add funds before deploying", theme.accent_red)
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_sm)
            .bg(theme.bg_primary)
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(theme.radius_full)
                    .bg(color),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(color)
                    .child(label),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };

    let cost_display: AnyElement = match data.estimated_cost {
        Some(cost) => {
            let cost_str = format!("{cost:.6}");
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px(theme.space_3)
                .py(theme.space_2)
                .rounded(theme.radius_sm)
                .bg(theme.bg_primary)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Estimated deployment cost:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.accent_yellow)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(cost_str),
                )
                .into_any_element()
        }
        None => div().into_any_element(),
    };

    card(theme)
        .child(section_title("\u{1F4B0}", "Wallet", theme))
        .child(section_desc(
            "Connect or create a wallet to fund the deployment",
            theme,
        ))
        .child(separator(theme))
        .child(wallet_display)
        // Action buttons
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap(theme.space_3)
                .child(accent_button("Create New Wallet", theme.accent_aqua, theme))
                .child(secondary_button("Import Existing", theme)),
        )
        .child(fund_status)
        .child(cost_display)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Step 4: Deploy
// ---------------------------------------------------------------------------

fn render_deploy_step(data: &TokenLaunchData, theme: &HiveTheme) -> AnyElement {
    let chain_name = data.selected_chain.map(|c| c.name()).unwrap_or("--");
    let chain_standard = data.selected_chain.map(|c| c.standard()).unwrap_or("--");
    let wallet_addr = data.wallet_address.as_deref().unwrap_or("No wallet");
    let wallet_balance = data
        .wallet_balance
        .map(|b| format!("{b:.6}"))
        .unwrap_or_else(|| "0".to_string());
    let cost = data
        .estimated_cost
        .map(|c| format!("{c:.6}"))
        .unwrap_or_else(|| "Calculating...".to_string());

    let name_display = if data.token_name.is_empty() { "--" } else { &data.token_name };
    let symbol_display = if data.token_symbol.is_empty() { "--" } else { &data.token_symbol };
    let supply_display = if data.total_supply.is_empty() { "--" } else { &data.total_supply };

    let status_display: AnyElement = match &data.deploy_status {
        DeployStatus::NotStarted => div().into_any_element(),
        DeployStatus::Deploying => {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap(theme.space_2)
                .py(theme.space_3)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.accent_cyan)
                        .child("\u{23F3} Deploying token..."),
                )
                .into_any_element()
        }
        DeployStatus::Success(contract_addr) => {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(theme.space_2)
                .py(theme.space_3)
                .px(theme.space_4)
                .rounded(theme.radius_md)
                .bg(theme.bg_primary)
                .child(
                    div()
                        .text_size(theme.font_size_lg)
                        .text_color(theme.accent_green)
                        .font_weight(FontWeight::BOLD)
                        .child("\u{2705} Token Deployed Successfully!"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(theme.space_1)
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child("Contract Address:"),
                        )
                        .child(
                            div()
                                .px(theme.space_2)
                                .py(px(2.0))
                                .rounded(theme.radius_sm)
                                .bg(theme.bg_tertiary)
                                .text_size(theme.font_size_sm)
                                .font_family(theme.font_mono.clone())
                                .text_color(theme.accent_aqua)
                                .child(contract_addr.clone()),
                        ),
                )
                .into_any_element()
        }
        DeployStatus::Failed(msg) => {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(theme.space_2)
                .py(theme.space_3)
                .px(theme.space_4)
                .rounded(theme.radius_md)
                .bg(theme.bg_primary)
                .child(
                    div()
                        .text_size(theme.font_size_lg)
                        .text_color(theme.accent_red)
                        .font_weight(FontWeight::BOLD)
                        .child("\u{274C} Deployment Failed"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.accent_red)
                        .child(msg.clone()),
                )
                .into_any_element()
        }
    };

    card(theme)
        .child(section_title("\u{1F680}", "Review & Deploy", theme))
        .child(section_desc(
            "Verify your token configuration before deployment",
            theme,
        ))
        .child(separator(theme))
        // Summary
        .child(summary_row("Chain", chain_name, theme.accent_cyan, theme))
        .child(summary_row("Standard", chain_standard, theme.accent_cyan, theme))
        .child(summary_row("Token Name", name_display, theme.text_primary, theme))
        .child(summary_row("Symbol", symbol_display, theme.text_primary, theme))
        .child(summary_row("Total Supply", supply_display, theme.text_primary, theme))
        .child(summary_row(
            "Decimals",
            &data.decimals.to_string(),
            theme.text_primary,
            theme,
        ))
        .child(separator(theme))
        // Wallet info
        .child(summary_row("Wallet", wallet_addr, theme.accent_aqua, theme))
        .child(summary_row("Balance", &wallet_balance, theme.accent_green, theme))
        .child(summary_row("Estimated Cost", &cost, theme.accent_yellow, theme))
        .child(separator(theme))
        // Deploy button (centered, large)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .py(theme.space_3)
                .child(
                    div()
                        .id("btn-deploy-token")
                        .px(theme.space_8)
                        .py(theme.space_3)
                        .rounded(theme.radius_lg)
                        .bg(theme.accent_aqua)
                        .text_size(theme.font_size_lg)
                        .text_color(theme.text_on_accent)
                        .font_weight(FontWeight::BOLD)
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            window.dispatch_action(Box::new(TokenLaunchDeploy), cx);
                        })
                        .child("\u{1F680} Deploy Token"),
                ),
        )
        // Status area
        .child(status_display)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Navigation (Back / Next)
// ---------------------------------------------------------------------------

fn render_navigation(data: &TokenLaunchData, theme: &HiveTheme) -> AnyElement {
    let show_back = data.current_step != WizardStep::SelectChain;
    let is_deploy_step = data.current_step == WizardStep::Deploy;
    let next_enabled = data.can_advance();
    let current_index = data.current_step.index();

    let back_button: AnyElement = if show_back {
        let back_step = current_index.saturating_sub(1);
        div()
            .id("btn-wizard-back")
            .px(theme.space_4)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .bg(theme.bg_tertiary)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_base)
            .text_color(theme.text_secondary)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                window.dispatch_action(
                    Box::new(TokenLaunchSetStep { step: back_step }),
                    cx,
                );
            })
            .child("\u{2190} Back")
            .into_any_element()
    } else {
        div().into_any_element()
    };

    let next_button: AnyElement = if is_deploy_step {
        div().into_any_element()
    } else if next_enabled {
        let next_step = current_index + 1;
        div()
            .id("btn-wizard-next")
            .px(theme.space_4)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .bg(theme.accent_aqua)
            .text_size(theme.font_size_base)
            .text_color(theme.text_on_accent)
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                window.dispatch_action(
                    Box::new(TokenLaunchSetStep { step: next_step }),
                    cx,
                );
            })
            .child("Next \u{2192}")
            .into_any_element()
    } else {
        disabled_button("Next \u{2192}", theme)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .pt(theme.space_2)
        .child(back_button)
        .child(div().flex_1())
        .child(next_button)
        .into_any_element()
}
