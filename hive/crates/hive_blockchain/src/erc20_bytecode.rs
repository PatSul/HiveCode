use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::wallet_store::Chain;

/// Pre-compiled ERC-20 contract ABI and bytecode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc20Contract {
    pub abi: serde_json::Value,
    pub bytecode: String,
}

/// Returns a minimal ERC-20 contract with a standard ABI and placeholder bytecode.
///
/// The ABI covers the six mandatory ERC-20 view/transfer functions plus the
/// constructor. The bytecode is a placeholder -- real deployment requires a
/// compiled Solidity artifact or an equivalent.
pub fn get_erc20_contract() -> Erc20Contract {
    let abi = serde_json::json!([
        {
            "type": "constructor",
            "inputs": [
                { "name": "name_", "type": "string" },
                { "name": "symbol_", "type": "string" },
                { "name": "decimals_", "type": "uint8" },
                { "name": "totalSupply_", "type": "uint256" }
            ]
        },
        {
            "type": "function",
            "name": "name",
            "inputs": [],
            "outputs": [{ "name": "", "type": "string" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "symbol",
            "inputs": [],
            "outputs": [{ "name": "", "type": "string" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "decimals",
            "inputs": [],
            "outputs": [{ "name": "", "type": "uint8" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "totalSupply",
            "inputs": [],
            "outputs": [{ "name": "", "type": "uint256" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "balanceOf",
            "inputs": [{ "name": "account", "type": "address" }],
            "outputs": [{ "name": "", "type": "uint256" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "transfer",
            "inputs": [
                { "name": "to", "type": "address" },
                { "name": "amount", "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        },
        {
            "type": "function",
            "name": "approve",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount", "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        },
        {
            "type": "function",
            "name": "transferFrom",
            "inputs": [
                { "name": "from", "type": "address" },
                { "name": "to", "type": "address" },
                { "name": "amount", "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        }
    ]);

    Erc20Contract {
        abi,
        bytecode: "0x_PLACEHOLDER_BYTECODE".to_string(),
    }
}

/// Network-specific configuration for a blockchain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub name: String,
    pub chain_id: u64,
    pub rpc_url: String,
    pub explorer_url: String,
}

/// Returns default chain configurations for all supported networks.
pub fn get_chain_configs() -> HashMap<Chain, ChainConfig> {
    let mut configs = HashMap::new();

    configs.insert(
        Chain::Ethereum,
        ChainConfig {
            name: "Ethereum Mainnet".to_string(),
            chain_id: 1,
            rpc_url: "https://eth.llamarpc.com".to_string(),
            explorer_url: "https://etherscan.io".to_string(),
        },
    );

    configs.insert(
        Chain::Base,
        ChainConfig {
            name: "Base Mainnet".to_string(),
            chain_id: 8453,
            rpc_url: "https://mainnet.base.org".to_string(),
            explorer_url: "https://basescan.org".to_string(),
        },
    );

    configs.insert(
        Chain::Solana,
        ChainConfig {
            name: "Solana Mainnet".to_string(),
            chain_id: 0,
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            explorer_url: "https://explorer.solana.com".to_string(),
        },
    );

    configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erc20_contract_has_expected_abi_entries() {
        let contract = get_erc20_contract();
        let abi = contract.abi.as_array().unwrap();
        // constructor + 8 functions
        assert_eq!(abi.len(), 9);
    }

    #[test]
    fn erc20_contract_serializes() {
        let contract = get_erc20_contract();
        let json = serde_json::to_string(&contract).unwrap();
        assert!(json.contains("transfer"));
        assert!(json.contains("balanceOf"));
    }

    #[test]
    fn chain_configs_cover_all_chains() {
        let configs = get_chain_configs();
        assert!(configs.contains_key(&Chain::Ethereum));
        assert!(configs.contains_key(&Chain::Base));
        assert!(configs.contains_key(&Chain::Solana));
    }

    #[test]
    fn chain_config_ids_match_chain_enum() {
        let configs = get_chain_configs();
        for (chain, config) in &configs {
            assert_eq!(chain.chain_id(), config.chain_id);
        }
    }

    #[test]
    fn chain_config_rpc_urls_are_https() {
        let configs = get_chain_configs();
        for config in configs.values() {
            assert!(
                config.rpc_url.starts_with("https://"),
                "RPC URL must be HTTPS: {}",
                config.rpc_url
            );
        }
    }
}
