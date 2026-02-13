//! Fetch and cache the live Google Gemini model catalog.
//!
//! Fetches from `https://generativelanguage.googleapis.com/v1/models` and
//! caches for 5 minutes. Google returns `inputTokenLimit` which we use for
//! the context window; pricing comes from the static registry.

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
    models: Vec<GoogleModel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleModel {
    /// Format: "models/gemini-2.5-pro"
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    input_token_limit: Option<u32>,
}

/// Fetch the Google Gemini model catalog, returning cached results if fresh.
pub async fn fetch_google_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
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
        .get("https://generativelanguage.googleapis.com/v1/models")
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Google API returned {}", resp.status()));
    }

    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let models: Vec<ModelInfo> = body
        .models
        .into_iter()
        .filter(|m| m.name.contains("gemini"))
        .map(|m| {
            // Google returns "models/gemini-2.5-pro" — strip the prefix
            let id = m
                .name
                .strip_prefix("models/")
                .unwrap_or(&m.name)
                .to_string();
            let context = m.input_token_limit.unwrap_or(1_048_576);

            // Try to find in static registry for pricing
            if let Some(known) = MODEL_REGISTRY.iter().find(|r| r.id == id) {
                return known.clone();
            }

            let name = m.display_name.unwrap_or_else(|| id.clone());
            // Unknown model — safe defaults
            ModelInfo {
                id,
                name,
                provider: "google".into(),
                provider_type: ProviderType::Google,
                tier: ModelTier::Mid,
                context_window: context,
                input_price_per_mtok: 0.50,
                output_price_per_mtok: 2.0,
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
                provider: "google".into(),
                provider_type: ProviderType::Google,
                tier: ModelTier::Mid,
                context_window: 1_048_576,
                input_price_per_mtok: 0.5,
                output_price_per_mtok: 2.0,
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
