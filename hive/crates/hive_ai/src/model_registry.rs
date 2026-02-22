//! Static model registry ported from the Electron MODEL_REGISTRY.
//!
//! Provides lookup helpers for model resolution by id, provider, or tier.

use once_cell::sync::Lazy;
use std::collections::HashSet;

use crate::types::{ModelCapabilities, ModelCapability, ModelInfo, ModelTier, ProviderType};

// ---------------------------------------------------------------------------
// Capability helpers
// ---------------------------------------------------------------------------

fn caps(list: &[ModelCapability]) -> ModelCapabilities {
    ModelCapabilities::new(list)
}

/// Return provider-level capabilities that apply to all models from a provider.
pub fn provider_capabilities(provider: ProviderType) -> HashSet<ModelCapability> {
    match provider {
        ProviderType::Anthropic => [ModelCapability::ToolUse, ModelCapability::StructuredOutput]
            .into_iter()
            .collect(),
        ProviderType::OpenAI => [
            ModelCapability::ToolUse,
            ModelCapability::NativeAgents,
            ModelCapability::StructuredOutput,
        ]
        .into_iter()
        .collect(),
        ProviderType::OpenRouter => [ModelCapability::ToolUse].into_iter().collect(),
        ProviderType::Google => [ModelCapability::ToolUse, ModelCapability::StructuredOutput]
            .into_iter()
            .collect(),
        ProviderType::Groq => [ModelCapability::ToolUse].into_iter().collect(),
        ProviderType::XAI => [ModelCapability::ToolUse, ModelCapability::StructuredOutput]
            .into_iter()
            .collect(),
        _ => HashSet::new(),
    }
}

// ---------------------------------------------------------------------------
// Registry data
// ---------------------------------------------------------------------------

