//! Cost tracking and token estimation for AI model usage.
//!
//! Provides token counting (heuristic), cost calculation from model pricing,
//! budget tracking with daily/monthly limits, and cost history.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::model_registry::MODEL_REGISTRY;

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate token count from text using a character-based heuristic.
///
/// Uses ~4 characters per token for English text (GPT/Claude average).
/// This is a rough estimate — actual tokenization varies by model.
pub fn estimate_tokens(text: &str) -> usize {
    // ~4 chars per token for English, slightly less for code
    let chars = text.len();
    (chars + 3) / 4 // round up
}

/// Estimate tokens for a chat message (role + content).
pub fn estimate_message_tokens(role: &str, content: &str) -> usize {
    // ~4 tokens overhead per message (role, formatting)
    4 + estimate_tokens(role) + estimate_tokens(content)
}

/// Estimate total tokens for a conversation (system prompt + messages).
pub fn estimate_conversation_tokens(
    system_prompt: Option<&str>,
    messages: &[(&str, &str)], // (role, content) pairs
) -> usize {
    let system_tokens = system_prompt.map(|s| estimate_tokens(s) + 4).unwrap_or(0);
    let message_tokens: usize = messages
        .iter()
        .map(|(role, content)| estimate_message_tokens(role, content))
        .sum();
    system_tokens + message_tokens + 3 // 3 tokens for conversation framing
}

// ---------------------------------------------------------------------------
// Cost calculation
// ---------------------------------------------------------------------------

/// Calculate the cost of a request given input/output token counts and model ID.
///
/// Returns `(input_cost, output_cost, total_cost)` in USD.
pub fn calculate_cost(model_id: &str, input_tokens: usize, output_tokens: usize) -> CostBreakdown {
    let (input_rate, output_rate) = MODEL_REGISTRY
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| (m.input_price_per_mtok, m.output_price_per_mtok))
        .unwrap_or((0.0, 0.0)); // unknown model = free (local)

    let input_cost = (input_tokens as f64 / 1_000_000.0) * input_rate;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * output_rate;

    CostBreakdown {
        input_tokens,
        output_tokens,
        input_cost,
        output_cost,
        total_cost: input_cost + output_cost,
        model_id: model_id.to_string(),
    }
}

/// Predict the cost of a request before sending it.
///
/// Estimates output tokens as 2x the input (typical for chat responses).
pub fn predict_cost(model_id: &str, input_text: &str) -> CostBreakdown {
    let input_tokens = estimate_tokens(input_text);
    let estimated_output = input_tokens * 2; // rough heuristic
    calculate_cost(model_id, input_tokens, estimated_output)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Breakdown of a single request's cost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub input_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
    pub model_id: String,
}

/// A recorded cost event from a completed request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecord {
    pub timestamp: DateTime<Utc>,
    pub model_id: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost: f64,
}

/// Budget limits for cost control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetLimits {
    pub daily_limit: Option<f64>,
    pub monthly_limit: Option<f64>,
}

impl Default for BudgetLimits {
    fn default() -> Self {
        Self {
            daily_limit: None,
            monthly_limit: None,
        }
    }
}

// ---------------------------------------------------------------------------
// CostTracker
// ---------------------------------------------------------------------------

/// Tracks cumulative cost across sessions with budget enforcement.
pub struct CostTracker {
    records: Vec<CostRecord>,
    budget: BudgetLimits,
}

impl CostTracker {
    /// Create a new tracker with optional budget limits.
    pub fn new(budget: BudgetLimits) -> Self {
        Self {
            records: Vec::new(),
            budget,
        }
    }

    /// Record a completed request's cost.
    pub fn record(&mut self, model_id: &str, input_tokens: usize, output_tokens: usize) {
        let breakdown = calculate_cost(model_id, input_tokens, output_tokens);
        self.records.push(CostRecord {
            timestamp: Utc::now(),
            model_id: model_id.to_string(),
            input_tokens,
            output_tokens,
            cost: breakdown.total_cost,
        });
    }

    /// Total cost of all recorded requests.
    pub fn total_cost(&self) -> f64 {
        self.records.iter().map(|r| r.cost).sum()
    }

    /// Total cost for today (UTC).
    pub fn today_cost(&self) -> f64 {
        let today = Utc::now().date_naive();
        self.cost_for_date(today)
    }

    /// Total cost for a specific date (UTC).
    pub fn cost_for_date(&self, date: NaiveDate) -> f64 {
        self.records
            .iter()
            .filter(|r| r.timestamp.date_naive() == date)
            .map(|r| r.cost)
            .sum()
    }

