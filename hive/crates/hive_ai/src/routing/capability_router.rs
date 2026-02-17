//! Capability-Based Model Router
//!
//! Extends the existing routing system with model self-reported capability
//! scoring. While the [`ModelRouter`](super::model_router::ModelRouter)
//! classifies request *complexity* and picks a tier, this module classifies the
//! *task type* of a request and scores each available model on how well it can
//! handle that specific kind of work.
//!
//! # Integration with `ModelRouter`
//!
//! This module is designed to be composed with `ModelRouter`, **not** replace
//! it.  The recommended integration point is a new method on `ModelRouter`:
//!
//! ```rust,ignore
//! impl ModelRouter {
//!     /// Given a set of available models, return a capability-aware
//!     /// recommendation that considers both complexity tier and per-model
//!     /// strengths.
//!     pub fn route_with_capabilities(
//!         &self,
//!         messages: &[ChatMessage],
//!         available_models: &[ModelInfo],
//!         explicit_model: Option<&str>,
//!         context: Option<&ClassificationContext>,
//!     ) -> RoutingDecision {
//!         // If the user explicitly chose a model, respect that.
//!         if explicit_model.is_some() {
//!             return self.route(messages, explicit_model, context);
//!         }
//!
//!         // 1. Classify complexity to get the preferred tier.
//!         let complexity = self.classify(messages, context);
//!         let tier = complexity.tier;
//!
//!         // 2. Use CapabilityRouter to rank models for the detected task.
//!         let cap_router = CapabilityRouter::new();
//!         let recommendation = cap_router.recommend(
//!             messages,
//!             available_models,
//!             Some(tier),
//!         );
//!
//!         // 3. Build the final RoutingDecision, merging capability insight
//!         //    with the existing fallback / provider-health logic.
//!         RoutingDecision {
//!             provider: resolve_provider(&recommendation.model_id),
//!             model_id: recommendation.model_id,
//!             tier,
//!             reasoning: format!(
//!                 "{} | capability: {}",
//!                 complexity.reasoning,
//!                 recommendation.reasoning,
//!             ),
//!         }
//!     }
//! }
//! ```
//!
//! Alternatively, the `CapabilityRouter` can be used standalone to re-rank a
//! list of candidate models produced by any other selection mechanism.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::types::{ChatMessage, MessageRole, ModelInfo, ModelTier};

use super::auto_fallback::ProviderType;

// ---------------------------------------------------------------------------
// Task type classification
// ---------------------------------------------------------------------------

/// High-level task types inferred from user messages.
///
/// This enum is intentionally broader than
/// [`super::complexity_classifier::TaskType`] because it captures
/// capability-relevant categories (e.g. `Translation`, `DataAnalysis`,
/// `Agentic`) that do not map cleanly to a complexity tier but *do* differ
/// across models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTaskType {
    Coding,
    Reasoning,
    CreativeWriting,
    Math,
    InstructionFollowing,
    Translation,
    Summarization,
    DataAnalysis,
    ToolUse,
    Agentic,
    Vision,
    GeneralChat,
}

impl std::fmt::Display for CapabilityTaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Coding => "coding",
            Self::Reasoning => "reasoning",
            Self::CreativeWriting => "creative writing",
            Self::Math => "math",
            Self::InstructionFollowing => "instruction following",
            Self::Translation => "translation",
            Self::Summarization => "summarization",
            Self::DataAnalysis => "data analysis",
            Self::ToolUse => "tool use",
            Self::Agentic => "agentic",
            Self::Vision => "vision",
            Self::GeneralChat => "general chat",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Model strengths
// ---------------------------------------------------------------------------

/// Self-reported / benchmark-derived capability scores for a model family.
///
/// Each score is in the range `0.0..=1.0` where 1.0 represents
/// state-of-the-art performance in that dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStrengths {
    /// Prefix or substring used to match model IDs (case-insensitive).
    /// For example, `"claude-opus-4"` matches `"claude-opus-4-20250514"`.
    pub model_pattern: String,
    pub coding_score: f32,
    pub reasoning_score: f32,
    pub creative_writing_score: f32,
    pub math_score: f32,
    pub instruction_following: f32,
    pub multilingual_score: f32,
    pub long_context_score: f32,
    pub speed_score: f32,
    pub tool_use_score: f32,
    pub vision_score: f32,
    /// Multi-step autonomous task performance (agent loops, multi-file edits).
    pub agentic_score: f32,
}

impl ModelStrengths {
    /// Return the score most relevant to the given task type.
    pub fn score_for_task(&self, task: &CapabilityTaskType) -> f32 {
        match task {
            CapabilityTaskType::Coding => self.coding_score,
            CapabilityTaskType::Reasoning => self.reasoning_score,
            CapabilityTaskType::CreativeWriting => self.creative_writing_score,
            CapabilityTaskType::Math => self.math_score,
            CapabilityTaskType::InstructionFollowing => self.instruction_following,
            CapabilityTaskType::Translation => self.multilingual_score,
            CapabilityTaskType::Summarization => {
                // Summarization benefits from both instruction-following and
                // long-context handling.
                (self.instruction_following + self.long_context_score) / 2.0
            }
            CapabilityTaskType::DataAnalysis => {
                // Data analysis is a blend of reasoning + math + coding.
                (self.reasoning_score + self.math_score + self.coding_score) / 3.0
            }
            CapabilityTaskType::ToolUse => self.tool_use_score,
            CapabilityTaskType::Agentic => self.agentic_score,
            CapabilityTaskType::Vision => self.vision_score,
            CapabilityTaskType::GeneralChat => {
                // General chat: average of instruction-following and creative
                // ability to keep conversations engaging.
                (self.instruction_following + self.creative_writing_score) / 2.0
            }
        }
    }

