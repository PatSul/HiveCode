use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A Solana wallet identified by its base58-encoded public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaWallet {
    pub address: String,
}

/// Parameters for creating a new SPL token on Solana.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplTokenParams {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub supply: u64,
    pub metadata_uri: Option<String>,
}

/// Result of a successful SPL token deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplDeployResult {
    pub mint_address: String,
    pub tx_signature: String,
}

/// Estimate the cost (in SOL) to create an SPL token with metadata.
///
/// This is a stub. A real implementation would query Solana rent exemption
/// costs and current fee rates via `@solana/web3.js` or a Rust Solana SDK.
pub async fn estimate_deploy_cost() -> Result<f64> {
    // Approximate: rent for mint account + metadata account + transaction fees
    Ok(0.05)
}

/// Create a new SPL token on Solana.
///
/// This is a stub. A real implementation requires the `solana-sdk` and
/// `spl-token` crates to build and sign the create-mint transaction.
pub async fn create_spl_token(
    _params: SplTokenParams,
    _private_key: &[u8],
) -> Result<SplDeployResult> {
    anyhow::bail!(
        "Solana SPL token creation is not yet implemented -- add `solana-sdk` and `spl-token` crates to enable"
    )
}

/// Query the SOL balance for a wallet address.
///
/// This is a stub. A real implementation would call `getBalance` on a Solana
/// RPC endpoint.
pub async fn get_balance(address: &str) -> Result<f64> {
    let _ = address;
    anyhow::bail!(
        "Solana balance queries are not yet implemented -- add `solana-sdk` crate to enable RPC calls"
    )
}

/// Query SPL token balances for a wallet address.
///
/// This is a stub. A real implementation would call `getTokenAccountsByOwner`
/// on a Solana RPC endpoint.
pub async fn get_token_balances(address: &str) -> Result<Vec<(String, f64)>> {
    let _ = address;
    anyhow::bail!(
        "Solana token balance queries are not yet implemented -- add `solana-sdk` crate to enable RPC calls"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solana_wallet_serializes() {
        let wallet = SolanaWallet {
            address: "7EcDhSYGxXyscszYEp35KHN8vvw3svAuLKTzXwCFLtV".into(),
        };
        let json = serde_json::to_string(&wallet).unwrap();
        let parsed: SolanaWallet = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.address, wallet.address);
    }

    #[test]
    fn spl_token_params_serializes() {
        let params = SplTokenParams {
            name: "HiveToken".into(),
            symbol: "HIVE".into(),
            decimals: 9,
            supply: 1_000_000_000,
            metadata_uri: Some("https://example.com/meta.json".into()),
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: SplTokenParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.symbol, "HIVE");
        assert_eq!(parsed.decimals, 9);
        assert!(parsed.metadata_uri.is_some());
    }

    #[test]
    fn spl_token_params_optional_metadata() {
        let params = SplTokenParams {
            name: "Bare".into(),
            symbol: "BARE".into(),
            decimals: 6,
            supply: 1000,
            metadata_uri: None,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: SplTokenParams = serde_json::from_str(&json).unwrap();
        assert!(parsed.metadata_uri.is_none());
    }

    #[test]
    fn spl_deploy_result_serializes() {
        let result = SplDeployResult {
            mint_address: "TokenMintAddress123".into(),
            tx_signature: "5KtP8...sig".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: SplDeployResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mint_address, "TokenMintAddress123");
    }

    #[tokio::test]
    async fn estimate_deploy_cost_returns_placeholder() {
        let cost = estimate_deploy_cost().await.unwrap();
        assert!(cost > 0.0);
    }

    #[tokio::test]
    async fn create_spl_token_returns_not_implemented() {
        let params = SplTokenParams {
            name: "Test".into(),
            symbol: "T".into(),
            decimals: 9,
            supply: 1000,
            metadata_uri: None,
        };
        let result = create_spl_token(params, &[0u8; 64]).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn get_balance_returns_not_implemented() {
        let result = get_balance("SomeAddress").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_token_balances_returns_not_implemented() {
        let result = get_token_balances("SomeAddress").await;
        assert!(result.is_err());
    }
}
