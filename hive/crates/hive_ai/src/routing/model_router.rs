//! Model Router
//!
//! Orchestrates the complexity classifier and auto-fallback manager to produce
//! a final routing decision for each request. Supports both explicit model
//! selection and automatic tier-based routing.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::types::{ChatMessage, ModelTier};

use super::auto_fallback::{AutoFallbackManager, FallbackConfig, FallbackReason, ProviderType};
use super::complexity_classifier::{ClassificationContext, ComplexityClassifier, ComplexityResult};

// ---------------------------------------------------------------------------
// Tier Adjuster trait (for learning system integration)
// ---------------------------------------------------------------------------

/// Trait for external tier adjustment based on learned routing data.
///
/// Implementations (e.g. `RoutingLearner` in `hive_learn`) can override the
/// classified tier for specific task types when outcome data shows the
/// classifier consistently over- or under-estimates complexity.
pub trait TierAdjuster: Send + Sync {
    /// Given a `task_type` and the tier the classifier chose, return an adjusted
    /// tier string if learning data suggests a change, or `None` to keep the
    /// original classification.
    fn adjust_tier(&self, task_type: &str, classified_tier: &str) -> Option<String>;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The final routing decision produced by the [`ModelRouter`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The provider to send the request to.
    pub provider: ProviderType,
    /// The specific model identifier (e.g. `"claude-opus-4-20250514"`).
    pub model_id: String,
    /// The tier of the selected model.
    pub tier: ModelTier,
    /// Human-readable explanation of why this route was chosen.
    pub reasoning: String,
}

/// Known model-to-provider mappings for explicit model resolution.
struct ModelMapping {
    prefix: &'static str,
    provider: ProviderType,
}

/// Well-known model prefixes and their providers.
static MODEL_MAPPINGS: &[ModelMapping] = &[
    ModelMapping {
        prefix: "claude-",
        provider: ProviderType::Anthropic,
    },
    ModelMapping {
        prefix: "gpt-",
        provider: ProviderType::OpenAI,
    },
    ModelMapping {
        prefix: "o1",
        provider: ProviderType::OpenAI,
    },
    ModelMapping {
        prefix: "o3",
        provider: ProviderType::OpenAI,
    },
    ModelMapping {
        prefix: "gemini-",
        provider: ProviderType::Google,
    },
    // OpenRouter uses org/model format
    ModelMapping {
        prefix: "anthropic/",
        provider: ProviderType::OpenRouter,
    },
    ModelMapping {
        prefix: "openai/",
        provider: ProviderType::OpenRouter,
    },
    ModelMapping {
        prefix: "google/",
        provider: ProviderType::OpenRouter,
    },
    ModelMapping {
        prefix: "meta-llama/",
        provider: ProviderType::OpenRouter,
    },
    ModelMapping {
        prefix: "deepseek/",
        provider: ProviderType::OpenRouter,
    },
    ModelMapping {
        prefix: "qwen/",
        provider: ProviderType::OpenRouter,
    },
    ModelMapping {
        prefix: "mistralai/",
        provider: ProviderType::OpenRouter,
    },
    // Groq models
    ModelMapping {
        prefix: "groq/",
        provider: ProviderType::Groq,
    },
    // HuggingFace models
    ModelMapping {
        prefix: "hf/",
        provider: ProviderType::HuggingFace,
    },
];

// ---------------------------------------------------------------------------
// ModelRouter
// ---------------------------------------------------------------------------

/// The main router that combines complexity classification with provider
/// fallback management to produce routing decisions.
pub struct ModelRouter {
    classifier: ComplexityClassifier,
    fallback_manager: AutoFallbackManager,
    tier_adjuster: Option<Arc<dyn TierAdjuster>>,
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRouter {
    /// Create a new router with default configuration.
    pub fn new() -> Self {
        Self {
            classifier: ComplexityClassifier::new(),
            fallback_manager: AutoFallbackManager::with_defaults(),
            tier_adjuster: None,
        }
    }

    /// Create a new router with a custom fallback configuration.
    pub fn with_config(fallback_config: FallbackConfig) -> Self {
        Self {
            classifier: ComplexityClassifier::new(),
            fallback_manager: AutoFallbackManager::new(fallback_config),
            tier_adjuster: None,
        }
    }

    /// Set a tier adjuster for learning-based routing adjustments.
    ///
    /// When set, the auto-routing path will consult the adjuster after
    /// classification and may override the tier if learning data suggests
    /// the classifier's output should be corrected.
    pub fn set_tier_adjuster(&mut self, adjuster: Arc<dyn TierAdjuster>) {
        self.tier_adjuster = Some(adjuster);
    }

    /// Route a request to the best available provider and model.
    ///
    /// If `explicit_model` is provided, the router resolves it directly to a
    /// provider (falling back through the chain if that provider is down).
    /// Otherwise it classifies the request complexity and picks the best
    /// available provider for the determined tier.
    pub fn route(
        &self,
        messages: &[ChatMessage],
        explicit_model: Option<&str>,
        context: Option<&ClassificationContext>,
    ) -> RoutingDecision {
        // --- Explicit model path ---
        if let Some(model_id) = explicit_model {
            return self.route_explicit(model_id, messages, context);
        }

        // --- Auto-routing path ---
        self.route_auto(messages, context)
    }

    /// Record the outcome of a request so the fallback manager can update
    /// provider health tracking.
    pub fn record_result(
        &self,
        provider: ProviderType,
        success: bool,
        reason: Option<FallbackReason>,
    ) {
        if success {
            self.fallback_manager.record_success(provider);
        } else {
            self.fallback_manager
                .record_failure(provider, reason.unwrap_or(FallbackReason::ServerError));
        }
    }

    /// Access the underlying fallback manager (e.g. to set provider availability).
    pub fn fallback_manager(&self) -> &AutoFallbackManager {
        &self.fallback_manager
    }

    /// Access the underlying complexity classifier.
    pub fn classifier(&self) -> &ComplexityClassifier {
        &self.classifier
    }

    /// Classify request complexity without routing.
    pub fn classify(
        &self,
        messages: &[ChatMessage],
        context: Option<&ClassificationContext>,
    ) -> ComplexityResult {
        self.classifier.classify(messages, context)
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Route when the user has explicitly chosen a model.
    fn route_explicit(
        &self,
        model_id: &str,
        _messages: &[ChatMessage],
        _context: Option<&ClassificationContext>,
    ) -> RoutingDecision {
        let provider = resolve_provider(model_id);
        let tier = infer_tier(model_id);

        // Check if the resolved provider is available
        if self.fallback_manager.is_available(provider) {
            debug!(
                model = model_id,
                %provider,
                "Explicit model routed directly"
            );
            return RoutingDecision {
                provider,
                model_id: model_id.to_string(),
                tier,
                reasoning: format!(
                    "Explicit model selection: {} via {}",
                    model_id, provider
                ),
            };
        }

        // Provider is down — try to find the same model via a different provider
        // (e.g. claude-opus via OpenRouter instead of Anthropic directly).
        info!(
            model = model_id,
            %provider,
            "Explicit model's provider unavailable, trying fallback"
        );

        let chain = self.fallback_manager.get_fallback_chain(tier);
        for fallback_provider in chain {
            if fallback_provider != provider && self.fallback_manager.is_available(fallback_provider)
            {
                // For OpenRouter we can proxy most models
                if fallback_provider == ProviderType::OpenRouter {
                    let or_model = openrouter_model_id(model_id, provider);
                    return RoutingDecision {
                        provider: ProviderType::OpenRouter,
                        model_id: or_model.clone(),
                        tier,
                        reasoning: format!(
                            "Explicit model {} proxied via OpenRouter ({}) because {} is unavailable",
                            model_id, or_model, provider
                        ),
                    };
                }
            }
        }

        // Last resort: just try the original anyway and let the caller handle errors
        RoutingDecision {
            provider,
            model_id: model_id.to_string(),
            tier,
            reasoning: format!(
                "Explicit model {} — provider {} may be unavailable",
                model_id, provider
            ),
        }
    }

    /// Automatic routing based on complexity classification.
    fn route_auto(
        &self,
        messages: &[ChatMessage],
        context: Option<&ClassificationContext>,
    ) -> RoutingDecision {
        let result = self.classifier.classify(messages, context);

        // Check if the learning system wants to override the tier
        let tier = if let Some(ref adjuster) = self.tier_adjuster {
            let tier_str = format!("{:?}", result.tier).to_lowercase();
            let task_type = result.factors.task_type.to_string();
            if let Some(adjusted) = adjuster.adjust_tier(&task_type, &tier_str) {
                let new_tier = match adjusted.as_str() {
                    "free" => ModelTier::Free,
                    "budget" => ModelTier::Budget,
                    "standard" | "mid" => ModelTier::Mid,
                    "premium" | "enterprise" => ModelTier::Premium,
                    _ => result.tier,
                };
                if new_tier != result.tier {
                    info!(
                        original_tier = ?result.tier,
                        adjusted_tier = ?new_tier,
                        task = %task_type,
                        "Tier adjusted by learning system"
                    );
                }
                new_tier
            } else {
                result.tier
            }
        } else {
            result.tier
        };

        info!(
            tier = ?tier,
            score = result.score,
            task = %result.factors.task_type,
            "Auto-routing: classified request"
        );

        let chain = self.fallback_manager.get_fallback_chain(tier);

        // Find the best available entry from the fallback chain that matches
        // the desired tier (or close to it).
        for provider in &chain {
            // Find the first chain entry for this provider at or near the tier
            if let Some(entry) = self
                .fallback_manager
                .get_next_fallback(
                    // We treat this as "give me a fallback from any provider"
                    // by using a dummy original.
                    ProviderType::GenericLocal,
                    "__auto__",
                    FallbackReason::ProviderDown,
                    &[], // no tried list — we handle ordering via the chain
                )
            {
                // Only use if the provider matches what the chain told us
                if entry.provider == *provider {
                    return RoutingDecision {
                        provider: entry.provider,
                        model_id: entry.model.clone(),
                        tier,
                        reasoning: format!(
                            "Auto-routed: {} | Model: {} ({})",
                            result.reasoning, entry.model, entry.provider
                        ),
                    };
                }
            }
        }

        // If we got a recommended model from the classifier, use that
        if let Some(ref model) = result.recommended_model {
            let provider = resolve_provider(model);
            return RoutingDecision {
                provider,
                model_id: model.clone(),
                tier,
                reasoning: format!(
                    "Auto-routed (classifier recommendation): {} | {}",
                    result.reasoning, model
                ),
            };
        }

        // Ultimate fallback: use the default model for the tier
        let (model, provider) = default_for_tier(tier);
        RoutingDecision {
            provider,
            model_id: model.to_string(),
            tier,
            reasoning: format!(
                "Auto-routed (default): {} | {}",
                result.reasoning, model
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Resolution helpers
// ---------------------------------------------------------------------------

/// Resolve a model ID string to its most likely provider.
fn resolve_provider(model_id: &str) -> ProviderType {
    for mapping in MODEL_MAPPINGS {
        if model_id.starts_with(mapping.prefix) {
            return mapping.provider;
        }
    }
    // If it contains a `/` it's likely an OpenRouter org/model format
    if model_id.contains('/') {
        return ProviderType::OpenRouter;
    }
    // Unknown model — assume local
    ProviderType::Ollama
}

/// Infer a tier from a model ID string.
fn infer_tier(model_id: &str) -> ModelTier {
    let lower = model_id.to_lowercase();

    // Premium models
    if lower.contains("opus")
        || lower.contains("gpt-4o")
        || lower.contains("o1")
        || lower.contains("o3")
        || lower.contains("gemini-1.5-pro")
        || lower.contains("gemini-2")
    {
        // gpt-4o-mini is Mid, not Premium
        if lower.contains("mini") {
            return ModelTier::Mid;
        }
        return ModelTier::Premium;
    }

    // Mid models
    if lower.contains("sonnet")
        || lower.contains("mini")
        || lower.contains("flash")
        || lower.contains("gemini-1.5-flash")
    {
        return ModelTier::Mid;
    }

    // Budget models
    if lower.contains("haiku")
        || lower.contains("deepseek")
        || lower.contains("llama")
        || lower.contains("qwen")
        || lower.contains("mistral")
    {
        return ModelTier::Budget;
    }

    // Default
    ModelTier::Mid
}

/// Build an OpenRouter-style model ID from a direct model ID and its provider.
fn openrouter_model_id(model_id: &str, provider: ProviderType) -> String {
    let prefix = match provider {
        ProviderType::Anthropic => "anthropic",
        ProviderType::OpenAI => "openai",
        ProviderType::Google => "google",
        ProviderType::Groq => "groq",
        ProviderType::HuggingFace => "huggingface",
        _ => return model_id.to_string(),
    };
    format!("{}/{}", prefix, model_id)
}

/// Return the default model and provider for a given tier.
fn default_for_tier(tier: ModelTier) -> (&'static str, ProviderType) {
    match tier {
        ModelTier::Premium => ("claude-opus-4-20250514", ProviderType::Anthropic),
        ModelTier::Mid => ("claude-sonnet-4-20250514", ProviderType::Anthropic),
        ModelTier::Budget => ("deepseek/deepseek-chat", ProviderType::OpenRouter),
        ModelTier::Free => ("llama3.2", ProviderType::Ollama),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessageRole;
    use chrono::Utc;

    fn user_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: Utc::now(),
        }
    }

    fn setup_router() -> ModelRouter {
        let router = ModelRouter::new();
        router
            .fallback_manager()
            .set_available(ProviderType::Anthropic, true);
        router
            .fallback_manager()
            .set_available(ProviderType::OpenAI, true);
        router
            .fallback_manager()
            .set_available(ProviderType::OpenRouter, true);
        router
    }

    #[test]
    fn explicit_model_routes_directly() {
        let router = setup_router();
        let decision = router.route(
            &[user_msg("hello")],
            Some("claude-opus-4-20250514"),
            None,
        );
        assert_eq!(decision.provider, ProviderType::Anthropic);
        assert_eq!(decision.model_id, "claude-opus-4-20250514");
    }

    #[test]
    fn explicit_openrouter_model() {
        let router = setup_router();
        let decision = router.route(
            &[user_msg("hello")],
            Some("deepseek/deepseek-chat"),
            None,
        );
        assert_eq!(decision.provider, ProviderType::OpenRouter);
    }

    #[test]
    fn auto_route_simple_question_budget() {
        let router = setup_router();
        let decision = router.route(&[user_msg("What is Rust?")], None, None);
        assert_eq!(decision.tier, ModelTier::Budget);
    }

    #[test]
    fn auto_route_architecture_premium() {
        let router = setup_router();
        let decision = router.route(
            &[user_msg("Design the system architecture for our new platform")],
            None,
            None,
        );
        assert_eq!(decision.tier, ModelTier::Premium);
    }

    #[test]
    fn resolve_provider_claude() {
        assert_eq!(
            resolve_provider("claude-sonnet-4-20250514"),
            ProviderType::Anthropic
        );
    }

    #[test]
    fn resolve_provider_gpt() {
        assert_eq!(resolve_provider("gpt-4o"), ProviderType::OpenAI);
    }

    #[test]
    fn resolve_provider_openrouter_format() {
        assert_eq!(
            resolve_provider("meta-llama/llama-3.3-70b-instruct"),
            ProviderType::OpenRouter
        );
    }

    #[test]
    fn infer_tier_premium() {
        assert_eq!(infer_tier("claude-opus-4-20250514"), ModelTier::Premium);
        assert_eq!(infer_tier("gpt-4o"), ModelTier::Premium);
    }

    #[test]
    fn infer_tier_mid() {
        assert_eq!(infer_tier("claude-sonnet-4-20250514"), ModelTier::Mid);
        assert_eq!(infer_tier("gpt-4o-mini"), ModelTier::Mid);
    }

    #[test]
    fn infer_tier_budget() {
        assert_eq!(
            infer_tier("deepseek/deepseek-chat"),
            ModelTier::Budget
        );
        assert_eq!(infer_tier("claude-haiku-4-5-20251001"), ModelTier::Budget);
    }

    #[test]
    fn record_result_success() {
        let router = setup_router();
        router.record_result(ProviderType::Anthropic, true, None);
        assert!(router.fallback_manager().is_available(ProviderType::Anthropic));
    }

    #[test]
    fn record_result_failure() {
        let router = setup_router();
        for _ in 0..3 {
            router.record_result(
                ProviderType::OpenAI,
                false,
                Some(FallbackReason::ServerError),
            );
        }
        assert!(!router.fallback_manager().is_available(ProviderType::OpenAI));
    }

    #[test]
    fn tier_adjuster_overrides_classification() {
        struct TestAdjuster;
        impl TierAdjuster for TestAdjuster {
            fn adjust_tier(&self, task_type: &str, _classified_tier: &str) -> Option<String> {
                if task_type == "simple question" {
                    Some("premium".to_string())
                } else {
                    None
                }
            }
        }

        let mut router = setup_router();
        router.set_tier_adjuster(Arc::new(TestAdjuster));

        // "What is Rust?" normally classifies as Budget, but our adjuster upgrades it
        let decision = router.route(&[user_msg("What is Rust?")], None, None);
        assert_eq!(decision.tier, ModelTier::Premium);
    }

    #[test]
    fn tier_adjuster_none_keeps_original() {
        struct NoOpAdjuster;
        impl TierAdjuster for NoOpAdjuster {
            fn adjust_tier(&self, _: &str, _: &str) -> Option<String> {
                None
            }
        }

        let mut router = setup_router();
        router.set_tier_adjuster(Arc::new(NoOpAdjuster));

        let decision = router.route(&[user_msg("What is Rust?")], None, None);
        assert_eq!(decision.tier, ModelTier::Budget);
    }

    #[test]
    fn fallback_when_provider_down() {
        let router = setup_router();
        // Take Anthropic down
        router.fallback_manager().set_available(ProviderType::Anthropic, false);

        let decision = router.route(
            &[user_msg("hello")],
            Some("claude-opus-4-20250514"),
            None,
        );
        // Should have fallen back — either via OpenRouter proxy or another provider
        assert_ne!(decision.provider, ProviderType::Anthropic);
    }
}
