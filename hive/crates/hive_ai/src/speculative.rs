//! Speculative Decoding — "Guess and Check" strategy.
//!
//! Sends the same request to a fast "draft" model and the primary model in
//! parallel. The draft model's output streams to the UI immediately as a
//! preview, while the primary model's (higher-quality) output replaces it
//! when ready.
//!
//! This gives users near-instant feedback from a cheap/fast model while still
//! getting the full quality of their chosen model. Users can see how much
//! time they save compared to waiting for the primary model alone.
//!
//! The feature is entirely optional and controlled via `HiveConfig`.
//!
//! ## Credits
//!
//! Inspired by speculative decoding research. Benchmarking approach informed
//! by [DraftBench](https://github.com/alexziskind1/draftbench) by Alex Ziskind
//! — a tool for measuring speculative decoding speedups across draft/target
//! model combinations.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::providers::AiProvider;
use crate::types::{ChatRequest, ModelTier, StreamChunk};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Speculative decoding settings extracted from HiveConfig.
#[derive(Debug, Clone)]
pub struct SpeculativeConfig {
    /// Whether speculative decoding is enabled.
    pub enabled: bool,
    /// Explicit draft model override (e.g. "gpt-4o-mini"). If `None`,
    /// the system picks a model one tier below the primary.
    pub draft_model: Option<String>,
    /// Show speed comparison metrics to the user.
    pub show_metrics: bool,
}

impl Default for SpeculativeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            draft_model: None,
            show_metrics: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Draft model selection
// ---------------------------------------------------------------------------

/// Pick an appropriate draft model one tier below the primary.
///
/// If the user has configured an explicit `draft_model` in settings, that is
/// returned. Otherwise we select a sensible fast model based on the primary
/// model's inferred tier.
pub fn select_draft_model(primary_model: &str, config: &SpeculativeConfig) -> Option<String> {
    // User override
    if let Some(ref dm) = config.draft_model {
        if !dm.is_empty() {
            return Some(dm.clone());
        }
    }

    // Auto-select: the smallest/fastest model available.
    // Per speculative decoding best practice: smaller is better for the draft.
    // The draft just needs to give a rough preview; the primary verifies quality.
    let tier = infer_tier_from_model(primary_model);
    let draft = match tier {
        ModelTier::Premium => "gpt-4o-mini",     // Tiny + fast cloud model
        ModelTier::Mid => "gpt-4o-mini",         // Same — smallest cloud option
        ModelTier::Budget => "llama3.2",         // Budget → local (free + instant)
        ModelTier::Free => return None,          // Already cheapest — no speculation
    };

    Some(draft.to_string())
}

/// Infer model tier from the model ID string (mirrors routing logic).
fn infer_tier_from_model(model_id: &str) -> ModelTier {
    let lower = model_id.to_lowercase();

    if lower.contains("opus")
        || (lower.contains("gpt-4o") && !lower.contains("mini"))
        || lower.contains("o1")
        || lower.contains("o3")
        || lower.contains("gemini-2")
    {
        return ModelTier::Premium;
    }

    if lower.contains("sonnet") || lower.contains("mini") || lower.contains("flash") {
        return ModelTier::Mid;
    }

    if lower.contains("haiku")
        || lower.contains("deepseek")
        || lower.contains("llama")
        || lower.contains("qwen")
        || lower.contains("mistral")
    {
        return ModelTier::Budget;
    }

    ModelTier::Mid
}

// ---------------------------------------------------------------------------
// Speculative stream combiner
// ---------------------------------------------------------------------------

/// Metadata about the speculative decoding result, shown to the user.
#[derive(Debug, Clone)]
pub struct SpeculativeMetrics {
    /// Time to first token from the draft model (ms).
    pub draft_first_token_ms: u64,
    /// Time to first token from the primary model (ms).
    pub primary_first_token_ms: u64,
    /// Time saved by showing draft output while waiting (ms).
    pub time_saved_ms: u64,
    /// Name of the draft model used.
    pub draft_model: String,
    /// Name of the primary model.
    pub primary_model: String,
    /// Whether the primary model finished (vs timeout/error).
    pub primary_completed: bool,
}

