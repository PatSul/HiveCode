//! LiteLLM provider (unified LLM proxy gateway).
//!
//! LiteLLM provides an OpenAI-compatible API that can route requests to
//! any backend (Anthropic, OpenAI, Cohere, etc.) through a single proxy.
//! Default proxy address is `http://localhost:4000`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::debug;

use super::openai_sse::{self, ChatCompletionResponse};
use super::{AiProvider, ProviderError};
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelInfo, ModelTier, ProviderType,
    StreamChunk, TokenUsage,
};

// ---------------------------------------------------------------------------
// Default
// ---------------------------------------------------------------------------

/// Default LiteLLM proxy address.
const DEFAULT_BASE_URL: &str = "http://localhost:4000";

// ---------------------------------------------------------------------------
// Wire types (serialization only)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct LiteLLMChatRequest {
    model: String,
    messages: Vec<LiteLLMMessage>,
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
struct LiteLLMMessage {
    role: String,
    content: String,
}

// ---------------------------------------------------------------------------
// Wire types for model discovery (deserialization only)
// ---------------------------------------------------------------------------

/// Response from GET `/model/info`.
#[derive(Debug, Deserialize)]
struct ModelInfoResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    model_name: String,
    #[serde(default)]
    model_info: Option<ModelEntryInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelEntryInfo {
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    input_cost_per_token: Option<f64>,
    #[serde(default)]
    output_cost_per_token: Option<f64>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// LiteLLM proxy provider -- routes to any LLM backend via a unified
/// OpenAI-compatible API.
pub struct LiteLLMProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl LiteLLMProvider {
    /// Create a new LiteLLM provider without authentication.
    ///
    /// `base_url` defaults to `http://localhost:4000` when `None`.
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            api_key: None,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            client: reqwest::Client::new(),
        }
    }

    /// Create a provider with an API key for authenticated proxies.
    ///
    /// `base_url` defaults to `http://localhost:4000` when `None`.
    pub fn with_api_key(api_key: String, base_url: Option<String>) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            client: reqwest::Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Convert generic messages to the LiteLLM wire format.
    fn convert_messages(
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Vec<LiteLLMMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(LiteLLMMessage {
                role: "system".into(),
                content: sys.to_string(),
            });
        }

        for m in messages {
            out.push(LiteLLMMessage {
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
    fn build_body(&self, request: &ChatRequest, stream: bool) -> LiteLLMChatRequest {
        LiteLLMChatRequest {
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

    /// Send a POST to the chat completions endpoint.
    ///
    /// Authorization header is only added when an API key is configured.
    async fn post_completions(
        &self,
        body: &LiteLLMChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(body);

        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {key}"));
        }

        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        // Map HTTP error codes to typed errors.
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
                "LiteLLM API error {status}: {text}"
            )));
        }

        Ok(resp)
    }

    /// Fetch available models from the LiteLLM `/model/info` endpoint.
    async fn fetch_models(&self) -> Vec<ModelInfo> {
        let url = format!("{}/model/info", self.base_url);

        let mut req_builder = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {key}"));
        }

        let resp = match req_builder.send().await {
            Ok(r) => r,
            Err(e) => {
                debug!("LiteLLM model discovery failed: {e}");
                return Vec::new();
            }
        };

        if !resp.status().is_success() {
            debug!(
                "LiteLLM /model/info returned status {}",
                resp.status()
            );
            return Vec::new();
        }

        let info: ModelInfoResponse = match resp.json().await {
            Ok(i) => i,
            Err(e) => {
                debug!("LiteLLM /model/info JSON parse error: {e}");
                return Vec::new();
            }
        };

        info.data
            .into_iter()
            .map(|entry| {
                let context_window = entry
                    .model_info
                    .as_ref()
                    .and_then(|i| i.max_tokens)
                    .unwrap_or(4096);

                // LiteLLM reports cost per token; convert to per million tokens.
                let input_price_per_mtok = entry
                    .model_info
                    .as_ref()
                    .and_then(|i| i.input_cost_per_token)
                    .map(|c| c * 1_000_000.0)
                    .unwrap_or(0.0);

                let output_price_per_mtok = entry
                    .model_info
                    .as_ref()
                    .and_then(|i| i.output_cost_per_token)
                    .map(|c| c * 1_000_000.0)
                    .unwrap_or(0.0);

                ModelInfo {
                    id: entry.model_name.clone(),
                    name: entry.model_name,
                    provider: "LiteLLM".into(),
                    provider_type: ProviderType::LiteLLM,
                    tier: ModelTier::Mid,
                    context_window,
                    input_price_per_mtok,
                    output_price_per_mtok,
                    capabilities: Default::default(),
                }
            })
            .collect()
    }
}

