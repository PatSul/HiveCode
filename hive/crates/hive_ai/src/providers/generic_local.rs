//! Generic local provider for any OpenAI-compatible server.
//!
//! Supports vLLM, LocalAI, llama.cpp, text-generation-webui, and other servers
//! that implement the OpenAI chat completions format. Uses the shared SSE
//! parsing from [`super::openai_sse`].
//!
//! Because not all backends support `/v1/models`, an optional `default_model`
//! can be configured as a fallback.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::openai_sse::{self, ChatCompletionResponse};
use super::{AiProvider, ProviderError};
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelInfo, ModelTier, ProviderType,
    StreamChunk, TokenUsage,
};

// ---------------------------------------------------------------------------
// Wire types (serialization only)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct GenericLocalChatRequest {
    model: String,
    messages: Vec<GenericLocalMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct GenericLocalMessage {
    role: String,
    content: String,
}

// ---------------------------------------------------------------------------
// Models listing response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Option<Vec<ModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// A catch-all provider for OpenAI-compatible local servers (vLLM, LocalAI,
/// llama.cpp, text-generation-webui, etc.).
pub struct GenericLocalProvider {
    base_url: String,
    default_model: Option<String>,
    client: reqwest::Client,
}

impl GenericLocalProvider {
    /// Create a new generic local provider with just a base URL.
    ///
    /// Defaults to `http://localhost:8080`.
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            default_model: None,
            client: reqwest::Client::new(),
        }
    }

    /// Create a provider with a configurable default model name.
    ///
    /// The `default_model` is returned from `get_models()` when the server
    /// does not support the `/v1/models` listing endpoint.
    pub fn with_default_model(base_url: String, default_model: String) -> Self {
        Self {
            base_url,
            default_model: if default_model.is_empty() {
                None
            } else {
                Some(default_model)
            },
            client: reqwest::Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Convert generic messages to the OpenAI wire format.
    fn convert_messages(
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Vec<GenericLocalMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(GenericLocalMessage {
                role: "system".into(),
                content: sys.to_string(),
            });
        }

        for m in messages {
            out.push(GenericLocalMessage {
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
    fn build_body(&self, request: &ChatRequest, stream: bool) -> GenericLocalChatRequest {
        GenericLocalChatRequest {
            model: request.model.clone(),
            messages: Self::convert_messages(&request.messages, request.system_prompt.as_deref()),
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
    /// No API key is sent -- local servers typically don't require one.
    async fn post_completions(
        &self,
        body: &GenericLocalChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        {
            return Err(ProviderError::Timeout);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "Generic local API error {status}: {text}"
            )));
        }

        Ok(resp)
    }

    /// Try to query `/v1/models` and return parsed model entries.
    ///
    /// Returns `None` if the endpoint is unreachable or returns an error,
    /// so callers can fall back to the configured `default_model`.
    async fn fetch_remote_models(&self) -> Option<Vec<ModelEntry>> {
        let url = format!("{}/v1/models", self.base_url);
        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                debug!("Generic local /v1/models returned {}", r.status());
                return None;
            }
            Err(e) => {
                debug!("Generic local server not reachable for model listing: {e}");
                return None;
            }
        };

        match resp.json::<ModelsResponse>().await {
            Ok(data) => Some(data.data.unwrap_or_default()),
            Err(e) => {
                warn!("Failed to parse generic local models response: {e}");
                None
            }
        }
    }
}

#[async_trait]
impl AiProvider for GenericLocalProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::GenericLocal
    }

    fn name(&self) -> &str {
        "Generic Local"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/v1/models", self.base_url);
        matches!(
            self.client.get(&url).timeout(std::time::Duration::from_secs(2)).send().await,
            Ok(r) if r.status().is_success()
        )
    }

    /// Query `/v1/models` if available. Falls back to the configured
    /// `default_model` when the endpoint does not respond.
    async fn get_models(&self) -> Vec<ModelInfo> {
        // Try the remote endpoint first.
        if let Some(entries) = self.fetch_remote_models().await {
            if !entries.is_empty() {
                return entries
                    .into_iter()
                    .map(|m| ModelInfo {
                        id: m.id.clone(),
                        name: m.id,
                        provider: "generic_local".into(),
                        provider_type: ProviderType::GenericLocal,
                        tier: ModelTier::Free,
                        context_window: 8192,
                        input_price_per_mtok: 0.0,
                        output_price_per_mtok: 0.0,
                        capabilities: Default::default(),
                    })
                    .collect();
            }
        }

        // Fall back to default_model if configured.
        match &self.default_model {
            Some(model) => vec![ModelInfo {
                id: model.clone(),
                name: model.clone(),
                provider: "generic_local".into(),
                provider_type: ProviderType::GenericLocal,
                tier: ModelTier::Free,
                context_window: 8192,
                input_price_per_mtok: 0.0,
                output_price_per_mtok: 0.0,
                capabilities: Default::default(),
            }],
            None => vec![],
        }
    }

    /// Non-streaming chat completion.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let body = self.build_body(request, false);
        let resp = self.post_completions(&body).await?;

        let data: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("JSON parse error: {e}")))?;

        let choice = data
            .choices
            .first()
            .ok_or_else(|| ProviderError::Other("No choices in generic local response".into()))?;

        let content = choice.message.content.clone().unwrap_or_default();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
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
            max_tokens: 2048,
            temperature: Some(0.5),
            system_prompt: None,
            tools: None,
        }
    }

    #[test]
    fn provider_metadata() {
        let provider = GenericLocalProvider::new("http://localhost:8080".into());
        assert_eq!(provider.provider_type(), ProviderType::GenericLocal);
        assert_eq!(provider.name(), "Generic Local");
    }

    #[test]
    fn stores_base_url() {
        let provider = GenericLocalProvider::new("http://10.0.0.5:5000".into());
        assert_eq!(provider.base_url, "http://10.0.0.5:5000");
    }

    #[test]
    fn with_default_model_stores_model() {
        let provider = GenericLocalProvider::with_default_model(
            "http://localhost:8080".into(),
            "my-local-model".into(),
        );
        assert_eq!(provider.default_model.as_deref(), Some("my-local-model"));
    }

    #[test]
    fn with_default_model_empty_string_is_none() {
        let provider =
            GenericLocalProvider::with_default_model("http://localhost:8080".into(), String::new());
        assert!(provider.default_model.is_none());
    }

    #[test]
    fn build_body_basic() {
        let provider = GenericLocalProvider::new("http://localhost:8080".into());
        let req = sample_request("llama-3.1-8b");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "llama-3.1-8b");
        assert_eq!(body.max_tokens, Some(2048));
        assert_eq!(body.temperature, Some(0.5));
        assert!(!body.stream);
        assert!(body.stream_options.is_none());
    }

    #[test]
    fn build_body_stream_includes_usage_option() {
        let provider = GenericLocalProvider::new("http://localhost:8080".into());
        let req = sample_request("llama-3.1-8b");
        let body = provider.build_body(&req, true);

        assert!(body.stream);
        assert!(body.stream_options.is_some());
        assert!(body.stream_options.unwrap().include_usage);
    }

    #[test]
    fn build_body_with_system_prompt() {
        let provider = GenericLocalProvider::new("http://localhost:8080".into());
        let mut req = sample_request("llama-3.1-8b");
        req.system_prompt = Some("Be helpful.".into());
        let body = provider.build_body(&req, false);

        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[0].content, "Be helpful.");
        assert_eq!(body.messages[1].role, "user");
    }

    #[test]
    fn request_body_serializes_correctly() {
        let provider = GenericLocalProvider::new("http://localhost:8080".into());
        let req = sample_request("llama-3.1-8b");
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "llama-3.1-8b");
        assert_eq!(json["max_tokens"], 2048);
        assert_eq!(json["temperature"], 0.5);
        assert_eq!(json["stream"], false);
        assert!(json.get("stream_options").is_none());
    }

    #[test]
    fn convert_messages_all_roles() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: "System msg".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: "User msg".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: "Assistant msg".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: MessageRole::Error,
                content: "Error msg".into(),
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let converted = GenericLocalProvider::convert_messages(&messages, None);

        assert_eq!(converted.len(), 4);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
        assert_eq!(converted[3].role, "user"); // Error maps to user
    }

    #[tokio::test]
    async fn stream_chat_parses_mock_sse() {
        let sse_payload = concat!(
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{\"content\":\" there\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"gen-1\",\"choices\":[{\"delta\":{},\"index\":0,\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":6,\"completion_tokens\":2,\"total_tokens\":8}}\n\n",
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

        assert!(
            chunks.len() >= 2,
            "expected at least 2 chunks, got {}",
            chunks.len()
        );
        assert_eq!(chunks[0].content, "Hi");
        assert!(!chunks[0].done);
        assert_eq!(chunks[1].content, " there");
        assert!(!chunks[1].done);

        let last = chunks.last().unwrap();
        assert!(last.done);
        let usage = last.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 6);
        assert_eq!(usage.completion_tokens, 2);
        assert_eq!(usage.total_tokens, 8);
    }

    #[test]
    fn no_temperature_omitted_from_body() {
        let provider = GenericLocalProvider::new("http://localhost:8080".into());
        let mut req = sample_request("test-model");
        req.temperature = None;
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert!(json.get("temperature").is_none());
    }
}
