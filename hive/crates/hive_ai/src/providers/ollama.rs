//! Ollama provider -- local model inference via the Ollama REST API.
//!
//! This is the first fully-implemented provider because it requires no API key
//! and provides immediate local testing.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::{AiProvider, ProviderError};
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelInfo, ModelTier, ProviderType,
    StreamChunk, TokenUsage,
};

// ---------------------------------------------------------------------------
// Ollama API types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelEntry {
    name: String,
    #[allow(dead_code)]
    modified_at: Option<String>,
    #[allow(dead_code)]
    size: Option<u64>,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: Option<OllamaResponseMessage>,
    done: bool,
    eval_count: Option<u32>,
    prompt_eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Ollama local model provider.
pub struct OllamaProvider {
    base_url: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Create a new provider pointing at the given Ollama server.
    /// Defaults to `http://localhost:11434` when `None` is passed.
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".into()),
            client: reqwest::Client::new(),
        }
    }

    /// Convert our generic messages to the Ollama wire format.
    fn convert_messages(messages: &[ChatMessage]) -> Vec<OllamaChatMessage> {
        messages
            .iter()
            .map(|m| OllamaChatMessage {
                role: match m.role {
                    crate::types::MessageRole::User => "user".into(),
                    crate::types::MessageRole::Assistant => "assistant".into(),
                    crate::types::MessageRole::System => "system".into(),
                    crate::types::MessageRole::Error => "user".into(), // map errors to user
                },
                content: m.content.clone(),
            })
            .collect()
    }

    /// Build the Ollama request body for a chat request.
    fn build_body(&self, request: &ChatRequest, stream: bool) -> OllamaChatRequest {
        let mut messages = Self::convert_messages(&request.messages);

        // Prepend system prompt as a system message if provided.
        if let Some(ref sys) = request.system_prompt {
            messages.insert(
                0,
                OllamaChatMessage {
                    role: "system".into(),
                    content: sys.clone(),
                },
            );
        }

        OllamaChatRequest {
            model: request.model.clone(),
            messages,
            stream,
            options: Some(OllamaOptions {
                num_predict: Some(request.max_tokens),
                temperature: request.temperature,
            }),
        }
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Ollama
    }

    fn name(&self) -> &str {
        "Ollama (Local)"
    }

    /// Ping `/api/tags` with a short timeout.
    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        match self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    /// Fetch the list of locally-pulled models from Ollama.
    async fn get_models(&self) -> Vec<ModelInfo> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                warn!("Ollama /api/tags returned {}", r.status());
                return vec![];
            }
            Err(e) => {
                debug!("Ollama not reachable: {e}");
                return vec![];
            }
        };

        let data: OllamaTagsResponse = match resp.json().await {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to parse Ollama tags response: {e}");
                return vec![];
            }
        };

        data.models
            .unwrap_or_default()
            .into_iter()
            .map(|m| ModelInfo {
                id: m.name.clone(),
                name: m.name,
                provider: "ollama".into(),
                provider_type: ProviderType::Ollama,
                tier: ModelTier::Free,
                context_window: 8192, // default; varies per model
                input_price_per_mtok: 0.0,
                output_price_per_mtok: 0.0,
                capabilities: Default::default(),
            })
            .collect()
    }

    /// Non-streaming chat completion.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/api/chat", self.base_url);
        let body = self.build_body(request, false);

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "Ollama API error: {status} - {text}"
            )));
        }

        let data: OllamaChatResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("JSON parse error: {e}")))?;

        let content = data
            .message
            .map(|m| m.content)
            .unwrap_or_default();

        let prompt_tokens = data.prompt_eval_count.unwrap_or(0);
        let completion_tokens = data.eval_count.unwrap_or(0);

        Ok(ChatResponse {
            content,
            model: data.model,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            finish_reason: FinishReason::Stop,
            thinking: None,
        })
    }

    /// Streaming chat -- spawns a task that reads NDJSON lines and sends
    /// [`StreamChunk`]s over an mpsc channel.
    async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        let url = format!("{}/api/chat", self.base_url);
        let body = self.build_body(request, true);

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "Ollama API error: {status} - {text}"
            )));
        }

        let (tx, rx) = mpsc::channel::<StreamChunk>(64);

        // Spawn a background task to consume the NDJSON stream.
        tokio::spawn(async move {
            use futures::StreamExt;

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Ollama stream read error: {e}");
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete lines (NDJSON).
                while let Some(newline_pos) = buffer.find('\n') {
                    let line: String = buffer.drain(..=newline_pos).collect();
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<OllamaChatResponse>(line) {
                        Ok(data) => {
                            let content = data
                                .message
                                .map(|m| m.content)
                                .unwrap_or_default();

                            let done = data.done;

                            let usage = if done {
                                let p = data.prompt_eval_count.unwrap_or(0);
                                let c = data.eval_count.unwrap_or(0);
                                Some(TokenUsage {
                                    prompt_tokens: p,
                                    completion_tokens: c,
                                    total_tokens: p + c,
                                })
                            } else {
                                None
                            };

                            let chunk = StreamChunk {
                                content,
                                done,
                                thinking: None,
                                usage,
                            };

                            if tx.send(chunk).await.is_err() {
                                // Receiver dropped.
                                return;
                            }

                            if done {
                                return;
                            }
                        }
                        Err(e) => {
                            debug!("Skipping malformed Ollama JSON line: {e}");
                        }
                    }
                }
            }

            // Stream ended without a done=true message -- send a final chunk.
            let _ = tx
                .send(StreamChunk {
                    content: String::new(),
                    done: true,
                    thinking: None,
                    usage: None,
                })
                .await;
        });

        Ok(rx)
    }
}
