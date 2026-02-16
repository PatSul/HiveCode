//! Fetch and cache the live Hugging Face model catalog.
//!
//! Uses the Hugging Face Inference API endpoint to list available models.
//! HF does not expose pricing (most Inference API usage is free or
//! pay-per-use), so we assign free/budget tier defaults.

use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::types::{ModelInfo, ModelTier, ProviderType};

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

/// A model entry from the HF Hub API.
#[derive(Debug, Deserialize)]
struct HfModel {
    #[serde(rename = "modelId")]
    model_id: String,
    #[serde(default)]
    pipeline_tag: Option<String>,
}

// ---------------------------------------------------------------------------
// Tier / pricing helpers
// ---------------------------------------------------------------------------

fn classify_model(model_id: &str) -> (ModelTier, f64, f64) {
    let id_lower = model_id.to_ascii_lowercase();

    // Large models — estimate Budget tier
    if id_lower.contains("70b")
        || id_lower.contains("72b")
        || id_lower.contains("65b")
        || id_lower.contains("180b")
        || id_lower.contains("405b")
    {
        return (ModelTier::Budget, 0.0, 0.0);
    }

    // Everything else on HF — Free tier
    (ModelTier::Free, 0.0, 0.0)
}

fn display_name_from_id(model_id: &str) -> String {
    // Use the part after the org slash, or the whole id
    let short = model_id
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(model_id);
    short.to_string()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch text-generation models from the Hugging Face Hub API.
///
/// Uses `https://huggingface.co/api/models?pipeline_tag=text-generation&sort=likes&direction=-1&limit=100`
/// to get the most popular text-generation models.
pub async fn fetch_huggingface_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    // Check cache first
    {
        let cache = CACHE.lock();
        if let Some(fetched_at) = cache.fetched_at
            && fetched_at.elapsed() < CACHE_TTL && !cache.models.is_empty() {
                return Ok(cache.models.clone());
            }
    }

    let client = reqwest::Client::new();
    let resp = client
        .get("https://huggingface.co/api/models")
        .query(&[
            ("pipeline_tag", "text-generation"),
            ("sort", "likes"),
            ("direction", "-1"),
            ("limit", "200"),
        ])
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Hugging Face API returned {}", resp.status()));
    }

    let body: Vec<HfModel> = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let models: Vec<ModelInfo> = body
        .into_iter()
        .filter(|m| {
            // Only include text-generation models that look like chat/instruct
            m.pipeline_tag.as_deref() == Some("text-generation")
        })
        .map(|m| {
            let (tier, input_price, output_price) = classify_model(&m.model_id);
            let name = display_name_from_id(&m.model_id);

            ModelInfo {
                id: m.model_id,
                name,
                provider: "hugging_face".into(),
                provider_type: ProviderType::HuggingFace,
                tier,
                context_window: 4_096, // HF API doesn't expose this
                input_price_per_mtok: input_price,
                output_price_per_mtok: output_price,
                capabilities: Default::default(),
            }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_large_model() {
        let (tier, ..) = classify_model("meta-llama/Llama-3.3-70B-Instruct");
        assert_eq!(tier, ModelTier::Budget);
    }

    #[test]
    fn classify_small_model() {
        let (tier, ..) = classify_model("microsoft/Phi-3-mini-4k-instruct");
        assert_eq!(tier, ModelTier::Free);
    }

    #[test]
    fn display_name_with_org() {
        assert_eq!(
            display_name_from_id("meta-llama/Llama-3.3-70B-Instruct"),
            "Llama-3.3-70B-Instruct"
        );
    }

    #[test]
    fn display_name_without_org() {
        assert_eq!(display_name_from_id("my-model"), "my-model");
    }

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "hugging_face".into(),
                provider_type: ProviderType::HuggingFace,
                tier: ModelTier::Free,
                context_window: 4096,
                input_price_per_mtok: 0.0,
                output_price_per_mtok: 0.0,
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