    /// Total cost for the current month (UTC).
    pub fn month_cost(&self) -> f64 {
        let now = Utc::now();
        let (year, month) = (now.date_naive().year(), now.date_naive().month());
        self.records
            .iter()
            .filter(|r| {
                let d = r.timestamp.date_naive();
                d.year() == year && d.month() == month
            })
            .map(|r| r.cost)
            .sum()
    }

    /// Total API calls recorded.
    pub fn total_calls(&self) -> usize {
        self.records.len()
    }

    /// Total input tokens across all requests.
    pub fn total_input_tokens(&self) -> usize {
        self.records.iter().map(|r| r.input_tokens).sum()
    }

    /// Total output tokens across all requests.
    pub fn total_output_tokens(&self) -> usize {
        self.records.iter().map(|r| r.output_tokens).sum()
    }

    /// Cost breakdown by model.
    pub fn cost_by_model(&self) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        for record in &self.records {
            *map.entry(record.model_id.clone()).or_insert(0.0) += record.cost;
        }
        map
    }

    /// Daily cost history (date -> cost).
    pub fn daily_history(&self) -> Vec<(NaiveDate, f64)> {
        let mut map: HashMap<NaiveDate, f64> = HashMap::new();
        for record in &self.records {
            let date = record.timestamp.date_naive();
            *map.entry(date).or_insert(0.0) += record.cost;
        }
        let mut days: Vec<_> = map.into_iter().collect();
        days.sort_by_key(|(date, _)| *date);
        days
    }

    /// Check if today's budget is exceeded.
    pub fn is_daily_budget_exceeded(&self) -> bool {
        match self.budget.daily_limit {
            Some(limit) => self.today_cost() >= limit,
            None => false,
        }
    }

    /// Check if monthly budget is exceeded.
    pub fn is_monthly_budget_exceeded(&self) -> bool {
        match self.budget.monthly_limit {
            Some(limit) => self.month_cost() >= limit,
            None => false,
        }
    }

    /// Remaining daily budget (None if no limit set).
    pub fn daily_remaining(&self) -> Option<f64> {
        self.budget
            .daily_limit
            .map(|limit| (limit - self.today_cost()).max(0.0))
    }

    /// Reset today's records.
    pub fn reset_today(&mut self) {
        let today = Utc::now().date_naive();
        self.records.retain(|r| r.timestamp.date_naive() != today);
    }

    /// Clear all records.
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Get all records (for CSV export, etc.).
    pub fn records(&self) -> &[CostRecord] {
        &self.records
    }

    /// Update budget limits.
    pub fn set_budget(&mut self, budget: BudgetLimits) {
        self.budget = budget;
    }

    /// Export records as CSV string.
    pub fn export_csv(&self) -> String {
        let mut csv = String::from("timestamp,model_id,input_tokens,output_tokens,cost\n");
        for r in &self.records {
            csv.push_str(&format!(
                "{},{},{},{},{:.6}\n",
                r.timestamp.to_rfc3339(),
                r.model_id,
                r.input_tokens,
                r.output_tokens,
                r.cost
            ));
        }
        csv
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new(BudgetLimits::default())
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
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hi"), 1); // 2 chars -> 1 token
        assert_eq!(estimate_tokens("hello world!"), 3); // 12 chars -> 3 tokens
    }

    #[test]
    fn estimate_tokens_longer_text() {
        let text = "The quick brown fox jumps over the lazy dog"; // 43 chars
        let tokens = estimate_tokens(text);
        assert!(tokens >= 10 && tokens <= 12); // ~11 tokens
    }

    #[test]
    fn estimate_message_tokens_includes_overhead() {
        let msg_tokens = estimate_message_tokens("user", "hello");
        let raw_tokens = estimate_tokens("user") + estimate_tokens("hello");
        assert_eq!(msg_tokens, raw_tokens + 4); // 4 tokens overhead
    }

    #[test]
    fn estimate_conversation_tokens_with_system() {
        let tokens = estimate_conversation_tokens(
            Some("You are helpful"),
            &[("user", "hello"), ("assistant", "Hi there!")],
        );
        assert!(tokens > 0);
        // Should be > than just message tokens due to system prompt + framing
        let msg_only = estimate_conversation_tokens(None, &[("user", "hello")]);
        assert!(tokens > msg_only);
    }

    #[test]
    fn calculate_cost_known_model() {
        let breakdown = calculate_cost("claude-haiku-4-5-20251001", 1_000, 500);
        // Haiku: $0.80/Mtok input, $4.00/Mtok output
        assert!(breakdown.input_cost > 0.0);
        assert!(breakdown.output_cost > 0.0);
        assert_eq!(
            breakdown.total_cost,
            breakdown.input_cost + breakdown.output_cost
        );
    }

    #[test]
    fn calculate_cost_unknown_model_is_zero() {
        let breakdown = calculate_cost("local-llama", 1_000, 500);
        assert_eq!(breakdown.total_cost, 0.0);
    }

    #[test]
    fn predict_cost_produces_estimate() {
        let prediction = predict_cost("claude-sonnet-4-5-20250929", "Hello, how are you?");
        assert!(prediction.input_tokens > 0);
        assert!(prediction.output_tokens > 0);
        assert!(prediction.total_cost > 0.0);
    }

    #[test]
    fn cost_tracker_record_and_total() {
        let mut tracker = CostTracker::default();
        tracker.record("claude-haiku-4-5-20251001", 1000, 500);
        tracker.record("claude-haiku-4-5-20251001", 2000, 1000);

        assert_eq!(tracker.total_calls(), 2);
        assert_eq!(tracker.total_input_tokens(), 3000);
        assert_eq!(tracker.total_output_tokens(), 1500);
        assert!(tracker.total_cost() > 0.0);
    }

    #[test]
    fn cost_tracker_today_cost() {
        let mut tracker = CostTracker::default();
        tracker.record("claude-haiku-4-5-20251001", 1000, 500);
        assert!(tracker.today_cost() > 0.0);
        assert_eq!(tracker.today_cost(), tracker.total_cost());
    }

    #[test]
    fn cost_tracker_by_model() {
        let mut tracker = CostTracker::default();
        tracker.record("claude-haiku-4-5-20251001", 1000, 500);
        tracker.record("claude-sonnet-4-5-20250929", 1000, 500);
        tracker.record("claude-haiku-4-5-20251001", 500, 200);

        let by_model = tracker.cost_by_model();
        assert_eq!(by_model.len(), 2);
        assert!(by_model.contains_key("claude-haiku-4-5-20251001"));
        assert!(by_model.contains_key("claude-sonnet-4-5-20250929"));
    }

    #[test]
    fn cost_tracker_budget_not_exceeded_when_no_limit() {
        let tracker = CostTracker::default();
        assert!(!tracker.is_daily_budget_exceeded());
        assert!(!tracker.is_monthly_budget_exceeded());
        assert!(tracker.daily_remaining().is_none());
    }

    #[test]
    fn cost_tracker_budget_exceeded() {
        let budget = BudgetLimits {
            daily_limit: Some(0.001), // very low
            monthly_limit: Some(0.001),
        };
        let mut tracker = CostTracker::new(budget);
        // Claude Opus 4.6: $5/Mtok input = $0.005 for 1000 tokens
        tracker.record("claude-opus-4-6", 1000, 0);

        assert!(tracker.is_daily_budget_exceeded());
        assert!(tracker.is_monthly_budget_exceeded());
    }

    #[test]
    fn cost_tracker_daily_remaining() {
        let budget = BudgetLimits {
            daily_limit: Some(1.0),
            monthly_limit: None,
        };
        let mut tracker = CostTracker::new(budget);
        tracker.record("local-model", 1000, 500); // free model

        let remaining = tracker.daily_remaining().unwrap();
        assert!((remaining - 1.0).abs() < 0.01); // ~$1 remaining
    }

    #[test]
    fn cost_tracker_clear_and_reset() {
        let mut tracker = CostTracker::default();
        tracker.record("claude-haiku-4-5-20251001", 1000, 500);
        assert!(tracker.total_calls() > 0);

        tracker.reset_today();
        assert_eq!(tracker.total_calls(), 0);

        tracker.record("claude-haiku-4-5-20251001", 1000, 500);
        tracker.clear();
        assert_eq!(tracker.total_calls(), 0);
        assert_eq!(tracker.total_cost(), 0.0);
    }

    #[test]
    fn cost_tracker_export_csv() {
        let mut tracker = CostTracker::default();
        tracker.record("claude-haiku-4-5-20251001", 1000, 500);

        let csv = tracker.export_csv();
        assert!(csv.starts_with("timestamp,model_id,"));
        assert!(csv.contains("claude-haiku-4-5-20251001"));
        assert!(csv.contains("1000"));
        assert!(csv.contains("500"));
    }

    #[test]
    fn cost_tracker_daily_history() {
        let mut tracker = CostTracker::default();
        tracker.record("claude-haiku-4-5-20251001", 1000, 500);
        tracker.record("claude-haiku-4-5-20251001", 2000, 1000);

        let history = tracker.daily_history();
        assert_eq!(history.len(), 1); // all today
        assert!(history[0].1 > 0.0);
    }

    #[test]
    fn cost_breakdown_serialization() {
        let breakdown = calculate_cost("claude-haiku-4-5-20251001", 1000, 500);
        let json = serde_json::to_string(&breakdown).unwrap();
        let deserialized: CostBreakdown = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.input_tokens, breakdown.input_tokens);
        assert_eq!(deserialized.model_id, breakdown.model_id);
    }
}
