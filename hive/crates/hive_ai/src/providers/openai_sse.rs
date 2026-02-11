//! Shared SSE parsing for OpenAI-compatible chat completion streams.
//!
//! Both the OpenAI and OpenRouter providers use the same SSE wire format:
//!
//! ```text
//! data: {"id":"...","choices":[{"delta":{"content":"Hello"},...}]}
//! data: {"id":"...","choices":[{"delta":{"content":" world"},...}]}
//! data: [DONE]
//! ```
//!
//! This module provides the shared types and a helper that drives an SSE byte
//! stream and sends [`StreamChunk`]s over an mpsc channel.

use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::types::{StreamChunk, TokenUsage};

// ---------------------------------------------------------------------------
// Wire types (deserialization only)
// ---------------------------------------------------------------------------

/// Top-level SSE JSON frame from `/chat/completions` (streaming).
#[derive(Debug, Deserialize)]
pub(crate) struct SseFrame {
    #[allow(dead_code)]
    pub id: Option<String>,
    pub choices: Vec<SseChoice>,
    pub usage: Option<SseUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseChoice {
    pub delta: Option<SseDelta>,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseDelta {
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseUsage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// Non-streaming response from `/chat/completions` with `stream: false`.
#[derive(Debug, Deserialize)]
pub(crate) struct ChatCompletionResponse {
    pub choices: Vec<CompletionChoice>,
    pub model: String,
    pub usage: Option<SseUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompletionChoice {
    pub message: CompletionMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompletionMessage {
    pub content: Option<String>,
}

// ---------------------------------------------------------------------------
// SSE stream driver
// ---------------------------------------------------------------------------

/// Consume a `reqwest::Response` that returns SSE-formatted chat completion
/// deltas and forward them as [`StreamChunk`]s on the given `tx` channel.
///
/// This function is meant to be spawned via `tokio::spawn`.
pub(crate) async fn drive_sse_stream(
    resp: reqwest::Response,
    tx: mpsc::Sender<StreamChunk>,
) {
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut accumulated_usage: Option<TokenUsage> = None;

    while let Some(chunk_result) = stream.next().await {
        let bytes = match chunk_result {
            Ok(b) => b,
            Err(e) => {
                warn!("SSE stream read error: {e}");
                break;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&bytes));

        // Process complete lines from the buffer.
        while let Some(newline_pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=newline_pos).collect();
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            // SSE lines start with "data: ".
            let Some(data) = line.strip_prefix("data: ") else {
                // Could be a comment line (`:`) or other SSE field -- skip.
                continue;
            };

            // Terminal sentinel.
            if data == "[DONE]" {
                let chunk = StreamChunk {
                    content: String::new(),
                    done: true,
                    thinking: None,
                    usage: accumulated_usage.take(),
                };
                let _ = tx.send(chunk).await;
                return;
            }

            // Parse the JSON frame.
            match serde_json::from_str::<SseFrame>(data) {
                Ok(frame) => {
                    // Extract delta content (may be empty on role-only deltas).
                    let content = frame
                        .choices
                        .first()
                        .and_then(|c| c.delta.as_ref())
                        .and_then(|d| d.content.clone())
                        .unwrap_or_default();

                    // Track usage if the final chunk includes it.
                    if let Some(u) = &frame.usage {
                        let p = u.prompt_tokens.unwrap_or(0);
                        let c = u.completion_tokens.unwrap_or(0);
                        accumulated_usage = Some(TokenUsage {
                            prompt_tokens: p,
                            completion_tokens: c,
                            total_tokens: u.total_tokens.unwrap_or(p + c),
                        });
                    }

                    // Only send chunks with actual content (skip empty role-only deltas).
                    if !content.is_empty() {
                        let chunk = StreamChunk {
                            content,
                            done: false,
                            thinking: None,
                            usage: None,
                        };
                        if tx.send(chunk).await.is_err() {
                            return; // receiver dropped
                        }
                    }
                }
                Err(e) => {
                    debug!("Skipping malformed SSE JSON: {e} -- data: {data}");
                }
            }
        }
    }

    // Stream ended without [DONE] -- send a final sentinel.
    let _ = tx
        .send(StreamChunk {
            content: String::new(),
            done: true,
            thinking: None,
            usage: accumulated_usage,
        })
        .await;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sse_frame_with_delta() {
        let json = r#"{"id":"chatcmpl-abc","choices":[{"delta":{"content":"Hello"},"index":0,"finish_reason":null}]}"#;
        let frame: SseFrame = serde_json::from_str(json).unwrap();
        assert_eq!(frame.choices.len(), 1);
        let content = frame.choices[0]
            .delta
            .as_ref()
            .unwrap()
            .content
            .as_deref();
        assert_eq!(content, Some("Hello"));
    }

    #[test]
    fn parse_sse_frame_with_usage() {
        let json = r#"{"id":"chatcmpl-abc","choices":[{"delta":{},"index":0,"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}}"#;
        let frame: SseFrame = serde_json::from_str(json).unwrap();
        let usage = frame.usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(10));
        assert_eq!(usage.completion_tokens, Some(20));
        assert_eq!(usage.total_tokens, Some(30));
    }

    #[test]
    fn parse_sse_frame_empty_delta() {
        let json = r#"{"id":"chatcmpl-abc","choices":[{"delta":{"role":"assistant"},"index":0,"finish_reason":null}]}"#;
        let frame: SseFrame = serde_json::from_str(json).unwrap();
        let content = frame.choices[0]
            .delta
            .as_ref()
            .unwrap()
            .content
            .as_deref();
        assert_eq!(content, None);
    }

    #[test]
    fn parse_completion_response() {
        let json = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hello!" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
        }"#;
        let resp: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("Hello!"));
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = resp.usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(5));
        assert_eq!(usage.completion_tokens, Some(2));
    }

    #[tokio::test]
    async fn drive_sse_stream_parses_chunks() {
        // Build a fake SSE payload.
        let sse_payload = concat!(
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{\"content\":\" world\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"delta\":{},\"index\":0,\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n",
        );

        // Build a mock response using a hand-crafted stream.
        let body_stream =
            futures::stream::once(async move { Ok::<_, reqwest::Error>(bytes::Bytes::from(sse_payload)) });
        let resp = http::Response::builder()
            .status(200)
            .body(reqwest::Body::wrap_stream(body_stream))
            .unwrap();
        let resp = reqwest::Response::from(resp);

        let (tx, mut rx) = mpsc::channel::<StreamChunk>(32);

        tokio::spawn(async move {
            drive_sse_stream(resp, tx).await;
        });

        // Collect all chunks.
        let mut chunks = Vec::new();
        while let Some(chunk) = rx.recv().await {
            chunks.push(chunk);
        }

        // Should have: "Hello", " world", done=true
        assert!(chunks.len() >= 2, "expected at least 2 chunks, got {}", chunks.len());

        // Content chunks.
        assert_eq!(chunks[0].content, "Hello");
        assert!(!chunks[0].done);
        assert_eq!(chunks[1].content, " world");
        assert!(!chunks[1].done);

        // Final done chunk with usage.
        let last = chunks.last().unwrap();
        assert!(last.done);
        let usage = last.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 3);
        assert_eq!(usage.completion_tokens, 2);
        assert_eq!(usage.total_tokens, 5);
    }
}
