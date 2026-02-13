//! Hugging Face Inference API provider.
//!
//! Uses the OpenAI-compatible chat completions endpoint at
//! `api-inference.huggingface.co/v1`. Streaming uses the same SSE
//! wire format parsed by [`super::openai_sse`].

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
// Wire types (serialization only)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct HfChatRequest {
    model: String,
    messages: Vec<HfMessage>,
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
struct HfMessage {
    role: String,
    content: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Hugging Face Inference API provider.
///
/// Supports models hosted on HF such as Llama, Mixtral, and others via the
/// OpenAI-compatible `/v1/chat/completions` endpoint.
pub struct HuggingFaceProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl HuggingFaceProvider {
    /// Create a new Hugging Face provider.
    ///
    /// Pass an empty string for `api_key` to create an unavailable provider
    /// that can still be configured later.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url: "https://api-inference.huggingface.co/v1".into(),
            client: reqwest::Client::new(),
        }
    }

    /// Create a provider with a custom base URL (useful for dedicated
    /// inference endpoints).
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

    /// Convert generic messages to the HF wire format.
    fn convert_messages(messages: &[ChatMessage], system_prompt: Option<&str>) -> Vec<HfMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(HfMessage {
                role: "system".into(),
                content: sys.to_string(),
            });
        }

        for m in messages {
            out.push(HfMessage {
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
    fn build_body(&self, request: &ChatRequest, stream: bool) -> HfChatRequest {
        HfChatRequest {
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

    /// Get the API key or return an error.
    fn require_key(&self) -> Result<&str, ProviderError> {
        self.api_key.as_deref().ok_or(ProviderError::InvalidKey)
    }

    /// Send a POST to the chat completions endpoint.
    async fn post_completions(
        &self,
        body: &HfChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let key = self.require_key()?;
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        // Map HTTP error codes to typed errors.
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
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
                "Hugging Face API error {status}: {text}"
            )));
        }

        Ok(resp)
    }
}

#[async_trait]
impl AiProvider for HuggingFaceProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::HuggingFace
    }

    fn name(&self) -> &str {
        "Hugging Face"
    }

    async fn is_available(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        crate::model_registry::models_for_provider(ProviderType::HuggingFace)
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

        let choice = data
            .choices
            .first()
            .ok_or_else(|| ProviderError::Other("No choices in Hugging Face response".into()))?;

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
        let provider = HuggingFaceProvider::new("hf_test".into());
        let req = sample_request("meta-llama/Llama-3.3-70B-Instruct");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "meta-llama/Llama-3.3-70B-Instruct");
        assert_eq!(body.max_tokens, Some(1024));
        assert_eq!(body.temperature, Some(0.7));
        assert!(!body.stream);
        assert!(body.stream_options.is_none());
    }

    #[test]
    fn build_body_stream_includes_usage() {
        let provider = HuggingFaceProvider::new("hf_test".into());
        let req = sample_request("meta-llama/Llama-3.3-70B-Instruct");
        let body = provider.build_body(&req, true);

        assert!(body.stream);
        assert!(body.stream_options.is_some());
        assert!(body.stream_options.unwrap().include_usage);
    }

    #[test]
    fn build_body_with_system_prompt() {
        let provider = HuggingFaceProvider::new("hf_test".into());
        let mut req = sample_request("mistralai/Mixtral-8x7B-Instruct-v0.1");
        req.system_prompt = Some("You are helpful.".into());
        let body = provider.build_body(&req, false);

        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[0].content, "You are helpful.");
        assert_eq!(body.messages[1].role, "user");
    }

    #[test]
    fn provider_metadata() {
        let provider = HuggingFaceProvider::new("hf_test".into());
        assert_eq!(provider.provider_type(), ProviderType::HuggingFace);
        assert_eq!(provider.name(), "Hugging Face");
    }

    #[tokio::test]
    async fn is_available_with_key() {
        let provider = HuggingFaceProvider::new("hf_test".into());
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn is_available_without_key() {
        let provider = HuggingFaceProvider::new(String::new());
        assert!(!provider.is_available().await);
    }

    #[test]
    fn require_key_returns_error_when_missing() {
        let provider = HuggingFaceProvider::new(String::new());
        assert!(provider.require_key().is_err());
    }

    #[test]
    fn request_body_serializes_correctly() {
        let provider = HuggingFaceProvider::new("hf_test".into());
        let req = sample_request("meta-llama/Llama-3.3-70B-Instruct");
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "meta-llama/Llama-3.3-70B-Instruct");
        assert_eq!(json["max_tokens"], 1024);
        // f32 0.7 doesn't round-trip exactly through JSON, so compare approximately.
        let temp = json["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.001, "temperature was {temp}");
        assert_eq!(json["stream"], false);
    }

    #[tokio::test]
    async fn stream_chat_parses_mock_sse() {
        // Build a fake SSE payload.
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
}