    /// Check whether this entry matches a model ID (case-insensitive prefix
    /// or substring match).
    pub fn matches(&self, model_id: &str) -> bool {
        let id_lower = model_id.to_lowercase();
        let pattern_lower = self.model_pattern.to_lowercase();
        id_lower.contains(&pattern_lower)
    }
}

// ---------------------------------------------------------------------------
// Known model strengths database
// ---------------------------------------------------------------------------

/// Hardcoded capability scores derived from model documentation, published
/// benchmarks, and self-reported capabilities as of early 2025.
///
/// Scores are intentionally conservative; a missing capability (e.g. vision
/// for a text-only model) gets `0.0`.
///
/// Entries are ordered from most-specific pattern to least-specific so that
/// the first match wins in `lookup_strengths`.
static KNOWN_MODEL_STRENGTHS: Lazy<Vec<ModelStrengths>> = Lazy::new(|| {
    vec![
        // -----------------------------------------------------------------
        // Anthropic
        // -----------------------------------------------------------------
        ModelStrengths {
            model_pattern: "claude-opus-4".into(),
            coding_score: 0.97,
            reasoning_score: 0.98,
            creative_writing_score: 0.93,
            math_score: 0.95,
            instruction_following: 0.96,
            multilingual_score: 0.90,
            long_context_score: 0.95,
            speed_score: 0.4,
            tool_use_score: 0.98,
            vision_score: 0.92,
            agentic_score: 0.98,
        },
        ModelStrengths {
            model_pattern: "claude-sonnet-4".into(),
            coding_score: 0.94,
            reasoning_score: 0.93,
            creative_writing_score: 0.90,
            math_score: 0.92,
            instruction_following: 0.95,
            multilingual_score: 0.88,
            long_context_score: 0.93,
            speed_score: 0.7,
            tool_use_score: 0.95,
            vision_score: 0.90,
            agentic_score: 0.95,
        },
        ModelStrengths {
            model_pattern: "claude-3-5-haiku".into(),
            coding_score: 0.82,
            reasoning_score: 0.78,
            creative_writing_score: 0.80,
            math_score: 0.75,
            instruction_following: 0.88,
            multilingual_score: 0.80,
            long_context_score: 0.85,
            speed_score: 0.95,
            tool_use_score: 0.85,
            vision_score: 0.75,
            agentic_score: 0.70,
        },
        // Also match the "claude-haiku" naming variant
        ModelStrengths {
            model_pattern: "claude-haiku".into(),
            coding_score: 0.82,
            reasoning_score: 0.78,
            creative_writing_score: 0.80,
            math_score: 0.75,
            instruction_following: 0.88,
            multilingual_score: 0.80,
            long_context_score: 0.85,
            speed_score: 0.95,
            tool_use_score: 0.85,
            vision_score: 0.75,
            agentic_score: 0.70,
        },
        // -----------------------------------------------------------------
        // OpenAI
        // -----------------------------------------------------------------
        ModelStrengths {
            model_pattern: "gpt-4o-mini".into(),
            coding_score: 0.78,
            reasoning_score: 0.72,
            creative_writing_score: 0.75,
            math_score: 0.70,
            instruction_following: 0.85,
            multilingual_score: 0.82,
            long_context_score: 0.80,
            speed_score: 0.95,
            tool_use_score: 0.80,
            vision_score: 0.82,
            agentic_score: 0.60,
        },
        // gpt-4o (must come after gpt-4o-mini so the more-specific pattern
        // matches first)
        ModelStrengths {
            model_pattern: "gpt-4o".into(),
            coding_score: 0.92,
            reasoning_score: 0.90,
            creative_writing_score: 0.92,
            math_score: 0.88,
            instruction_following: 0.93,
            multilingual_score: 0.92,
            long_context_score: 0.85,
            speed_score: 0.75,
            tool_use_score: 0.92,
            vision_score: 0.95,
            agentic_score: 0.88,
        },
        ModelStrengths {
            model_pattern: "o3".into(),
            coding_score: 0.95,
            reasoning_score: 0.98,
            creative_writing_score: 0.70,
            math_score: 0.99,
            instruction_following: 0.90,
            multilingual_score: 0.85,
            long_context_score: 0.80,
            speed_score: 0.3,
            tool_use_score: 0.90,
            vision_score: 0.88,
            agentic_score: 0.92,
        },
        // -----------------------------------------------------------------
        // Google
        // -----------------------------------------------------------------
        ModelStrengths {
            model_pattern: "gemini-2.5-pro".into(),
            coding_score: 0.93,
            reasoning_score: 0.92,
            creative_writing_score: 0.88,
            math_score: 0.93,
            instruction_following: 0.91,
            multilingual_score: 0.94,
            long_context_score: 0.98,
            speed_score: 0.6,
            tool_use_score: 0.90,
            vision_score: 0.93,
            agentic_score: 0.90,
        },
        ModelStrengths {
            model_pattern: "gemini-2.5-flash".into(),
            coding_score: 0.85,
            reasoning_score: 0.82,
            creative_writing_score: 0.80,
            math_score: 0.82,
            instruction_following: 0.88,
            multilingual_score: 0.90,
            long_context_score: 0.95,
            speed_score: 0.92,
            tool_use_score: 0.85,
            vision_score: 0.88,
            agentic_score: 0.75,
        },
        // Older Gemini naming (matches gemini-1.5-pro, gemini-pro, etc.)
        ModelStrengths {
            model_pattern: "gemini-pro".into(),
            coding_score: 0.88,
            reasoning_score: 0.87,
            creative_writing_score: 0.85,
            math_score: 0.85,
            instruction_following: 0.89,
            multilingual_score: 0.91,
            long_context_score: 0.92,
            speed_score: 0.65,
            tool_use_score: 0.85,
            vision_score: 0.88,
            agentic_score: 0.80,
        },
        // -----------------------------------------------------------------
        // DeepSeek (via OpenRouter)
        // -----------------------------------------------------------------
        ModelStrengths {
            model_pattern: "deepseek-r1".into(),
            coding_score: 0.88,
            reasoning_score: 0.95,
            creative_writing_score: 0.65,
            math_score: 0.96,
            instruction_following: 0.80,
            multilingual_score: 0.78,
            long_context_score: 0.75,
            speed_score: 0.4,
            tool_use_score: 0.70,
            vision_score: 0.0,
            agentic_score: 0.65,
        },
        ModelStrengths {
            model_pattern: "deepseek-chat".into(),
            coding_score: 0.90,
            reasoning_score: 0.88,
            creative_writing_score: 0.78,
            math_score: 0.92,
            instruction_following: 0.85,
            multilingual_score: 0.80,
            long_context_score: 0.80,
            speed_score: 0.80,
            tool_use_score: 0.75,
            vision_score: 0.0,
            agentic_score: 0.70,
        },
        // Catch-all for other deepseek models
        ModelStrengths {
            model_pattern: "deepseek".into(),
            coding_score: 0.88,
            reasoning_score: 0.86,
            creative_writing_score: 0.72,
            math_score: 0.90,
            instruction_following: 0.82,
            multilingual_score: 0.78,
            long_context_score: 0.78,
            speed_score: 0.60,
            tool_use_score: 0.72,
            vision_score: 0.0,
            agentic_score: 0.65,
        },
        // -----------------------------------------------------------------
        // Meta Llama
        // -----------------------------------------------------------------
        ModelStrengths {
            model_pattern: "llama-3.3-70b".into(),
            coding_score: 0.82,
            reasoning_score: 0.80,
            creative_writing_score: 0.78,
            math_score: 0.78,
            instruction_following: 0.85,
            multilingual_score: 0.75,
            long_context_score: 0.80,
            speed_score: 0.85,
            tool_use_score: 0.72,
            vision_score: 0.0,
            agentic_score: 0.60,
        },
        // Catch-all for other llama models
        ModelStrengths {
            model_pattern: "llama".into(),
            coding_score: 0.75,
            reasoning_score: 0.72,
            creative_writing_score: 0.70,
            math_score: 0.68,
            instruction_following: 0.78,
            multilingual_score: 0.65,
            long_context_score: 0.70,
            speed_score: 0.88,
            tool_use_score: 0.60,
            vision_score: 0.0,
            agentic_score: 0.50,
        },
        // -----------------------------------------------------------------
        // Qwen
        // -----------------------------------------------------------------
        ModelStrengths {
            model_pattern: "qwen-2.5-72b".into(),
            coding_score: 0.88,
            reasoning_score: 0.85,
            creative_writing_score: 0.80,
            math_score: 0.90,
            instruction_following: 0.86,
            multilingual_score: 0.95,
            long_context_score: 0.82,
            speed_score: 0.75,
            tool_use_score: 0.78,
            vision_score: 0.0,
            agentic_score: 0.65,
        },
        ModelStrengths {
            model_pattern: "qwen".into(),
            coding_score: 0.82,
            reasoning_score: 0.80,
            creative_writing_score: 0.75,
            math_score: 0.85,
            instruction_following: 0.82,
            multilingual_score: 0.92,
            long_context_score: 0.78,
            speed_score: 0.78,
            tool_use_score: 0.72,
            vision_score: 0.0,
            agentic_score: 0.55,
        },
        // -----------------------------------------------------------------
        // Mistral
        // -----------------------------------------------------------------
        ModelStrengths {
            model_pattern: "mistral-large".into(),
            coding_score: 0.86,
            reasoning_score: 0.84,
            creative_writing_score: 0.82,
            math_score: 0.82,
            instruction_following: 0.88,
            multilingual_score: 0.92,
            long_context_score: 0.80,
            speed_score: 0.7,
            tool_use_score: 0.80,
            vision_score: 0.0,
            agentic_score: 0.65,
        },
        ModelStrengths {
            model_pattern: "mistral".into(),
            coding_score: 0.78,
            reasoning_score: 0.76,
            creative_writing_score: 0.78,
            math_score: 0.74,
            instruction_following: 0.82,
            multilingual_score: 0.88,
            long_context_score: 0.75,
            speed_score: 0.82,
            tool_use_score: 0.72,
            vision_score: 0.0,
            agentic_score: 0.55,
        },
    ]
});

