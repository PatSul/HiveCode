//! Fetch and cache the live OpenRouter model catalog.
//!
//! The catalog is fetched from `https://openrouter.ai/api/v1/models` and cached
//! for 5 minutes to avoid excessive API calls.

use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::types::{ModelInfo, ModelTier, ProviderType};

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

const CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

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

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    context_length: Option<u32>,
    #[serde(default)]
    pricing: Option<Pricing>,
}

#[derive(Debug, Deserialize)]
struct Pricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch the OpenRouter model catalog, returning cached results if fresh.
pub async fn fetch_openrouter_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    // Check cache first
    {
        let cache = CACHE.lock();
        if let Some(fetched_at) = cache.fetched_at {
            if fetched_at.elapsed() < CACHE_TTL && !cache.models.is_empty() {
                return Ok(cache.models.clone());
            }
        }
    }

    // Fetch from API
    let client = reqwest::Client::new();
    let resp = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("OpenRouter API returned {}", resp.status()));
    }

    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let models: Vec<ModelInfo> = body
        .data
        .into_iter()
        .filter_map(|m| {
            let pricing = m.pricing.as_ref()?;
            // Parse per-token prices (strings like "0.000003") → per-million-token
            let input_per_tok: f64 = pricing.prompt.as_deref()?.parse().ok()?;
            let output_per_tok: f64 = pricing.completion.as_deref()?.parse().ok()?;
            let input_price = input_per_tok * 1_000_000.0;
            let output_price = output_per_tok * 1_000_000.0;

            let name = m.name.unwrap_or_else(|| m.id.clone());
            let tier = classify_tier(input_price);

            Some(ModelInfo {
                id: m.id,
                name,
                provider: "openrouter".into(),
                provider_type: ProviderType::OpenRouter,
                tier,
                context_window: m.context_length.unwrap_or(4096),
                input_price_per_mtok: input_price,
                output_price_per_mtok: output_price,
                capabilities: Default::default(),
            })
        })
        .collect();

    // Update cache
    {
        let mut cache = CACHE.lock();
        cache.models = models.clone();
        cache.fetched_at = Some(Instant::now());
    }

    Ok(models)
}

/// Classify a model tier based on input price per million tokens.
fn classify_tier(input_price_per_mtok: f64) -> ModelTier {
    if input_price_per_mtok <= 0.0 {
        ModelTier::Free
    } else if input_price_per_mtok < 1.0 {
        ModelTier::Budget
    } else if input_price_per_mtok < 10.0 {
        ModelTier::Mid
    } else {
        ModelTier::Premium
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_tier_free() {
        assert_eq!(classify_tier(0.0), ModelTier::Free);
    }

    #[test]
    fn classify_tier_budget() {
        assert_eq!(classify_tier(0.5), ModelTier::Budget);
        assert_eq!(classify_tier(0.99), ModelTier::Budget);
    }

    #[test]
    fn classify_tier_mid() {
        assert_eq!(classify_tier(1.0), ModelTier::Mid);
        assert_eq!(classify_tier(5.0), ModelTier::Mid);
    }

    #[test]
    fn classify_tier_premium() {
        assert_eq!(classify_tier(10.0), ModelTier::Premium);
        assert_eq!(classify_tier(75.0), ModelTier::Premium);
    }

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "openrouter".into(),
                provider_type: ProviderType::OpenRouter,
                tier: ModelTier::Budget,
                context_window: 4096,
                input_price_per_mtok: 0.5,
                output_price_per_mtok: 1.0,
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
