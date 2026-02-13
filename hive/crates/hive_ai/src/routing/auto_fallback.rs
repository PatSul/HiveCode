//! Auto-Fallback Manager
//!
//! Tracks provider health (rate limits, failures, availability) and provides
//! intelligent fallback chains when a provider is unavailable.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::types::ModelTier;

// ---------------------------------------------------------------------------
// Provider type (will move to types.rs when the other agent finishes)
// ---------------------------------------------------------------------------

/// Supported AI provider backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    OpenAI,
    OpenRouter,
    Google,
    Groq,
    LiteLLM,
    HuggingFace,
    Ollama,
    LMStudio,
    GenericLocal,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Anthropic => "anthropic",
            Self::OpenAI => "openai",
            Self::OpenRouter => "openrouter",
            Self::Google => "google",
            Self::Groq => "groq",
            Self::LiteLLM => "litellm",
            Self::HuggingFace => "hugging_face",
            Self::Ollama => "ollama",
            Self::LMStudio => "lmstudio",
            Self::GenericLocal => "generic_local",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Health status of a single provider.
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub available: bool,
    pub rate_limited_until: Option<Instant>,
    pub consecutive_failures: u32,
    pub last_success: Option<Instant>,
    pub last_failure: Option<Instant>,
    pub last_error: Option<String>,
    pub budget_exhausted: bool,
}

impl Default for ProviderStatus {
    fn default() -> Self {
        Self {
            available: false,
            rate_limited_until: None,
            consecutive_failures: 0,
            last_success: None,
            last_failure: None,
            last_error: None,
            budget_exhausted: false,
        }
    }
}

/// Why a fallback was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    RateLimit,
    ServerError,
    Timeout,
    BudgetExhausted,
    ProviderDown,
    ModelUnavailable,
}

impl FallbackReason {
    /// Parse an error message into a `FallbackReason`.
    pub fn from_error(error: &str) -> Self {
        let lower = error.to_lowercase();
        if lower.contains("rate limit")
            || lower.contains("429")
            || lower.contains("too many requests")
        {
            Self::RateLimit
        } else if lower.contains("timeout") || lower.contains("timed out") {
            Self::Timeout
        } else if lower.contains("budget")
            || lower.contains("insufficient funds")
            || lower.contains("quota")
        {
            Self::BudgetExhausted
        } else if lower.contains("model")
            && (lower.contains("not found") || lower.contains("unavailable"))
        {
            Self::ModelUnavailable
        } else if lower.contains("500") || lower.contains("502") || lower.contains("503") {
            Self::ServerError
        } else {
            Self::ServerError
        }
    }
}

/// Configuration for the fallback manager.
#[derive(Debug, Clone)]
pub struct FallbackConfig {
    /// Maximum consecutive failures before a provider is marked unavailable.
    pub max_consecutive_failures: u32,
    /// How long to wait after a rate-limit before retrying a provider.
    pub rate_limit_cooldown: Duration,
    /// How long to wait after generic failures before retrying a provider.
    pub failure_cooldown: Duration,
    /// Whether to respect budget exhaustion signals.
    pub budget_fallback_enabled: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            max_consecutive_failures: 3,
            rate_limit_cooldown: Duration::from_secs(60),
            failure_cooldown: Duration::from_secs(30),
            budget_fallback_enabled: true,
        }
    }
}

/// An entry in the static fallback chain.
#[derive(Debug, Clone)]
pub struct FallbackChainEntry {
    pub provider: ProviderType,
    pub model: String,
    pub priority: u32,
    pub cost_tier: ModelTier,
}

/// A recorded fallback event for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackEvent {
    /// Milliseconds since some reference epoch (we use duration since the
    /// manager was created to avoid `Instant` serialization issues).
    pub age_ms: u64,
    pub original_provider: String,
    pub original_model: String,
    pub fallback_provider: String,
    pub fallback_model: String,
    pub reason: FallbackReason,
}

// ---------------------------------------------------------------------------
// Default fallback chain
// ---------------------------------------------------------------------------

