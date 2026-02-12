//! OpenRouter provider (multi-model gateway).
//!
//! OpenRouter uses the same chat completions API format as OpenAI, so this
//! provider reuses the shared SSE parsing from [`super::openai_sse`].  The key
//! differences are:
//!
//! - Base URL: `https://openrouter.ai/api/v1`
//! - Extra headers: `HTTP-Referer` and `X-Title`
//! - Model IDs use `org/name` format (e.g. `deepseek/deepseek-chat`)

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::mpsc;

use super::openai_sse::{self, ChatCompletionResponse};
use super::{AiProvider, ProviderError};
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelInfo, ProviderType, StreamChunk,
    TokenUsage,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const HTTP_REFERER: &str = "https://hive.airglow.studio";
const X_TITLE: &str = "Hive";

// ---------------------------------------------------------------------------
// Wire types (serialization only)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OpenRouterChatRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    /// When streaming, ask the API to include usage in the final chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct OpenRouterMessage {
    role: String,
    content: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// OpenRouter API provider -- routes to 200+ upstream models via a unified
/// OpenAI-compatible gateway.
pub struct OpenRouterProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url: DEFAULT_BASE_URL.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Create a provider with a custom base URL.
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url,
            client: reqwest::Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Convert generic messages to the OpenRouter wire format.
    fn convert_messages(
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Vec<OpenRouterMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(OpenRouterMessage {
                role: "system".into(),
                content: sys.to_string(),
            });
        }

        for m in messages {
            out.push(OpenRouterMessage {
                role: match m.role {
                    crate::types::MessageRole::User => "user".into(),
                    crate::types::MessageRole::Assistant => "assistant".into(),
                    crate::types::MessageRole::System => "system".into(),
                    crate::types::MessageRole::Error => "user".into(),
                    crate::types::MessageRole::Tool => "user".into(),
                },
                content: m.content.clone(),
            });
        }

        out
    }

    /// Build the JSON request body.
    fn build_body(&self, request: &ChatRequest, stream: bool) -> OpenRouterChatRequest {
        OpenRouterChatRequest {
            model: request.model.clone(),
            messages: Self::convert_messages(
                &request.messages,
                request.system_prompt.as_deref(),
            ),
            stream,
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            stream_options: if stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
        }
    }

    /// Get the API key or return an error.
    fn require_key(&self) -> Result<&str, ProviderError> {
        self.api_key
            .as_deref()
            .ok_or(ProviderError::InvalidKey)
    }

    /// Send a POST to the chat completions endpoint with OpenRouter-specific
    /// headers.
    async fn post_completions(
        &self,
        body: &OpenRouterChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let key = self.require_key()?;
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", HTTP_REFERER)
            .header("X-Title", X_TITLE)
            .json(body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ProviderError::InvalidKey);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimit);
        }
        if status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        {
            return Err(ProviderError::Timeout);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "OpenRouter API error {status}: {text}"
            )));
        }

        Ok(resp)
    }
}

