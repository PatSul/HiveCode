//! Anthropic (Claude) provider — full SSE streaming implementation.
//!
//! Talks directly to the Anthropic Messages API (`/v1/messages`) with support
//! for both non-streaming and streaming (SSE) completions, including extended
//! thinking blocks.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::{AiProvider, ProviderError};
use crate::types::{
    ChatRequest, ChatResponse, FinishReason, ModelInfo, ProviderType, StreamChunk, TokenUsage,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const API_BASE: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 4096;
const REQUEST_TIMEOUT_SECS: u64 = 60;

// ---------------------------------------------------------------------------
// Anthropic API request/response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

// -- Non-streaming response types --

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    model: String,
    usage: ApiUsage,
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// -- SSE streaming types --

#[derive(Debug, Deserialize)]
struct SseMessageStart {
    message: Option<SseMessageInfo>,
}

#[derive(Debug, Deserialize)]
struct SseMessageInfo {
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct SseContentBlockStart {
    content_block: Option<SseContentBlock>,
}

#[derive(Debug, Deserialize)]
struct SseContentBlock {
    #[serde(rename = "type")]
    block_type: String,
}

#[derive(Debug, Deserialize)]
struct SseContentBlockDelta {
    delta: Option<SseDelta>,
}

#[derive(Debug, Deserialize)]
struct SseMessageDelta {
    #[allow(dead_code)]
    delta: Option<SseMessageDeltaInner>,
    usage: Option<SseMessageDeltaUsage>,
}

#[derive(Debug, Deserialize)]
struct SseMessageDeltaInner {
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseMessageDeltaUsage {
    output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
}

// -- Error response --

#[derive(Debug, Deserialize)]
struct AnthropicErrorResponse {
    error: Option<AnthropicErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    message: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Anthropic API provider (Claude models).
pub struct AnthropicProvider {
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { api_key, client }
    }

    /// Convert generic chat messages to Anthropic's format, extracting the
    /// system prompt from any `System` role messages.
    fn build_request(&self, request: &ChatRequest, stream: bool) -> AnthropicRequest {
        // Extract system messages and combine them.
        let system_from_messages: Vec<&str> = request
            .messages
            .iter()
            .filter(|m| m.role == crate::types::MessageRole::System)
            .map(|m| m.content.as_str())
            .collect();

        // Use explicit system_prompt if provided, otherwise combine system messages.
        let system = if let Some(ref sys) = request.system_prompt {
            Some(sys.clone())
        } else if !system_from_messages.is_empty() {
            Some(system_from_messages.join("\n\n"))
        } else {
            None
        };

        // Build conversation messages (non-system only).
        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != crate::types::MessageRole::System)
            .map(|m| AnthropicMessage {
                role: match m.role {
                    crate::types::MessageRole::User => "user".into(),
                    crate::types::MessageRole::Assistant => "assistant".into(),
                    crate::types::MessageRole::Error => "user".into(),
                    _ => "user".into(),
                },
                content: m.content.clone(),
            })
            .collect();

        AnthropicRequest {
            model: request.model.clone(),
            max_tokens: if request.max_tokens > 0 {
                request.max_tokens
            } else {
                DEFAULT_MAX_TOKENS
            },
            messages,
            system,
            temperature: request.temperature,
            stream,
        }
    }

    /// Map an HTTP status code (and optional body) to a ProviderError.
    fn map_status_error(status: reqwest::StatusCode, body: &str) -> ProviderError {
        match status.as_u16() {
            401 | 403 => ProviderError::InvalidKey,
            429 => ProviderError::RateLimit,
            s if s >= 500 => ProviderError::Other(format!(
                "Anthropic server error {s}: {}",
                truncate_error(body)
            )),
            _ => ProviderError::Other(format!(
                "Anthropic API error {}: {}",
                status,
                truncate_error(body)
            )),
        }
    }

    /// Map a reqwest error to a ProviderError.
    fn map_reqwest_error(e: reqwest::Error) -> ProviderError {
        if e.is_timeout() {
            ProviderError::Timeout
        } else if e.is_connect() {
            ProviderError::Network(format!("Connection failed: {e}"))
        } else {
            ProviderError::Network(e.to_string())
        }
    }

    /// Map a stop_reason string to our FinishReason enum.
    fn map_stop_reason(reason: &str) -> FinishReason {
        match reason {
            "end_turn" | "stop" => FinishReason::Stop,
            "max_tokens" => FinishReason::Length,
            "content_filter" => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        }
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }

    fn name(&self) -> &str {
        "Anthropic"
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        crate::model_registry::models_for_provider(ProviderType::Anthropic)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Non-streaming completion via the Anthropic Messages API.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let body = self.build_request(request, false);

        let resp = self
            .client
            .post(API_BASE)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Self::map_status_error(status, &text));
        }

        let data: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("Failed to parse response: {e}")))?;

        // Extract text content and thinking content from blocks.
        let mut text_content = String::new();
        let mut thinking_content = String::new();

        for block in &data.content {
            match block.block_type.as_str() {
                "text" => {
                    if let Some(ref t) = block.text {
                        text_content.push_str(t);
                    }
                }
                "thinking" => {
                    if let Some(ref t) = block.thinking {
                        thinking_content.push_str(t);
                    }
                }
                _ => {}
            }
        }

        let stop_reason = data
            .stop_reason
            .as_deref()
            .map(Self::map_stop_reason)
            .unwrap_or(FinishReason::Stop);

        Ok(ChatResponse {
            content: text_content,
            model: data.model,
            usage: TokenUsage {
                prompt_tokens: data.usage.input_tokens,
                completion_tokens: data.usage.output_tokens,
                total_tokens: data.usage.input_tokens + data.usage.output_tokens,
            },
            finish_reason: stop_reason,
            thinking: if thinking_content.is_empty() {
                None
            } else {
                Some(thinking_content)
            },
        })
    }

    /// Streaming completion via SSE.
    ///
    /// Spawns a background task that reads SSE events from the response body
    /// and sends `StreamChunk`s over an mpsc channel. Handles `text`,
    /// `thinking`, and usage/stop events.
    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let body = self.build_request(request, true);

        // Use a longer timeout for streaming — the initial connection should
        // happen within the default timeout, but we don't want the overall
        // request to time out while we're reading chunks.
        let resp = self
            .client
            .post(API_BASE)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS * 5))
            .json(&body)
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Self::map_status_error(status, &text));
        }

        let (tx, rx) = mpsc::channel::<StreamChunk>(64);

        // Spawn the SSE consumer task.
        tokio::spawn(async move {
            use futures::StreamExt;

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            // State tracked across SSE events.
            let mut input_tokens: u32 = 0;
            let mut output_tokens: u32 = 0;
            let mut current_block_type = String::new();
            let mut current_event_type = String::new();

            while let Some(chunk_result) = stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Anthropic stream read error: {e}");
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete lines.
                while let Some(newline_pos) = buffer.find('\n') {
                    let line: String = buffer.drain(..=newline_pos).collect();
                    let line = line.trim_end();

                    if line.is_empty() {
                        // Empty line after event+data = end of SSE event block.
                        continue;
                    }

                    // Parse `event: <type>` lines.
                    if let Some(event_type) = line.strip_prefix("event: ") {
                        current_event_type = event_type.trim().to_string();
                        continue;
                    }

                    // Parse `data: <json>` lines.
                    if let Some(data) = line.strip_prefix("data: ") {
                        let data = data.trim();
                        if data == "[DONE]" {
                            continue;
                        }

                        if let Err(send_err) = process_sse_event(
                            &current_event_type,
                            data,
                            &mut input_tokens,
                            &mut output_tokens,
                            &mut current_block_type,
                            &tx,
                        )
                        .await
                        {
                            if send_err {
                                // Receiver dropped — stop.
                                return;
                            }
                        }

                        // Reset event type after processing.
                        current_event_type.clear();
                    }
                }
            }

            // Stream ended — send a final done chunk if we haven't already.
            let _ = tx
                .send(StreamChunk {
                    content: String::new(),
                    done: true,
                    thinking: None,
                    usage: Some(TokenUsage {
                        prompt_tokens: input_tokens,
                        completion_tokens: output_tokens,
                        total_tokens: input_tokens + output_tokens,
                    }),
                })
                .await;
        });

        Ok(rx)
    }
}

