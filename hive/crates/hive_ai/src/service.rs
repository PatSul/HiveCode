//! AI Service — central orchestration layer.
//!
//! Manages provider instances, routes requests via the [`ModelRouter`],
//! and exposes a simple `chat` / `stream_chat` API that the UI layer calls.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::cost::{CostBreakdown, CostTracker, calculate_cost};
use crate::discovery::LocalDiscovery;
use crate::providers::anthropic::AnthropicProvider;
use crate::providers::gemini::GeminiProvider;
use crate::providers::generic_local::GenericLocalProvider;
use crate::providers::groq::GroqProvider;
use crate::providers::huggingface::HuggingFaceProvider;
use crate::providers::litellm::LiteLLMProvider;
use crate::providers::lmstudio::LMStudioProvider;
use crate::providers::ollama::OllamaProvider;
use crate::providers::openai::OpenAIProvider;
use crate::providers::openrouter::OpenRouterProvider;
use crate::providers::{AiProvider, ProviderError};
use crate::routing::ModelRouter;
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, ProviderType, StreamChunk, ToolDefinition,
};

// ---------------------------------------------------------------------------
// Config bridge
// ---------------------------------------------------------------------------

/// Provider configuration passed from the config layer.
#[derive(Debug, Clone, Default)]
pub struct AiServiceConfig {
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub openrouter_api_key: Option<String>,
    pub google_api_key: Option<String>,
    pub groq_api_key: Option<String>,
    pub huggingface_api_key: Option<String>,
    pub litellm_url: Option<String>,
    pub litellm_api_key: Option<String>,
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub local_provider_url: Option<String>,
    pub privacy_mode: bool,
    pub default_model: String,
    pub auto_routing: bool,
}

// ---------------------------------------------------------------------------
// AiService
// ---------------------------------------------------------------------------

/// The main AI service that the application uses for all chat interactions.
pub struct AiService {
    providers: HashMap<ProviderType, Arc<dyn AiProvider>>,
    router: ModelRouter,
    cost_tracker: CostTracker,
    config: AiServiceConfig,
    discovery: Option<Arc<LocalDiscovery>>,
}

impl AiService {
    /// Create a new service from the given configuration.
    pub fn new(config: AiServiceConfig) -> Self {
        let mut providers: HashMap<ProviderType, Arc<dyn AiProvider>> = HashMap::new();

        // Register cloud providers (skipped in privacy mode)
        if !config.privacy_mode {
            if let Some(ref key) = config.anthropic_api_key
                && !key.is_empty()
            {
                providers.insert(
                    ProviderType::Anthropic,
                    Arc::new(AnthropicProvider::new(key.clone())),
                );
                info!("Anthropic provider registered");
            }
            if let Some(ref key) = config.openai_api_key
                && !key.is_empty()
            {
                providers.insert(
                    ProviderType::OpenAI,
                    Arc::new(OpenAIProvider::new(key.clone())),
                );
                info!("OpenAI provider registered");
            }
            if let Some(ref key) = config.openrouter_api_key
                && !key.is_empty()
            {
                providers.insert(
                    ProviderType::OpenRouter,
                    Arc::new(OpenRouterProvider::new(key.clone())),
                );
                info!("OpenRouter provider registered");
            }
            if let Some(ref key) = config.google_api_key
                && !key.is_empty()
            {
                providers.insert(
                    ProviderType::Google,
                    Arc::new(GeminiProvider::new(key.clone())),
                );
                info!("Google Gemini provider registered");
            }
            if let Some(ref key) = config.groq_api_key
                && !key.is_empty()
            {
                providers.insert(ProviderType::Groq, Arc::new(GroqProvider::new(key.clone())));
                info!("Groq provider registered");
            }
            if let Some(ref key) = config.huggingface_api_key
                && !key.is_empty()
            {
                providers.insert(
                    ProviderType::HuggingFace,
                    Arc::new(HuggingFaceProvider::new(key.clone())),
                );
                info!("HuggingFace provider registered");
            }
        }

        // LiteLLM proxy (always available if configured — it's user-hosted)
        if let Some(ref url) = config.litellm_url
            && !url.is_empty()
        {
            let provider = if let Some(ref key) = config.litellm_api_key {
                LiteLLMProvider::with_api_key(key.clone(), Some(url.clone()))
            } else {
                LiteLLMProvider::new(Some(url.clone()))
            };
            providers.insert(ProviderType::LiteLLM, Arc::new(provider));
            debug!("LiteLLM provider registered at {}", url);
        }

        // Local providers (always available)
        if !config.ollama_url.is_empty() {
            providers.insert(
                ProviderType::Ollama,
                Arc::new(OllamaProvider::new(Some(config.ollama_url.clone()))),
            );
            debug!("Ollama provider registered at {}", config.ollama_url);
        }
        if !config.lmstudio_url.is_empty() {
            providers.insert(
                ProviderType::LMStudio,
                Arc::new(LMStudioProvider::new(Some(config.lmstudio_url.clone()))),
            );
            debug!("LMStudio provider registered at {}", config.lmstudio_url);
        }
        if let Some(ref url) = config.local_provider_url
            && !url.is_empty()
        {
            providers.insert(
                ProviderType::GenericLocal,
                Arc::new(GenericLocalProvider::new(url.clone())),
            );
            debug!("GenericLocal provider registered at {}", url);
        }

        info!("{} AI provider(s) registered", providers.len());

        Self {
            providers,
            router: ModelRouter::new(),
            cost_tracker: CostTracker::new(crate::cost::BudgetLimits::default()),
            config,
            discovery: None,
        }
    }