/// Look up the [`ModelStrengths`] for a given model ID.
///
/// Returns the first matching entry from [`KNOWN_MODEL_STRENGTHS`], or `None`
/// for completely unknown models.
pub fn lookup_strengths(model_id: &str) -> Option<&'static ModelStrengths> {
    KNOWN_MODEL_STRENGTHS.iter().find(|s| s.matches(model_id))
}

// ---------------------------------------------------------------------------
// Task classification from messages
// ---------------------------------------------------------------------------

/// Keyword groups used by [`classify_task`]. Each entry is
/// `(CapabilityTaskType, &[keywords])`. The classifier checks the latest user
/// message for these keywords (case-insensitive) and returns the first match.
///
/// Order matters: more specific task types should appear first.
static TASK_KEYWORDS: Lazy<Vec<(CapabilityTaskType, Vec<&'static str>)>> = Lazy::new(|| {
    // ORDER MATTERS: more-specific / multi-word patterns must come before
    // generic single-word patterns. Within each category, list phrases that
    // could collide with other categories first so the intended category wins.
    vec![
        // Vision -- check early because if the user attaches an image
        // they usually say "look at this", "what's in this image", etc.
        (
            CapabilityTaskType::Vision,
            vec![
                "image", "picture", "screenshot", "photo",
                "look at this", "what's in this", "describe this image",
                "what do you see",
            ],
        ),
        // Tool use -- check before coding because tool invocations often
        // contain code-like syntax.
        (
            CapabilityTaskType::ToolUse,
            vec![
                "use the tool", "call the function", "invoke", "api call",
                "run the command", "execute the tool", "tool_call",
                "function_call",
            ],
        ),
        // Agentic -- multi-step autonomous work
        (
            CapabilityTaskType::Agentic,
            vec![
                "multi-step", "autonomous", "agent",
                "plan and execute", "workflow", "pipeline",
                "do all of", "complete the entire", "automate",
                "multi-file", "across files", "whole project",
                "refactor the entire", "migrate",
            ],
        ),
        // Math -- before coding because math problems can contain words
        // like "solve" or "function" in a mathematical sense.
        (
            CapabilityTaskType::Math,
            vec![
                "calculate", "compute", "solve", "equation", "proof",
                "prove", "theorem", "integral", "derivative", "matrix",
                "algebra", "probability", "statistics", "mathematical",
                "formula", "arithmetic", "calculus", "irrational",
            ],
        ),
        // Translation -- before reasoning because "explain in french"
        // should be translation, not reasoning.
        (
            CapabilityTaskType::Translation,
            vec![
                "translate", "translation", "in french", "in spanish",
                "in german", "in japanese", "in chinese", "in korean",
                "to english", "from english", "localize", "localization",
                "multilingual", "en espanol",
            ],
        ),
        // Summarization -- before creative writing because "article" is
        // in the creative writing list but "summarize this article" should
        // be summarization.
        (
            CapabilityTaskType::Summarization,
            vec![
                "summarize", "summary", "tl;dr", "tldr",
                "condense", "recap", "key points", "main points",
                "outline", "digest",
            ],
        ),
        // Creative writing -- before coding because "write a story" should
        // not match the generic "write" in coding context.
        (
            CapabilityTaskType::CreativeWriting,
            vec![
                "write a story", "poem", "creative", "fiction", "narrative",
                "blog post", "article", "essay", "draft", "copywriting",
                "dialogue", "screenplay", "lyrics", "story",
            ],
        ),
        // Reasoning -- before coding because "why" and "explain" are very
        // common in reasoning questions that happen to mention code topics.
        (
            CapabilityTaskType::Reasoning,
            vec![
                "why is", "why does", "why do", "why are", "why would",
                "explain why", "explain how", "analyze", "reason about",
                "compare", "evaluate", "trade-off", "tradeoff",
                "pros and cons", "argument", "logic", "deduce", "infer",
                "critical thinking",
            ],
        ),
        // Coding -- the most common task type; uses fairly specific
        // multi-word patterns first, then falls back to single keywords
        // that are strongly indicative of coding intent.
        (
            CapabilityTaskType::Coding,
            vec![
                "write a function", "fix this code", "code review",
                "fix the bug", "implement a", "implement the",
                "code", "function", "compile", "syntax",
                "algorithm", "debug", "refactor",
                "class", "method", "variable", "endpoint", "library",
                "framework", "component", "module", "crate", "package",
                "implement", "bug",
            ],
        ),
        // Data analysis
        (
            CapabilityTaskType::DataAnalysis,
            vec![
                "dataset", "csv", "json data", "analyze the data",
                "visualization", "plot", "graph", "metrics",
                "dashboard", "insights", "trends",
            ],
        ),
        // Instruction following (very generic; acts as catch-all before
        // GeneralChat when the user gives precise, imperative instructions)
        (
            CapabilityTaskType::InstructionFollowing,
            vec![
                "follow these instructions", "do exactly", "step 1",
                "format as", "output as", "strictly", "precisely",
                "make sure to",
            ],
        ),
    ]
});

