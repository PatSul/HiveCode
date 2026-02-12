//! LM Studio provider (local OpenAI-compatible server).
//!
//! LM Studio exposes an OpenAI-compatible API at `http://localhost:1234/v1`.
//! No API key is required. Streaming uses SSE and shares the parsing logic in
//! [`super::openai_sse`].

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
struct LMStudioChatRequest {
    model: String,
    messages: Vec<LMStudioMessage>,
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
struct LMStudioMessage {
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

/// LM Studio local provider -- speaks the OpenAI API format at `/v1`.
pub struct LMStudioProvider {
    base_url: String,
    client: reqwest::Client,
}

impl LMStudioProvider {
    /// Create a new LM Studio provider.
    ///
    /// Defaults to `http://localhost:1234` when `None` is passed.
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "http://localhost:1234".into()),
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
    ) -> Vec<LMStudioMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(LMStudioMessage {
                role: "system".into(),
                content: sys.to_string(),
            });
        }

        for m in messages {
            out.push(LMStudioMessage {
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
    fn build_body(&self, request: &ChatRequest, stream: bool) -> LMStudioChatRequest {
        LMStudioChatRequest {
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
    /// LM Studio does not require an API key, so no `Authorization` header is
    /// sent.
    async fn post_completions(
        &self,
        body: &LMStudioChatRequest,
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
                "LM Studio API error {status}: {text}"
            )));
        }

        Ok(resp)
    }
}

#[async_trait]
impl AiProvider for LMStudioProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::LMStudio
    }

    fn name(&self) -> &str {
        "LM Studio"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/v1/models", self.base_url);
        matches!(
            self.client.get(&url).timeout(std::time::Duration::from_secs(2)).send().await,
            Ok(r) if r.status().is_success()
        )
    }

    /// Query the `/v1/models` endpoint and return all loaded models.
    async fn get_models(&self) -> Vec<ModelInfo> {
        let url = format!("{}/v1/models", self.base_url);
        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                warn!("LM Studio /v1/models returned {}", r.status());
                return vec![];
            }
            Err(e) => {
                debug!("LM Studio not reachable: {e}");
                return vec![];
            }
        };

        let data: ModelsResponse = match resp.json().await {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to parse LM Studio models response: {e}");
                return vec![];
            }
        };

        data.data
            .unwrap_or_default()
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: m.id,
                provider: "lmstudio".into(),
                provider_type: ProviderType::LMStudio,
                tier: ModelTier::Free,
                context_window: 8192,
                input_price_per_mtok: 0.0,
                output_price_per_mtok: 0.0,
                capabilities: Default::default(),
            })
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
            ProviderError::Other("No choices in LM Studio response".into())
        })?;

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
            max_tokens: 1024,
            temperature: Some(0.7),
            system_prompt: None,
            tools: None,
        }
    }

    #[test]
    fn provider_metadata() {
        let provider = LMStudioProvider::new(None);
        assert_eq!(provider.provider_type(), ProviderType::LMStudio);
        assert_eq!(provider.name(), "LM Studio");
    }

    #[test]
    fn default_base_url() {
        let provider = LMStudioProvider::new(None);
        assert_eq!(provider.base_url, "http://localhost:1234");
    }

    #[test]
    fn custom_base_url() {
        let provider = LMStudioProvider::new(Some("http://192.168.1.100:1234".into()));
        assert_eq!(provider.base_url, "http://192.168.1.100:1234");
    }

    #[test]
    fn build_body_basic() {
        let provider = LMStudioProvider::new(None);
        let req = sample_request("qwen2.5-coder-7b");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "qwen2.5-coder-7b");
        assert_eq!(body.max_tokens, Some(1024));
        assert_eq!(body.temperature, Some(0.7));
        assert!(!body.stream);
        assert!(body.stream_options.is_none());
    }

    #[test]
    fn build_body_stream_includes_usage_option() {
        let provider = LMStudioProvider::new(None);
        let req = sample_request("llama-3.1-8b");
        let body = provider.build_body(&req, true);

        assert!(body.stream);
        assert!(body.stream_options.is_some());
        assert!(body.stream_options.unwrap().include_usage);
    }

    #[test]
    fn build_body_with_system_prompt() {
        let provider = LMStudioProvider::new(None);
        let mut req = sample_request("qwen2.5-coder-7b");
        req.system_prompt = Some("You are a coding assistant.".into());
        let body = provider.build_body(&req, false);

        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[0].content, "You are a coding assistant.");
        assert_eq!(body.messages[1].role, "user");
    }

    #[test]
    fn request_body_serializes_correctly() {
        let provider = LMStudioProvider::new(None);
        let req = sample_request("qwen2.5-coder-7b");
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "qwen2.5-coder-7b");
        assert_eq!(json["max_tokens"], 1024);
        let temp = json["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.001, "temperature was {temp}");
        assert_eq!(json["stream"], false);
        // stream_options should not appear when not streaming.
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

        let converted = LMStudioProvider::convert_messages(&messages, None);

        assert_eq!(converted.len(), 4);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
        assert_eq!(converted[3].role, "user"); // Error maps to user
    }

    #[tokio::test]
    async fn stream_chat_parses_mock_sse() {
        let sse_payload = concat!(
            "data: {\"id\":\"lms-1\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"lms-1\",\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"lms-1\",\"choices\":[{\"delta\":{\"content\":\" world\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"lms-1\",\"choices\":[{\"delta\":{},\"index\":0,\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":2,\"total_tokens\":10}}\n\n",
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

        assert!(chunks.len() >= 2, "expected at least 2 chunks, got {}", chunks.len());
        assert_eq!(chunks[0].content, "Hello");
        assert!(!chunks[0].done);
        assert_eq!(chunks[1].content, " world");
        assert!(!chunks[1].done);

        let last = chunks.last().unwrap();
        assert!(last.done);
        let usage = last.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 8);
        assert_eq!(usage.completion_tokens, 2);
        assert_eq!(usage.total_tokens, 10);
    }

    #[test]
    fn no_temperature_omitted_from_body() {
        let provider = LMStudioProvider::new(None);
        let mut req = sample_request("test-model");
        req.temperature = None;
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        // temperature should not appear in JSON when None.
        assert!(json.get("temperature").is_none());
    }
}