    /// Update configuration (e.g. after settings change).
    pub fn update_config(&mut self, config: AiServiceConfig) {
        *self = Self::new(config);
    }

    /// The currently configured default model.
    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    /// Whether privacy mode is active.
    pub fn privacy_mode(&self) -> bool {
        self.config.privacy_mode
    }

    /// Access the cost tracker.
    pub fn cost_tracker(&self) -> &CostTracker {
        &self.cost_tracker
    }

    /// Access the cost tracker mutably.
    pub fn cost_tracker_mut(&mut self) -> &mut CostTracker {
        &mut self.cost_tracker
    }

    /// Access the model router (read-only, e.g. for building panel data).
    pub fn router(&self) -> &ModelRouter {
        &self.router
    }

    /// Access the model router mutably (e.g. to set a tier adjuster).
    pub fn router_mut(&mut self) -> &mut ModelRouter {
        &mut self.router
    }

    /// Rebuild the auto-routing fallback chain based on the user's project models.
    ///
    /// Looks up each model ID in `MODEL_REGISTRY` to determine its tier and
    /// provider, then builds a new fallback chain grouped by tier. For any tier
    /// that has no project models, the default chain entries for that tier are
    /// preserved.
    pub fn rebuild_fallback_chain_from_project_models(&mut self, project_models: &[String]) {
        use crate::model_registry::MODEL_REGISTRY;
        use crate::routing::{FallbackChainEntry, default_fallback_chain_pub};

        if project_models.is_empty() {
            // Empty project list → revert to defaults.
            let default_chain = default_fallback_chain_pub();
            self.router.update_fallback_chain(default_chain);
            return;
        }

        let registry = &*MODEL_REGISTRY;

        let mut chain: Vec<FallbackChainEntry> = Vec::new();
        let mut priority = 0u32;

        // Group project models by tier.
        let tiers = [
            crate::types::ModelTier::Premium,
            crate::types::ModelTier::Mid,
            crate::types::ModelTier::Budget,
            crate::types::ModelTier::Free,
        ];

        for tier in &tiers {
            let mut tier_entries: Vec<FallbackChainEntry> = Vec::new();

            for model_id in project_models {
                if let Some(info) = registry.iter().find(|m| m.id == *model_id)
                    && info.tier == *tier
                {
                    tier_entries.push(FallbackChainEntry {
                        provider: map_to_router_provider(info.provider_type),
                        model: model_id.clone(),
                        priority,
                        cost_tier: *tier,
                    });
                    priority += 1;
                }
            }

            // If no project models for this tier, keep the defaults.
            if tier_entries.is_empty() {
                let defaults = default_fallback_chain_pub();
                for entry in defaults {
                    if entry.cost_tier == *tier {
                        let mut e = entry;
                        e.priority = priority;
                        tier_entries.push(e);
                        priority += 1;
                    }
                }
            }

            chain.extend(tier_entries);
        }

        info!(
            entries = chain.len(),
            "Rebuilt fallback chain from {} project models",
            project_models.len()
        );
        self.router.update_fallback_chain(chain);
    }