#[async_trait]
impl AiProvider for OpenRouterProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenRouter
    }

    fn name(&self) -> &str {
        "OpenRouter"
    }

    async fn is_available(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        crate::model_registry::models_for_provider(ProviderType::OpenRouter)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Non-streaming chat completion.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let body = self.build_body(request, false);
        let resp = self.post_completions(&body).await?;

        let data: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("JSON parse error: {e}")))?;

        let choice = data.choices.first().ok_or_else(|| {
            ProviderError::Other("No choices in OpenRouter response".into())
        })?;

        let content = choice.message.content.clone().unwrap_or_default();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        };

        let usage = data.usage.map(|u| {
            let p = u.prompt_tokens.unwrap_or(0);
            let c = u.completion_tokens.unwrap_or(0);
            TokenUsage {
                prompt_tokens: p,
                completion_tokens: c,
                total_tokens: u.total_tokens.unwrap_or(p + c),
            }
        }).unwrap_or_default();

        Ok(ChatResponse {
            content,
            model: data.model,
            usage,
            finish_reason,
            thinking: None,
            tool_calls: None,
        })
    }

    /// Streaming chat completion via SSE.
    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let body = self.build_body(request, true);
        let resp = self.post_completions(&body).await?;

        let (tx, rx) = mpsc::channel::<StreamChunk>(64);

        tokio::spawn(async move {
            openai_sse::drive_sse_stream(resp, tx).await;
        });

        Ok(rx)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, ChatRequest, MessageRole};

    fn sample_request(model: &str) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Hello".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            }],
            model: model.into(),
            max_tokens: 2048,
            temperature: Some(0.5),
            system_prompt: None,
            tools: None,
        }
    }

    #[test]
    fn build_body_basic() {
        let provider = OpenRouterProvider::new("or-test".into());
        let req = sample_request("deepseek/deepseek-chat");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "deepseek/deepseek-chat");
        assert_eq!(body.max_tokens, Some(2048));
        assert_eq!(body.temperature, Some(0.5));
        assert!(!body.stream);
        assert!(body.stream_options.is_none());
    }

    #[test]
    fn build_body_stream_includes_usage_option() {
        let provider = OpenRouterProvider::new("or-test".into());
        let req = sample_request("anthropic/claude-sonnet-4");
        let body = provider.build_body(&req, true);

        assert!(body.stream);
        assert!(body.stream_options.is_some());
        assert!(body.stream_options.unwrap().include_usage);
    }

    #[test]
    fn build_body_with_system_prompt() {
        let provider = OpenRouterProvider::new("or-test".into());
        let mut req = sample_request("deepseek/deepseek-chat");
        req.system_prompt = Some("Be concise.".into());
        let body = provider.build_body(&req, false);

        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[0].content, "Be concise.");
        assert_eq!(body.messages[1].role, "user");
    }

    #[test]
    fn provider_metadata() {
        let provider = OpenRouterProvider::new("or-test".into());
        assert_eq!(provider.provider_type(), ProviderType::OpenRouter);
        assert_eq!(provider.name(), "OpenRouter");
    }

    #[tokio::test]
    async fn is_available_with_key() {
        let provider = OpenRouterProvider::new("or-test".into());
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn is_available_without_key() {
        let provider = OpenRouterProvider::new(String::new());
        assert!(!provider.is_available().await);
    }

    #[test]
    fn require_key_returns_error_when_missing() {
        let provider = OpenRouterProvider::new(String::new());
        assert!(provider.require_key().is_err());
    }

    #[test]
    fn request_body_serializes_correctly() {
        let provider = OpenRouterProvider::new("or-test".into());
        let req = sample_request("deepseek/deepseek-chat");
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "deepseek/deepseek-chat");
        assert_eq!(json["max_tokens"], 2048);
        assert_eq!(json["temperature"], 0.5);
        assert_eq!(json["stream"], false);
    }

    #[test]
    fn openrouter_headers_are_correct() {
        assert_eq!(HTTP_REFERER, "https://hive.airglow.studio");
        assert_eq!(X_TITLE, "Hive");
    }

    #[tokio::test]
    async fn stream_chat_parses_mock_sse() {
        let sse_payload = concat!(
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{\"content\":\"Deep\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{\"content\":\"Seek\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{},\"index\":0,\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":4,\"total_tokens\":14}}\n\n",
            "data: [DONE]\n\n",
        );

        let body_stream = futures::stream::once(async move {
            Ok::<_, reqwest::Error>(bytes::Bytes::from(sse_payload))
        });
        let resp = http::Response::builder()
            .status(200)
            .body(reqwest::Body::wrap_stream(body_stream))
            .unwrap();
        let resp = reqwest::Response::from(resp);

        let (tx, mut rx) = mpsc::channel::<StreamChunk>(32);

        tokio::spawn(async move {
            openai_sse::drive_sse_stream(resp, tx).await;
        });

        let mut chunks = Vec::new();
        while let Some(chunk) = rx.recv().await {
            chunks.push(chunk);
        }

        assert_eq!(chunks[0].content, "Deep");
        assert!(!chunks[0].done);
        assert_eq!(chunks[1].content, "Seek");
        assert!(!chunks[1].done);

        let last = chunks.last().unwrap();
        assert!(last.done);
        let usage = last.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 4);
        assert_eq!(usage.total_tokens, 14);
    }
}
