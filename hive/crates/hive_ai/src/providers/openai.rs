//! OpenAI provider (GPT, o1, o3 models).
//!
//! Uses raw `reqwest` with the OpenAI `/chat/completions` endpoint.
//! Streaming uses SSE (`stream: true`) and shares the parsing logic in
//! [`super::openai_sse`].

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::mpsc;

use super::openai_sse::{self, ChatCompletionResponse};
use super::{AiProvider, ProviderError};
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelInfo, ProviderType, StreamChunk,
    ToolCall, TokenUsage,
};

// ---------------------------------------------------------------------------
// Wire types (serialization only)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    /// When streaming, ask the API to include usage in the final chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCallMsg>>,
}

#[derive(Debug, Serialize)]
struct OpenAIToolCallMsg {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIFunctionCall,
}

#[derive(Debug, Serialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// OpenAI API provider (GPT-4o, GPT-4o-mini, o1, o3 models).
pub struct OpenAIProvider {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider.
    ///
    /// Pass an empty string or `None` for `api_key` to create an unavailable
    /// provider that can still be configured later.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url: "https://api.openai.com/v1".into(),
            client: reqwest::Client::new(),
        }
    }

    /// Create a provider with a custom base URL (useful for proxies / Azure).
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

    /// Returns `true` for reasoning models (o1, o3, o4) that don't accept
    /// `temperature` or standard `max_tokens`.
    fn is_reasoning_model(model: &str) -> bool {
        model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4")
    }

    /// Convert generic messages to the OpenAI wire format.
    fn convert_messages(
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Vec<OpenAIMessage> {
        let mut out = Vec::with_capacity(messages.len() + 1);

        if let Some(sys) = system_prompt {
            out.push(OpenAIMessage {
                role: "system".into(),
                content: Some(serde_json::Value::String(sys.to_string())),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        for m in messages {
            let role = match m.role {
                crate::types::MessageRole::User => "user",
                crate::types::MessageRole::Assistant => "assistant",
                crate::types::MessageRole::System => "system",
                crate::types::MessageRole::Error => "user",
                crate::types::MessageRole::Tool => "tool",
            };

            // Tool result messages use "tool" role with tool_call_id.
            if m.role == crate::types::MessageRole::Tool {
                out.push(OpenAIMessage {
                    role: role.into(),
                    content: Some(serde_json::Value::String(m.content.clone())),
                    tool_call_id: m.tool_call_id.clone(),
                    tool_calls: None,
                });
                continue;
            }

            // Assistant messages with tool_calls.
            if m.role == crate::types::MessageRole::Assistant {
                if let Some(ref calls) = m.tool_calls {
                    let tc_msgs: Vec<OpenAIToolCallMsg> = calls
                        .iter()
                        .map(|c| OpenAIToolCallMsg {
                            id: c.id.clone(),
                            call_type: "function".into(),
                            function: OpenAIFunctionCall {
                                name: c.name.clone(),
                                arguments: serde_json::to_string(&c.input).unwrap_or_default(),
                            },
                        })
                        .collect();
                    out.push(OpenAIMessage {
                        role: role.into(),
                        content: if m.content.is_empty() {
                            None
                        } else {
                            Some(serde_json::Value::String(m.content.clone()))
                        },
                        tool_call_id: None,
                        tool_calls: Some(tc_msgs),
                    });
                    continue;
                }
            }

            out.push(OpenAIMessage {
                role: role.into(),
                content: Some(serde_json::Value::String(m.content.clone())),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        out
    }

    /// Build the JSON request body.
    fn build_body(&self, request: &ChatRequest, stream: bool) -> OpenAIChatRequest {
        let is_reasoning = Self::is_reasoning_model(&request.model);

        OpenAIChatRequest {
            model: request.model.clone(),
            messages: Self::convert_messages(
                &request.messages,
                request.system_prompt.as_deref(),
            ),
            stream,
            // Reasoning models use `max_completion_tokens` instead.
            max_tokens: if is_reasoning {
                None
            } else {
                Some(request.max_tokens)
            },
            max_completion_tokens: if is_reasoning {
                Some(request.max_tokens)
            } else {
                None
            },
            // Reasoning models don't accept temperature.
            temperature: if is_reasoning {
                None
            } else {
                request.temperature
            },
            stream_options: if stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
            tools: request.tools.as_ref().map(|defs| {
                defs.iter()
                    .map(|t| OpenAITool {
                        tool_type: "function".into(),
                        function: OpenAIFunction {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect()
            }),
        }
    }

    /// Get the API key or return an error.
    fn require_key(&self) -> Result<&str, ProviderError> {
        self.api_key
            .as_deref()
            .ok_or(ProviderError::InvalidKey)
    }

    /// Send a POST to the chat completions endpoint.
    async fn post_completions(
        &self,
        body: &OpenAIChatRequest,
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
                "OpenAI API error {status}: {text}"
            )));
        }

        Ok(resp)
    }
}

#[async_trait]
impl AiProvider for OpenAIProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenAI
    }

    fn name(&self) -> &str {
        "OpenAI"
    }

    async fn is_available(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        crate::model_registry::models_for_provider(ProviderType::OpenAI)
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
            ProviderError::Other("No choices in OpenAI response".into())
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

        // Extract tool calls from the response.
        let tool_calls = choice
            .message
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        input: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                    })
                    .collect()
            });

        Ok(ChatResponse {
            content,
            model: data.model,
            usage,
            finish_reason,
            thinking: None,
            tool_calls,
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
            messages: vec![ChatMessage::text(MessageRole::User, "Hello")],
            model: model.into(),
            max_tokens: 1024,
            temperature: Some(0.7),
            system_prompt: None,
            tools: None,
        }
    }

    #[test]
    fn build_body_standard_model() {
        let provider = OpenAIProvider::new("sk-test".into());
        let req = sample_request("gpt-4o");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "gpt-4o");
        assert_eq!(body.max_tokens, Some(1024));
        assert!(body.max_completion_tokens.is_none());
        assert_eq!(body.temperature, Some(0.7));
        assert!(!body.stream);
        assert!(body.stream_options.is_none());
    }

    #[test]
    fn build_body_reasoning_model_o1() {
        let provider = OpenAIProvider::new("sk-test".into());
        let req = sample_request("o1-mini");
        let body = provider.build_body(&req, false);

        assert_eq!(body.model, "o1-mini");
        // o1 uses max_completion_tokens, not max_tokens.
        assert!(body.max_tokens.is_none());
        assert_eq!(body.max_completion_tokens, Some(1024));
        // o1 ignores temperature.
        assert!(body.temperature.is_none());
    }

    #[test]
    fn build_body_reasoning_model_o3() {
        let provider = OpenAIProvider::new("sk-test".into());
        let req = sample_request("o3");
        let body = provider.build_body(&req, false);

        assert!(body.max_tokens.is_none());
        assert_eq!(body.max_completion_tokens, Some(1024));
        assert!(body.temperature.is_none());
    }

    #[test]
    fn build_body_stream_includes_usage_option() {
        let provider = OpenAIProvider::new("sk-test".into());
        let req = sample_request("gpt-4o");
        let body = provider.build_body(&req, true);

        assert!(body.stream);
        assert!(body.stream_options.is_some());
        assert!(body.stream_options.unwrap().include_usage);
    }

    #[test]
    fn build_body_with_system_prompt() {
        let provider = OpenAIProvider::new("sk-test".into());
        let mut req = sample_request("gpt-4o");
        req.system_prompt = Some("You are helpful.".into());
        let body = provider.build_body(&req, false);

        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(
            body.messages[0].content,
            Some(serde_json::Value::String("You are helpful.".into()))
        );
        assert_eq!(body.messages[1].role, "user");
    }

    #[test]
    fn is_reasoning_model_detection() {
        assert!(OpenAIProvider::is_reasoning_model("o1"));
        assert!(OpenAIProvider::is_reasoning_model("o1-mini"));
        assert!(OpenAIProvider::is_reasoning_model("o1-preview"));
        assert!(OpenAIProvider::is_reasoning_model("o3"));
        assert!(OpenAIProvider::is_reasoning_model("o3-mini"));
        assert!(OpenAIProvider::is_reasoning_model("o4-mini"));
        assert!(!OpenAIProvider::is_reasoning_model("gpt-4o"));
        assert!(!OpenAIProvider::is_reasoning_model("gpt-4o-mini"));
    }

    #[test]
    fn provider_metadata() {
        let provider = OpenAIProvider::new("sk-test".into());
        assert_eq!(provider.provider_type(), ProviderType::OpenAI);
        assert_eq!(provider.name(), "OpenAI");
    }

    #[tokio::test]
    async fn is_available_with_key() {
        let provider = OpenAIProvider::new("sk-test".into());
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn is_available_without_key() {
        let provider = OpenAIProvider::new(String::new());
        assert!(!provider.is_available().await);
    }

    #[test]
    fn require_key_returns_error_when_missing() {
        let provider = OpenAIProvider::new(String::new());
        assert!(provider.require_key().is_err());
    }

    #[test]
    fn request_body_serializes_correctly() {
        let provider = OpenAIProvider::new("sk-test".into());
        let req = sample_request("gpt-4o");
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "gpt-4o");
        assert_eq!(json["max_tokens"], 1024);
        // f32 0.7 doesn't round-trip exactly through JSON, so compare approximately.
        let temp = json["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.001, "temperature was {temp}");
        assert_eq!(json["stream"], false);
        // max_completion_tokens should not appear for non-reasoning models.
        assert!(json.get("max_completion_tokens").is_none());
    }

    #[test]
    fn reasoning_model_request_serializes_correctly() {
        let provider = OpenAIProvider::new("sk-test".into());
        let req = sample_request("o1-mini");
        let body = provider.build_body(&req, false);
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "o1-mini");
        assert_eq!(json["max_completion_tokens"], 1024);
        // max_tokens and temperature should not appear.
        assert!(json.get("max_tokens").is_none());
        assert!(json.get("temperature").is_none());
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