    /// List all registered providers.
    pub fn available_providers(&self) -> Vec<ProviderType> {
        self.providers.keys().copied().collect()
    }

    /// Resolve a model ID to its provider.
    fn resolve_provider(&self, model_id: &str) -> Option<(ProviderType, Arc<dyn AiProvider>)> {
        // Use the router to pick the provider
        let decision = self.router.route(&[], Some(model_id), None);
        let provider_type = map_router_provider(decision.provider);
        if let Some(provider) = self.providers.get(&provider_type) {
            return Some((provider_type, provider.clone()));
        }
        // Fallback: pick the first available provider
        if let Some((pt, p)) = self.providers.iter().next() {
            return Some((*pt, p.clone()));
        }
        None
    }

    /// Send a non-streaming chat request.
    pub async fn chat(
        &mut self,
        messages: Vec<ChatMessage>,
        model: &str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<ChatResponse, ProviderError> {
        let (provider_type, provider) = self
            .resolve_provider(model)
            .ok_or_else(|| ProviderError::Other("No providers available".into()))?;

        let request = ChatRequest {
            messages,
            model: model.to_string(),
            max_tokens: 4096,
            temperature: None,
            system_prompt: None,
            tools,
        };

        info!(
            "Sending chat request to {:?} model={}",
            provider_type, model
        );
        let response = provider.chat(&request).await?;

        // Track cost
        let cost = calculate_cost(
            model,
            response.usage.prompt_tokens as usize,
            response.usage.completion_tokens as usize,
        );
        self.cost_tracker.record(
            model,
            response.usage.prompt_tokens as usize,
            response.usage.completion_tokens as usize,
        );

        self.router
            .record_result(map_to_router_provider(provider_type), true, None);

        info!(
            "Chat response: {} tokens, ${:.6}",
            response.usage.total_tokens, cost.total_cost
        );

        Ok(response)
    }

    /// Send a streaming chat request. Returns a receiver for stream chunks.
    pub async fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        system_prompt: Option<String>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let (provider_type, provider) = self
            .resolve_provider(model)
            .ok_or_else(|| ProviderError::Other("No providers available".into()))?;

        let request = ChatRequest {
            messages,
            model: model.to_string(),
            max_tokens: 4096,
            temperature: None,
            system_prompt,
            tools,
        };

        info!("Starting stream to {:?} model={}", provider_type, model);
        provider.stream_chat(&request).await
    }

