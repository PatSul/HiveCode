use crate::storage::LearningStorage;
use crate::types::*;
use std::sync::Arc;

/// Adjusts routing tier assignments based on outcome data.
///
/// Analyzes accumulated quality scores for (task_type, tier) combinations and
/// recommends tier upgrades when quality is consistently low, or tier downgrades
/// when quality is consistently high (to save cost).
pub struct RoutingLearner {
    storage: Arc<LearningStorage>,
    adjustments: std::sync::Mutex<Vec<RoutingAdjustment>>,
}

/// The tier hierarchy from lowest to highest quality/cost.
const TIER_ORDER: &[&str] = &["free", "standard", "premium", "enterprise"];

impl RoutingLearner {
    pub fn new(storage: Arc<LearningStorage>) -> Self {
        Self {
            storage,
            adjustments: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Analyze all (task_type, tier) combinations with sufficient data and generate
    /// routing adjustments.
    ///
    /// For each combo with 10+ outcomes:
    /// - If exponential moving average quality < 0.5, recommend tier upgrade.
    /// - If exponential moving average quality > 0.85, recommend tier downgrade.
    ///
    /// The adjustments are stored both in-memory and persisted via storage.
    pub fn analyze(&self) -> Result<Vec<RoutingAdjustment>, String> {
        let stats = self.storage.task_tier_stats()?;
        let mut new_adjustments = Vec::new();

        for (task_type, tier, count, _raw_avg) in &stats {
            if *count < 10 {
                continue;
            }

            // Fetch recent quality scores for EMA computation
            let scores = self
                .storage
                .task_tier_quality_scores(task_type, tier, 100)?;
            if scores.is_empty() {
                continue;
            }

            let ema = compute_ema(&scores, 0.1);

            if ema < 0.5 {
                // Quality is low: recommend upgrading to a higher tier
                if let Some(higher) = next_tier_up(tier) {
                    let adj = RoutingAdjustment {
                        task_type: task_type.clone(),
                        from_tier: tier.clone(),
                        to_tier: higher.to_string(),
                        confidence: (0.5 - ema).min(0.5) * 2.0, // 0.0..1.0 scale
                        reason: format!(
                            "EMA quality {ema:.2} < 0.5 over {count} samples; upgrading tier"
                        ),
                    };
                    self.storage.save_routing_adjustment(&adj)?;
                    self.storage.log_learning(&LearningLogEntry {
                        id: 0,
                        event_type: "routing_adjustment".into(),
                        description: format!(
                            "Recommending upgrade for {task_type}: {tier} -> {higher} (EMA={ema:.2})"
                        ),
                        details: serde_json::to_string(&adj).unwrap_or_default(),
                        reversible: true,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    })?;
                    new_adjustments.push(adj);
                }
            } else if ema > 0.85 {
                // Quality is high: recommend downgrading to save cost
                if let Some(lower) = next_tier_down(tier) {
                    let adj = RoutingAdjustment {
                        task_type: task_type.clone(),
                        from_tier: tier.clone(),
                        to_tier: lower.to_string(),
                        confidence: (ema - 0.85).min(0.15) / 0.15, // 0.0..1.0 scale
                        reason: format!(
                            "EMA quality {ema:.2} > 0.85 over {count} samples; downgrading tier to save cost"
                        ),
                    };
                    self.storage.save_routing_adjustment(&adj)?;
                    self.storage.log_learning(&LearningLogEntry {
                        id: 0,
                        event_type: "routing_adjustment".into(),
                        description: format!(
                            "Recommending downgrade for {task_type}: {tier} -> {lower} (EMA={ema:.2})"
                        ),
                        details: serde_json::to_string(&adj).unwrap_or_default(),
                        reversible: true,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    })?;
                    new_adjustments.push(adj);
                }
            }
        }

        // Store adjustments in memory
        {
            let mut locked = self
                .adjustments
                .lock()
                .map_err(|e| format!("Lock error: {e}"))?;
            locked.extend(new_adjustments.clone());
        }

        Ok(new_adjustments)
    }

    /// Query stored adjustments for a specific (task_type, classified_tier) pair.
    ///
    /// Returns the recommended tier if an adjustment exists, or None.
    pub fn adjust_tier(&self, task_type: &str, classified_tier: &str) -> Option<String> {
        // First check in-memory adjustments (most recent)
        if let Ok(locked) = self.adjustments.lock() {
            for adj in locked.iter().rev() {
                if adj.task_type == task_type && adj.from_tier == classified_tier {
                    return Some(adj.to_tier.clone());
                }
            }
        }

        // Fall back to persisted adjustments
        self.storage
            .get_routing_adjustment(task_type, classified_tier)
            .ok()
            .flatten()
    }

    /// Return a snapshot of all current in-memory routing adjustments.
    pub fn current_adjustments(&self) -> Vec<RoutingAdjustment> {
        self.adjustments
            .lock()
            .map(|locked| locked.clone())
            .unwrap_or_default()
    }

    /// Reset all in-memory and persisted adjustments.
    pub fn clear_adjustments(&self) {
        if let Ok(mut locked) = self.adjustments.lock() {
            locked.clear();
        }
        let _ = self.storage.clear_routing_adjustments();
    }
}

/// Compute exponential moving average from a list of scores.
///
/// Scores are ordered most-recent-first. The EMA gives more weight to recent values.
/// `alpha` controls smoothing (higher = more weight on recent data).
fn compute_ema(scores: &[f64], alpha: f64) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }

