//! AI provider trait and implementations.
//!
//! Each provider module exposes a struct that implements [`AiProvider`].

pub mod anthropic;
pub mod gemini;
pub mod generic_local;
pub mod groq;
pub mod huggingface;
pub mod litellm;
pub mod lmstudio;
pub mod ollama;
pub mod openai;
pub(crate) mod openai_sse;
pub mod openrouter;
pub mod openrouter_catalog;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::{ChatRequest, ChatResponse, ModelInfo, ProviderType, StreamChunk};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that any provider may return.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Rate limited")]
    RateLimit,

    #[error("Invalid API key")]
    InvalidKey,

    #[error("Model not available: {0}")]
    ModelUnavailable(String),

    #[error("Timeout")]
    Timeout,

    #[error("Budget exceeded")]
    BudgetExceeded,

    #[error("Provider error: {0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Unified interface for all AI backends (cloud and local).
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Which kind of provider this is.
    fn provider_type(&self) -> ProviderType;

    /// Human-readable display name.
    fn name(&self) -> &str;

    /// Quick health-check (e.g. ping the API).
    async fn is_available(&self) -> bool;

    /// List models the provider currently exposes.
    async fn get_models(&self) -> Vec<ModelInfo>;

    /// Non-streaming completion.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError>;

    /// Streaming completion -- returns a channel that yields chunks.
    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError>;
}
