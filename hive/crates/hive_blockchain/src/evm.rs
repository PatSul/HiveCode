use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::wallet_store::Chain;

/// An EVM-compatible wallet address and its chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmWallet {
    pub address: String,
    pub chain: Chain,
}

/// Parameters for deploying a new ERC-20 token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenDeployParams {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
    pub chain: Chain,
}

/// Result of a successful token deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub tx_hash: String,
    pub contract_address: String,
    pub chain: Chain,
    pub gas_used: u64,
}

/// Estimate the cost (in native currency) to deploy an ERC-20 contract.
///
/// This is a stub. A real implementation would query the chain's gas oracle and
/// simulate the deployment transaction via an RPC provider such as `alloy`.
pub async fn estimate_deploy_cost(chain: Chain) -> Result<f64> {
    if !chain.is_evm() {
        anyhow::bail!("{chain} is not an EVM chain");
    }
    // Placeholder costs based on typical ERC-20 deployment gas (~1.2M gas).
    let cost = match chain {
        Chain::Ethereum => 0.015, // ~1.2M gas * 12 gwei
        Chain::Base => 0.0001,    // L2 gas is much cheaper
        Chain::Solana => unreachable!(),
    };
    Ok(cost)
}

/// Deploy an ERC-20 token to the specified EVM chain.
///
/// This is a stub. A real implementation requires the `alloy` crate (or
/// equivalent) to construct and broadcast the deployment transaction.
pub async fn deploy_token(params: TokenDeployParams, _private_key: &[u8]) -> Result<DeployResult> {
    if !params.chain.is_evm() {
        anyhow::bail!("{} is not an EVM chain", params.chain);
    }
    anyhow::bail!(
        "EVM deployment is not yet implemented -- add the `alloy` crate to enable on-chain transactions for {}",
        params.chain
    )
}

/// Query the native currency balance for an address on the given chain.
///
/// This is a stub. A real implementation would issue an `eth_getBalance` JSON-RPC
/// call via the configured RPC endpoint.
pub async fn get_balance(address: &str, chain: Chain) -> Result<f64> {
    if !chain.is_evm() {
        anyhow::bail!("{chain} is not an EVM chain");
    }
    let _ = address;
    anyhow::bail!(
        "EVM balance queries are not yet implemented -- add the `alloy` crate to enable RPC calls for {}",
        chain
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_deploy_params_serializes() {
        let params = TokenDeployParams {
            name: "TestToken".into(),
            symbol: "TT".into(),
            decimals: 18,
            total_supply: "1000000000000000000000000".into(),
            chain: Chain::Ethereum,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: TokenDeployParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "TestToken");
        assert_eq!(parsed.decimals, 18);
    }

    #[test]
    fn deploy_result_serializes() {
        let result = DeployResult {
            tx_hash: "0xabc123".into(),
            contract_address: "0xdef456".into(),
            chain: Chain::Base,
            gas_used: 1_200_000,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("0xabc123"));
        let parsed: DeployResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.gas_used, 1_200_000);
    }

    #[test]
    fn evm_wallet_serializes() {
        let wallet = EvmWallet {
            address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
            chain: Chain::Ethereum,
        };
        let json = serde_json::to_string(&wallet).unwrap();
        let parsed: EvmWallet = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.address, wallet.address);
    }

    #[tokio::test]
    async fn estimate_deploy_cost_returns_placeholder() {
        let cost = estimate_deploy_cost(Chain::Ethereum).await.unwrap();
        assert!(cost > 0.0);
    }

    #[tokio::test]
    async fn estimate_deploy_cost_rejects_solana() {
        let result = estimate_deploy_cost(Chain::Solana).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn deploy_token_returns_not_implemented() {
        let params = TokenDeployParams {
            name: "Test".into(),
            symbol: "T".into(),
            decimals: 18,
            total_supply: "1000".into(),
            chain: Chain::Ethereum,
        };
        let result = deploy_token(params, &[0u8; 32]).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn get_balance_returns_not_implemented() {
        let result = get_balance("0x0000000000000000000000000000000000000000", Chain::Base).await;
        assert!(result.is_err());
    }
}