    // Iterate from oldest to newest (reverse of storage order)
    let mut ema = scores[scores.len() - 1];
    for &score in scores.iter().rev().skip(1) {
        ema = alpha * score + (1.0 - alpha) * ema;
    }
    ema
}

/// Get the next higher tier, if one exists.
fn next_tier_up(tier: &str) -> Option<&'static str> {
    let pos = TIER_ORDER.iter().position(|&t| t == tier)?;
    if pos + 1 < TIER_ORDER.len() {
        Some(TIER_ORDER[pos + 1])
    } else {
        None
    }
}

/// Get the next lower tier, if one exists.
fn next_tier_down(tier: &str) -> Option<&'static str> {
    let pos = TIER_ORDER.iter().position(|&t| t == tier)?;
    if pos > 0 {
        Some(TIER_ORDER[pos - 1])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_storage() -> Arc<LearningStorage> {
        Arc::new(LearningStorage::in_memory().unwrap())
    }

    fn insert_outcomes(
        storage: &LearningStorage,
        task_type: &str,
        tier: &str,
        count: u32,
        quality: f64,
    ) {
        for i in 0..count {
            let record = OutcomeRecord {
                conversation_id: "conv-1".into(),
                message_id: format!("msg-{task_type}-{tier}-{i}"),
                model_id: "test-model".into(),
                task_type: task_type.into(),
                tier: tier.into(),
                persona: None,
                outcome: Outcome::Accepted,
                edit_distance: None,
                follow_up_count: 0,
                quality_score: quality,
                cost: 0.001,
                latency_ms: 300,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            storage.record_outcome(&record).unwrap();
        }
    }

    // ── EMA tests ────────────────────────────────────────────────────

    #[test]
    fn test_ema_single_value() {
        assert!((compute_ema(&[0.8], 0.1) - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ema_constant_values() {
        // All same value => EMA = that value
        let scores = vec![0.7; 20];
        let ema = compute_ema(&scores, 0.1);
        assert!((ema - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_ema_empty() {
        assert!((compute_ema(&[], 0.1) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ema_recent_weighted_higher() {
        // Most recent (index 0) is 1.0, oldest is 0.0
        // With alpha=0.5, EMA = 0.5 for this config (exactly at boundary).
        // Use more recent high values to get EMA > 0.5.
        let scores = vec![1.0, 1.0, 0.0, 0.0, 0.0];
        let ema = compute_ema(&scores, 0.5);
        // Two recent 1.0 values should push EMA above 0.5
        assert!(ema > 0.5, "ema was {ema}");
    }

    // ── tier navigation tests ────────────────────────────────────────

    #[test]
    fn test_next_tier_up() {
        assert_eq!(next_tier_up("free"), Some("standard"));
        assert_eq!(next_tier_up("standard"), Some("premium"));
        assert_eq!(next_tier_up("premium"), Some("enterprise"));
        assert_eq!(next_tier_up("enterprise"), None);
        assert_eq!(next_tier_up("unknown"), None);
    }

    #[test]
    fn test_next_tier_down() {
        assert_eq!(next_tier_down("enterprise"), Some("premium"));
        assert_eq!(next_tier_down("premium"), Some("standard"));
        assert_eq!(next_tier_down("standard"), Some("free"));
        assert_eq!(next_tier_down("free"), None);
        assert_eq!(next_tier_down("unknown"), None);
    }

    // ── analyze tests ────────────────────────────────────────────────

    #[test]
    fn test_analyze_recommends_upgrade_for_low_quality() {
        let storage = make_storage();
        insert_outcomes(&storage, "code_gen", "standard", 15, 0.3);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        let adjustments = learner.analyze().unwrap();

        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].task_type, "code_gen");
        assert_eq!(adjustments[0].from_tier, "standard");
        assert_eq!(adjustments[0].to_tier, "premium");
        assert!(adjustments[0].confidence > 0.0);
    }

    #[test]
    fn test_analyze_recommends_downgrade_for_high_quality() {
        let storage = make_storage();
        insert_outcomes(&storage, "chat", "premium", 15, 0.95);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        let adjustments = learner.analyze().unwrap();

        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].task_type, "chat");
        assert_eq!(adjustments[0].from_tier, "premium");
        assert_eq!(adjustments[0].to_tier, "standard");
    }

    #[test]
    fn test_analyze_no_adjustment_for_medium_quality() {
        let storage = make_storage();
        insert_outcomes(&storage, "code_gen", "standard", 15, 0.7);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        let adjustments = learner.analyze().unwrap();

        assert!(adjustments.is_empty());
    }

    #[test]
    fn test_analyze_skips_insufficient_data() {
        let storage = make_storage();
        insert_outcomes(&storage, "code_gen", "standard", 5, 0.2); // only 5, need 10+

        let learner = RoutingLearner::new(Arc::clone(&storage));
        let adjustments = learner.analyze().unwrap();

        assert!(adjustments.is_empty());
    }

    #[test]
    fn test_analyze_no_upgrade_from_enterprise() {
        let storage = make_storage();
        insert_outcomes(&storage, "code_gen", "enterprise", 15, 0.3);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        let adjustments = learner.analyze().unwrap();

        // Can't upgrade from enterprise (already top tier)
        assert!(adjustments.is_empty());
    }

    #[test]
    fn test_analyze_no_downgrade_from_free() {
        let storage = make_storage();
        insert_outcomes(&storage, "chat", "free", 15, 0.95);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        let adjustments = learner.analyze().unwrap();

        // Can't downgrade from free (already bottom tier)
        assert!(adjustments.is_empty());
    }

    // ── adjust_tier tests ────────────────────────────────────────────

    #[test]
    fn test_adjust_tier_returns_adjustment() {
        let storage = make_storage();
        insert_outcomes(&storage, "code_gen", "standard", 15, 0.3);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        learner.analyze().unwrap();

        let adjusted = learner.adjust_tier("code_gen", "standard");
        assert_eq!(adjusted, Some("premium".to_string()));
    }

    #[test]
    fn test_adjust_tier_returns_none_for_unknown() {
        let storage = make_storage();
        let learner = RoutingLearner::new(storage);
        assert_eq!(learner.adjust_tier("unknown", "standard"), None);
    }

    // ── clear_adjustments tests ──────────────────────────────────────

    #[test]
    fn test_clear_adjustments() {
        let storage = make_storage();
        insert_outcomes(&storage, "code_gen", "standard", 15, 0.3);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        learner.analyze().unwrap();
        assert!(learner.adjust_tier("code_gen", "standard").is_some());

        learner.clear_adjustments();
        assert!(learner.adjust_tier("code_gen", "standard").is_none());
    }

    // ── analysis logs to learning_log ────────────────────────────────

    #[test]
    fn test_analyze_logs_adjustments() {
        let storage = make_storage();
        insert_outcomes(&storage, "code_gen", "standard", 15, 0.3);

        let learner = RoutingLearner::new(Arc::clone(&storage));
        learner.analyze().unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert!(!log.is_empty());
        assert!(log.iter().any(|e| e.event_type == "routing_adjustment"));
    }
}
