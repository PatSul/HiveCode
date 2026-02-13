//! Fleet Learning service for distributed AI instance coordination.
//!
//! Tracks usage patterns, model performance metrics, fleet insights,
//! and instance-level metrics across a fleet of AI instances. Enables
//! collective learning by identifying recurring patterns and surfacing
//! the best-performing models and workflows.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Classification of a discovered usage pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatternType {
    /// Recurring code style preferences.
    CodeStyle,
    /// Common error recovery strategies.
    ErrorRecovery,
    /// Preferred tool selections.
    ToolPreference,
    /// Repeated sequences of workflow steps.
    WorkflowSequence,
    /// Preferred model selections for task types.
    ModelPreference,
    /// Recurring prompt structures.
    PromptPattern,
}

/// A learned pattern observed across fleet interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningPattern {
    pub id: String,
    pub pattern_type: PatternType,
    pub description: String,
    pub frequency: u32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub confidence: f32,
}

/// Aggregated performance metrics for a specific model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPerformance {
    pub model_id: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub avg_latency_ms: f64,
    pub avg_cost: f64,
    pub avg_quality_score: f64,
    pub last_updated: DateTime<Utc>,
}

/// An insight derived from fleet-wide analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetInsight {
    pub id: String,
    pub insight_type: String,
    pub title: String,
    pub description: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Metrics tracked for a single fleet instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceMetrics {
    pub instance_id: String,
    pub total_requests: u64,
    pub total_cost: f64,
    pub total_tokens: u64,
    pub active_since: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub patterns_discovered: u32,
}

/// Aggregate summary across all fleet instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetSummary {
    pub total_instances: usize,
    pub total_requests: u64,
    pub total_cost: f64,
    pub total_tokens: u64,
    pub total_patterns: usize,
    pub total_insights: usize,
}

// ---------------------------------------------------------------------------
// FleetLearningService
// ---------------------------------------------------------------------------

/// Service that coordinates fleet-wide learning by tracking patterns,
/// model performance, insights, and instance metrics in memory.
pub struct FleetLearningService {
    patterns: Vec<LearningPattern>,
    model_performance: HashMap<String, ModelPerformance>,
    insights: Vec<FleetInsight>,
    instances: HashMap<String, InstanceMetrics>,
}

