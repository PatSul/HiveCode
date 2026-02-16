//! Context window management — tracks token usage and prunes messages
//! to stay within model-specific context limits.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Rough token estimate: ~4 characters per token for English text.
/// This is intentionally conservative (overestimates) to avoid truncation.
const CHARS_PER_TOKEN: usize = 4;

/// Estimate token count for a string.
pub fn estimate_tokens(text: &str) -> usize {
    // Use character count / 4 as a rough approximation.
    // More accurate would be tiktoken, but this avoids a heavy dependency.
    text.len().div_ceil(CHARS_PER_TOKEN)
}

// ---------------------------------------------------------------------------
// Context window
// ---------------------------------------------------------------------------

/// A message in the context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    pub tokens: usize,
    pub pinned: bool,
}

impl ContextMessage {
    /// Creates a new context message with an automatically estimated token count.
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = estimate_tokens(&content);
        Self {
            role: role.into(),
            content,
            tokens,
            pinned: false,
        }
    }

    /// Marks this message as pinned so it survives context pruning.
    pub fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }
}

/// Manages the context window for a conversation.
///
/// Keeps track of messages and their estimated token counts,
/// pruning oldest non-pinned messages when the limit is reached.
pub struct ContextWindow {
    messages: Vec<ContextMessage>,
    max_tokens: usize,
    system_prompt_tokens: usize,
}

impl ContextWindow {
    /// Creates a new context window with the given maximum token budget.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
            system_prompt_tokens: 0,
        }
    }

    /// Set the system prompt (counts toward the token budget).
    pub fn set_system_prompt(&mut self, prompt: &str) {
        self.system_prompt_tokens = estimate_tokens(prompt);
    }

    /// Add a message to the context. May trigger pruning.
    pub fn push(&mut self, message: ContextMessage) {
        self.messages.push(message);
        self.prune();
    }

    /// Total estimated tokens across all messages + system prompt.
    pub fn total_tokens(&self) -> usize {
        self.system_prompt_tokens + self.messages.iter().map(|m| m.tokens).sum::<usize>()
    }

    /// Available tokens remaining.
    pub fn available_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.total_tokens())
    }

    /// Number of messages in the window.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get all messages in order.
    pub fn messages(&self) -> &[ContextMessage] {
        &self.messages
    }

    /// Usage as a percentage (0.0 to 1.0+).
    pub fn usage_pct(&self) -> f64 {
        if self.max_tokens == 0 {
            return 1.0;
        }
        self.total_tokens() as f64 / self.max_tokens as f64
    }

    /// Whether the context is over budget.
    pub fn is_over_budget(&self) -> bool {
        self.total_tokens() > self.max_tokens
    }

    /// Prune oldest non-pinned messages until within budget.
    /// Uses a single-pass `retain()` instead of repeated `Vec::remove()` to
    /// avoid O(n^2) shifting.
    fn prune(&mut self) {
        let total = self.total_tokens();
        if total <= self.max_tokens {
            return;
        }
        let mut budget = total - self.max_tokens; // tokens we need to shed
        self.messages.retain(|m| {
            if budget == 0 || m.pinned {
                return true;
            }
            budget = budget.saturating_sub(m.tokens);
            false
        });
    }

    /// Clear all messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Summarize context state for debugging.
    pub fn summary(&self) -> ContextSummary {
        ContextSummary {
            message_count: self.messages.len(),
            total_tokens: self.total_tokens(),
            max_tokens: self.max_tokens,
            available_tokens: self.available_tokens(),
            usage_pct: self.usage_pct(),
            pinned_count: self.messages.iter().filter(|m| m.pinned).count(),
        }
    }
}

/// Summary of context window state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub message_count: usize,
    pub total_tokens: usize,
    pub max_tokens: usize,
    pub available_tokens: usize,
    pub usage_pct: f64,
    pub pinned_count: usize,
}

// ---------------------------------------------------------------------------
// Common model context sizes
// ---------------------------------------------------------------------------