/// A chunk from the speculative stream, annotated with source.
#[derive(Debug, Clone)]
pub struct SpeculativeChunk {
    /// The underlying stream chunk.
    pub chunk: StreamChunk,
    /// Whether this chunk is from the draft model (`true`) or primary (`false`).
    pub is_draft: bool,
    /// Set on the very last chunk — contains timing metrics.
    pub metrics: Option<SpeculativeMetrics>,
}

/// Run speculative decoding: stream from both draft and primary models in
/// parallel, returning a unified channel. Draft chunks arrive first (marked
/// `is_draft = true`). When the primary model starts producing output, a
/// `transition` chunk signals the switch, then primary chunks flow through.
///
/// The caller (ChatService) should:
/// 1. Render draft chunks with a visual "speculating..." indicator
/// 2. On transition, replace the accumulated text with the primary output
/// 3. Display metrics at the end if `show_metrics` is true
pub async fn speculative_stream(
    draft_provider: Arc<dyn AiProvider>,
    draft_request: ChatRequest,
    primary_provider: Arc<dyn AiProvider>,
    primary_request: ChatRequest,
    config: SpeculativeConfig,
) -> Result<mpsc::Receiver<SpeculativeChunk>, crate::providers::ProviderError> {
    let (tx, rx) = mpsc::channel(256);
    let start_time = Instant::now();

    let draft_model_name = draft_request.model.clone();
    let primary_model_name = primary_request.model.clone();
    let show_metrics = config.show_metrics;

    // Spawn both streams concurrently
    let draft_provider_clone = draft_provider.clone();
    let draft_request_clone = draft_request.clone();

    tokio::spawn(async move {
        let mut draft_first_token: Option<u64> = None;
        let mut primary_first_token: Option<u64> = None;
        let mut draft_complete = false;
        let mut primary_accumulated = String::new();

        // Start draft stream
        let draft_rx = match draft_provider_clone.stream_chat(&draft_request_clone).await {
            Ok(rx) => Some(rx),
            Err(e) => {
                warn!("Draft model stream failed: {e}; falling back to primary only");
                None
            }
        };

        // Start primary stream
        let primary_rx = match primary_provider.stream_chat(&primary_request).await {
            Ok(rx) => rx,
            Err(e) => {
                // If primary fails too, report the error
                let _ = tx
                    .send(SpeculativeChunk {
                        chunk: StreamChunk {
                            content: format!("Error: primary model failed: {e}"),
                            done: true,
                            thinking: None,
                            usage: None,
                            tool_calls: None,
                            stop_reason: None,
                        },
                        is_draft: false,
                        metrics: None,
                    })
                    .await;
                return;
            }
        };

        // Phase 1: Stream draft output while waiting for primary
        if let Some(mut draft_rx) = draft_rx {
            let mut primary_rx = primary_rx;

            // Interleave: prefer draft chunks, but check primary too
            loop {
                tokio::select! {
                    // Draft chunk
                    draft_chunk = draft_rx.recv(), if !draft_complete => {
                        match draft_chunk {
                            Some(chunk) => {
                                if draft_first_token.is_none() {
                                    draft_first_token = Some(start_time.elapsed().as_millis() as u64);
                                    debug!("Draft first token: {}ms", draft_first_token.unwrap());
                                }
                                let is_done = chunk.done;
                                let _ = tx.send(SpeculativeChunk {
                                    chunk,
                                    is_draft: true,
                                    metrics: None,
                                }).await;
                                if is_done {
                                    draft_complete = true;
                                }
                            }
                            None => {
                                draft_complete = true;
                            }
                        }
                    }
                    // Primary chunk — when we get the first one, send transition signal
                    primary_chunk = primary_rx.recv() => {
                        match primary_chunk {
                            Some(chunk) => {
                                if primary_first_token.is_none() {
                                    primary_first_token = Some(start_time.elapsed().as_millis() as u64);
                                    info!(
                                        "Primary first token: {}ms (draft was {}ms)",
                                        primary_first_token.unwrap(),
                                        draft_first_token.unwrap_or(0)
                                    );

                                    // Send a transition marker (empty content, not done)
                                    let _ = tx.send(SpeculativeChunk {
                                        chunk: StreamChunk {
                                            content: String::new(),
                                            done: false,
                                            thinking: None,
                                            usage: None,
                                            tool_calls: None,
                                            stop_reason: None,
                                        },
                                        is_draft: false,  // marks transition to primary
                                        metrics: None,
                                    }).await;
                                }

                                primary_accumulated.push_str(&chunk.content);
                                let is_done = chunk.done;

                                let metrics = if is_done && show_metrics {
                                    let dft = draft_first_token.unwrap_or(0);
                                    let pft = primary_first_token.unwrap_or(0);
                                    Some(SpeculativeMetrics {
                                        draft_first_token_ms: dft,
                                        primary_first_token_ms: pft,
                                        time_saved_ms: if pft > dft { pft - dft } else { 0 },
                                        draft_model: draft_model_name.clone(),
                                        primary_model: primary_model_name.clone(),
                                        primary_completed: true,
                                    })
                                } else {
                                    None
                                };

                                let _ = tx.send(SpeculativeChunk {
                                    chunk,
                                    is_draft: false,
                                    metrics,
                                }).await;

                                if is_done {
                                    return;
                                }
                            }
                            None => {
                                // Primary stream ended unexpectedly
                                return;
                            }
                        }
                    }
                }
            }
        } else {
            // No draft — just forward primary directly
            let mut primary_rx = primary_rx;
            while let Some(chunk) = primary_rx.recv().await {
                let is_done = chunk.done;
                let _ = tx
                    .send(SpeculativeChunk {
                        chunk,
                        is_draft: false,
                        metrics: None,
                    })
                    .await;
                if is_done {
                    return;
                }
            }
        }
    });

    Ok(rx)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_tier_premium() {
        assert_eq!(
            infer_tier_from_model("claude-opus-4-20250514"),
            ModelTier::Premium
        );
        assert_eq!(infer_tier_from_model("gpt-4o"), ModelTier::Premium);
        assert_eq!(infer_tier_from_model("o3-mini"), ModelTier::Mid); // mini override
    }

    #[test]
    fn test_infer_tier_mid() {
        assert_eq!(
            infer_tier_from_model("claude-sonnet-4-20250514"),
            ModelTier::Mid
        );
        assert_eq!(infer_tier_from_model("gpt-4o-mini"), ModelTier::Mid);
        assert_eq!(
            infer_tier_from_model("gemini-1.5-flash"),
            ModelTier::Mid
        );
    }

    #[test]
    fn test_infer_tier_budget() {
        assert_eq!(
            infer_tier_from_model("claude-haiku-4-5-20251001"),
            ModelTier::Budget
        );
        assert_eq!(
            infer_tier_from_model("deepseek/deepseek-chat"),
            ModelTier::Budget
        );
    }

    #[test]
    fn test_select_draft_from_premium() {
        let config = SpeculativeConfig::default();
        let draft = select_draft_model("claude-opus-4-20250514", &config);
        assert_eq!(draft, Some("gpt-4o-mini".to_string()));
    }

    #[test]
    fn test_select_draft_explicit_override() {
        let config = SpeculativeConfig {
            enabled: true,
            draft_model: Some("my-custom-draft".into()),
            show_metrics: true,
        };
        let draft = select_draft_model("claude-opus-4-20250514", &config);
        assert_eq!(draft, Some("my-custom-draft".to_string()));
    }

    #[test]
    fn test_select_draft_free_returns_none() {
        let config = SpeculativeConfig::default();
        let draft = select_draft_model("llama3.2", &config);
        assert!(draft.is_none());
    }

    #[test]
    fn test_default_config() {
        let config = SpeculativeConfig::default();
        assert!(!config.enabled);
        assert!(config.draft_model.is_none());
        assert!(config.show_metrics);
    }
}