/// All known cloud models with pricing and metadata.
pub static MODEL_REGISTRY: Lazy<Vec<ModelInfo>> = Lazy::new(|| {
    vec![
        // ---- Anthropic ----
        ModelInfo {
            id: "claude-opus-4-6".into(),
            name: "Claude Opus 4.6".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Premium,
            context_window: 200_000,
            input_price_per_mtok: 5.0,
            output_price_per_mtok: 25.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "claude-opus-4-5-20251101".into(),
            name: "Claude Opus 4.5".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Premium,
            context_window: 200_000,
            input_price_per_mtok: 5.0,
            output_price_per_mtok: 25.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "claude-opus-4-1-20250805".into(),
            name: "Claude Opus 4.1".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Premium,
            context_window: 200_000,
            input_price_per_mtok: 15.0,
            output_price_per_mtok: 75.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "claude-opus-4-20250514".into(),
            name: "Claude Opus 4".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Premium,
            context_window: 200_000,
            input_price_per_mtok: 15.0,
            output_price_per_mtok: 75.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "claude-sonnet-4-5-20250929".into(),
            name: "Claude Sonnet 4.5".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 3.0,
            output_price_per_mtok: 15.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "claude-sonnet-4-20250514".into(),
            name: "Claude Sonnet 4".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 3.0,
            output_price_per_mtok: 15.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2025-05-14".into()),
        },
        ModelInfo {
            id: "claude-haiku-4-5-20251001".into(),
            name: "Claude Haiku 4.5".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Budget,
            context_window: 200_000,
            input_price_per_mtok: 1.0,
            output_price_per_mtok: 5.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        // ---- OpenAI ----
        ModelInfo {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 128_000,
            input_price_per_mtok: 2.5,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2024-05-13".into()),
        },
        ModelInfo {
            id: "gpt-4o-mini".into(),
            name: "GPT-4o Mini".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Budget,
            context_window: 128_000,
            input_price_per_mtok: 0.15,
            output_price_per_mtok: 0.6,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2024-07-18".into()),
        },
        ModelInfo {
            id: "gpt-5".into(),
            name: "GPT-5".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 400_000,
            input_price_per_mtok: 1.25,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5-mini".into(),
            name: "GPT-5 Mini".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Budget,
            context_window: 400_000,
            input_price_per_mtok: 0.25,
            output_price_per_mtok: 2.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5-nano".into(),
            name: "GPT-5 Nano".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Budget,
            context_window: 400_000,
            input_price_per_mtok: 0.05,
            output_price_per_mtok: 0.40,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5.1".into(),
            name: "GPT-5.1".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 400_000,
            input_price_per_mtok: 1.25,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5.2".into(),
            name: "GPT-5.2".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 400_000,
            input_price_per_mtok: 1.75,
            output_price_per_mtok: 14.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5.2-pro".into(),
            name: "GPT-5.2 Pro".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Premium,
            context_window: 400_000,
            input_price_per_mtok: 21.0,
            output_price_per_mtok: 168.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        // ---- OpenAI Codex ----
        // NOTE: gpt-5.3-codex and gpt-5.3-codex-spark are excluded — no public
        // API access yet (ChatGPT-only / Codex CLI). They will appear
        // automatically once OpenAI adds them to the /v1/models endpoint.
        ModelInfo {
            id: "gpt-5.2-codex".into(),
            name: "GPT-5.2 Codex".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Premium,
            context_window: 400_000,
            input_price_per_mtok: 1.75,
            output_price_per_mtok: 14.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5.1-codex".into(),
            name: "GPT-5.1 Codex".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 400_000,
            input_price_per_mtok: 1.25,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5.1-codex-mini".into(),
            name: "GPT-5.1 Codex Mini".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Budget,
            context_window: 400_000,
            input_price_per_mtok: 0.25,
            output_price_per_mtok: 2.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-5-codex".into(),
            name: "GPT-5 Codex".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 400_000,
            input_price_per_mtok: 1.25,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "codex-mini-latest".into(),
            name: "Codex Mini".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 1.50,
            output_price_per_mtok: 6.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "o3".into(),
            name: "o3".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 2.0,
            output_price_per_mtok: 8.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "o3-mini".into(),
            name: "o3 Mini".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 1.10,
            output_price_per_mtok: 4.40,
            capabilities: caps(&[
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2025-01-31".into()),
        },
        ModelInfo {
            id: "o4-mini".into(),
            name: "o4 Mini".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 1.10,
            output_price_per_mtok: 4.40,
            capabilities: caps(&[
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-4.1".into(),
            name: "GPT-4.1".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 1_048_576,
            input_price_per_mtok: 2.0,
            output_price_per_mtok: 8.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::NativeAgents,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gpt-4.1-mini".into(),
            name: "GPT-4.1 Mini".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Budget,
            context_window: 1_048_576,
            input_price_per_mtok: 0.40,
            output_price_per_mtok: 1.60,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        // ---- DeepSeek (via OpenRouter) ----
        ModelInfo {
            id: "deepseek/deepseek-chat".into(),
            name: "DeepSeek Chat".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Budget,
            context_window: 128_000,
            input_price_per_mtok: 0.14,
            output_price_per_mtok: 0.28,
            capabilities: caps(&[ModelCapability::ToolUse, ModelCapability::LongContext]),
            release_date: None,
        },
        ModelInfo {
            id: "deepseek/deepseek-r1".into(),
            name: "DeepSeek R1".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Mid,
            context_window: 128_000,
            input_price_per_mtok: 0.55,
            output_price_per_mtok: 2.19,
            capabilities: caps(&[
                ModelCapability::ExtendedThinking,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        // ---- OpenRouter Models ----
        // Meta Llama
        ModelInfo {
            id: "meta-llama/llama-3.3-70b-instruct".into(),
            name: "Llama 3.3 70B".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Budget,
            context_window: 131_072,
            input_price_per_mtok: 0.39,
            output_price_per_mtok: 0.39,
            capabilities: caps(&[ModelCapability::ToolUse, ModelCapability::LongContext]),
            release_date: None,
        },
        // Mistral
        ModelInfo {
            id: "mistralai/mistral-large-2411".into(),
            name: "Mistral Large".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Mid,
            context_window: 128_000,
            input_price_per_mtok: 2.0,
            output_price_per_mtok: 6.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "mistralai/mistral-small-2503".into(),
            name: "Mistral Small".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Budget,
            context_window: 32_000,
            input_price_per_mtok: 0.1,
            output_price_per_mtok: 0.3,
            capabilities: caps(&[ModelCapability::ToolUse, ModelCapability::StructuredOutput]),
            release_date: None,
        },
        // Google Gemini (via OpenRouter)
        ModelInfo {
            id: "google/gemini-2.0-flash-001".into(),
            name: "Gemini 2.0 Flash".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Budget,
            context_window: 1_048_576,
            input_price_per_mtok: 0.1,
            output_price_per_mtok: 0.4,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "google/gemini-2.5-pro-preview".into(),
            name: "Gemini 2.5 Pro".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Mid,
            context_window: 1_048_576,
            input_price_per_mtok: 1.25,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        // Qwen
        ModelInfo {
            id: "qwen/qwen-2.5-72b-instruct".into(),
            name: "Qwen 2.5 72B".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Budget,
            context_window: 131_072,
            input_price_per_mtok: 0.36,
            output_price_per_mtok: 0.36,
            capabilities: caps(&[ModelCapability::ToolUse, ModelCapability::LongContext]),
            release_date: None,
        },
        // Anthropic via OpenRouter
        ModelInfo {
            id: "anthropic/claude-sonnet-4".into(),
            name: "Claude Sonnet 4 (OR)".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 3.0,
            output_price_per_mtok: 15.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "anthropic/claude-haiku-4".into(),
            name: "Claude Haiku 4 (OR)".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Budget,
            context_window: 200_000,
            input_price_per_mtok: 0.8,
            output_price_per_mtok: 4.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        // OpenAI via OpenRouter
        ModelInfo {
            id: "openai/gpt-4o".into(),
            name: "GPT-4o (OR)".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Mid,
            context_window: 128_000,
            input_price_per_mtok: 2.5,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "openai/o3-mini".into(),
            name: "o3-mini (OR)".into(),
            provider: "openrouter".into(),
            provider_type: ProviderType::OpenRouter,
            tier: ModelTier::Mid,
            context_window: 200_000,
            input_price_per_mtok: 1.1,
            output_price_per_mtok: 4.4,
            capabilities: caps(&[
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        // ---- Google Gemini (direct API) ----
        ModelInfo {
            id: "gemini-3.1-pro-preview".into(),
            name: "Gemini 3.1 Pro".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Premium,
            context_window: 1_048_576,
            input_price_per_mtok: 2.0,
            output_price_per_mtok: 12.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gemini-3.1-flash-preview".into(),
            name: "Gemini 3.1 Flash".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Mid,
            context_window: 1_048_576,
            input_price_per_mtok: 0.50,
            output_price_per_mtok: 3.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gemini-3-pro-preview".into(),
            name: "Gemini 3 Pro".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Premium,
            context_window: 1_048_576,
            input_price_per_mtok: 2.0,
            output_price_per_mtok: 12.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gemini-3-flash-preview".into(),
            name: "Gemini 3 Flash".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Mid,
            context_window: 1_048_576,
            input_price_per_mtok: 0.50,
            output_price_per_mtok: 3.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gemini-2.5-flash-lite".into(),
            name: "Gemini 2.5 Flash Lite".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Budget,
            context_window: 1_048_576,
            input_price_per_mtok: 0.075,
            output_price_per_mtok: 0.30,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gemini-2.5-pro".into(),
            name: "Gemini 2.5 Pro".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Premium,
            context_window: 1_048_576,
            input_price_per_mtok: 1.25,
            output_price_per_mtok: 10.0,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gemini-2.5-flash".into(),
            name: "Gemini 2.5 Flash".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Mid,
            context_window: 1_048_576,
            input_price_per_mtok: 0.15,
            output_price_per_mtok: 0.60,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::ExtendedThinking,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: None,
        },
        ModelInfo {
            id: "gemini-2.0-flash".into(),
            name: "Gemini 2.0 Flash".into(),
            provider: "google".into(),
            provider_type: ProviderType::Google,
            tier: ModelTier::Budget,
            context_window: 1_048_576,
            input_price_per_mtok: 0.10,
            output_price_per_mtok: 0.40,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::Vision,
                ModelCapability::CodeExecution,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2025-02-05".into()),
        },
        // ---- Groq ----
        ModelInfo {
            id: "llama-3.3-70b-versatile".into(),
            name: "Llama 3.3 70B (Groq)".into(),
            provider: "groq".into(),
            provider_type: ProviderType::Groq,
            tier: ModelTier::Budget,
            context_window: 128_000,
            input_price_per_mtok: 0.59,
            output_price_per_mtok: 0.79,
            capabilities: caps(&[ModelCapability::ToolUse, ModelCapability::LongContext]),
            release_date: None,
        },
        ModelInfo {
            id: "llama-3.1-8b-instant".into(),
            name: "Llama 3.1 8B (Groq)".into(),
            provider: "groq".into(),
            provider_type: ProviderType::Groq,
            tier: ModelTier::Budget,
            context_window: 128_000,
            input_price_per_mtok: 0.05,
            output_price_per_mtok: 0.08,
            capabilities: caps(&[ModelCapability::ToolUse, ModelCapability::LongContext]),
            release_date: None,
        },
        ModelInfo {
            id: "mixtral-8x7b-32768".into(),
            name: "Mixtral 8x7B (Groq)".into(),
            provider: "groq".into(),
            provider_type: ProviderType::Groq,
            tier: ModelTier::Budget,
            context_window: 32_768,
            input_price_per_mtok: 0.24,
            output_price_per_mtok: 0.24,
            capabilities: caps(&[ModelCapability::ToolUse]),
            release_date: None,
        },
        ModelInfo {
            id: "gemma2-9b-it".into(),
            name: "Gemma 2 9B (Groq)".into(),
            provider: "groq".into(),
            provider_type: ProviderType::Groq,
            tier: ModelTier::Budget,
            context_window: 8_192,
            input_price_per_mtok: 0.20,
            output_price_per_mtok: 0.20,
            capabilities: caps(&[]),
            release_date: None,
        },
        // ---- xAI (Grok) ----
        ModelInfo {
            id: "grok-3".into(),
            name: "Grok 3".into(),
            provider: "xai".into(),
            provider_type: ProviderType::XAI,
            tier: ModelTier::Premium,
            context_window: 131_072,
            input_price_per_mtok: 3.00,
            output_price_per_mtok: 15.00,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2025-02-17".into()),
        },
        ModelInfo {
            id: "grok-3-mini".into(),
            name: "Grok 3 Mini".into(),
            provider: "xai".into(),
            provider_type: ProviderType::XAI,
            tier: ModelTier::Mid,
            context_window: 131_072,
            input_price_per_mtok: 0.30,
            output_price_per_mtok: 0.50,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2025-02-17".into()),
        },
        ModelInfo {
            id: "grok-2-1212".into(),
            name: "Grok 2".into(),
            provider: "xai".into(),
            provider_type: ProviderType::XAI,
            tier: ModelTier::Mid,
            context_window: 131_072,
            input_price_per_mtok: 2.00,
            output_price_per_mtok: 10.00,
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2024-12-12".into()),
        },
        // ---- Hugging Face ----
        ModelInfo {
            id: "meta-llama/Llama-3.3-70B-Instruct".into(),
            name: "Llama 3.3 70B (HF)".into(),
            provider: "hugging_face".into(),
            provider_type: ProviderType::HuggingFace,
            tier: ModelTier::Budget,
            context_window: 128_000,
            input_price_per_mtok: 0.0,
            output_price_per_mtok: 0.0,
            capabilities: caps(&[ModelCapability::LongContext]),
            release_date: None,
        },
        ModelInfo {
            id: "mistralai/Mixtral-8x7B-Instruct-v0.1".into(),
            name: "Mixtral 8x7B (HF)".into(),
            provider: "hugging_face".into(),
            provider_type: ProviderType::HuggingFace,
            tier: ModelTier::Budget,
            context_window: 32_768,
            input_price_per_mtok: 0.0,
            output_price_per_mtok: 0.0,
            capabilities: caps(&[]),
            release_date: None,
        },
        ModelInfo {
            id: "microsoft/Phi-3-mini-4k-instruct".into(),
            name: "Phi-3 Mini (HF)".into(),
            provider: "hugging_face".into(),
            provider_type: ProviderType::HuggingFace,
            tier: ModelTier::Free,
            context_window: 4_096,
            input_price_per_mtok: 0.0,
            output_price_per_mtok: 0.0,
            capabilities: caps(&[]),
            release_date: None,
        },
    ]
});

// ---------------------------------------------------------------------------
// Enrichment helper
// ---------------------------------------------------------------------------

/// Look up a model by exact id in the static registry.
///
/// This is used to enrich live-catalog models with known pricing, tier,
/// capabilities, and context-window data that APIs often omit.
pub fn lookup_by_id(id: &str) -> Option<&'static ModelInfo> {
    MODEL_REGISTRY.iter().find(|m| m.id == id)
}

/// Enrich a model from a live catalog with metadata from the static registry.
///
/// If the registry has a matching entry (by exact id), the following fields
/// are overwritten with the registry values (which are typically more accurate
/// than what the provider API returns):
///
/// - `tier` (catalog APIs rarely expose this)
/// - `input_price_per_mtok` / `output_price_per_mtok` (only if the registry
///   value is nonzero — catalogs sometimes return 0 for paid models)
/// - `capabilities` (only if the registry entry has capabilities and the
///   catalog entry does not)
/// - `context_window` (only if the registry value is larger — catalog APIs
///   sometimes report a lower default)
///
/// The `name` field is also updated if the registry provides a friendlier
/// display name (i.e. differs from the raw `id`).
pub fn enrich_from_registry(model: &mut ModelInfo) {
    if let Some(reg) = lookup_by_id(&model.id) {
        // Always prefer the registry tier — catalog APIs don't expose this.
        model.tier = reg.tier;

        // Pricing: prefer registry if non-zero (catalogs often return 0).
        if reg.input_price_per_mtok > 0.0 || reg.output_price_per_mtok > 0.0 {
            model.input_price_per_mtok = reg.input_price_per_mtok;
            model.output_price_per_mtok = reg.output_price_per_mtok;
        }

        // Capabilities: prefer registry if it has data and catalog doesn't.
        if model.capabilities.is_empty() && !reg.capabilities.is_empty() {
            model.capabilities = reg.capabilities.clone();
        }

        // Context window: prefer the larger value.
        if reg.context_window > model.context_window {
            model.context_window = reg.context_window;
        }

        // Display name: prefer the registry's friendlier name.
        if reg.name != reg.id {
            model.name = reg.name.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// Resolve a model by exact id or case-insensitive name substring.
pub fn resolve_model(input: &str) -> Option<&'static ModelInfo> {
    let needle = input.trim().to_lowercase();

    // 1. Exact id match
    if let Some(m) = MODEL_REGISTRY.iter().find(|m| m.id == needle) {
        return Some(m);
    }

    // 2. Exact id match against the raw (untrimmed) input for full model IDs
    let trimmed = input.trim();
    if let Some(m) = MODEL_REGISTRY.iter().find(|m| m.id == trimmed) {
        return Some(m);
    }

    // 3. Substring match on id or display name
    if let Some(m) = MODEL_REGISTRY
        .iter()
        .find(|m| m.id.contains(&needle) || m.name.to_lowercase().contains(&needle))
    {
        return Some(m);
    }

    None
}

/// Return all models belonging to a given provider.
pub fn models_for_provider(provider: ProviderType) -> Vec<&'static ModelInfo> {
    MODEL_REGISTRY
        .iter()
        .filter(|m| m.provider_type == provider)
        .collect()
}

/// Return all models at a given tier.
pub fn models_for_tier(tier: ModelTier) -> Vec<&'static ModelInfo> {
    MODEL_REGISTRY.iter().filter(|m| m.tier == tier).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_id_lookup() {
        let m = resolve_model("claude-opus-4-6").unwrap();
        assert_eq!(m.name, "Claude Opus 4.6");
    }

    #[test]
    fn substring_lookup() {
        let m = resolve_model("gpt-4o-mini").unwrap();
        assert_eq!(m.name, "GPT-4o Mini");
    }

    #[test]
    fn name_substring_lookup() {
        let m = resolve_model("Sonnet 4.5").unwrap();
        assert_eq!(m.id, "claude-sonnet-4-5-20250929");
    }

    #[test]
    fn provider_filter() {
        let anthropic = models_for_provider(ProviderType::Anthropic);
        assert_eq!(anthropic.len(), 7);
        assert!(
            anthropic
                .iter()
                .all(|m| m.provider_type == ProviderType::Anthropic)
        );

        let openai = models_for_provider(ProviderType::OpenAI);
        assert_eq!(openai.len(), 18);
        assert!(
            openai
                .iter()
                .all(|m| m.provider_type == ProviderType::OpenAI)
        );

        let openrouter = models_for_provider(ProviderType::OpenRouter);
        assert_eq!(openrouter.len(), 12);
        assert!(
            openrouter
                .iter()
                .all(|m| m.provider_type == ProviderType::OpenRouter)
        );

        let google = models_for_provider(ProviderType::Google);
        assert_eq!(google.len(), 8);
        assert!(
            google
                .iter()
                .all(|m| m.provider_type == ProviderType::Google)
        );

        let groq = models_for_provider(ProviderType::Groq);
        assert_eq!(groq.len(), 4);
        assert!(groq.iter().all(|m| m.provider_type == ProviderType::Groq));

        let xai = models_for_provider(ProviderType::XAI);
        assert_eq!(xai.len(), 3);
        assert!(xai.iter().all(|m| m.provider_type == ProviderType::XAI));

        let hf = models_for_provider(ProviderType::HuggingFace);
        assert_eq!(hf.len(), 3);
        assert!(
            hf.iter()
                .all(|m| m.provider_type == ProviderType::HuggingFace)
        );
    }

    #[test]
    fn tier_filter() {
        let budget = models_for_tier(ModelTier::Budget);
        assert!(!budget.is_empty());
        assert!(budget.iter().all(|m| m.tier == ModelTier::Budget));
    }

    #[test]
    fn capabilities_on_models() {
        let opus = resolve_model("claude-opus-4-6").unwrap();
        assert!(opus.capabilities.has(ModelCapability::ToolUse));
        assert!(opus.capabilities.has(ModelCapability::Vision));
        assert!(opus.capabilities.has(ModelCapability::ExtendedThinking));

        let gpt4o = resolve_model("gpt-4o").unwrap();
        assert!(gpt4o.capabilities.has(ModelCapability::NativeAgents));
        assert!(gpt4o.capabilities.has(ModelCapability::Vision));

        let phi3 = resolve_model("Phi-3").unwrap();
        assert!(phi3.capabilities.is_empty());
    }

    #[test]
    fn provider_capabilities_check() {
        let openai_caps = provider_capabilities(ProviderType::OpenAI);
        assert!(openai_caps.contains(&ModelCapability::NativeAgents));

        let anthropic_caps = provider_capabilities(ProviderType::Anthropic);
        assert!(anthropic_caps.contains(&ModelCapability::ToolUse));
        assert!(!anthropic_caps.contains(&ModelCapability::NativeAgents));

        let local_caps = provider_capabilities(ProviderType::Ollama);
        assert!(local_caps.is_empty());
    }
}