/// Classify the primary task type from a conversation.
///
/// Inspects the latest user message (and, if present, the system prompt)
/// for keywords that indicate what kind of task this is. Falls back to
/// [`CapabilityTaskType::GeneralChat`] when no specific category matches.
///
/// Also inspects recent assistant messages for tool-call activity, which
/// biases classification toward `ToolUse` or `Agentic`.
pub fn classify_task(messages: &[ChatMessage]) -> CapabilityTaskType {
    let user_msg = latest_user_message(messages);
    let system_prompt = system_prompt(messages);
    let combined = format!("{} {}", system_prompt, user_msg);
    let lower = combined.to_lowercase();

    // Check for tool-call activity in recent assistant messages (last 5).
    let recent_tool_activity = messages
        .iter()
        .rev()
        .take(5)
        .any(|m| m.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty()));

    // If there is recent tool usage and the user message does not strongly
    // indicate a different task, bias toward ToolUse.
    if recent_tool_activity {
        // Still allow specific overrides (e.g. explicit math or coding).
        let explicit_override = keyword_classify(&lower);
        if let Some(task) = explicit_override {
            return task;
        }
        return CapabilityTaskType::ToolUse;
    }

    keyword_classify(&lower).unwrap_or(CapabilityTaskType::GeneralChat)
}