// ---------------------------------------------------------------------------
// SSE event processing (extracted for testability)
// ---------------------------------------------------------------------------

/// Process a single SSE event. Returns `Ok(())` on success.
/// Returns `Err(true)` if the channel receiver has been dropped (should stop),
/// `Err(false)` for parse errors (continue processing).
async fn process_sse_event(
    event_type: &str,
    data: &str,
    input_tokens: &mut u32,
    output_tokens: &mut u32,
    current_block_type: &mut String,
    tx: &mpsc::Sender<StreamChunk>,
) -> Result<(), bool> {
    match event_type {
        "message_start" => {
            if let Ok(msg) = serde_json::from_str::<SseMessageStart>(data) {
                if let Some(info) = msg.message {
                    if let Some(usage) = info.usage {
                        *input_tokens = usage.input_tokens;
                    }
                }
            }
        }

        "content_block_start" => {
            if let Ok(block) = serde_json::from_str::<SseContentBlockStart>(data) {
                if let Some(cb) = block.content_block {
                    *current_block_type = cb.block_type;
                }
            }
        }

        "content_block_delta" => {
            if let Ok(delta_msg) = serde_json::from_str::<SseContentBlockDelta>(data) {
                if let Some(delta) = delta_msg.delta {
                    let delta_type = delta.delta_type.as_deref().unwrap_or("");

                    match delta_type {
                        "text_delta" => {
                            if let Some(text) = delta.text {
                                let chunk = StreamChunk {
                                    content: text,
                                    done: false,
                                    thinking: None,
                                    usage: None,
                                };
                                if tx.send(chunk).await.is_err() {
                                    return Err(true);
                                }
                            }
                        }
                        "thinking_delta" => {
                            if let Some(thinking) = delta.thinking {
                                let chunk = StreamChunk {
                                    content: String::new(),
                                    done: false,
                                    thinking: Some(thinking),
                                    usage: None,
                                };
                                if tx.send(chunk).await.is_err() {
                                    return Err(true);
                                }
                            }
                        }
                        _ => {
                            debug!("Unknown delta type: {delta_type}");
                        }
                    }
                }
            }
        }

        "content_block_stop" => {
            *current_block_type = String::new();
        }

        "message_delta" => {
            if let Ok(msg_delta) = serde_json::from_str::<SseMessageDelta>(data) {
                if let Some(usage) = msg_delta.usage {
                    if let Some(out) = usage.output_tokens {
                        *output_tokens = out;
                    }
                }
                // stop_reason is tracked but we send the final chunk on message_stop.
            }
        }

        "message_stop" => {
            let chunk = StreamChunk {
                content: String::new(),
                done: true,
                thinking: None,
                usage: Some(TokenUsage {
                    prompt_tokens: *input_tokens,
                    completion_tokens: *output_tokens,
                    total_tokens: *input_tokens + *output_tokens,
                }),
            };
            if tx.send(chunk).await.is_err() {
                return Err(true);
            }
        }

        "ping" => {
            // Anthropic sends periodic pings during streaming; ignore.
        }

        "error" => {
            warn!("Anthropic SSE error event: {data}");
        }

        _ => {
            debug!("Unknown SSE event type: {event_type}");
        }
    }

    Ok(())
}