fn default_fallback_chain() -> Vec<FallbackChainEntry> {
    vec![
        // Premium tier
        FallbackChainEntry {
            provider: ProviderType::Anthropic,
            model: "claude-opus-4-20250514".into(),
            priority: 1,
            cost_tier: ModelTier::Premium,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenAI,
            model: "gpt-4o".into(),
            priority: 2,
            cost_tier: ModelTier::Premium,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenRouter,
            model: "anthropic/claude-opus-4".into(),
            priority: 3,
            cost_tier: ModelTier::Premium,
        },
        // Mid tier
        FallbackChainEntry {
            provider: ProviderType::Anthropic,
            model: "claude-sonnet-4-20250514".into(),
            priority: 10,
            cost_tier: ModelTier::Mid,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenAI,
            model: "gpt-4o-mini".into(),
            priority: 11,
            cost_tier: ModelTier::Mid,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenRouter,
            model: "anthropic/claude-sonnet-4".into(),
            priority: 12,
            cost_tier: ModelTier::Mid,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenRouter,
            model: "google/gemini-pro-1.5".into(),
            priority: 13,
            cost_tier: ModelTier::Mid,
        },
        // Budget tier
        FallbackChainEntry {
            provider: ProviderType::Anthropic,
            model: "claude-haiku-4-5-20251001".into(),
            priority: 20,
            cost_tier: ModelTier::Budget,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenRouter,
            model: "deepseek/deepseek-chat".into(),
            priority: 21,
            cost_tier: ModelTier::Budget,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenRouter,
            model: "meta-llama/llama-3.3-70b-instruct".into(),
            priority: 22,
            cost_tier: ModelTier::Budget,
        },
        FallbackChainEntry {
            provider: ProviderType::OpenRouter,
            model: "qwen/qwen-2.5-72b-instruct".into(),
            priority: 23,
            cost_tier: ModelTier::Budget,
        },
        // Groq (fast inference)
        FallbackChainEntry {
            provider: ProviderType::Groq,
            model: "llama-3.3-70b-versatile".into(),
            priority: 24,
            cost_tier: ModelTier::Budget,
        },
        // Free tier (local)
        FallbackChainEntry {
            provider: ProviderType::Ollama,
            model: "llama3.2".into(),
            priority: 100,
            cost_tier: ModelTier::Free,
        },
        FallbackChainEntry {
            provider: ProviderType::Ollama,
            model: "codellama".into(),
            priority: 101,
            cost_tier: ModelTier::Free,
        },
        FallbackChainEntry {
            provider: ProviderType::Ollama,
            model: "mistral".into(),
            priority: 102,
            cost_tier: ModelTier::Free,
        },
    ]
}

// ---------------------------------------------------------------------------
// AutoFallbackManager
// ---------------------------------------------------------------------------

/// Tracks provider health and selects fallback providers/models when the
/// primary choice is unavailable.
pub struct AutoFallbackManager {
    provider_status: RwLock<HashMap<ProviderType, ProviderStatus>>,
    config: FallbackConfig,
    fallback_chain: Vec<FallbackChainEntry>,
    /// History of fallback events (capped at 1000 entries).
    history: RwLock<Vec<FallbackEvent>>,
    /// Instant the manager was created, used as epoch for `FallbackEvent::age_ms`.
    created_at: Instant,
}

impl AutoFallbackManager {
    /// Create a new fallback manager with the given configuration.
    pub fn new(config: FallbackConfig) -> Self {
        let mut status_map = HashMap::new();
        for provider in &[
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
        ] {
            status_map.insert(*provider, ProviderStatus::default());
        }

        Self {
            provider_status: RwLock::new(status_map),
            config,
            fallback_chain: default_fallback_chain(),
            history: RwLock::new(Vec::new()),
            created_at: Instant::now(),
        }
    }

    /// Create a manager with [`FallbackConfig::default()`].
    pub fn with_defaults() -> Self {
        Self::new(FallbackConfig::default())
    }

    // ------------------------------------------------------------------
    // Provider availability
    // ------------------------------------------------------------------