impl FleetLearningService {
    /// Create a new, empty fleet learning service.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
            model_performance: HashMap::new(),
            insights: Vec::new(),
            instances: HashMap::new(),
        }
    }

    // -- Patterns --

    /// Record or update a learning pattern.
    ///
    /// If a pattern with the same type and description already exists,
    /// its frequency is incremented and timestamps/confidence are updated.
    /// Otherwise, a new pattern is created.
    pub fn record_pattern(
        &mut self,
        pattern_type: PatternType,
        description: &str,
        confidence: f32,
    ) -> &LearningPattern {
        let now = Utc::now();

        // Look for an existing pattern with matching type + description.
        let existing_idx = self
            .patterns
            .iter()
            .position(|p| p.pattern_type == pattern_type && p.description == description);

        if let Some(idx) = existing_idx {
            let pattern = &mut self.patterns[idx];
            pattern.frequency += 1;
            pattern.last_seen = now;
            pattern.confidence = confidence;
            debug!(
                "Updated pattern '{}' (type={:?}), frequency={}",
                description, pattern_type, pattern.frequency
            );
            &self.patterns[idx]
        } else {
            let pattern = LearningPattern {
                id: Uuid::new_v4().to_string(),
                pattern_type: pattern_type.clone(),
                description: description.to_string(),
                frequency: 1,
                first_seen: now,
                last_seen: now,
                confidence,
            };
            debug!(
                "Recorded new pattern '{}' (type={:?})",
                description, pattern_type
            );
            self.patterns.push(pattern);
            self.patterns.last().unwrap()
        }
    }

    /// Get all patterns matching a specific type.
    pub fn get_patterns(&self, pattern_type: &PatternType) -> Vec<&LearningPattern> {
        self.patterns
            .iter()
            .filter(|p| &p.pattern_type == pattern_type)
            .collect()
    }

    /// Get the most frequent patterns, up to `limit`.
    pub fn top_patterns(&self, limit: usize) -> Vec<&LearningPattern> {
        let mut sorted: Vec<&LearningPattern> = self.patterns.iter().collect();
        sorted.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        sorted.truncate(limit);
        sorted
    }

    /// Return a reference to all patterns.
    pub fn all_patterns(&self) -> &[LearningPattern] {
        &self.patterns
    }

    // -- Model Performance --

    /// Record a model performance data point.
    ///
    /// Updates running averages for latency, cost, and quality score.
    pub fn record_model_performance(
        &mut self,
        model_id: &str,
        success: bool,
        latency_ms: f64,
        cost: f64,
        quality_score: f64,
    ) {
        let now = Utc::now();
        let perf = self
            .model_performance
            .entry(model_id.to_string())
            .or_insert_with(|| ModelPerformance {
                model_id: model_id.to_string(),
                total_requests: 0,
                success_count: 0,
                avg_latency_ms: 0.0,
                avg_cost: 0.0,
                avg_quality_score: 0.0,
                last_updated: now,
            });

        let n = perf.total_requests as f64;
        perf.total_requests += 1;
        if success {
            perf.success_count += 1;
        }

        // Incremental average update: new_avg = (old_avg * n + value) / (n + 1)
        let n1 = perf.total_requests as f64;
        perf.avg_latency_ms = (perf.avg_latency_ms * n + latency_ms) / n1;
        perf.avg_cost = (perf.avg_cost * n + cost) / n1;
        perf.avg_quality_score = (perf.avg_quality_score * n + quality_score) / n1;
        perf.last_updated = now;

        debug!(
            "Recorded performance for model '{}': requests={}, avg_quality={:.2}",
            model_id, perf.total_requests, perf.avg_quality_score
        );
    }

    /// Get performance metrics for a specific model.
    pub fn get_model_performance(&self, model_id: &str) -> Option<&ModelPerformance> {
        self.model_performance.get(model_id)
    }

    /// Find the model with the highest average quality score.
    ///
    /// Returns `None` if no model performance data has been recorded.
    pub fn best_model_by_quality(&self) -> Option<&ModelPerformance> {
        self.model_performance.values().max_by(|a, b| {
            a.avg_quality_score
                .partial_cmp(&b.avg_quality_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Find the model with the lowest average cost.
    ///
    /// Returns `None` if no model performance data has been recorded.
    pub fn best_model_by_cost(&self) -> Option<&ModelPerformance> {
        self.model_performance.values().min_by(|a, b| {
            a.avg_cost
                .partial_cmp(&b.avg_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    // -- Insights --

    /// Add a fleet insight.
    pub fn add_insight(
        &mut self,
        insight_type: &str,
        title: &str,
        description: &str,
        data: serde_json::Value,
    ) -> &FleetInsight {
        let insight = FleetInsight {
            id: Uuid::new_v4().to_string(),
            insight_type: insight_type.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            data,
            created_at: Utc::now(),
        };
        debug!("Added insight: '{}' (type={})", title, insight_type);
        self.insights.push(insight);
        self.insights.last().unwrap()
    }

    /// Get the most recent insights, up to `limit`.
    pub fn recent_insights(&self, limit: usize) -> Vec<&FleetInsight> {
        let start = self.insights.len().saturating_sub(limit);
        self.insights[start..].iter().collect()
    }

    /// Return a reference to all insights.
    pub fn all_insights(&self) -> &[FleetInsight] {
        &self.insights
    }

    // -- Instance Metrics --

    /// Register a new fleet instance.
    ///
    /// If the instance already exists, this is a no-op.
    pub fn register_instance(&mut self, instance_id: &str) {
        let now = Utc::now();
        self.instances
            .entry(instance_id.to_string())
            .or_insert_with(|| {
                debug!("Registered fleet instance '{}'", instance_id);
                InstanceMetrics {
                    instance_id: instance_id.to_string(),
                    total_requests: 0,
                    total_cost: 0.0,
                    total_tokens: 0,
                    active_since: now,
                    last_active: now,
                    patterns_discovered: 0,
                }
            });
    }

    /// Update metrics for an existing instance.
    ///
    /// Increments the instance's counters by the given amounts.
    /// Returns `true` if the instance was found and updated, `false` otherwise.
    pub fn update_instance_metrics(
        &mut self,
        instance_id: &str,
        requests: u64,
        cost: f64,
        tokens: u64,
    ) -> bool {
        if let Some(instance) = self.instances.get_mut(instance_id) {
            instance.total_requests += requests;
            instance.total_cost += cost;
            instance.total_tokens += tokens;
            instance.last_active = Utc::now();
            debug!(
                "Updated instance '{}': requests={}, cost={:.4}, tokens={}",
                instance_id, instance.total_requests, instance.total_cost, instance.total_tokens
            );
            true
        } else {
            false
        }
    }

    /// Get metrics for a specific instance.
    pub fn get_instance_metrics(&self, instance_id: &str) -> Option<&InstanceMetrics> {
        self.instances.get(instance_id)
    }

    /// Generate an aggregate summary across all fleet instances.
    pub fn fleet_summary(&self) -> FleetSummary {
        let total_requests: u64 = self.instances.values().map(|i| i.total_requests).sum();
        let total_cost: f64 = self.instances.values().map(|i| i.total_cost).sum();
        let total_tokens: u64 = self.instances.values().map(|i| i.total_tokens).sum();

        FleetSummary {
            total_instances: self.instances.len(),
            total_requests,
            total_cost,
            total_tokens,
            total_patterns: self.patterns.len(),
            total_insights: self.insights.len(),
        }
    }
}

impl Default for FleetLearningService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Pattern recording --

    #[test]
    fn test_record_new_pattern() {
        let mut service = FleetLearningService::new();
        let pattern = service.record_pattern(PatternType::CodeStyle, "prefer snake_case", 0.85);
        assert_eq!(pattern.pattern_type, PatternType::CodeStyle);
        assert_eq!(pattern.description, "prefer snake_case");
        assert_eq!(pattern.frequency, 1);
        assert!((pattern.confidence - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_record_pattern_increments_frequency() {
        let mut service = FleetLearningService::new();
        service.record_pattern(PatternType::CodeStyle, "prefer snake_case", 0.80);
        service.record_pattern(PatternType::CodeStyle, "prefer snake_case", 0.90);
        service.record_pattern(PatternType::CodeStyle, "prefer snake_case", 0.95);

        let patterns = service.get_patterns(&PatternType::CodeStyle);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].frequency, 3);
        assert!((patterns[0].confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_different_descriptions_create_separate_patterns() {
        let mut service = FleetLearningService::new();
        service.record_pattern(PatternType::CodeStyle, "prefer snake_case", 0.8);
        service.record_pattern(PatternType::CodeStyle, "prefer camelCase", 0.7);

        let patterns = service.get_patterns(&PatternType::CodeStyle);
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn test_different_types_create_separate_patterns() {
        let mut service = FleetLearningService::new();
        service.record_pattern(PatternType::CodeStyle, "use async", 0.8);
        service.record_pattern(PatternType::ToolPreference, "use async", 0.8);

        assert_eq!(service.all_patterns().len(), 2);
    }

    #[test]
    fn test_get_patterns_filters_by_type() {
        let mut service = FleetLearningService::new();
        service.record_pattern(PatternType::CodeStyle, "snake_case", 0.8);
        service.record_pattern(PatternType::ErrorRecovery, "retry on 429", 0.9);
        service.record_pattern(PatternType::CodeStyle, "trailing comma", 0.7);

        let code_style = service.get_patterns(&PatternType::CodeStyle);
        assert_eq!(code_style.len(), 2);

        let error_recovery = service.get_patterns(&PatternType::ErrorRecovery);
        assert_eq!(error_recovery.len(), 1);
        assert_eq!(error_recovery[0].description, "retry on 429");

        let workflow = service.get_patterns(&PatternType::WorkflowSequence);
        assert!(workflow.is_empty());
    }

    #[test]
    fn test_top_patterns_sorted_by_frequency() {
        let mut service = FleetLearningService::new();
        // Record "a" 3 times, "b" 1 time, "c" 5 times
        for _ in 0..3 {
            service.record_pattern(PatternType::CodeStyle, "a", 0.8);
        }
        service.record_pattern(PatternType::ErrorRecovery, "b", 0.7);
        for _ in 0..5 {
            service.record_pattern(PatternType::ToolPreference, "c", 0.9);
        }

        let top = service.top_patterns(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].description, "c"); // frequency 5
        assert_eq!(top[1].description, "a"); // frequency 3
    }

    #[test]
    fn test_top_patterns_limit_exceeds_count() {
        let mut service = FleetLearningService::new();
        service.record_pattern(PatternType::CodeStyle, "only_one", 0.8);

        let top = service.top_patterns(10);
        assert_eq!(top.len(), 1);
    }

    // -- Model Performance --

    #[test]
    fn test_record_model_performance_single() {
        let mut service = FleetLearningService::new();
        service.record_model_performance("gpt-4", true, 250.0, 0.03, 0.9);

        let perf = service.get_model_performance("gpt-4").unwrap();
        assert_eq!(perf.model_id, "gpt-4");
        assert_eq!(perf.total_requests, 1);
        assert_eq!(perf.success_count, 1);
        assert!((perf.avg_latency_ms - 250.0).abs() < 0.001);
        assert!((perf.avg_cost - 0.03).abs() < 0.001);
        assert!((perf.avg_quality_score - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_record_model_performance_running_average() {
        let mut service = FleetLearningService::new();
        service.record_model_performance("gpt-4", true, 200.0, 0.02, 0.8);
        service.record_model_performance("gpt-4", true, 400.0, 0.04, 1.0);

        let perf = service.get_model_performance("gpt-4").unwrap();
        assert_eq!(perf.total_requests, 2);
        assert_eq!(perf.success_count, 2);
        assert!((perf.avg_latency_ms - 300.0).abs() < 0.001);
        assert!((perf.avg_cost - 0.03).abs() < 0.001);
        assert!((perf.avg_quality_score - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_record_model_performance_failure() {
        let mut service = FleetLearningService::new();
        service.record_model_performance("gpt-4", true, 200.0, 0.02, 0.8);
        service.record_model_performance("gpt-4", false, 500.0, 0.02, 0.2);

        let perf = service.get_model_performance("gpt-4").unwrap();
        assert_eq!(perf.total_requests, 2);
        assert_eq!(perf.success_count, 1);
    }

    #[test]
    fn test_get_model_performance_unknown() {
        let service = FleetLearningService::new();
        assert!(service.get_model_performance("nonexistent").is_none());
    }

    #[test]
    fn test_best_model_by_quality() {
        let mut service = FleetLearningService::new();
        service.record_model_performance("gpt-4", true, 300.0, 0.05, 0.95);
        service.record_model_performance("claude-3", true, 200.0, 0.03, 0.85);
        service.record_model_performance("llama-3", true, 100.0, 0.01, 0.70);

        let best = service.best_model_by_quality().unwrap();
        assert_eq!(best.model_id, "gpt-4");
    }

    #[test]
    fn test_best_model_by_cost() {
        let mut service = FleetLearningService::new();
        service.record_model_performance("gpt-4", true, 300.0, 0.05, 0.95);
        service.record_model_performance("claude-3", true, 200.0, 0.03, 0.85);
        service.record_model_performance("llama-3", true, 100.0, 0.01, 0.70);

        let best = service.best_model_by_cost().unwrap();
        assert_eq!(best.model_id, "llama-3");
    }

    #[test]
    fn test_best_model_empty_service() {
        let service = FleetLearningService::new();
        assert!(service.best_model_by_quality().is_none());
        assert!(service.best_model_by_cost().is_none());
    }

    // -- Insights --

    #[test]
    fn test_add_insight() {
        let mut service = FleetLearningService::new();
        let insight = service.add_insight(
            "optimization",
            "Switch to Haiku for simple tasks",
            "Analysis shows simple tasks can use a cheaper model",
            serde_json::json!({"savings_pct": 40}),
        );
        assert_eq!(insight.insight_type, "optimization");
        assert_eq!(insight.title, "Switch to Haiku for simple tasks");
        assert_eq!(insight.data["savings_pct"], 40);
    }

    #[test]
    fn test_recent_insights_ordering() {
        let mut service = FleetLearningService::new();
        service.add_insight("a", "First", "desc", serde_json::json!({}));
        service.add_insight("b", "Second", "desc", serde_json::json!({}));
        service.add_insight("c", "Third", "desc", serde_json::json!({}));

        let recent = service.recent_insights(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].title, "Second");
        assert_eq!(recent[1].title, "Third");
    }

    #[test]
    fn test_recent_insights_limit_exceeds_count() {
        let mut service = FleetLearningService::new();
        service.add_insight("a", "Only", "desc", serde_json::json!({}));

        let recent = service.recent_insights(10);
        assert_eq!(recent.len(), 1);
    }

    // -- Instance Metrics --

    #[test]
    fn test_register_instance() {
        let mut service = FleetLearningService::new();
        service.register_instance("instance-1");

        let metrics = service.get_instance_metrics("instance-1").unwrap();
        assert_eq!(metrics.instance_id, "instance-1");
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.total_cost, 0.0);
        assert_eq!(metrics.total_tokens, 0);
    }

    #[test]
    fn test_register_instance_idempotent() {
        let mut service = FleetLearningService::new();
        service.register_instance("instance-1");
        service.update_instance_metrics("instance-1", 5, 0.1, 1000);

        // Re-registering should NOT reset metrics.
        service.register_instance("instance-1");
        let metrics = service.get_instance_metrics("instance-1").unwrap();
        assert_eq!(metrics.total_requests, 5);
    }

    #[test]
    fn test_update_instance_metrics() {
        let mut service = FleetLearningService::new();
        service.register_instance("instance-1");

        let updated = service.update_instance_metrics("instance-1", 10, 0.50, 5000);
        assert!(updated);

        let metrics = service.get_instance_metrics("instance-1").unwrap();
        assert_eq!(metrics.total_requests, 10);
        assert!((metrics.total_cost - 0.50).abs() < 0.001);
        assert_eq!(metrics.total_tokens, 5000);
    }

    #[test]
    fn test_update_instance_metrics_accumulates() {
        let mut service = FleetLearningService::new();
        service.register_instance("instance-1");
        service.update_instance_metrics("instance-1", 10, 0.50, 5000);
        service.update_instance_metrics("instance-1", 5, 0.25, 2500);

        let metrics = service.get_instance_metrics("instance-1").unwrap();
        assert_eq!(metrics.total_requests, 15);
        assert!((metrics.total_cost - 0.75).abs() < 0.001);
        assert_eq!(metrics.total_tokens, 7500);
    }

    #[test]
    fn test_update_nonexistent_instance_returns_false() {
        let mut service = FleetLearningService::new();
        let updated = service.update_instance_metrics("ghost", 1, 0.01, 100);
        assert!(!updated);
    }

    #[test]
    fn test_get_instance_metrics_unknown() {
        let service = FleetLearningService::new();
        assert!(service.get_instance_metrics("nonexistent").is_none());
    }

    // -- Fleet Summary --

    #[test]
    fn test_fleet_summary_empty() {
        let service = FleetLearningService::new();
        let summary = service.fleet_summary();
        assert_eq!(summary.total_instances, 0);
        assert_eq!(summary.total_requests, 0);
        assert_eq!(summary.total_cost, 0.0);
        assert_eq!(summary.total_tokens, 0);
        assert_eq!(summary.total_patterns, 0);
        assert_eq!(summary.total_insights, 0);
    }

    #[test]
    fn test_fleet_summary_aggregates() {
        let mut service = FleetLearningService::new();

        // Register and update two instances
        service.register_instance("inst-1");
        service.update_instance_metrics("inst-1", 100, 5.0, 50000);

        service.register_instance("inst-2");
        service.update_instance_metrics("inst-2", 200, 10.0, 100000);

        // Add some patterns and insights
        service.record_pattern(PatternType::CodeStyle, "snake_case", 0.8);
        service.record_pattern(PatternType::ErrorRecovery, "retry", 0.9);
        service.add_insight("perf", "title", "desc", serde_json::json!({}));

        let summary = service.fleet_summary();
        assert_eq!(summary.total_instances, 2);
        assert_eq!(summary.total_requests, 300);
        assert!((summary.total_cost - 15.0).abs() < 0.001);
        assert_eq!(summary.total_tokens, 150000);
        assert_eq!(summary.total_patterns, 2);
        assert_eq!(summary.total_insights, 1);
    }

    // -- Serialization --

    #[test]
    fn test_learning_pattern_serialization() {
        let mut service = FleetLearningService::new();
        service.record_pattern(PatternType::PromptPattern, "chain of thought", 0.92);

        let pattern = &service.all_patterns()[0];
        let json = serde_json::to_string(pattern).unwrap();
        let deserialized: LearningPattern = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pattern_type, PatternType::PromptPattern);
        assert_eq!(deserialized.description, "chain of thought");
        assert_eq!(deserialized.frequency, 1);
        assert!((deserialized.confidence - 0.92).abs() < 0.001);
    }

    #[test]
    fn test_model_performance_serialization() {
        let mut service = FleetLearningService::new();
        service.record_model_performance("gpt-4", true, 250.0, 0.03, 0.9);

        let perf = service.get_model_performance("gpt-4").unwrap();
        let json = serde_json::to_string(perf).unwrap();
        let deserialized: ModelPerformance = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model_id, "gpt-4");
        assert_eq!(deserialized.total_requests, 1);
        assert!((deserialized.avg_quality_score - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_fleet_insight_serialization() {
        let mut service = FleetLearningService::new();
        service.add_insight(
            "cost",
            "Reduce costs",
            "Use smaller models",
            serde_json::json!({"recommendation": "haiku"}),
        );

        let insight = &service.all_insights()[0];
        let json = serde_json::to_string(insight).unwrap();
        let deserialized: FleetInsight = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.insight_type, "cost");
        assert_eq!(deserialized.title, "Reduce costs");
        assert_eq!(deserialized.data["recommendation"], "haiku");
    }

    #[test]
    fn test_instance_metrics_serialization() {
        let mut service = FleetLearningService::new();
        service.register_instance("inst-1");
        service.update_instance_metrics("inst-1", 42, 1.23, 9999);

        let metrics = service.get_instance_metrics("inst-1").unwrap();
        let json = serde_json::to_string(metrics).unwrap();
        let deserialized: InstanceMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.instance_id, "inst-1");
        assert_eq!(deserialized.total_requests, 42);
        assert!((deserialized.total_cost - 1.23).abs() < 0.001);
        assert_eq!(deserialized.total_tokens, 9999);
    }

    // -- Default --

    #[test]
    fn test_default_service() {
        let service = FleetLearningService::default();
        assert!(service.all_patterns().is_empty());
        assert!(service.all_insights().is_empty());
        assert_eq!(service.fleet_summary().total_instances, 0);
    }

    // -- Pattern type enum coverage --

    #[test]
    fn test_all_pattern_types() {
        let mut service = FleetLearningService::new();
        let types = vec![
            PatternType::CodeStyle,
            PatternType::ErrorRecovery,
            PatternType::ToolPreference,
            PatternType::WorkflowSequence,
            PatternType::ModelPreference,
            PatternType::PromptPattern,
        ];

        for pt in &types {
            service.record_pattern(pt.clone(), "test", 0.5);
        }

        assert_eq!(service.all_patterns().len(), types.len());

        for pt in &types {
            let matches = service.get_patterns(pt);
            assert_eq!(matches.len(), 1);
        }
    }
}
