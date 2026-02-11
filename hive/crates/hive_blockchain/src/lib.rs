// Phase 7: Blockchain wallet + token launch

pub mod erc20_bytecode;
pub mod evm;
pub mod rpc_config;
pub mod solana;
pub mod wallet_store;

// Re-export primary types for convenient access.
pub use erc20_bytecode::{ChainConfig, Erc20Contract, get_chain_configs, get_erc20_contract};
pub use evm::{DeployResult, EvmWallet, TokenDeployParams};
pub use rpc_config::{RpcConfig, RpcConfigStore, validate_url};
pub use solana::{SolanaWallet, SplDeployResult, SplTokenParams};
pub use wallet_store::{Chain, WalletEntry, WalletStore, decrypt_key, encrypt_key};