/// Get the default context window size for a model.
pub fn model_context_size(model_id: &str) -> usize {
    match model_id {
        // Anthropic
        "claude-opus-4" | "claude-sonnet-4" => 200_000,
        "claude-haiku-3.5" => 200_000,
        // OpenAI
        "gpt-4o" | "gpt-4o-mini" => 128_000,
        "o1" | "o1-mini" => 128_000,
        // Local / small
        "llama3.2" | "llama3.2:latest" => 128_000,
        "mistral" | "mistral:latest" => 32_000,
        "codellama" | "codellama:latest" => 16_000,
        // Default
        _ => 8_000,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_basic() {
        // "hello" = 5 chars → ceil(5/4) = 2 tokens
        assert_eq!(estimate_tokens("hello"), 2);
        // Empty string = 0
        assert_eq!(estimate_tokens(""), 0);
        // 100 chars → 25 tokens
        let s = "a".repeat(100);
        assert_eq!(estimate_tokens(&s), 25);
    }

    #[test]
    fn context_message_creation() {
        let msg = ContextMessage::new("user", "Hello, how are you?");
        assert_eq!(msg.role, "user");
        assert!(!msg.pinned);
        assert!(msg.tokens > 0);
    }

    #[test]
    fn context_message_pinned() {
        let msg = ContextMessage::new("system", "Important").pinned();
        assert!(msg.pinned);
    }

    #[test]
    fn context_window_basic() {
        let mut ctx = ContextWindow::new(1000);
        assert_eq!(ctx.message_count(), 0);
        assert_eq!(ctx.total_tokens(), 0);
        assert_eq!(ctx.available_tokens(), 1000);

        ctx.push(ContextMessage::new("user", "Hello"));
        assert_eq!(ctx.message_count(), 1);
        assert!(ctx.total_tokens() > 0);
    }

    #[test]
    fn context_window_with_system_prompt() {
        let mut ctx = ContextWindow::new(100);
        ctx.set_system_prompt("You are a helpful assistant.");
        let base_tokens = ctx.total_tokens();
        assert!(base_tokens > 0);

        ctx.push(ContextMessage::new("user", "Hi"));
        assert!(ctx.total_tokens() > base_tokens);
    }

    #[test]
    fn context_window_pruning() {
        // Small context window — 50 tokens
        let mut ctx = ContextWindow::new(50);

        // Add messages that together exceed 50 tokens
        ctx.push(ContextMessage::new("user", "a".repeat(100))); // ~25 tokens
        assert_eq!(ctx.message_count(), 1);

        ctx.push(ContextMessage::new("assistant", "b".repeat(100))); // ~25 tokens
        assert_eq!(ctx.message_count(), 2);

        // This should trigger pruning — total would be ~75 tokens
        ctx.push(ContextMessage::new("user", "c".repeat(100))); // ~25 tokens
        assert!(ctx.total_tokens() <= 50);
        // At least one old message should have been removed
        assert!(ctx.message_count() <= 2);
    }

    #[test]
    fn context_window_pinned_messages_survive_pruning() {
        let mut ctx = ContextWindow::new(30);

        // Add a pinned message
        ctx.push(ContextMessage::new("system", "a".repeat(40)).pinned()); // ~10 tokens
        // Add unpinned messages
        ctx.push(ContextMessage::new("user", "b".repeat(40))); // ~10 tokens
        ctx.push(ContextMessage::new("assistant", "c".repeat(40))); // ~10 tokens

        // When pruning happens, pinned message should survive
        let pinned_count = ctx.messages().iter().filter(|m| m.pinned).count();
        assert!(pinned_count >= 1);

        // The pinned message should still be there
        assert!(
            ctx.messages()
                .iter()
                .any(|m| m.pinned && m.role == "system")
        );
    }

    #[test]
    fn context_window_usage_pct() {
        let mut ctx = ContextWindow::new(100);
        assert!((ctx.usage_pct() - 0.0).abs() < f64::EPSILON);

        ctx.push(ContextMessage::new("user", "a".repeat(200))); // ~50 tokens
        assert!(ctx.usage_pct() > 0.0);
        assert!(ctx.usage_pct() <= 1.0);
    }

    #[test]
    fn context_window_clear() {
        let mut ctx = ContextWindow::new(1000);
        ctx.push(ContextMessage::new("user", "Hello"));
        ctx.push(ContextMessage::new("assistant", "Hi"));
        assert_eq!(ctx.message_count(), 2);

        ctx.clear();
        assert_eq!(ctx.message_count(), 0);
    }

    #[test]
    fn context_window_summary() {
        let mut ctx = ContextWindow::new(1000);
        ctx.push(ContextMessage::new("user", "Hello").pinned());
        ctx.push(ContextMessage::new("assistant", "Hi"));

        let summary = ctx.summary();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.max_tokens, 1000);
        assert_eq!(summary.pinned_count, 1);
        assert!(summary.total_tokens > 0);
        assert!(summary.available_tokens < 1000);
    }

    #[test]
    fn model_context_sizes() {
        assert_eq!(model_context_size("claude-opus-4"), 200_000);
        assert_eq!(model_context_size("gpt-4o"), 128_000);
        assert_eq!(model_context_size("mistral"), 32_000);
        assert_eq!(model_context_size("unknown-model"), 8_000);
    }

    #[test]
    fn context_window_zero_capacity() {
        let mut ctx = ContextWindow::new(0);
        assert!(ctx.is_over_budget() || ctx.total_tokens() == 0);
        ctx.push(ContextMessage::new("user", "hello"));
        // Should prune immediately, but if all messages are needed it stays over
        assert!(ctx.is_over_budget() || ctx.message_count() == 0);
    }
}
