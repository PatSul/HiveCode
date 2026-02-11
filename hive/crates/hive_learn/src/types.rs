use serde::{Deserialize, Serialize};

/// What happened after an AI response was shown to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Accepted,
    Corrected,
    Regenerated,
    Ignored,
    Unknown,
}

impl Outcome {
    pub fn base_quality_score(self) -> f64 {
        match self {
            Self::Accepted => 0.9,
            Self::Corrected => 0.5,
            Self::Regenerated => 0.2,
            Self::Ignored => 0.1,
            Self::Unknown => 0.5,
        }
    }
}

/// Record of an AI response outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeRecord {
    pub conversation_id: String,
    pub message_id: String,
    pub model_id: String,
    pub task_type: String,
    pub tier: String,
    pub persona: Option<String>,
    pub outcome: Outcome,
    pub edit_distance: Option<f64>,
    pub follow_up_count: u32,
    pub quality_score: f64,
    pub cost: f64,
    pub latency_ms: u64,
    pub timestamp: String,
}

/// A routing history entry tracking routing decisions vs actual quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingHistoryEntry {
    pub task_type: String,
    pub classified_tier: String,
    pub actual_tier_needed: Option<String>,
    pub model_id: String,
    pub quality_score: f64,
    pub cost: f64,
    pub timestamp: String,
}

/// A learned routing adjustment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingAdjustment {
    pub task_type: String,
    pub from_tier: String,
    pub to_tier: String,
    pub confidence: f64,
    pub reason: String,
}

/// A learned user preference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreference {
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub observation_count: u32,
    pub last_updated: String,
}

/// A versioned prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub persona: String,
    pub version: u32,
    pub prompt_text: String,
    pub avg_quality: f64,
    pub sample_count: u32,
    pub is_active: bool,
    pub created_at: String,
}

/// A prompt refinement suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRefinement {
    pub persona: String,
    pub current_version: u32,
    pub suggested_prompt: String,
    pub reason: String,
}

/// A reusable code pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePattern {
    pub id: i64,
    pub pattern: String,
    pub language: String,
    pub category: String,
    pub description: String,
    pub quality_score: f64,
    pub use_count: u32,
    pub created_at: String,
}

/// An entry in the transparent learning log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningLogEntry {
    pub id: i64,
    pub event_type: String,
    pub description: String,
    pub details: String,
    pub reversible: bool,
    pub timestamp: String,
}

/// Quality trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityTrend {
    Improving,
    Declining,
    Stable,
}