/// Truncate error bodies to avoid bloating logs.
fn truncate_error(body: &str) -> String {
    // Try to extract a useful message from the JSON error body.
    if let Ok(err) = serde_json::from_str::<AnthropicErrorResponse>(body) {
        if let Some(detail) = err.error {
            if let Some(msg) = detail.message {
                return msg;
            }
        }
    }

    // Fall back to truncated raw body.
    if body.len() > 200 {
        format!("{}...", &body[..200])
    } else {
        body.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, MessageRole};

    // Helper to create a minimal ChatRequest for testing.
    fn test_request() -> ChatRequest {
        ChatRequest {
            messages: vec![
                ChatMessage {
                    role: MessageRole::User,
                    content: "Hello".into(),
                    timestamp: chrono::Utc::now(),
                },
            ],
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 1024,
            temperature: None,
            system_prompt: Some("You are helpful.".into()),
        }
    }

    // -- Request body construction tests --

    #[test]
    fn build_request_non_streaming() {
        let provider = AnthropicProvider::new("test-key".into());
        let req = test_request();
        let body = provider.build_request(&req, false);

        assert_eq!(body.model, "claude-sonnet-4-20250514");
        assert_eq!(body.max_tokens, 1024);
        assert!(!body.stream);
        assert_eq!(body.system, Some("You are helpful.".into()));
        assert_eq!(body.messages.len(), 1);
        assert_eq!(body.messages[0].role, "user");
        assert_eq!(body.messages[0].content, "Hello");
    }

    #[test]
    fn build_request_streaming() {
        let provider = AnthropicProvider::new("test-key".into());
        let req = test_request();
        let body = provider.build_request(&req, true);

        assert!(body.stream);
    }

    #[test]
    fn build_request_system_from_messages() {
        let provider = AnthropicProvider::new("test-key".into());
        let req = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: MessageRole::System,
                    content: "Be concise.".into(),
                    timestamp: chrono::Utc::now(),
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: "Hi".into(),
                    timestamp: chrono::Utc::now(),
                },
            ],
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            temperature: Some(0.7),
            system_prompt: None,
        };
        let body = provider.build_request(&req, false);

        // System message should be extracted, not in messages array.
        assert_eq!(body.system, Some("Be concise.".into()));
        assert_eq!(body.messages.len(), 1);
        assert_eq!(body.messages[0].role, "user");
        assert_eq!(body.temperature, Some(0.7));
    }

    #[test]
    fn build_request_explicit_system_prompt_wins() {
        let provider = AnthropicProvider::new("test-key".into());
        let req = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: MessageRole::System,
                    content: "From messages.".into(),
                    timestamp: chrono::Utc::now(),
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: "Hi".into(),
                    timestamp: chrono::Utc::now(),
                },
            ],
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            temperature: None,
            system_prompt: Some("Explicit system prompt.".into()),
        };
        let body = provider.build_request(&req, false);

        // Explicit system_prompt takes precedence.
        assert_eq!(body.system, Some("Explicit system prompt.".into()));
    }

    #[test]
    fn build_request_default_max_tokens() {
        let provider = AnthropicProvider::new("test-key".into());
        let req = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Hi".into(),
                timestamp: chrono::Utc::now(),
            }],
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 0,
            temperature: None,
            system_prompt: None,
        };
        let body = provider.build_request(&req, false);

        assert_eq!(body.max_tokens, DEFAULT_MAX_TOKENS);
    }

    // -- JSON serialization test --

    #[test]
    fn request_body_serializes_correctly() {
        let provider = AnthropicProvider::new("test-key".into());
        let req = test_request();
        let body = provider.build_request(&req, true);

        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["stream"], true);
        assert_eq!(json["model"], "claude-sonnet-4-20250514");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["system"], "You are helpful.");
        assert!(json["messages"].is_array());
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "Hello");
        // temperature is None -> should not be serialized
        assert!(json.get("temperature").is_none() || json["temperature"].is_null());
    }

    // -- Error mapping tests --

    #[test]
    fn map_status_401_to_invalid_key() {
        let err = AnthropicProvider::map_status_error(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"Invalid API key"}}"#,
        );
        assert!(matches!(err, ProviderError::InvalidKey));
    }

    #[test]
    fn map_status_403_to_invalid_key() {
        let err = AnthropicProvider::map_status_error(
            reqwest::StatusCode::FORBIDDEN,
            "forbidden",
        );
        assert!(matches!(err, ProviderError::InvalidKey));
    }

    #[test]
    fn map_status_429_to_rate_limit() {
        let err = AnthropicProvider::map_status_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "rate limited",
        );
        assert!(matches!(err, ProviderError::RateLimit));
    }

    #[test]
    fn map_status_500_to_other() {
        let err = AnthropicProvider::map_status_error(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "internal error",
        );
        assert!(matches!(err, ProviderError::Other(_)));
    }

    #[test]
    fn map_stop_reason_end_turn() {
        assert_eq!(
            AnthropicProvider::map_stop_reason("end_turn"),
            FinishReason::Stop
        );
    }

    #[test]
    fn map_stop_reason_max_tokens() {
        assert_eq!(
            AnthropicProvider::map_stop_reason("max_tokens"),
            FinishReason::Length
        );
    }

    // -- SSE event parsing tests --

    #[tokio::test]
    async fn parse_message_start_event() {
        let (tx, _rx) = mpsc::channel(16);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = String::new();

        let data = r#"{"type":"message_start","message":{"usage":{"input_tokens":42,"output_tokens":0}}}"#;

        let result = process_sse_event(
            "message_start",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(input_tokens, 42);
    }

    #[tokio::test]
    async fn parse_content_block_start_text() {
        let (tx, _rx) = mpsc::channel(16);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = String::new();

        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;

        let result = process_sse_event(
            "content_block_start",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(block_type, "text");
    }

    #[tokio::test]
    async fn parse_content_block_start_thinking() {
        let (tx, _rx) = mpsc::channel(16);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = String::new();

        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#;

        let result = process_sse_event(
            "content_block_start",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(block_type, "thinking");
    }

    #[tokio::test]
    async fn parse_text_delta() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = "text".to_string();

        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello, world!"}}"#;

        let result = process_sse_event(
            "content_block_delta",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        let chunk = rx.try_recv().unwrap();
        assert_eq!(chunk.content, "Hello, world!");
        assert!(!chunk.done);
        assert!(chunk.thinking.is_none());
    }

    #[tokio::test]
    async fn parse_thinking_delta() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = "thinking".to_string();

        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}"#;

        let result = process_sse_event(
            "content_block_delta",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        let chunk = rx.try_recv().unwrap();
        assert_eq!(chunk.content, "");
        assert!(!chunk.done);
        assert_eq!(chunk.thinking, Some("Let me think...".into()));
    }

    #[tokio::test]
    async fn parse_content_block_stop() {
        let (tx, _rx) = mpsc::channel(16);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = "text".to_string();

        let data = r#"{"type":"content_block_stop","index":0}"#;

        let result = process_sse_event(
            "content_block_stop",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(block_type, "");
    }

    #[tokio::test]
    async fn parse_message_delta_with_usage() {
        let (tx, _rx) = mpsc::channel(16);
        let mut input_tokens = 10u32;
        let mut output_tokens = 0u32;
        let mut block_type = String::new();

        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":55}}"#;

        let result = process_sse_event(
            "message_delta",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(output_tokens, 55);
    }

    #[tokio::test]
    async fn parse_message_stop() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut input_tokens = 10u32;
        let mut output_tokens = 55u32;
        let mut block_type = String::new();

        let data = r#"{"type":"message_stop"}"#;

        let result = process_sse_event(
            "message_stop",
            data,
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
        let chunk = rx.try_recv().unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.content, "");
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 55);
        assert_eq!(usage.total_tokens, 65);
    }

    #[tokio::test]
    async fn ping_event_is_ignored() {
        let (tx, _rx) = mpsc::channel(16);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = String::new();

        let result = process_sse_event(
            "ping",
            "{}",
            &mut input_tokens,
            &mut output_tokens,
            &mut block_type,
            &tx,
        )
        .await;

        assert!(result.is_ok());
    }

    // -- Full SSE stream simulation --

    #[tokio::test]
    async fn simulate_full_sse_stream() {
        let (tx, mut rx) = mpsc::channel(64);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = String::new();

        // Simulate a complete Anthropic SSE stream sequence.
        let events = vec![
            ("message_start", r#"{"type":"message_start","message":{"usage":{"input_tokens":25,"output_tokens":0}}}"#),
            ("content_block_start", r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" there!"}}"#),
            ("content_block_stop", r#"{"type":"content_block_stop","index":0}"#),
            ("message_delta", r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":8}}"#),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        for (event_type, data) in &events {
            process_sse_event(
                event_type,
                data,
                &mut input_tokens,
                &mut output_tokens,
                &mut block_type,
                &tx,
            )
            .await
            .unwrap();
        }

        // Collect all chunks.
        let mut chunks = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            chunks.push(chunk);
        }

        assert_eq!(chunks.len(), 3); // "Hello", " there!", final done
        assert_eq!(chunks[0].content, "Hello");
        assert!(!chunks[0].done);
        assert_eq!(chunks[1].content, " there!");
        assert!(!chunks[1].done);
        assert!(chunks[2].done);
        assert_eq!(chunks[2].content, "");

        let final_usage = chunks[2].usage.as_ref().unwrap();
        assert_eq!(final_usage.prompt_tokens, 25);
        assert_eq!(final_usage.completion_tokens, 8);
        assert_eq!(final_usage.total_tokens, 33);
    }

    #[tokio::test]
    async fn simulate_thinking_stream() {
        let (tx, mut rx) = mpsc::channel(64);
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut block_type = String::new();

        let events = vec![
            ("message_start", r#"{"type":"message_start","message":{"usage":{"input_tokens":30,"output_tokens":0}}}"#),
            // Thinking block
            ("content_block_start", r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Analyzing the request..."}}"#),
            ("content_block_stop", r#"{"type":"content_block_stop","index":0}"#),
            // Text block
            ("content_block_start", r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Here's my answer."}}"#),
            ("content_block_stop", r#"{"type":"content_block_stop","index":1}"#),
            ("message_delta", r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":20}}"#),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        for (event_type, data) in &events {
            process_sse_event(
                event_type,
                data,
                &mut input_tokens,
                &mut output_tokens,
                &mut block_type,
                &tx,
            )
            .await
            .unwrap();
        }

        let mut chunks = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            chunks.push(chunk);
        }

        // Should have: thinking delta, text delta, done
        assert_eq!(chunks.len(), 3);

        // Thinking chunk
        assert_eq!(chunks[0].content, "");
        assert_eq!(
            chunks[0].thinking,
            Some("Analyzing the request...".into())
        );
        assert!(!chunks[0].done);

        // Text chunk
        assert_eq!(chunks[1].content, "Here's my answer.");
        assert!(chunks[1].thinking.is_none());
        assert!(!chunks[1].done);

        // Done chunk
        assert!(chunks[2].done);
        let usage = chunks[2].usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 30);
        assert_eq!(usage.completion_tokens, 20);
    }

    // -- Non-streaming response parsing test --

    #[test]
    fn parse_non_streaming_response() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Hello!"}
            ],
            "model": "claude-sonnet-4-20250514",
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "stop_reason": "end_turn"
        }"#;

        let resp: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.content[0].block_type, "text");
        assert_eq!(resp.content[0].text, Some("Hello!".into()));
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert_eq!(resp.stop_reason, Some("end_turn".into()));
        assert_eq!(resp.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn parse_response_with_thinking() {
        let json = r#"{
            "content": [
                {"type": "thinking", "thinking": "Let me analyze..."},
                {"type": "text", "text": "My answer."}
            ],
            "model": "claude-sonnet-4-20250514",
            "usage": {"input_tokens": 20, "output_tokens": 15},
            "stop_reason": "end_turn"
        }"#;

        let resp: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 2);
        assert_eq!(resp.content[0].block_type, "thinking");
        assert_eq!(resp.content[0].thinking, Some("Let me analyze...".into()));
        assert_eq!(resp.content[1].block_type, "text");
        assert_eq!(resp.content[1].text, Some("My answer.".into()));
    }

    // -- Error body parsing --

    #[test]
    fn truncate_error_parses_json() {
        let body = r#"{"error":{"type":"authentication_error","message":"Invalid API key provided"}}"#;
        let result = truncate_error(body);
        assert_eq!(result, "Invalid API key provided");
    }

    #[test]
    fn truncate_error_fallback_for_non_json() {
        let body = "Something went wrong";
        let result = truncate_error(body);
        assert_eq!(result, "Something went wrong");
    }

    #[test]
    fn truncate_error_truncates_long_body() {
        let body = "x".repeat(300);
        let result = truncate_error(&body);
        assert!(result.len() < 210);
        assert!(result.ends_with("..."));
    }

    // -- Availability --

    #[tokio::test]
    async fn is_available_with_key() {
        let provider = AnthropicProvider::new("sk-ant-test123".into());
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn is_not_available_without_key() {
        let provider = AnthropicProvider::new(String::new());
        assert!(!provider.is_available().await);
    }

    // -- Provider metadata --

    #[test]
    fn provider_type_is_anthropic() {
        let provider = AnthropicProvider::new("key".into());
        assert_eq!(provider.provider_type(), ProviderType::Anthropic);
    }

    #[test]
    fn provider_name() {
        let provider = AnthropicProvider::new("key".into());
        assert_eq!(provider.name(), "Anthropic");
    }
}
