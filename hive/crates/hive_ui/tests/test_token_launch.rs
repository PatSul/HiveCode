use hive_ui::panels::token_launch::*;

#[test]
fn new_starts_at_select_chain() {
    let data = TokenLaunchData::new();
    assert_eq!(data.current_step, WizardStep::SelectChain);
    assert_eq!(data.selected_chain, None);
    assert!(data.token_name.is_empty());
    assert!(data.token_symbol.is_empty());
    assert!(data.total_supply.is_empty());
    assert_eq!(data.decimals, 9);
    assert_eq!(data.wallet_address, None);
    assert_eq!(data.wallet_balance, None);
    assert_eq!(data.estimated_cost, None);
    assert_eq!(data.deploy_status, DeployStatus::NotStarted);
}

#[test]
fn default_matches_new() {
    let from_new = TokenLaunchData::new();
    let from_default = TokenLaunchData::default();
    assert_eq!(from_new.current_step, from_default.current_step);
    assert_eq!(from_new.selected_chain, from_default.selected_chain);
    assert_eq!(from_new.token_name, from_default.token_name);
    assert_eq!(from_new.decimals, from_default.decimals);
}

#[test]
fn cannot_advance_step1_without_chain() {
    let data = TokenLaunchData::new();
    assert!(!data.can_advance());
}

#[test]
fn can_advance_step1_with_chain() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Solana);
    assert!(data.can_advance());
}

#[test]
fn advance_step_moves_forward_when_valid() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Ethereum);
    assert!(data.advance_step());
    assert_eq!(data.current_step, WizardStep::TokenDetails);
}

#[test]
fn advance_step_blocked_when_invalid() {
    let mut data = TokenLaunchData::new();
    // No chain selected, so cannot advance
    assert!(!data.advance_step());
    assert_eq!(data.current_step, WizardStep::SelectChain);
}

#[test]
fn cannot_advance_step2_without_name_symbol_supply() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Base);
    data.advance_step(); // -> TokenDetails

    // Missing all fields
    assert!(!data.can_advance());

    // Only name
    data.token_name = "MyToken".into();
    assert!(!data.can_advance());

    // Name + symbol but no supply
    data.token_symbol = "MTK".into();
    assert!(!data.can_advance());

    // All three filled
    data.total_supply = "1000000".into();
    assert!(data.can_advance());
}

#[test]
fn cannot_advance_step3_without_wallet() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Solana);
    data.advance_step(); // -> TokenDetails
    data.token_name = "Hive".into();
    data.token_symbol = "HIVE".into();
    data.total_supply = "1000000000".into();
    data.advance_step(); // -> WalletSetup

    assert_eq!(data.current_step, WizardStep::WalletSetup);
    assert!(!data.can_advance());

    data.wallet_address = Some("7EcDhSYGxXyscszYEp35KHN8vvw3svAuLKTzXwCFLtV".into());
    assert!(data.can_advance());
}

#[test]
fn cannot_advance_past_deploy() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Ethereum);
    data.advance_step(); // -> TokenDetails
    data.token_name = "T".into();
    data.token_symbol = "T".into();
    data.total_supply = "1".into();
    data.advance_step(); // -> WalletSetup
    data.wallet_address = Some("0xabc".into());
    data.advance_step(); // -> Deploy

    assert_eq!(data.current_step, WizardStep::Deploy);
    assert!(!data.can_advance());
    assert!(!data.advance_step());
    assert_eq!(data.current_step, WizardStep::Deploy);
}

#[test]
fn go_back_from_step1_does_nothing() {
    let mut data = TokenLaunchData::new();
    assert!(!data.go_back());
    assert_eq!(data.current_step, WizardStep::SelectChain);
}

#[test]
fn go_back_returns_to_previous_step() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Solana);
    data.advance_step(); // -> TokenDetails
    assert_eq!(data.current_step, WizardStep::TokenDetails);

    assert!(data.go_back());
    assert_eq!(data.current_step, WizardStep::SelectChain);
}

#[test]
fn go_back_preserves_entered_data() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Base);
    data.advance_step(); // -> TokenDetails
    data.token_name = "KeepMe".into();
    data.token_symbol = "KM".into();

    data.go_back(); // -> SelectChain
    assert_eq!(data.token_name, "KeepMe");
    assert_eq!(data.token_symbol, "KM");
    assert_eq!(data.selected_chain, Some(ChainOption::Base));
}