/// Self-evaluation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfEvaluationReport {
    pub overall_quality: f64,
    pub trend: QualityTrend,
    pub best_model: Option<String>,
    pub worst_model: Option<String>,
    pub misroute_rate: f64,
    pub cost_per_quality_point: f64,
    pub weak_areas: Vec<String>,
    pub correction_rate: f64,
    pub regeneration_rate: f64,
    pub total_interactions: u64,
    pub generated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outcome_base_quality_scores() {
        assert!((Outcome::Accepted.base_quality_score() - 0.9).abs() < f64::EPSILON);
        assert!((Outcome::Corrected.base_quality_score() - 0.5).abs() < f64::EPSILON);
        assert!((Outcome::Regenerated.base_quality_score() - 0.2).abs() < f64::EPSILON);
        assert!((Outcome::Ignored.base_quality_score() - 0.1).abs() < f64::EPSILON);
        assert!((Outcome::Unknown.base_quality_score() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_outcome_serde_roundtrip() {
        let outcomes = [
            Outcome::Accepted,
            Outcome::Corrected,
            Outcome::Regenerated,
            Outcome::Ignored,
            Outcome::Unknown,
        ];
        for outcome in &outcomes {
            let json = serde_json::to_string(outcome).unwrap();
            let parsed: Outcome = serde_json::from_str(&json).unwrap();
            assert_eq!(*outcome, parsed);
        }
    }

    #[test]
    fn test_outcome_serde_snake_case() {
        assert_eq!(serde_json::to_string(&Outcome::Accepted).unwrap(), "\"accepted\"");
        assert_eq!(serde_json::to_string(&Outcome::Corrected).unwrap(), "\"corrected\"");
        assert_eq!(serde_json::to_string(&Outcome::Regenerated).unwrap(), "\"regenerated\"");
        assert_eq!(serde_json::to_string(&Outcome::Ignored).unwrap(), "\"ignored\"");
        assert_eq!(serde_json::to_string(&Outcome::Unknown).unwrap(), "\"unknown\"");
    }

    #[test]
    fn test_outcome_record_serde_roundtrip() {
        let record = OutcomeRecord {
            conversation_id: "conv-001".to_string(),
            message_id: "msg-001".to_string(),
            model_id: "gpt-4o".to_string(),
            task_type: "code_generation".to_string(),
            tier: "premium".to_string(),
            persona: Some("coder".to_string()),
            outcome: Outcome::Accepted,
            edit_distance: Some(0.15),
            follow_up_count: 2,
            quality_score: 0.85,
            cost: 0.003,
            latency_ms: 1200,
            timestamp: "2026-02-10T12:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&record).unwrap();
        let parsed: OutcomeRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.conversation_id, "conv-001");
        assert_eq!(parsed.message_id, "msg-001");
        assert_eq!(parsed.model_id, "gpt-4o");
        assert_eq!(parsed.task_type, "code_generation");
        assert_eq!(parsed.tier, "premium");
        assert_eq!(parsed.persona, Some("coder".to_string()));
        assert_eq!(parsed.outcome, Outcome::Accepted);
        assert_eq!(parsed.edit_distance, Some(0.15));
        assert_eq!(parsed.follow_up_count, 2);
        assert!((parsed.quality_score - 0.85).abs() < f64::EPSILON);
        assert!((parsed.cost - 0.003).abs() < f64::EPSILON);
        assert_eq!(parsed.latency_ms, 1200);
        assert_eq!(parsed.timestamp, "2026-02-10T12:00:00Z");
    }

    #[test]
    fn test_outcome_record_with_none_fields() {
        let record = OutcomeRecord {
            conversation_id: "conv-002".to_string(),
            message_id: "msg-002".to_string(),
            model_id: "claude-3".to_string(),
            task_type: "chat".to_string(),
            tier: "standard".to_string(),
            persona: None,
            outcome: Outcome::Unknown,
            edit_distance: None,
            follow_up_count: 0,
            quality_score: 0.5,
            cost: 0.001,
            latency_ms: 500,
            timestamp: "2026-02-10T13:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&record).unwrap();
        let parsed: OutcomeRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.persona, None);
        assert_eq!(parsed.edit_distance, None);
        assert_eq!(parsed.outcome, Outcome::Unknown);
    }

    #[test]
    fn test_quality_trend_serde() {
        let trends = [QualityTrend::Improving, QualityTrend::Declining, QualityTrend::Stable];
        for trend in &trends {
            let json = serde_json::to_string(trend).unwrap();
            let parsed: QualityTrend = serde_json::from_str(&json).unwrap();
            assert_eq!(*trend, parsed);
        }
    }

    #[test]
    fn test_routing_history_entry_serde() {
        let entry = RoutingHistoryEntry {
            task_type: "code_review".to_string(),
            classified_tier: "standard".to_string(),
            actual_tier_needed: Some("premium".to_string()),
            model_id: "gpt-4o-mini".to_string(),
            quality_score: 0.6,
            cost: 0.001,
            timestamp: "2026-02-10T14:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RoutingHistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.task_type, "code_review");
        assert_eq!(parsed.actual_tier_needed, Some("premium".to_string()));
    }

    #[test]
    fn test_self_evaluation_report_serde() {
        let report = SelfEvaluationReport {
            overall_quality: 0.78,
            trend: QualityTrend::Improving,
            best_model: Some("claude-3-opus".to_string()),
            worst_model: Some("local-7b".to_string()),
            misroute_rate: 0.12,
            cost_per_quality_point: 0.004,
            weak_areas: vec!["code_review".to_string(), "debugging".to_string()],
            correction_rate: 0.15,
            regeneration_rate: 0.08,
            total_interactions: 500,
            generated_at: "2026-02-10T15:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: SelfEvaluationReport = serde_json::from_str(&json).unwrap();

        assert!((parsed.overall_quality - 0.78).abs() < f64::EPSILON);
        assert_eq!(parsed.trend, QualityTrend::Improving);
        assert_eq!(parsed.best_model, Some("claude-3-opus".to_string()));
        assert_eq!(parsed.weak_areas.len(), 2);
        assert_eq!(parsed.total_interactions, 500);
    }
}
