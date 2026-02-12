use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Represents a chat message in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Error,
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// Specific capabilities a model may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    ToolUse,
    NativeAgents,
    NativeMultiAgent,
    Vision,
    ExtendedThinking,
    CodeExecution,
    StructuredOutput,
    LongContext,
}

/// Set of capabilities for a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    caps: HashSet<ModelCapability>,
}

impl ModelCapabilities {
    pub fn new(caps: &[ModelCapability]) -> Self {
        Self {
            caps: caps.iter().copied().collect(),
        }
    }

    pub fn has(&self, cap: ModelCapability) -> bool {
        self.caps.contains(&cap)
    }

    pub fn supports_native_agents(&self) -> bool {
        self.has(ModelCapability::NativeAgents) || self.has(ModelCapability::NativeMultiAgent)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModelCapability> {
        self.caps.iter()
    }

    pub fn len(&self) -> usize {
        self.caps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }
}

/// Model information for display in the UI and cost tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub provider_type: ProviderType,
    pub tier: ModelTier,
    pub context_window: u32,
    pub input_price_per_mtok: f64,
    pub output_price_per_mtok: f64,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Free,
    Budget,
    Mid,
    Premium,
}

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
        match self {
            Self::Anthropic => write!(f, "anthropic"),
            Self::OpenAI => write!(f, "openai"),
            Self::OpenRouter => write!(f, "openrouter"),
            Self::Google => write!(f, "google"),
            Self::Groq => write!(f, "groq"),
            Self::LiteLLM => write!(f, "litellm"),
            Self::HuggingFace => write!(f, "hugging_face"),
            Self::Ollama => write!(f, "ollama"),
            Self::LMStudio => write!(f, "lmstudio"),
            Self::GenericLocal => write!(f, "generic_local"),
        }
    }
}

// ---------------------------------------------------------------------------
// Provider config
// ---------------------------------------------------------------------------

/// Configuration for an AI provider connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Connectivity
// ---------------------------------------------------------------------------

/// Connectivity state for the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityState {
    Online,
    LocalOnly,
    Offline,
}

// ---------------------------------------------------------------------------
// Request / Response
// ---------------------------------------------------------------------------

/// A request to an AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

fn default_max_tokens() -> u32 {
    4096
}

/// Token usage statistics returned by providers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    Error,
}

/// Complete response from an AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
    pub finish_reason: FinishReason,
    /// Extended thinking / chain-of-thought output, if any.
    #[serde(default)]
    pub thinking: Option<String>,
}

/// A single chunk from a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub content: String,
    pub done: bool,
    #[serde(default)]
    pub thinking: Option<String>,
    /// Usage is typically only present on the final chunk.
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}
