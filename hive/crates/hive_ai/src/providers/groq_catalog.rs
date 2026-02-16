//! Fetch and cache the live Groq model catalog.
//!
//! The catalog is fetched from `https://api.groq.com/openai/v1/models` and cached
//! for 5 minutes to avoid excessive API calls.  Groq exposes an OpenAI-compatible
//! models endpoint, but does not include pricing data, so we assign reasonable
//! defaults based on model name.

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
    data: Vec<GroqModel>,
}

#[derive(Debug, Deserialize)]
struct GroqModel {
    id: String,
    #[serde(default)]
    context_window: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    owned_by: Option<String>,
}

// ---------------------------------------------------------------------------
// Pricing / tier helpers
// ---------------------------------------------------------------------------

/// Assign a pricing tier and per-million-token prices based on the model id.
///
/// Groq does not expose pricing in its models API, so we use reasonable
/// defaults derived from public Groq pricing pages.
fn classify_model(model_id: &str) -> (ModelTier, f64, f64) {
    let id_lower = model_id.to_ascii_lowercase();

    if id_lower.contains("llama") {
        (ModelTier::Budget, 0.05, 0.08)
    } else if id_lower.contains("mixtral") {
        (ModelTier::Budget, 0.24, 0.24)
    } else if id_lower.contains("gemma") {
        (ModelTier::Budget, 0.10, 0.10)
    } else {
        (ModelTier::Mid, 0.50, 0.50)
    }
}

/// Derive a human-friendly display name from the raw model id.
///
/// Replaces hyphens with spaces and title-cases each word, but keeps
/// all-numeric tokens (like version numbers) untouched.
fn display_name_from_id(id: &str) -> String {
    id.split('-')
        .map(|part| {
            if part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                part.to_string()
            } else {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        let mut s = first.to_uppercase().to_string();
                        s.extend(chars);
                        s
                    }
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch the Groq model catalog, returning cached results if fresh.
pub async fn fetch_groq_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    // Check cache first
    {
        let cache = CACHE.lock();
        if let Some(fetched_at) = cache.fetched_at
            && fetched_at.elapsed() < CACHE_TTL && !cache.models.is_empty() {
                return Ok(cache.models.clone());
            }
    }

    // Fetch from API
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.groq.com/openai/v1/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Groq API returned {}", resp.status()));
    }

    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let models: Vec<ModelInfo> = body
        .data
        .into_iter()
        .map(|m| {
            let (tier, input_price, output_price) = classify_model(&m.id);
            let name = display_name_from_id(&m.id);

            ModelInfo {
                id: m.id,
                name,
                provider: "groq".into(),
                provider_type: ProviderType::Groq,
                tier,
                context_window: m.context_window.unwrap_or(4096),
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

    // -- classify_model ------------------------------------------------------

    #[test]
    fn classify_llama_model() {
        let (tier, input, output) = classify_model("llama-3.3-70b-versatile");
        assert_eq!(tier, ModelTier::Budget);
        assert!((input - 0.05).abs() < f64::EPSILON);
        assert!((output - 0.08).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_mixtral_model() {
        let (tier, input, output) = classify_model("mixtral-8x7b-32768");
        assert_eq!(tier, ModelTier::Budget);
        assert!((input - 0.24).abs() < f64::EPSILON);
        assert!((output - 0.24).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_gemma_model() {
        let (tier, input, output) = classify_model("gemma2-9b-it");
        assert_eq!(tier, ModelTier::Budget);
        assert!((input - 0.10).abs() < f64::EPSILON);
        assert!((output - 0.10).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_unknown_model() {
        let (tier, input, output) = classify_model("some-new-model");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 0.50).abs() < f64::EPSILON);
        assert!((output - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_is_case_insensitive() {
        let (tier, ..) = classify_model("LLaMA-3.1-8B");
        assert_eq!(tier, ModelTier::Budget);

        let (tier, ..) = classify_model("Mixtral-Large");
        assert_eq!(tier, ModelTier::Budget);

        let (tier, ..) = classify_model("GEMMA-7B");
        assert_eq!(tier, ModelTier::Budget);
    }

    // -- display_name_from_id ------------------------------------------------

    #[test]
    fn display_name_simple() {
        assert_eq!(
            display_name_from_id("llama-3.3-70b-versatile"),
            "Llama 3.3 70b Versatile"
        );
    }

    #[test]
    fn display_name_single_word() {
        assert_eq!(display_name_from_id("mixtral"), "Mixtral");
    }

    // -- invalidate_cache ----------------------------------------------------

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "groq".into(),
                provider_type: ProviderType::Groq,
                tier: ModelTier::Budget,
                context_window: 4096,
                input_price_per_mtok: 0.05,
                output_price_per_mtok: 0.08,
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