#[test]
fn reset_clears_everything() {
    let mut data = TokenLaunchData::new();
    data.selected_chain = Some(ChainOption::Ethereum);
    data.advance_step();
    data.token_name = "ResetMe".into();
    data.token_symbol = "RM".into();
    data.total_supply = "999".into();
    data.decimals = 18;
    data.wallet_address = Some("0xdef".into());
    data.wallet_balance = Some(1.5);
    data.estimated_cost = Some(0.01);
    data.deploy_status = DeployStatus::Failed("oops".into());

    data.reset();

    assert_eq!(data.current_step, WizardStep::SelectChain);
    assert_eq!(data.selected_chain, None);
    assert!(data.token_name.is_empty());
    assert!(data.token_symbol.is_empty());
    assert!(data.total_supply.is_empty());
    assert_eq!(data.decimals, 9);
    assert_eq!(data.wallet_address, None);
    assert_eq!(data.wallet_balance, None);
    assert_eq!(data.estimated_cost, None);
    assert_eq!(data.deploy_status, DeployStatus::NotStarted);
}

#[test]
fn full_wizard_flow_forward_and_back() {
    let mut data = TokenLaunchData::new();

    // Step 1: select chain
    data.selected_chain = Some(ChainOption::Solana);
    assert!(data.advance_step());
    assert_eq!(data.current_step, WizardStep::TokenDetails);

    // Step 2: fill token details
    data.token_name = "HiveToken".into();
    data.token_symbol = "HIVE".into();
    data.total_supply = "1000000000".into();
    data.decimals = 9;
    assert!(data.advance_step());
    assert_eq!(data.current_step, WizardStep::WalletSetup);

    // Step 3: connect wallet
    data.wallet_address = Some("7EcDhSYGxXyscszYEp35KHN8vvw3svAuLKTzXwCFLtV".into());
    data.wallet_balance = Some(2.0);
    data.estimated_cost = Some(0.05);
    assert!(data.advance_step());
    assert_eq!(data.current_step, WizardStep::Deploy);

    // Go back two steps
    assert!(data.go_back());
    assert_eq!(data.current_step, WizardStep::WalletSetup);
    assert!(data.go_back());
    assert_eq!(data.current_step, WizardStep::TokenDetails);

    // Data still intact
    assert_eq!(data.token_name, "HiveToken");
    assert_eq!(data.wallet_address.as_deref(), Some("7EcDhSYGxXyscszYEp35KHN8vvw3svAuLKTzXwCFLtV"));
}

#[test]
fn has_sufficient_funds_logic() {
    let mut data = TokenLaunchData::new();

    // No balance or cost set
    assert!(!data.has_sufficient_funds());

    // Only balance set
    data.wallet_balance = Some(1.0);
    assert!(!data.has_sufficient_funds());

    // Both set, balance >= cost
    data.estimated_cost = Some(0.5);
    assert!(data.has_sufficient_funds());

    // Balance exactly equals cost
    data.wallet_balance = Some(0.5);
    assert!(data.has_sufficient_funds());

    // Balance less than cost
    data.wallet_balance = Some(0.4);
    assert!(!data.has_sufficient_funds());
}

#[test]
fn wizard_step_index_mapping() {
    assert_eq!(WizardStep::SelectChain.index(), 0);
    assert_eq!(WizardStep::TokenDetails.index(), 1);
    assert_eq!(WizardStep::WalletSetup.index(), 2);
    assert_eq!(WizardStep::Deploy.index(), 3);
}

#[test]
fn chain_option_properties() {
    assert_eq!(ChainOption::Solana.name(), "Solana");
    assert_eq!(ChainOption::Solana.standard(), "SPL Token");
    assert_eq!(ChainOption::Solana.default_decimals(), 9);

    assert_eq!(ChainOption::Ethereum.name(), "Ethereum");
    assert_eq!(ChainOption::Ethereum.standard(), "ERC-20");
    assert_eq!(ChainOption::Ethereum.default_decimals(), 18);

    assert_eq!(ChainOption::Base.name(), "Base");
    assert_eq!(ChainOption::Base.standard(), "ERC-20");
    assert_eq!(ChainOption::Base.default_decimals(), 18);
}

#[test]
fn deploy_status_variants() {
    let not_started = DeployStatus::NotStarted;
    let deploying = DeployStatus::Deploying;
    let success = DeployStatus::Success("0xabc".into());
    let failed = DeployStatus::Failed("gas too low".into());

    assert_eq!(not_started, DeployStatus::NotStarted);
    assert_eq!(deploying, DeployStatus::Deploying);
    assert_ne!(success, failed);

    if let DeployStatus::Success(addr) = &success {
        assert_eq!(addr, "0xabc");
    } else {
        panic!("expected Success variant");
    }

    if let DeployStatus::Failed(msg) = &failed {
        assert_eq!(msg, "gas too low");
    } else {
        panic!("expected Failed variant");
    }
}
