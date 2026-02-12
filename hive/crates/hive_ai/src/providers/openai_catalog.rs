//! Fetch and cache the live OpenAI model catalog.
//!
//! Fetches from `https://api.openai.com/v1/models` and caches for 5 minutes.
//! OpenAI doesn't return pricing or context window, so we fall back to the
//! static registry for known models and use safe defaults for unknown ones.

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
    data: Vec<OpenAIModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModel {
    id: String,
}

/// Fetch the OpenAI model catalog, returning cached results if fresh.
pub async fn fetch_openai_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    {
        let cache = CACHE.lock();
        if let Some(fetched_at) = cache.fetched_at {
            if fetched_at.elapsed() < CACHE_TTL && !cache.models.is_empty() {
                return Ok(cache.models.clone());
            }
        }
    }

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("OpenAI API returned {}", resp.status()));
    }

    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let models: Vec<ModelInfo> = body
        .data
        .into_iter()
        .filter(|m| {
            m.id.starts_with("gpt-")
                || m.id.starts_with("o1")
                || m.id.starts_with("o3")
                || m.id.starts_with("o4")
                || m.id.starts_with("chatgpt-")
        })
        .map(|m| {
            // Try to find in static registry for pricing/context
            if let Some(known) = MODEL_REGISTRY.iter().find(|r| r.id == m.id) {
                return known.clone();
            }
            // Unknown model — safe defaults
            ModelInfo {
                id: m.id.clone(),
                name: m.id,
                provider: "openai".into(),
                provider_type: ProviderType::OpenAI,
                tier: ModelTier::Mid,
                context_window: 128_000,
                input_price_per_mtok: 1.0,
                output_price_per_mtok: 4.0,
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
                provider: "openai".into(),
                provider_type: ProviderType::OpenAI,
                tier: ModelTier::Mid,
                context_window: 128_000,
                input_price_per_mtok: 1.0,
                output_price_per_mtok: 4.0,
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