    /// Prepare a streaming request without awaiting it.
    ///
    /// Returns the provider (Arc-cloned) and the request struct so the caller
    /// can spawn the async work outside of any Global borrow. Typical usage in
    /// a GPUI workspace:
    ///
    /// ```ignore
    /// let (provider, request) = cx.global::<AppAiService>().0
    ///     .prepare_stream(messages, model, None, None)
    ///     .ok_or("no provider")?;
    /// cx.spawn(async move |_, _| {
    ///     let rx = provider.stream_chat(&request).await?;
    ///     // feed rx into ChatService
    /// }).detach();
    /// ```
    pub fn prepare_stream(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        system_prompt: Option<String>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Option<(Arc<dyn AiProvider>, ChatRequest)> {
        let (_provider_type, provider) = self.resolve_provider(model)?;
        let request = ChatRequest {
            messages,
            model: model.to_string(),
            max_tokens: 4096,
            temperature: None,
            system_prompt,
            tools,
        };
        Some((provider, request))
    }

    /// Prepare a speculative decoding stream.
    ///
    /// Returns `(draft_provider, draft_request, primary_provider, primary_request)`
    /// so the caller can spawn `speculative::speculative_stream()`.
    ///
    /// Returns `None` if speculative decoding is not possible (no draft model,
    /// or providers not available).
    pub fn prepare_speculative_stream(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        system_prompt: Option<String>,
        tools: Option<Vec<ToolDefinition>>,
        spec_config: &crate::speculative::SpeculativeConfig,
    ) -> Option<(
        Arc<dyn AiProvider>,
        ChatRequest,
        Arc<dyn AiProvider>,
        ChatRequest,
    )> {
        // Select a draft model
        let draft_model = crate::speculative::select_draft_model(model, spec_config)?;

        // Resolve primary provider
        let (_pt_primary, primary_provider) = self.resolve_provider(model)?;
        // Resolve draft provider
        let (_pt_draft, draft_provider) = self.resolve_provider(&draft_model)?;

        let primary_request = ChatRequest {
            messages: messages.clone(),
            model: model.to_string(),
            max_tokens: 4096,
            temperature: None,
            system_prompt: system_prompt.clone(),
            tools: tools.clone(),
        };

        let draft_request = ChatRequest {
            messages,
            model: draft_model,
            max_tokens: 4096,
            temperature: None,
            system_prompt,
            // Don't pass tools to draft model — keep it simple and fast
            tools: None,
        };

        Some((draft_provider, draft_request, primary_provider, primary_request))
    }

    /// Estimate the cost of a message before sending.
    pub fn estimate_cost(&self, text: &str, model: &str) -> CostBreakdown {
        let input_tokens = crate::cost::estimate_tokens(text);
        // Assume 2x output tokens for estimation
        let output_tokens = input_tokens * 2;
        calculate_cost(model, input_tokens, output_tokens)
    }

    // -- Local discovery -----------------------------------------------------

    /// Initialize local AI discovery from config URLs.
    ///
    /// Creates the `LocalDiscovery` engine but does **not** run a scan yet.
    /// The caller should spawn a scan via `discovery.scan_all().await`.
    pub fn start_discovery(&mut self) -> Arc<LocalDiscovery> {
        let mut config_urls = Vec::new();
        if !self.config.ollama_url.is_empty() {
            config_urls.push((ProviderType::Ollama, self.config.ollama_url.clone()));
        }
        if !self.config.lmstudio_url.is_empty() {
            config_urls.push((ProviderType::LMStudio, self.config.lmstudio_url.clone()));
        }
        if let Some(ref url) = self.config.local_provider_url
            && !url.is_empty()
        {
            config_urls.push((ProviderType::GenericLocal, url.clone()));
        }

        let discovery = Arc::new(LocalDiscovery::new(config_urls));
        self.discovery = Some(Arc::clone(&discovery));
        info!("Local AI discovery initialized");
        discovery
    }

    /// Access the discovery engine, if initialized.
    pub fn discovery(&self) -> Option<&Arc<LocalDiscovery>> {
        self.discovery.as_ref()
    }

    /// All models discovered from local AI servers.
    pub fn all_available_models(&self) -> Vec<crate::types::ModelInfo> {
        self.discovery
            .as_ref()
            .map(|d| d.snapshot().all_models())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Provider type mapping (routing crate uses its own ProviderType)
// ---------------------------------------------------------------------------

fn map_router_provider(rp: crate::routing::ProviderType) -> ProviderType {
    match rp {
        crate::routing::ProviderType::Anthropic => ProviderType::Anthropic,
        crate::routing::ProviderType::OpenAI => ProviderType::OpenAI,
        crate::routing::ProviderType::OpenRouter => ProviderType::OpenRouter,
        crate::routing::ProviderType::Groq => ProviderType::Groq,
        crate::routing::ProviderType::LiteLLM => ProviderType::LiteLLM,
        crate::routing::ProviderType::HuggingFace => ProviderType::HuggingFace,
        crate::routing::ProviderType::Ollama => ProviderType::Ollama,
        crate::routing::ProviderType::LMStudio => ProviderType::LMStudio,
        crate::routing::ProviderType::Google => ProviderType::Google,
        crate::routing::ProviderType::GenericLocal => ProviderType::GenericLocal,
    }
}

fn map_to_router_provider(pt: ProviderType) -> crate::routing::ProviderType {
    match pt {
        ProviderType::Anthropic => crate::routing::ProviderType::Anthropic,
        ProviderType::OpenAI => crate::routing::ProviderType::OpenAI,
        ProviderType::OpenRouter => crate::routing::ProviderType::OpenRouter,
        ProviderType::Google => crate::routing::ProviderType::Google,
        ProviderType::Groq => crate::routing::ProviderType::Groq,
        ProviderType::LiteLLM => crate::routing::ProviderType::LiteLLM,
        ProviderType::HuggingFace => crate::routing::ProviderType::HuggingFace,
        ProviderType::Ollama => crate::routing::ProviderType::Ollama,
        ProviderType::LMStudio => crate::routing::ProviderType::LMStudio,
        ProviderType::GenericLocal => crate::routing::ProviderType::GenericLocal,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessageRole;

    fn test_config() -> AiServiceConfig {
        AiServiceConfig {
            anthropic_api_key: Some("sk-test-key".into()),
            openai_api_key: None,
            openrouter_api_key: None,
            google_api_key: None,
            groq_api_key: None,
            huggingface_api_key: None,
            litellm_url: None,
            litellm_api_key: None,
            ollama_url: "http://localhost:11434".into(),
            lmstudio_url: String::new(),
            local_provider_url: None,
            privacy_mode: false,
            default_model: "claude-sonnet-4-5".into(),
            auto_routing: true,
        }
    }

    #[test]
    fn test_service_creation() {
        let svc = AiService::new(test_config());
        assert!(!svc.available_providers().is_empty());
        assert_eq!(svc.default_model(), "claude-sonnet-4-5");
    }

    #[test]
    fn test_privacy_mode_excludes_cloud() {
        let mut config = test_config();
        config.privacy_mode = true;
        let svc = AiService::new(config);
        assert!(!svc.available_providers().contains(&ProviderType::Anthropic));
        assert!(!svc.available_providers().contains(&ProviderType::OpenAI));
    }

    #[test]
    fn test_empty_keys_not_registered() {
        let mut config = test_config();
        config.anthropic_api_key = Some(String::new());
        let svc = AiService::new(config);
        assert!(!svc.available_providers().contains(&ProviderType::Anthropic));
    }

    #[test]
    fn test_ollama_always_registered() {
        let mut config = test_config();
        config.privacy_mode = true;
        let svc = AiService::new(config);
        assert!(svc.available_providers().contains(&ProviderType::Ollama));
    }

    #[test]
    fn test_cost_estimation() {
        let svc = AiService::new(test_config());
        let cost = svc.estimate_cost("Hello, how are you?", "claude-sonnet-4-5-20250929");
        assert!(cost.total_cost >= 0.0);
    }

    #[test]
    fn test_cost_tracker_accessible() {
        let mut svc = AiService::new(test_config());
        svc.cost_tracker_mut()
            .record("claude-sonnet-4-5-20250929", 100, 50);
        assert!(svc.cost_tracker().total_cost() > 0.0);
    }

    #[test]
    fn test_update_config() {
        let mut svc = AiService::new(test_config());
        assert!(!svc.privacy_mode());
        let mut new_config = test_config();
        new_config.privacy_mode = true;
        svc.update_config(new_config);
        assert!(svc.privacy_mode());
    }

    #[test]
    fn test_no_providers_returns_none() {
        let config = AiServiceConfig {
            privacy_mode: true,
            default_model: "test".into(),
            ..Default::default()
        };
        let svc = AiService::new(config);
        assert!(svc.available_providers().is_empty());
        assert!(svc.resolve_provider("claude-sonnet-4-5").is_none());
    }

    #[test]
    fn test_prepare_stream_returns_provider_and_request() {
        let svc = AiService::new(test_config());
        let messages = vec![ChatMessage::text(MessageRole::User, "Hello")];
        let result = svc.prepare_stream(messages, "claude-sonnet-4-5", None, None);
        assert!(result.is_some());
        let (provider, request) = result.unwrap();
        assert_eq!(request.model, "claude-sonnet-4-5");
        assert_eq!(request.messages.len(), 1);
        assert!(!provider.name().is_empty());
    }

    #[test]
    fn test_prepare_stream_no_providers_returns_none() {
        let config = AiServiceConfig {
            privacy_mode: true,
            default_model: "test".into(),
            ..Default::default()
        };
        let svc = AiService::new(config);
        let result = svc.prepare_stream(vec![], "test-model", None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_provider_type_mapping_roundtrip() {
        let types = vec![
            ProviderType::Anthropic,
            ProviderType::OpenAI,
            ProviderType::OpenRouter,
            ProviderType::Google,
            ProviderType::Groq,
            ProviderType::LiteLLM,
            ProviderType::HuggingFace,
            ProviderType::Ollama,
            ProviderType::LMStudio,
            ProviderType::GenericLocal,
        ];
        for pt in types {
            let rp = map_to_router_provider(pt);
            let back = map_router_provider(rp);
            assert_eq!(pt, back);
        }
    }
}
