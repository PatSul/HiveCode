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
        ProviderType::Anthropic => [
            ModelCapability::ToolUse,
            ModelCapability::StructuredOutput,
        ]
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
        ProviderType::Google => [
            ModelCapability::ToolUse,
            ModelCapability::StructuredOutput,
        ]
        .into_iter()
        .collect(),
        ProviderType::Groq => [ModelCapability::ToolUse].into_iter().collect(),
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
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::LongContext,
            ]),
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
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::LongContext,
            ]),
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
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::StructuredOutput,
            ]),
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
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::LongContext,
            ]),
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
        },
        // ---- Google Gemini (direct API) ----
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
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::LongContext,
            ]),
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
            capabilities: caps(&[
                ModelCapability::ToolUse,
                ModelCapability::LongContext,
            ]),
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
        },
    ]
});

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
    if let Some(m) = MODEL_REGISTRY.iter().find(|m| {
        m.id.contains(&needle) || m.name.to_lowercase().contains(&needle)
    }) {
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
    MODEL_REGISTRY
        .iter()
        .filter(|m| m.tier == tier)
        .collect()
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
        assert!(anthropic.iter().all(|m| m.provider_type == ProviderType::Anthropic));

        let openai = models_for_provider(ProviderType::OpenAI);
        assert_eq!(openai.len(), 12);
        assert!(openai.iter().all(|m| m.provider_type == ProviderType::OpenAI));

        let openrouter = models_for_provider(ProviderType::OpenRouter);
        assert_eq!(openrouter.len(), 12);
        assert!(openrouter.iter().all(|m| m.provider_type == ProviderType::OpenRouter));

        let google = models_for_provider(ProviderType::Google);
        assert_eq!(google.len(), 6);
        assert!(google.iter().all(|m| m.provider_type == ProviderType::Google));

        let groq = models_for_provider(ProviderType::Groq);
        assert_eq!(groq.len(), 4);
        assert!(groq.iter().all(|m| m.provider_type == ProviderType::Groq));

        let hf = models_for_provider(ProviderType::HuggingFace);
        assert_eq!(hf.len(), 3);
        assert!(hf.iter().all(|m| m.provider_type == ProviderType::HuggingFace));
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