/// Try to match keywords from `TASK_KEYWORDS`. Returns the first matching
/// task type or `None`.
fn keyword_classify(text: &str) -> Option<CapabilityTaskType> {
    for (task_type, keywords) in TASK_KEYWORDS.iter() {
        if keywords.iter().any(|kw| text.contains(kw)) {
            return Some(*task_type);
        }
    }
    None
}

/// Extract the latest user message content.
fn latest_user_message(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::User)
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

/// Extract the system prompt (if any).
fn system_prompt(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .find(|m| m.role == MessageRole::System)
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Ranking
// ---------------------------------------------------------------------------

/// Tier weight multiplier applied when a model matches or exceeds the
/// preferred tier, or when it is below. Models at the preferred tier get the
/// full weight; models one tier above or below get a smaller penalty.
fn tier_weight(model_tier: ModelTier, preferred: Option<ModelTier>) -> f32 {
    let Some(pref) = preferred else {
        return 1.0;
    };

    fn tier_ord(t: ModelTier) -> i32 {
        match t {
            ModelTier::Free => 0,
            ModelTier::Budget => 1,
            ModelTier::Mid => 2,
            ModelTier::Premium => 3,
        }
    }

    let distance = (tier_ord(model_tier) - tier_ord(pref)).abs();
    match distance {
        0 => 1.0,       // exact match
        1 => 0.85,      // one tier away
        2 => 0.65,      // two tiers away
        _ => 0.50,      // three tiers away (e.g. Free vs Premium)
    }
}

/// Rank available models for a given task type.
///
/// Returns a list of `(ModelInfo, score)` sorted by score descending. The
/// score is `strength_for_task * tier_weight` so models that match the
/// preferred tier are favoured unless a different model dramatically
/// outperforms them for this specific task.
pub fn rank_models_for_task(
    task: &CapabilityTaskType,
    available_models: &[ModelInfo],
    tier_preference: Option<ModelTier>,
) -> Vec<(ModelInfo, f32)> {
    let mut scored: Vec<(ModelInfo, f32)> = available_models
        .iter()
        .map(|model| {
            let base_score = lookup_strengths(&model.id)
                .map(|s| s.score_for_task(task))
                .unwrap_or(0.5); // unknown models get a neutral baseline

            let tw = tier_weight(model.tier, tier_preference);
            let final_score = base_score * tw;

            (model.clone(), final_score)
        })
        .collect();

    // Sort descending by score, breaking ties by model name for stability.
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.id.cmp(&b.0.id))
    });

    scored
}

// ---------------------------------------------------------------------------
// RoutingRecommendation
// ---------------------------------------------------------------------------

/// The output of [`CapabilityRouter::recommend`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRecommendation {
    /// The model ID that was selected.
    pub model_id: String,
    /// The provider for the selected model.
    pub provider: ProviderType,
    /// The task type that was inferred from the messages.
    pub task_type: CapabilityTaskType,
    /// Capability score (0.0 - 1.0).
    pub score: f32,
    /// Human-readable explanation of the selection.
    pub reasoning: String,
}

// ---------------------------------------------------------------------------
// CapabilityRouter
// ---------------------------------------------------------------------------

/// A router that scores and ranks models based on their self-reported
/// capabilities relative to the inferred task type.
///
/// This is designed to be composed with the existing
/// [`ModelRouter`](super::model_router::ModelRouter) to add a
/// capability-awareness layer on top of complexity-based tier selection.
pub struct CapabilityRouter {
    /// Reference to the known model strengths. Currently always points at
    /// `KNOWN_MODEL_STRENGTHS`, but using a field allows future extension
    /// (e.g. loading custom profiles from a config file).
    strengths: &'static [ModelStrengths],
}

impl Default for CapabilityRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityRouter {
    /// Create a new `CapabilityRouter` backed by the built-in
    /// [`KNOWN_MODEL_STRENGTHS`] database.
    pub fn new() -> Self {
        Self {
            strengths: &KNOWN_MODEL_STRENGTHS,
        }
    }

    /// Classify the task type from a conversation.
    pub fn classify_task(&self, messages: &[ChatMessage]) -> CapabilityTaskType {
        classify_task(messages)
    }

    /// Look up the strengths entry for a model ID.
    pub fn get_strengths(&self, model_id: &str) -> Option<&ModelStrengths> {
        self.strengths.iter().find(|s| s.matches(model_id))
    }