    /// Mark a provider as available (resets consecutive failures).
    pub fn set_available(&self, provider: ProviderType, available: bool) {
        let mut map = self.provider_status.write();
        let status = map.entry(provider).or_default();
        status.available = available;
        if available {
            status.consecutive_failures = 0;
        }
        debug!(%provider, available, "Provider availability updated");
    }

    /// Record a successful request to a provider.
    pub fn record_success(&self, provider: ProviderType) {
        let mut map = self.provider_status.write();
        let status = map.entry(provider).or_default();
        status.consecutive_failures = 0;
        status.last_success = Some(Instant::now());
        status.last_error = None;
        debug!(%provider, "Provider success recorded");
    }

    /// Record a failure for a provider. If consecutive failures exceed the
    /// configured maximum the provider is automatically marked unavailable.
    pub fn record_failure(&self, provider: ProviderType, reason: FallbackReason) {
        let mut map = self.provider_status.write();
        let status = map.entry(provider).or_default();
        status.consecutive_failures += 1;
        status.last_failure = Some(Instant::now());
        status.last_error = Some(format!("{:?}", reason));

        match reason {
            FallbackReason::RateLimit => {
                let until = Instant::now() + self.config.rate_limit_cooldown;
                status.rate_limited_until = Some(until);
                warn!(
                    %provider,
                    cooldown_secs = self.config.rate_limit_cooldown.as_secs(),
                    "Provider rate-limited"
                );
            }
            FallbackReason::BudgetExhausted => {
                status.budget_exhausted = true;
                warn!(%provider, "Provider budget exhausted");
            }
            _ => {}
        }

        if status.consecutive_failures >= self.config.max_consecutive_failures {
            status.available = false;
            warn!(
                %provider,
                failures = status.consecutive_failures,
                "Provider auto-disabled after consecutive failures"
            );
        }
    }

    /// Check whether a provider is currently available (not rate-limited,
    /// not budget-exhausted, and not disabled).
    pub fn is_available(&self, provider: ProviderType) -> bool {
        let map = self.provider_status.read();
        let Some(status) = map.get(&provider) else {
            return false;
        };

        if !status.available {
            return false;
        }

        // Check rate-limit cooldown
        if let Some(until) = status.rate_limited_until {
            if Instant::now() < until {
                return false;
            }
        }

        // Check budget exhaustion
        if self.config.budget_fallback_enabled && status.budget_exhausted {
            return false;
        }

        true
    }

    /// Mark a provider as rate-limited with an optional custom duration.
    pub fn set_rate_limited(&self, provider: ProviderType, duration: Option<Duration>) {
        let cooldown = duration.unwrap_or(self.config.rate_limit_cooldown);
        let mut map = self.provider_status.write();
        let status = map.entry(provider).or_default();
        status.rate_limited_until = Some(Instant::now() + cooldown);
    }

    /// Mark/unmark a provider's budget as exhausted.
    pub fn set_budget_exhausted(&self, provider: ProviderType, exhausted: bool) {
        let mut map = self.provider_status.write();
        let status = map.entry(provider).or_default();
        status.budget_exhausted = exhausted;
    }

    // ------------------------------------------------------------------
    // Fallback chain
    // ------------------------------------------------------------------

    /// Return the ordered fallback chain for a given tier, skipping providers
    /// that are currently unavailable.
    ///
    /// The chain follows the rule: **Premium -> Mid -> Budget -> Free (local)**.
    /// Within each tier, providers are sorted by priority and unavailable ones
    /// are filtered out.
    pub fn get_fallback_chain(&self, tier: ModelTier) -> Vec<ProviderType> {
        let tier_order = tier_ordering(tier);

        let mut entries: Vec<&FallbackChainEntry> = self
            .fallback_chain
            .iter()
            .filter(|e| tier_ordering(e.cost_tier) >= tier_order)
            .filter(|e| self.is_available(e.provider))
            .collect();

        // Sort: same tier first, then by priority within each tier
        entries.sort_by(|a, b| {
            let ta = tier_ordering(a.cost_tier);
            let tb = tier_ordering(b.cost_tier);
            // Higher tier value = more expensive = should come first for
            // premium requests, but we want to match the *requested* tier
            // first, then fall down.
            let tier_dist_a = (ta as i32 - tier_order as i32).unsigned_abs();
            let tier_dist_b = (tb as i32 - tier_order as i32).unsigned_abs();
            tier_dist_a
                .cmp(&tier_dist_b)
                .then(a.priority.cmp(&b.priority))
        });

        // Deduplicate providers while preserving order
        let mut seen = std::collections::HashSet::new();
        entries
            .into_iter()
            .filter(|e| seen.insert(e.provider))
            .map(|e| e.provider)
            .collect()
    }

