//! Fetch and cache the live Anthropic model catalog.
//!
//! Fetches from `https://api.anthropic.com/v1/models` and caches for 5 minutes.
//! Anthropic doesn't return pricing in the API, so we fall back to the static
//! registry for known models and use safe defaults for unknown ones.

use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::model_registry::MODEL_REGISTRY;
use crate::types::{ModelInfo, ModelTier, ProviderType};

const CACHE_TTL: Duration = Duration::from_secs(300);

struct CatalogCache {
    models: Vec<ModelInfo>,
    fetched_at: Option<Instant>,
}

static CACHE: Mutex<CatalogCache> = Mutex::new(CatalogCache {
    models: Vec::new(),
    fetched_at: None,
});

/// Clear the cached catalog (e.g. when the API key changes).
pub fn invalidate_cache() {
    let mut cache = CACHE.lock();
    cache.models.clear();
    cache.fetched_at = None;
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModel {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
}

/// Fetch the Anthropic model catalog, returning cached results if fresh.
pub async fn fetch_anthropic_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    {
        let cache = CACHE.lock();
        if let Some(fetched_at) = cache.fetched_at
            && fetched_at.elapsed() < CACHE_TTL && !cache.models.is_empty() {
                return Ok(cache.models.clone());
            }
    }

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Anthropic API returned {}", resp.status()));
    }

    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let models: Vec<ModelInfo> = body
        .data
        .into_iter()
        .filter(|m| m.id.contains("claude"))
        .map(|m| {
            // Try to find in static registry for pricing/context
            if let Some(known) = MODEL_REGISTRY.iter().find(|r| r.id == m.id) {
                return known.clone();
            }
            let name = m.display_name.unwrap_or_else(|| m.id.clone());
            // Unknown model — safe defaults
            ModelInfo {
                id: m.id,
                name,
                provider: "anthropic".into(),
                provider_type: ProviderType::Anthropic,
                tier: ModelTier::Mid,
                context_window: 200_000,
                input_price_per_mtok: 3.0,
                output_price_per_mtok: 15.0,
                capabilities: Default::default(),
            }
        })
        .collect();

    {
        let mut cache = CACHE.lock();
        cache.models = models.clone();
        cache.fetched_at = Some(Instant::now());
    }

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "anthropic".into(),
                provider_type: ProviderType::Anthropic,
                tier: ModelTier::Mid,
                context_window: 200_000,
                input_price_per_mtok: 3.0,
                output_price_per_mtok: 15.0,
                capabilities: Default::default(),
            }];
            cache.fetched_at = Some(Instant::now());
        }

        invalidate_cache();

        let cache = CACHE.lock();
        assert!(cache.models.is_empty());
        assert!(cache.fetched_at.is_none());
    }
}