#[async_trait]
impl AiProvider for LiteLLMProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::LiteLLM
    }

    fn name(&self) -> &str {
        "LiteLLM"
    }

    /// LiteLLM proxy may or may not require auth, so we try a health check.
    /// If the check fails (network error, timeout), we return `true` anyway
    /// to avoid blocking the user when the proxy is simply slow.
    async fn is_available(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        let result = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;

        match result {
            Ok(resp) => resp.status().is_success(),
            // Network error or timeout -- assume reachable so the user can try.
            Err(_) => true,
        }
    }

    /// Discover models from the LiteLLM proxy. Returns an empty list on failure.
    async fn get_models(&self) -> Vec<ModelInfo> {
        self.fetch_models().await
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
            ProviderError::Other("No choices in LiteLLM response".into())
        })?;

        let content = choice.message.content.clone().unwrap_or_default();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        };

        let usage = data
            .usage
            .map(|u| {
                let p = u.prompt_tokens.unwrap_or(0);
                let c = u.completion_tokens.unwrap_or(0);
                TokenUsage {
                    prompt_tokens: p,
                    completion_tokens: c,
                    total_tokens: u.total_tokens.unwrap_or(p + c),
                }
            })
            .unwrap_or_default();

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
            max_tokens: 1024,
            temperature: Some(0.7),
            system_prompt: None,
            tools: None,
        }
    }

    #[test]
    fn build_body_standard() {
        let provider = LiteLLMProvider::new(None);
        let req = sample_request("gpt-4o");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "gpt-4o");
        assert_eq!(body.max_tokens, Some(1024));
        assert_eq!(body.temperature, Some(0.7));
        assert!(!body.stream);
        assert!(body.stream_options.is_none());
    }

    #[test]
    fn build_body_stream() {
        let provider = LiteLLMProvider::new(None);
        let req = sample_request("claude-3-opus");
        let body = provider.build_body(&req, true);

        assert!(body.stream);
        assert!(body.stream_options.is_some());
        assert!(body.stream_options.unwrap().include_usage);
    }

    #[test]
    fn build_body_with_system_prompt() {
        let provider = LiteLLMProvider::new(None);
        let mut req = sample_request("gpt-4o");
        req.system_prompt = Some("You are helpful.".into());
        let body = provider.build_body(&req, false);

        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[0].content, "You are helpful.");
        assert_eq!(body.messages[1].role, "user");
    }

    #[test]
    fn provider_metadata() {
        let provider = LiteLLMProvider::new(None);
        assert_eq!(provider.provider_type(), ProviderType::LiteLLM);
        assert_eq!(provider.name(), "LiteLLM");
    }

    #[test]
    fn default_base_url() {
        let provider = LiteLLMProvider::new(None);
        assert_eq!(provider.base_url, "http://localhost:4000");
    }

    #[test]
    fn custom_base_url() {
        let provider =
            LiteLLMProvider::new(Some("https://litellm.example.com".into()));
        assert_eq!(provider.base_url, "https://litellm.example.com");
    }

    #[tokio::test]
    async fn is_available_returns_true_with_no_key() {
        // LiteLLM doesn't require an API key by default.
        // Even when the health check fails (no server), is_available returns true.
        let provider = LiteLLMProvider::new(Some("http://127.0.0.1:1".into()));
        assert!(provider.is_available().await);
    }

    #[test]
    fn request_body_serializes_correctly() {
        let provider = LiteLLMProvider::new(None);
        let req = sample_request("claude-3-sonnet");
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "claude-3-sonnet");
        assert_eq!(json["max_tokens"], 1024);
        // f32 0.7 doesn't round-trip exactly through JSON, so compare approximately.
        let temp = json["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.001, "temperature was {temp}");
        assert_eq!(json["stream"], false);
        // stream_options should not appear for non-streaming requests.
        assert!(json.get("stream_options").is_none());
    }

    #[tokio::test]
    async fn stream_chat_parses_mock_sse() {
        // Build a fake SSE payload identical to the OpenAI format.
        let sse_payload = concat!(
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{\"content\":\" there\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{},\"index\":0,\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3,\"total_tokens\":8}}\n\n",
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

        assert_eq!(chunks[0].content, "Hi");
        assert!(!chunks[0].done);
        assert_eq!(chunks[1].content, " there");
        assert!(!chunks[1].done);

        let last = chunks.last().unwrap();
        assert!(last.done);
        let usage = last.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 5);
        assert_eq!(usage.completion_tokens, 3);
        assert_eq!(usage.total_tokens, 8);
    }

    #[test]
    fn with_api_key_stores_key() {
        let provider =
            LiteLLMProvider::with_api_key("sk-litellm-key".into(), None);
        assert_eq!(provider.api_key.as_deref(), Some("sk-litellm-key"));
        assert_eq!(provider.base_url, "http://localhost:4000");
    }

    #[test]
    fn with_api_key_empty_is_none() {
        let provider = LiteLLMProvider::with_api_key(String::new(), None);
        assert!(provider.api_key.is_none());
    }

    #[test]
    fn with_api_key_custom_base_url() {
        let provider = LiteLLMProvider::with_api_key(
            "sk-key".into(),
            Some("https://proxy.corp.internal".into()),
        );
        assert_eq!(provider.api_key.as_deref(), Some("sk-key"));
        assert_eq!(provider.base_url, "https://proxy.corp.internal");
    }

    #[test]
    fn model_entry_deserializes() {
        let json = r#"{
            "data": [
                {
                    "model_name": "gpt-4",
                    "model_info": {
                        "max_tokens": 8192,
                        "input_cost_per_token": 0.00003,
                        "output_cost_per_token": 0.00006
                    }
                },
                {
                    "model_name": "claude-3-haiku",
                    "model_info": null
                }
            ]
        }"#;

        let resp: ModelInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].model_name, "gpt-4");
        let info = resp.data[0].model_info.as_ref().unwrap();
        assert_eq!(info.max_tokens, Some(8192));
        assert!((info.input_cost_per_token.unwrap() - 0.00003).abs() < 1e-10);
        assert!((info.output_cost_per_token.unwrap() - 0.00006).abs() < 1e-10);

        assert_eq!(resp.data[1].model_name, "claude-3-haiku");
        assert!(resp.data[1].model_info.is_none());
    }

    #[test]
    fn convert_messages_maps_roles() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: "Hello".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: "Hi".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: MessageRole::System,
                content: "Be helpful".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: MessageRole::Error,
                content: "Oops".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let converted = LiteLLMProvider::convert_messages(&messages, None);
        assert_eq!(converted.len(), 4);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[2].role, "system");
        assert_eq!(converted[3].role, "user"); // Error maps to user
    }
}