    /// Produce a [`RoutingRecommendation`] for the given conversation and
    /// available model set.
    ///
    /// # Arguments
    ///
    /// * `messages` - The conversation history (system + user + assistant
    ///   messages).
    /// * `available_models` - The set of models the user has configured and
    ///   that are currently reachable.
    /// * `tier_preference` - Optional tier hint from the complexity
    ///   classifier. When set, models at this tier receive a score bonus.
    ///
    /// # Returns
    ///
    /// The top-ranked recommendation. If `available_models` is empty, the
    /// router falls back to a sensible default (Claude Sonnet 4).
    pub fn recommend(
        &self,
        messages: &[ChatMessage],
        available_models: &[ModelInfo],
        tier_preference: Option<ModelTier>,
    ) -> RoutingRecommendation {
        let task = classify_task(messages);

        debug!(%task, "Capability router: classified task");

        // Handle the empty-models edge case.
        if available_models.is_empty() {
            return self.default_recommendation(task);
        }

        let ranked = rank_models_for_task(&task, available_models, tier_preference);

        // The top entry is our recommendation.
        let (model, score) = ranked
            .first()
            .expect("rank_models_for_task always returns at least one entry when input is non-empty");

        let reasoning = self.build_reasoning(model, &task, *score);

        RoutingRecommendation {
            model_id: model.id.clone(),
            provider: self.resolve_provider_type(model),
            task_type: task,
            score: *score,
            reasoning,
        }
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    fn default_recommendation(&self, task: CapabilityTaskType) -> RoutingRecommendation {
        RoutingRecommendation {
            model_id: "claude-sonnet-4-20250514".into(),
            provider: ProviderType::Anthropic,
            task_type: task,
            score: 0.90,
            reasoning: format!(
                "No available models provided; defaulting to claude-sonnet-4 for {} task",
                task
            ),
        }
    }

    /// Build a human-readable reasoning string.
    fn build_reasoning(&self, model: &ModelInfo, task: &CapabilityTaskType, score: f32) -> String {
        let strengths_note = match self.get_strengths(&model.id) {
            Some(s) => {
                let raw = s.score_for_task(task);
                self.strength_description(task, raw)
            }
            None => "unknown capability profile".to_string(),
        };

        format!(
            "Selected {} for {} task (score: {:.2}) \u{2014} {}",
            model.id, task, score, strengths_note,
        )
    }

    /// Turn a raw score into a human-friendly description of why the model
    /// is good (or not) at the task.
    fn strength_description(&self, task: &CapabilityTaskType, raw_score: f32) -> String {
        let quality = if raw_score >= 0.95 {
            "state-of-the-art"
        } else if raw_score >= 0.90 {
            "excels at"
        } else if raw_score >= 0.80 {
            "strong at"
        } else if raw_score >= 0.65 {
            "adequate for"
        } else {
            "limited capability for"
        };

        let extra = match task {
            CapabilityTaskType::Coding => {
                "code generation and has strong tool-use capabilities"
            }
            CapabilityTaskType::Reasoning => {
                "deep analytical reasoning and multi-step logic"
            }
            CapabilityTaskType::CreativeWriting => {
                "creative prose, storytelling, and stylistic flexibility"
            }
            CapabilityTaskType::Math => {
                "mathematical problem-solving and formal proofs"
            }
            CapabilityTaskType::InstructionFollowing => {
                "precise instruction adherence and formatting"
            }
            CapabilityTaskType::Translation => {
                "multilingual translation and localization"
            }
            CapabilityTaskType::Summarization => {
                "distilling long documents into concise summaries"
            }
            CapabilityTaskType::DataAnalysis => {
                "data interpretation, pattern recognition, and quantitative reasoning"
            }
            CapabilityTaskType::ToolUse => {
                "structured tool invocation and function calling"
            }
            CapabilityTaskType::Agentic => {
                "multi-step autonomous tasks and cross-file changes"
            }
            CapabilityTaskType::Vision => {
                "image understanding and visual reasoning"
            }
            CapabilityTaskType::GeneralChat => {
                "conversational fluency and helpfulness"
            }
        };

        format!("{} {} \u{2014} {}", quality, task, extra)
    }

    /// Map a `ModelInfo` to a `ProviderType` from the auto_fallback module.
    fn resolve_provider_type(&self, model: &ModelInfo) -> ProviderType {
        // The model already carries a `provider_type` from `types.rs` but
        // it is `crate::types::ProviderType`. The routing subsystem uses
        // `auto_fallback::ProviderType`. We convert by matching the string
        // representation, which keeps both enums decoupled.
        match model.provider_type {
            crate::types::ProviderType::Anthropic => ProviderType::Anthropic,
            crate::types::ProviderType::OpenAI => ProviderType::OpenAI,
            crate::types::ProviderType::OpenRouter => ProviderType::OpenRouter,
            crate::types::ProviderType::Google => ProviderType::Google,
            crate::types::ProviderType::Groq => ProviderType::Groq,
            crate::types::ProviderType::LiteLLM => ProviderType::LiteLLM,
            crate::types::ProviderType::HuggingFace => ProviderType::HuggingFace,
            crate::types::ProviderType::Ollama => ProviderType::Ollama,
            crate::types::ProviderType::LMStudio => ProviderType::LMStudio,
            crate::types::ProviderType::GenericLocal => ProviderType::GenericLocal,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        MessageRole, ModelCapabilities, ModelInfo, ModelTier,
        ProviderType as TypesProviderType,
    };
    use chrono::Utc;

    // -- Helpers --

    fn user_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    fn system_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::System,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    fn assistant_msg_with_tool_calls(content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_call_id: None,
            tool_calls: Some(vec![crate::types::ToolCall {
                id: "call_1".into(),
                name: "read_file".into(),
                input: serde_json::json!({"path": "foo.rs"}),
            }]),
        }
    }

    fn make_model(id: &str, tier: ModelTier, provider: TypesProviderType) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            name: id.to_string(),
            provider: provider.to_string(),
            provider_type: provider,
            tier,
            context_window: 128_000,
            input_price_per_mtok: 0.0,
            output_price_per_mtok: 0.0,
            capabilities: ModelCapabilities::default(),
        }
    }

    fn sample_models() -> Vec<ModelInfo> {
        vec![
            make_model(
                "claude-opus-4-20250514",
                ModelTier::Premium,
                TypesProviderType::Anthropic,
            ),
            make_model(
                "claude-sonnet-4-20250514",
                ModelTier::Mid,
                TypesProviderType::Anthropic,
            ),
            make_model(
                "gpt-4o",
                ModelTier::Premium,
                TypesProviderType::OpenAI,
            ),
            make_model(
                "gpt-4o-mini",
                ModelTier::Mid,
                TypesProviderType::OpenAI,
            ),
            make_model(
                "deepseek/deepseek-chat",
                ModelTier::Budget,
                TypesProviderType::OpenRouter,
            ),
            make_model(
                "gemini-2.5-pro",
                ModelTier::Premium,
                TypesProviderType::Google,
            ),
            make_model(
                "o3",
                ModelTier::Premium,
                TypesProviderType::OpenAI,
            ),
        ]
    }

    // -- Task classification tests --

    #[test]
    fn classify_coding_task() {
        let msgs = vec![user_msg("Write a function to sort a list in Rust")];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::Coding);
    }

    #[test]
    fn classify_math_task() {
        let msgs = vec![user_msg("Solve this equation: 3x + 5 = 20")];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::Math);
    }

    #[test]
    fn classify_creative_writing_task() {
        let msgs = vec![user_msg("Write a story about a dragon and a knight")];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::CreativeWriting);
    }

    #[test]
    fn classify_reasoning_task() {
        let msgs = vec![user_msg("Why is Rust safer than C?")];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::Reasoning);
    }

    #[test]
    fn classify_translation_task() {
        let msgs = vec![user_msg("Translate this sentence to French: Hello, how are you?")];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::Translation);
    }

    #[test]
    fn classify_summarization_task() {
        let msgs = vec![user_msg("Summarize this document for me")];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::Summarization);
    }

    #[test]
    fn classify_agentic_task() {
        let msgs = vec![user_msg(
            "Refactor the entire authentication module across files and migrate to the new API",
        )];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::Agentic);
    }

    #[test]
    fn classify_tool_use_from_history() {
        let msgs = vec![
            user_msg("Read my config file"),
            assistant_msg_with_tool_calls("Sure, let me read that."),
            user_msg("Now update it"),
        ];
        // Recent tool call activity should bias toward ToolUse unless there
        // is a stronger keyword match in the latest message.
        let task = classify_task(&msgs);
        // "update" is generic enough that tool-use bias should win.
        assert_eq!(task, CapabilityTaskType::ToolUse);
    }

    #[test]
    fn classify_general_chat_fallback() {
        let msgs = vec![user_msg("Hello, how's your day going?")];
        assert_eq!(classify_task(&msgs), CapabilityTaskType::GeneralChat);
    }

    #[test]
    fn classify_with_system_prompt_context() {
        let msgs = vec![
            system_msg("You are a math tutor. Help students solve equations."),
            user_msg("Help me with this problem"),
        ];
        // "solve" + "equation" in system prompt should trigger Math
        assert_eq!(classify_task(&msgs), CapabilityTaskType::Math);
    }

    // -- Strength lookup tests --

    #[test]
    fn lookup_claude_opus() {
        let s = lookup_strengths("claude-opus-4-20250514").expect("should find claude opus");
        assert!(s.coding_score >= 0.95);
        assert!(s.agentic_score >= 0.95);
    }

    #[test]
    fn lookup_gpt4o_mini_before_gpt4o() {
        // gpt-4o-mini should match its specific entry, not the broader gpt-4o
        let s = lookup_strengths("gpt-4o-mini").expect("should find gpt-4o-mini");
        assert!(s.speed_score >= 0.90, "mini should be fast");
        assert!(
            s.coding_score < 0.85,
            "mini should have lower coding than full gpt-4o"
        );
    }

    #[test]
    fn lookup_deepseek_r1() {
        let s = lookup_strengths("deepseek/deepseek-r1").expect("should find deepseek-r1");
        assert!(s.reasoning_score >= 0.93);
        assert_eq!(s.vision_score, 0.0, "deepseek-r1 has no vision");
    }

    #[test]
    fn lookup_unknown_model_returns_none() {
        assert!(lookup_strengths("totally-unknown-model-xyz").is_none());
    }

    // -- Ranking tests --

    #[test]
    fn rank_coding_prefers_opus() {
        let models = sample_models();
        let ranked = rank_models_for_task(
            &CapabilityTaskType::Coding,
            &models,
            Some(ModelTier::Premium),
        );
        assert!(!ranked.is_empty());
        // Top model should be opus or o3 for coding at premium tier
        let top_id = &ranked[0].0.id;
        assert!(
            top_id.contains("opus") || top_id.contains("o3"),
            "Expected opus or o3 at top for coding, got {}",
            top_id,
        );
    }

    #[test]
    fn rank_math_prefers_o3() {
        let models = sample_models();
        let ranked = rank_models_for_task(
            &CapabilityTaskType::Math,
            &models,
            Some(ModelTier::Premium),
        );
        assert!(!ranked.is_empty());
        assert_eq!(
            ranked[0].0.id, "o3",
            "o3 should be top for math at premium tier"
        );
    }

    #[test]
    fn rank_with_budget_preference_penalizes_premium() {
        let models = sample_models();
        let ranked = rank_models_for_task(
            &CapabilityTaskType::Coding,
            &models,
            Some(ModelTier::Budget),
        );
        assert!(!ranked.is_empty());
        // Budget-tier deepseek should rank higher than if we preferred Premium
        let deepseek_pos = ranked.iter().position(|r| r.0.id.contains("deepseek"));
        let opus_pos = ranked.iter().position(|r| r.0.id.contains("opus"));
        if let (Some(ds), Some(op)) = (deepseek_pos, opus_pos) {
            assert!(
                ds < op,
                "With budget preference, deepseek ({}) should rank above opus ({})",
                ds,
                op,
            );
        }
    }

    #[test]
    fn rank_vision_filters_blind_models() {
        let models = sample_models();
        let ranked = rank_models_for_task(
            &CapabilityTaskType::Vision,
            &models,
            Some(ModelTier::Premium),
        );
        // Models with vision_score 0.0 should be at the bottom
        let deepseek_pos = ranked.iter().position(|r| r.0.id.contains("deepseek"));
        let gpt4o_pos = ranked.iter().position(|r| r.0.id == "gpt-4o");
        if let (Some(ds), Some(gpt)) = (deepseek_pos, gpt4o_pos) {
            assert!(
                gpt < ds,
                "gpt-4o (vision 0.95) should rank above deepseek (vision 0.0)"
            );
        }
    }

    #[test]
    fn rank_unknown_models_get_neutral_score() {
        let models = vec![make_model(
            "some-custom-local-model",
            ModelTier::Free,
            TypesProviderType::Ollama,
        )];
        let ranked = rank_models_for_task(
            &CapabilityTaskType::Coding,
            &models,
            None,
        );
        assert_eq!(ranked.len(), 1);
        // Unknown models get 0.5 base * 1.0 tier_weight = 0.5
        assert!((ranked[0].1 - 0.5).abs() < 0.01);
    }

    // -- CapabilityRouter integration tests --

    #[test]
    fn recommend_coding_task() {
        let router = CapabilityRouter::new();
        let models = sample_models();
        let msgs = vec![user_msg("Implement a binary search function in Rust")];
        let rec = router.recommend(&msgs, &models, Some(ModelTier::Premium));
        assert_eq!(rec.task_type, CapabilityTaskType::Coding);
        assert!(rec.score > 0.8);
        assert!(rec.reasoning.contains("coding"));
    }

    #[test]
    fn recommend_empty_models_falls_back() {
        let router = CapabilityRouter::new();
        let msgs = vec![user_msg("Hello")];
        let rec = router.recommend(&msgs, &[], None);
        assert_eq!(rec.model_id, "claude-sonnet-4-20250514");
        assert!(rec.reasoning.contains("defaulting"));
    }

    #[test]
    fn recommend_produces_human_readable_reasoning() {
        let router = CapabilityRouter::new();
        let models = sample_models();
        let msgs = vec![user_msg("Solve the equation 3x^2 + 5x - 2 = 0")];
        let rec = router.recommend(&msgs, &models, Some(ModelTier::Premium));
        assert_eq!(rec.task_type, CapabilityTaskType::Math);
        // The reasoning should mention the model, the task, and a score.
        assert!(rec.reasoning.contains("math"), "Reasoning: {}", rec.reasoning);
        assert!(rec.reasoning.contains("score:"), "Reasoning: {}", rec.reasoning);
    }

    // -- Tier weight tests --

    #[test]
    fn tier_weight_exact_match_is_one() {
        assert_eq!(tier_weight(ModelTier::Premium, Some(ModelTier::Premium)), 1.0);
        assert_eq!(tier_weight(ModelTier::Budget, Some(ModelTier::Budget)), 1.0);
    }

    #[test]
    fn tier_weight_one_away_is_reduced() {
        let w = tier_weight(ModelTier::Mid, Some(ModelTier::Premium));
        assert!(w < 1.0 && w > 0.5, "Weight was {}", w);
    }

    #[test]
    fn tier_weight_none_preference_is_neutral() {
        assert_eq!(tier_weight(ModelTier::Premium, None), 1.0);
        assert_eq!(tier_weight(ModelTier::Free, None), 1.0);
    }

    // -- Score-for-task tests --

    #[test]
    fn score_for_summarization_is_blended() {
        let s = lookup_strengths("claude-opus-4-20250514").unwrap();
        let score = s.score_for_task(&CapabilityTaskType::Summarization);
        let expected = (s.instruction_following + s.long_context_score) / 2.0;
        assert!((score - expected).abs() < 0.001);
    }

    #[test]
    fn score_for_data_analysis_is_blended() {
        let s = lookup_strengths("o3").unwrap();
        let score = s.score_for_task(&CapabilityTaskType::DataAnalysis);
        let expected = (s.reasoning_score + s.math_score + s.coding_score) / 3.0;
        assert!((score - expected).abs() < 0.001);
    }
}