    /// Get the next fallback entry for a failed request, skipping providers
    /// that have already been tried.
    pub fn get_next_fallback(
        &self,
        original_provider: ProviderType,
        original_model: &str,
        reason: FallbackReason,
        tried: &[ProviderType],
    ) -> Option<FallbackChainEntry> {
        let tried_set: std::collections::HashSet<ProviderType> = tried.iter().copied().collect();

        let mut candidates: Vec<&FallbackChainEntry> = self
            .fallback_chain
            .iter()
            .filter(|e| !tried_set.contains(&e.provider))
            .filter(|e| !(e.provider == original_provider && e.model == original_model))
            .filter(|e| self.is_available(e.provider))
            .collect();

        // For budget exhaustion, prefer cheaper tiers
        if reason == FallbackReason::BudgetExhausted {
            let original_entry = self
                .fallback_chain
                .iter()
                .find(|e| e.provider == original_provider && e.model == original_model);
            let original_tier_ord = original_entry
                .map(|e| tier_ordering(e.cost_tier))
                .unwrap_or(2); // default to Mid

            candidates.retain(|e| tier_ordering(e.cost_tier) < original_tier_ord);
        }

        candidates.sort_by_key(|e| e.priority);

        let result = candidates.first().map(|e| (*e).clone());

        // Record the fallback event
        if let Some(ref entry) = result {
            info!(
                from_provider = %original_provider,
                from_model = original_model,
                to_provider = %entry.provider,
                to_model = %entry.model,
                ?reason,
                "Fallback triggered"
            );

            let event = FallbackEvent {
                age_ms: self.created_at.elapsed().as_millis() as u64,
                original_provider: original_provider.to_string(),
                original_model: original_model.to_string(),
                fallback_provider: entry.provider.to_string(),
                fallback_model: entry.model.clone(),
                reason,
            };

            let mut history = self.history.write();
            history.push(event);
            // Cap history at 1000, keep last 500
            if history.len() > 1000 {
                let drain_end = history.len() - 500;
                history.drain(..drain_end);
            }
        }

        result
    }

    // ------------------------------------------------------------------
    // Status / history queries
    // ------------------------------------------------------------------

    /// Get a snapshot of all provider statuses.
    pub fn provider_statuses(&self) -> HashMap<ProviderType, ProviderStatus> {
        self.provider_status.read().clone()
    }

    /// Return the fallback history.
    pub fn fallback_history(&self) -> Vec<FallbackEvent> {
        self.history.read().clone()
    }

    /// Clear all rate-limit cooldowns.
    pub fn clear_rate_limits(&self) {
        let mut map = self.provider_status.write();
        for status in map.values_mut() {
            status.rate_limited_until = None;
        }
        info!("All rate-limit cooldowns cleared");
    }

