use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::erc20_bytecode::get_chain_configs;
use crate::wallet_store::Chain;

/// Configuration for a single RPC endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    pub chain: Chain,
    pub url: String,
    pub is_custom: bool,
    pub timeout_secs: u64,
}

const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Manages per-chain RPC endpoint configuration with custom override support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfigStore {
    configs: HashMap<Chain, RpcConfig>,
}

impl RpcConfigStore {
    /// Create a store populated with default RPC URLs from [`get_chain_configs`].
    pub fn with_defaults() -> Self {
        let chain_configs = get_chain_configs();
        let configs = chain_configs
            .into_iter()
            .map(|(chain, cc)| {
                let rpc = RpcConfig {
                    chain,
                    url: cc.rpc_url,
                    is_custom: false,
                    timeout_secs: DEFAULT_TIMEOUT_SECS,
                };
                (chain, rpc)
            })
            .collect();

        Self { configs }
    }

    /// Get the RPC configuration for a chain. Returns `None` if the chain has
    /// no configuration (should not happen after [`with_defaults`]).
    pub fn get_rpc(&self, chain: Chain) -> Option<&RpcConfig> {
        self.configs.get(&chain)
    }

    /// Override the RPC URL for a chain with a custom endpoint.
    ///
    /// Returns `Err` if the URL fails validation.
    pub fn set_custom_rpc(&mut self, chain: Chain, url: String) -> anyhow::Result<()> {
        if !validate_url(&url) {
            anyhow::bail!("invalid RPC URL: {url}");
        }

        let entry = self.configs.entry(chain).or_insert_with(|| RpcConfig {
            chain,
            url: String::new(),
            is_custom: false,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        });
        entry.url = url;
        entry.is_custom = true;
        Ok(())
    }

    /// Reset a chain's RPC URL back to the built-in default.
    pub fn reset_to_default(&mut self, chain: Chain) {
        let defaults = get_chain_configs();
        if let Some(default_config) = defaults.get(&chain) {
            let entry = self.configs.entry(chain).or_insert_with(|| RpcConfig {
                chain,
                url: String::new(),
                is_custom: false,
                timeout_secs: DEFAULT_TIMEOUT_SECS,
            });
            entry.url = default_config.rpc_url.clone();
            entry.is_custom = false;
        }
    }
}

impl Default for RpcConfigStore {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Validate that a URL is well-formed and uses HTTP or HTTPS.
pub fn validate_url(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let scheme = parsed.scheme();
            (scheme == "http" || scheme == "https") && parsed.host().is_some()
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_all_chains() {
        let store = RpcConfigStore::with_defaults();
        assert!(store.get_rpc(Chain::Ethereum).is_some());
        assert!(store.get_rpc(Chain::Base).is_some());
        assert!(store.get_rpc(Chain::Solana).is_some());
    }

    #[test]
    fn defaults_are_not_custom() {
        let store = RpcConfigStore::with_defaults();
        for chain in [Chain::Ethereum, Chain::Base, Chain::Solana] {
            let rpc = store.get_rpc(chain).unwrap();
            assert!(!rpc.is_custom);
        }
    }

    #[test]
    fn set_custom_rpc_marks_as_custom() {
        let mut store = RpcConfigStore::with_defaults();
        store
            .set_custom_rpc(Chain::Ethereum, "https://my-node.example.com".into())
            .unwrap();

        let rpc = store.get_rpc(Chain::Ethereum).unwrap();
        assert!(rpc.is_custom);
        assert_eq!(rpc.url, "https://my-node.example.com");
    }

    #[test]
    fn set_custom_rpc_rejects_invalid_url() {
        let mut store = RpcConfigStore::with_defaults();
        let result = store.set_custom_rpc(Chain::Base, "not-a-url".into());
        assert!(result.is_err());
    }

    #[test]
    fn set_custom_rpc_rejects_ftp_url() {
        let mut store = RpcConfigStore::with_defaults();
        let result = store.set_custom_rpc(Chain::Base, "ftp://files.example.com".into());
        assert!(result.is_err());
    }

    #[test]
    fn reset_to_default_restores_original_url() {
        let mut store = RpcConfigStore::with_defaults();
        let original = store.get_rpc(Chain::Ethereum).unwrap().url.clone();

        store
            .set_custom_rpc(Chain::Ethereum, "https://custom.example.com".into())
            .unwrap();
        assert_ne!(store.get_rpc(Chain::Ethereum).unwrap().url, original);

        store.reset_to_default(Chain::Ethereum);
        let after_reset = store.get_rpc(Chain::Ethereum).unwrap();
        assert_eq!(after_reset.url, original);
        assert!(!after_reset.is_custom);
    }

    #[test]
    fn validate_url_accepts_https() {
        assert!(validate_url("https://rpc.example.com"));
    }

    #[test]
    fn validate_url_accepts_http() {
        assert!(validate_url("http://localhost:8545"));
    }

    #[test]
    fn validate_url_rejects_garbage() {
        assert!(!validate_url(""));
        assert!(!validate_url("not a url"));
        assert!(!validate_url("ftp://server.com"));
        assert!(!validate_url("file:///etc/passwd"));
    }
}
