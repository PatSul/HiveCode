use crate::storage::LearningStorage;
use crate::types::*;
use std::sync::Arc;

/// Periodic self-assessment of the learning system's performance.
///
/// Computes aggregate statistics about response quality, model performance,
/// routing accuracy, cost efficiency, and identifies weak areas for improvement.
pub struct SelfEvaluator {
    storage: Arc<LearningStorage>,
}

impl SelfEvaluator {
    pub fn new(storage: Arc<LearningStorage>) -> Self {
        Self { storage }
    }

    /// Run a full self-evaluation and return a report.
    ///
    /// Computes:
    /// - Overall quality: average of last 100 outcomes
    /// - Trend: compare last 50 outcomes avg vs previous 50 (Improving if +0.05, Declining if -0.05, else Stable)
    /// - Best/worst model: highest/lowest avg quality among models with 5+ outcomes
    /// - Misroute rate: % of routing entries where actual_tier_needed != classified_tier
    /// - Cost efficiency: total cost / total quality points
    /// - Weak areas: task types with avg quality < 0.5
    /// - Correction rate: % of Corrected outcomes
    /// - Regeneration rate: % of Regenerated outcomes
    pub fn evaluate(&self) -> Result<SelfEvaluationReport, String> {
        let total_interactions = self.storage.outcome_count()?;

        // Overall quality: average of last 100 outcomes
        let overall_quality = if total_interactions > 0 {
            self.storage.avg_quality_recent(100)?
        } else {
            0.0
        };

        // Trend: compare recent 50 vs previous 50
        let trend = if total_interactions >= 100 {
            let recent_50 = self.storage.avg_quality_recent(50)?;
            let previous_50 = self.storage.avg_quality_at_offset(50, 50)?;
            let diff = recent_50 - previous_50;
            if diff > 0.05 {
                QualityTrend::Improving
            } else if diff < -0.05 {
                QualityTrend::Declining
            } else {
                QualityTrend::Stable
            }
        } else {
            QualityTrend::Stable
        };

        // Best/worst model (among those with 5+ outcomes)
        let model_stats = self.storage.model_quality_stats()?;
        let best_model = model_stats
            .iter()
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(name, _, _)| name.clone());
        let worst_model = model_stats
            .iter()
            .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(name, _, _)| name.clone());

        // Misroute rate
        let misroute_rate = self.storage.misroute_rate()?;

        // Cost efficiency: total cost / total quality points
        let (total_cost, total_quality_points) = self.storage.cost_quality_totals()?;
        let cost_per_quality_point = if total_quality_points > 0.0 {
            total_cost / total_quality_points
        } else {
            0.0
        };

        // Weak areas: task types with avg quality < 0.5
        let task_stats = self.storage.task_type_quality_stats()?;
        let weak_areas: Vec<String> = task_stats
            .iter()
            .filter(|(_, _, avg)| *avg < 0.5)
            .map(|(task_type, _, _)| task_type.clone())
            .collect();

        // Outcome distribution for correction and regeneration rates
        let distribution = self.storage.outcome_distribution()?;
        let total_count: u32 = distribution.iter().map(|(_, c)| c).sum();

        let correction_count = distribution
            .iter()
            .find(|(o, _)| o == "corrected")
            .map(|(_, c)| *c)
            .unwrap_or(0);
        let regeneration_count = distribution
            .iter()
            .find(|(o, _)| o == "regenerated")
            .map(|(_, c)| *c)
            .unwrap_or(0);

        let correction_rate = if total_count > 0 {
            correction_count as f64 / total_count as f64
        } else {
            0.0
        };
        let regeneration_rate = if total_count > 0 {
            regeneration_count as f64 / total_count as f64
        } else {
            0.0
        };

        let report = SelfEvaluationReport {
            overall_quality,
            trend,
            best_model,
            worst_model,
            misroute_rate,
            cost_per_quality_point,
            weak_areas,
            correction_rate,
            regeneration_rate,
            total_interactions,
            generated_at: chrono::Utc::now().to_rfc3339(),
        };

        // Log the evaluation
        self.storage.log_learning(&LearningLogEntry {
            id: 0,
            event_type: "self_evaluation".into(),
            description: format!(
                "Self-evaluation: quality={:.2}, trend={:?}, interactions={}",
                report.overall_quality, report.trend, report.total_interactions
            ),
            details: serde_json::to_string(&report).unwrap_or_default(),
            reversible: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })?;

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_evaluator() -> SelfEvaluator {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        SelfEvaluator::new(storage)
    }

    fn make_evaluator_with_storage() -> (SelfEvaluator, Arc<LearningStorage>) {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let evaluator = SelfEvaluator::new(Arc::clone(&storage));
        (evaluator, storage)
    }

    fn insert_outcome(
        storage: &LearningStorage,
        model: &str,
        task_type: &str,
        quality: f64,
        outcome: Outcome,
    ) {
        let outcome_str = match outcome {
            Outcome::Accepted => "accepted",
            Outcome::Corrected => "corrected",
            Outcome::Regenerated => "regenerated",
            Outcome::Ignored => "ignored",
            Outcome::Unknown => "unknown",
        };
        // Use record_outcome which serializes outcome as Debug format
        let record = OutcomeRecord {
            conversation_id: "conv-1".into(),
            message_id: format!("msg-{}", uuid::Uuid::new_v4()),
            model_id: model.into(),
            task_type: task_type.into(),
            tier: "standard".into(),
            persona: None,
            outcome,
            edit_distance: None,
            follow_up_count: 0,
            quality_score: quality,
            cost: 0.002,
            latency_ms: 500,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let _ = outcome_str; // Used for clarity; actual serialization is done by record_outcome
        storage.record_outcome(&record).unwrap();
    }

    // ── empty database tests ─────────────────────────────────────────

    #[test]
    fn test_evaluate_empty_db() {
        let evaluator = make_evaluator();
        let report = evaluator.evaluate().unwrap();

        assert!((report.overall_quality - 0.0).abs() < f64::EPSILON);
        assert_eq!(report.trend, QualityTrend::Stable);
        assert!(report.best_model.is_none());
        assert!(report.worst_model.is_none());
        assert!((report.misroute_rate - 0.0).abs() < f64::EPSILON);
        assert!((report.cost_per_quality_point - 0.0).abs() < f64::EPSILON);
        assert!(report.weak_areas.is_empty());
        assert!((report.correction_rate - 0.0).abs() < f64::EPSILON);
        assert!((report.regeneration_rate - 0.0).abs() < f64::EPSILON);
        assert_eq!(report.total_interactions, 0);
    }

    // ── overall quality tests ────────────────────────────────────────

    #[test]
    fn test_evaluate_overall_quality() {
        let (evaluator, storage) = make_evaluator_with_storage();

        for _ in 0..10 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }

        let report = evaluator.evaluate().unwrap();
        assert!((report.overall_quality - 0.8).abs() < 0.01);
        assert_eq!(report.total_interactions, 10);
    }

    // ── trend tests ──────────────────────────────────────────────────

    #[test]
    fn test_evaluate_trend_stable_insufficient_data() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // Less than 100 outcomes -> always Stable
        for _ in 0..50 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }

        let report = evaluator.evaluate().unwrap();
        assert_eq!(report.trend, QualityTrend::Stable);
    }

    #[test]
    fn test_evaluate_trend_stable_with_consistent_quality() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // 100 outcomes all with same quality -> Stable
        for _ in 0..100 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.7, Outcome::Accepted);
        }

        let report = evaluator.evaluate().unwrap();
        assert_eq!(report.trend, QualityTrend::Stable);
    }

    // ── best/worst model tests ───────────────────────────────────────

    #[test]
    fn test_evaluate_best_worst_model() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // 5+ outcomes per model (required threshold)
        for _ in 0..6 {
            insert_outcome(&storage, "good-model", "code_gen", 0.95, Outcome::Accepted);
            insert_outcome(&storage, "bad-model", "code_gen", 0.3, Outcome::Regenerated);
        }

        let report = evaluator.evaluate().unwrap();
        assert_eq!(report.best_model, Some("good-model".to_string()));
        assert_eq!(report.worst_model, Some("bad-model".to_string()));
    }

    #[test]
    fn test_evaluate_no_models_with_5_outcomes() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // Only 3 outcomes per model (below threshold)
        for _ in 0..3 {
            insert_outcome(&storage, "model-a", "code_gen", 0.8, Outcome::Accepted);
        }

        let report = evaluator.evaluate().unwrap();
        assert!(report.best_model.is_none());
        assert!(report.worst_model.is_none());
    }

    // ── cost efficiency tests ────────────────────────────────────────

    #[test]
    fn test_evaluate_cost_efficiency() {
        let (evaluator, storage) = make_evaluator_with_storage();

        for _ in 0..10 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }

        let report = evaluator.evaluate().unwrap();
        // cost per outcome = 0.002, quality per outcome = 0.8
        // total cost = 0.02, total quality = 8.0
        // cost per quality point = 0.02 / 8.0 = 0.0025
        assert!((report.cost_per_quality_point - 0.0025).abs() < 0.001);
    }

    // ── weak areas tests ─────────────────────────────────────────────

    #[test]
    fn test_evaluate_weak_areas() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // Good task type
        for _ in 0..5 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }
        // Bad task type
        for _ in 0..5 {
            insert_outcome(&storage, "gpt-4o", "debugging", 0.3, Outcome::Regenerated);
        }

        let report = evaluator.evaluate().unwrap();
        assert!(report.weak_areas.contains(&"debugging".to_string()));
        assert!(!report.weak_areas.contains(&"code_gen".to_string()));
    }

    #[test]
    fn test_evaluate_no_weak_areas() {
        let (evaluator, storage) = make_evaluator_with_storage();

        for _ in 0..5 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }

        let report = evaluator.evaluate().unwrap();
        assert!(report.weak_areas.is_empty());
    }

    // ── correction/regeneration rate tests ────────────────────────────

    #[test]
    fn test_evaluate_correction_rate() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // 8 accepted, 2 corrected -> correction rate = 0.2
        for _ in 0..8 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }
        for _ in 0..2 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.5, Outcome::Corrected);
        }

        let report = evaluator.evaluate().unwrap();
        assert!((report.correction_rate - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_evaluate_regeneration_rate() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // 7 accepted, 3 regenerated -> regeneration rate = 0.3
        for _ in 0..7 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }
        for _ in 0..3 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.2, Outcome::Regenerated);
        }

        let report = evaluator.evaluate().unwrap();
        assert!((report.regeneration_rate - 0.3).abs() < 0.01);
    }

    // ── logging tests ────────────────────────────────────────────────

    #[test]
    fn test_evaluate_logs_result() {
        let (evaluator, storage) = make_evaluator_with_storage();

        for _ in 0..5 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }

        evaluator.evaluate().unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert!(log.iter().any(|e| e.event_type == "self_evaluation"));
    }

    // ── misroute rate tests ──────────────────────────────────────────

    #[test]
    fn test_evaluate_misroute_rate_zero_when_no_data() {
        let evaluator = make_evaluator();
        let report = evaluator.evaluate().unwrap();
        assert!((report.misroute_rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_evaluate_misroute_rate_with_data() {
        let (evaluator, storage) = make_evaluator_with_storage();

        // 2 correct routes, 1 misroute
        let entry1 = RoutingHistoryEntry {
            task_type: "code_gen".into(),
            classified_tier: "standard".into(),
            actual_tier_needed: Some("standard".into()),
            model_id: "gpt-4o".into(),
            quality_score: 0.8,
            cost: 0.002,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let entry2 = RoutingHistoryEntry {
            task_type: "code_gen".into(),
            classified_tier: "standard".into(),
            actual_tier_needed: Some("standard".into()),
            model_id: "gpt-4o".into(),
            quality_score: 0.9,
            cost: 0.002,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let entry3 = RoutingHistoryEntry {
            task_type: "code_gen".into(),
            classified_tier: "standard".into(),
            actual_tier_needed: Some("premium".into()),
            model_id: "gpt-4o".into(),
            quality_score: 0.4,
            cost: 0.002,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        storage.record_routing(&entry1).unwrap();
        storage.record_routing(&entry2).unwrap();
        storage.record_routing(&entry3).unwrap();

        let report = evaluator.evaluate().unwrap();
        // 1 out of 3 = 0.333...
        assert!((report.misroute_rate - 1.0 / 3.0).abs() < 0.01);
    }

    // ── report serde roundtrip ───────────────────────────────────────

    #[test]
    fn test_report_serde_roundtrip() {
        let (evaluator, storage) = make_evaluator_with_storage();

        for _ in 0..5 {
            insert_outcome(&storage, "gpt-4o", "code_gen", 0.8, Outcome::Accepted);
        }

        let report = evaluator.evaluate().unwrap();
        let json = serde_json::to_string(&report).unwrap();
        let parsed: SelfEvaluationReport = serde_json::from_str(&json).unwrap();
        assert!((parsed.overall_quality - report.overall_quality).abs() < f64::EPSILON);
        assert_eq!(parsed.total_interactions, report.total_interactions);
    }
}