    /// Reset every provider to a clean slate.
    pub fn reset_all(&self) {
        let mut map = self.provider_status.write();
        for status in map.values_mut() {
            status.consecutive_failures = 0;
            status.last_error = None;
            status.rate_limited_until = None;
            status.budget_exhausted = false;
        }
        info!("All provider statuses reset");
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a `ModelTier` to a numeric ordering for comparison.
/// Higher = more expensive.
fn tier_ordering(tier: ModelTier) -> u8 {
    match tier {
        ModelTier::Free => 0,
        ModelTier::Budget => 1,
        ModelTier::Mid => 2,
        ModelTier::Premium => 3,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> AutoFallbackManager {
        let m = AutoFallbackManager::with_defaults();
        // Mark the main cloud providers as available
        m.set_available(ProviderType::Anthropic, true);
        m.set_available(ProviderType::OpenAI, true);
        m.set_available(ProviderType::OpenRouter, true);
        m
    }

    #[test]
    fn initially_unavailable() {
        let m = AutoFallbackManager::with_defaults();
        assert!(!m.is_available(ProviderType::Anthropic));
    }

    #[test]
    fn available_after_set() {
        let m = mgr();
        assert!(m.is_available(ProviderType::Anthropic));
    }

    #[test]
    fn rate_limit_makes_unavailable() {
        let m = mgr();
        m.set_rate_limited(ProviderType::Anthropic, Some(Duration::from_secs(600)));
        assert!(!m.is_available(ProviderType::Anthropic));
    }

    #[test]
    fn consecutive_failures_disable_provider() {
        let m = mgr();
        for _ in 0..3 {
            m.record_failure(ProviderType::OpenAI, FallbackReason::ServerError);
        }
        assert!(!m.is_available(ProviderType::OpenAI));
    }

    #[test]
    fn success_resets_failures() {
        let m = mgr();
        m.record_failure(ProviderType::OpenAI, FallbackReason::ServerError);
        m.record_failure(ProviderType::OpenAI, FallbackReason::ServerError);
        m.record_success(ProviderType::OpenAI);
        assert!(m.is_available(ProviderType::OpenAI));
    }

    #[test]
    fn fallback_chain_returns_available_providers() {
        let m = mgr();
        let chain = m.get_fallback_chain(ModelTier::Premium);
        assert!(!chain.is_empty());
        // All returned providers should be available
        for p in &chain {
            assert!(m.is_available(*p));
        }
    }

    #[test]
    fn budget_exhaustion_fallback_prefers_cheaper() {
        let m = mgr();
        m.set_available(ProviderType::Ollama, true);
        let fb = m.get_next_fallback(
            ProviderType::Anthropic,
            "claude-sonnet-4-20250514",
            FallbackReason::BudgetExhausted,
            &[],
        );
        // Should fall to a cheaper tier
        if let Some(entry) = fb {
            assert!(tier_ordering(entry.cost_tier) < tier_ordering(ModelTier::Mid));
        }
    }

    #[test]
    fn next_fallback_skips_tried() {
        let m = mgr();
        let fb = m.get_next_fallback(
            ProviderType::Anthropic,
            "claude-opus-4-20250514",
            FallbackReason::ServerError,
            &[ProviderType::OpenAI],
        );
        if let Some(entry) = fb {
            assert_ne!(entry.provider, ProviderType::Anthropic);
            assert_ne!(entry.provider, ProviderType::OpenAI);
        }
    }

    #[test]
    fn history_records_fallback() {
        let m = mgr();
        let _ = m.get_next_fallback(
            ProviderType::Anthropic,
            "claude-opus-4-20250514",
            FallbackReason::Timeout,
            &[],
        );
        let history = m.fallback_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].reason, FallbackReason::Timeout);
    }

    #[test]
    fn clear_rate_limits_restores_availability() {
        let m = mgr();
        m.set_rate_limited(ProviderType::Anthropic, Some(Duration::from_secs(600)));
        assert!(!m.is_available(ProviderType::Anthropic));
        m.clear_rate_limits();
        assert!(m.is_available(ProviderType::Anthropic));
    }

    #[test]
    fn parse_error_reason() {
        assert_eq!(
            FallbackReason::from_error("rate limit exceeded (429)"),
            FallbackReason::RateLimit
        );
        assert_eq!(
            FallbackReason::from_error("request timed out"),
            FallbackReason::Timeout
        );
        assert_eq!(
            FallbackReason::from_error("budget quota exceeded"),
            FallbackReason::BudgetExhausted
        );
        assert_eq!(
            FallbackReason::from_error("502 bad gateway"),
            FallbackReason::ServerError
        );
        assert_eq!(
            FallbackReason::from_error("model not found"),
            FallbackReason::ModelUnavailable
        );
    }
}
