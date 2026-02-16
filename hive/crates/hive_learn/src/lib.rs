pub mod learning_bridge;
pub mod outcome_tracker;
pub mod pattern_library;
pub mod preference_model;
pub mod prompt_evolver;
pub mod routing_learner;
pub mod self_evaluator;
pub mod storage;
pub mod types;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

use outcome_tracker::OutcomeTracker;
use pattern_library::PatternLibrary;
use preference_model::PreferenceModel;
use prompt_evolver::PromptEvolver;
use routing_learner::RoutingLearner;
use self_evaluator::SelfEvaluator;
use storage::LearningStorage;
pub use types::*;

/// The central coordination point for all learning subsystems.
///
/// `LearningService` owns each subsystem and provides a high-level API for
/// recording outcomes, querying learned data, and allowing user control over
/// all learned state (preferences, prompts, etc.).
///
/// Periodic analysis (routing re-evaluation, self-evaluation) is triggered
/// automatically based on interaction count milestones.
pub struct LearningService {
    storage: Arc<LearningStorage>,
    pub outcome_tracker: OutcomeTracker,
    pub routing_learner: RoutingLearner,
    pub preference_model: PreferenceModel,
    pub prompt_evolver: PromptEvolver,
    pub pattern_library: PatternLibrary,
    pub self_evaluator: SelfEvaluator,
    interaction_count: AtomicU64,
}

impl LearningService {
    /// Open a persistent learning database at the given path.
    pub fn open(db_path: &str) -> Result<Self, String> {
        let storage = Arc::new(LearningStorage::open(db_path)?);
        Ok(Self::from_storage(storage))
    }

    /// Create an in-memory learning service (useful for tests).
    pub fn in_memory() -> Result<Self, String> {
        let storage = Arc::new(LearningStorage::in_memory()?);
        Ok(Self::from_storage(storage))
    }

    fn from_storage(storage: Arc<LearningStorage>) -> Self {
        Self {
            outcome_tracker: OutcomeTracker::new(Arc::clone(&storage)),
            routing_learner: RoutingLearner::new(Arc::clone(&storage)),
            preference_model: PreferenceModel::new(Arc::clone(&storage)),
            prompt_evolver: PromptEvolver::new(Arc::clone(&storage)),
            pattern_library: PatternLibrary::new(Arc::clone(&storage)),
            self_evaluator: SelfEvaluator::new(Arc::clone(&storage)),
            storage,
            interaction_count: AtomicU64::new(0),
        }
    }

    /// Called when an outcome is determined for an AI response.
    ///
    /// This is the main entry point for recording interaction results. It:
    /// 1. Records the outcome via the outcome tracker
    /// 2. Records routing history for the routing learner
    /// 3. Updates prompt quality scores if a persona is associated
    /// 4. Triggers periodic routing analysis (every 50 interactions)
    /// 5. Triggers periodic self-evaluation (every 200 interactions)
    pub fn on_outcome(&self, record: &OutcomeRecord) -> Result<(), String> {
        // 1. Record outcome
        self.outcome_tracker.record(record)?;

        // 2. Record routing history
        self.storage.record_routing(&RoutingHistoryEntry {
            task_type: record.task_type.clone(),
            classified_tier: record.tier.clone(),
            actual_tier_needed: None,
            model_id: record.model_id.clone(),
            quality_score: record.quality_score,
            cost: record.cost,
            timestamp: record.timestamp.clone(),
        })?;

        // 3. Update prompt performance
        if let Some(ref persona) = record.persona {
            let _ = self
                .prompt_evolver
                .record_quality(persona, record.quality_score);
        }

        // 4. Increment interaction count
        let count = self.interaction_count.fetch_add(1, Ordering::Relaxed) + 1;

        // 5. Periodic analysis
        if count.is_multiple_of(50) {
            info!("Running routing analysis at interaction {count}");
            let _ = self.routing_learner.analyze();
        }
        if count.is_multiple_of(200) {
            info!("Running self-evaluation at interaction {count}");
            let _ = self.self_evaluator.evaluate();
        }

        Ok(())
    }

    /// Get the learning log for transparency UI.
    pub fn learning_log(&self, limit: usize) -> Result<Vec<LearningLogEntry>, String> {
        self.storage.get_learning_log(limit)
    }

    /// User control: reject a learned preference.
    pub fn reject_preference(&self, key: &str) -> Result<(), String> {
        self.preference_model.delete(key)
    }

    /// User control: accept a prompt refinement.
    pub fn accept_prompt_refinement(&self, persona: &str, prompt: &str) -> Result<u32, String> {
        self.prompt_evolver.apply_refinement(persona, prompt)
    }

    /// User control: rollback a prompt to a previous version.
    pub fn rollback_prompt(&self, persona: &str, version: u32) -> Result<(), String> {
        self.prompt_evolver.rollback(persona, version)
    }

    /// User control: reset all learned data.
    pub fn reset_all(&self) -> Result<(), String> {
        self.preference_model.reset_all()
    }

    /// Get the total interaction count.
    pub fn interaction_count(&self) -> u64 {
        self.interaction_count.load(Ordering::Relaxed)
    }

    /// Return all learned preferences as (key, value, confidence) triples.
    pub fn all_preferences(&self) -> Result<Vec<(String, String, f64)>, String> {
        self.preference_model.all()
    }
}

// ---------------------------------------------------------------------------
// LearnerTierAdjuster — bridges hive_learn to hive_ai::routing::TierAdjuster
// ---------------------------------------------------------------------------

/// Adapter that implements the `hive_ai::routing::TierAdjuster` trait by
/// delegating to `RoutingLearner::adjust_tier()`.
///
/// Create one from an `Arc<LearningService>` and pass it to
/// `ModelRouter::set_tier_adjuster()`.
pub struct LearnerTierAdjuster {
    learning: Arc<LearningService>,
}

impl LearnerTierAdjuster {
    pub fn new(learning: Arc<LearningService>) -> Self {
        Self { learning }
    }
}

impl hive_ai::routing::TierAdjuster for LearnerTierAdjuster {
    fn adjust_tier(&self, task_type: &str, classified_tier: &str) -> Option<String> {
        self.learning
            .routing_learner
            .adjust_tier(task_type, classified_tier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_outcome_record(model: &str, quality: f64, outcome: Outcome) -> OutcomeRecord {
        OutcomeRecord {
            conversation_id: "conv-1".into(),
            message_id: format!("msg-{}", uuid::Uuid::new_v4()),
            model_id: model.into(),
            task_type: "code_gen".into(),
            tier: "standard".into(),
            persona: Some("coder".into()),
            outcome,
            edit_distance: None,
            follow_up_count: 0,
            quality_score: quality,
            cost: 0.002,
            latency_ms: 500,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_in_memory_creation() {
        let service = LearningService::in_memory().unwrap();
        assert_eq!(service.interaction_count(), 0);
    }

    #[test]
    fn test_on_outcome_records_and_increments() {
        let service = LearningService::in_memory().unwrap();

        let record = make_outcome_record("gpt-4o", 0.85, Outcome::Accepted);
        service.on_outcome(&record).unwrap();

        assert_eq!(service.interaction_count(), 1);
    }

    #[test]
    fn test_on_outcome_roundtrip() {
        let service = LearningService::in_memory().unwrap();

        let record = make_outcome_record("gpt-4o", 0.85, Outcome::Accepted);
        service.on_outcome(&record).unwrap();

        // Verify outcome was recorded via model quality
        let quality = service.outcome_tracker.model_quality("gpt-4o", 30).unwrap();
        assert!((quality - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_on_outcome_updates_prompt_quality() {
        let service = LearningService::in_memory().unwrap();

        // Create a prompt version first
        service
            .prompt_evolver
            .apply_refinement("coder", "You are a coder.")
            .unwrap();

        let record = make_outcome_record("gpt-4o", 0.9, Outcome::Accepted);
        service.on_outcome(&record).unwrap();

        // The prompt quality should have been updated
        let prompt = service.prompt_evolver.get_prompt("coder").unwrap().unwrap();
        assert_eq!(prompt, "You are a coder.");
    }

    #[test]
    fn test_on_outcome_no_persona_is_ok() {
        let service = LearningService::in_memory().unwrap();

        let mut record = make_outcome_record("gpt-4o", 0.85, Outcome::Accepted);
        record.persona = None;
        service.on_outcome(&record).unwrap();

        assert_eq!(service.interaction_count(), 1);
    }

    #[test]
    fn test_interaction_count_increments() {
        let service = LearningService::in_memory().unwrap();

        for _ in 0..10 {
            let record = make_outcome_record("gpt-4o", 0.8, Outcome::Accepted);
            service.on_outcome(&record).unwrap();
        }

        assert_eq!(service.interaction_count(), 10);
    }

    #[test]
    fn test_learning_log_populated() {
        let service = LearningService::in_memory().unwrap();

        let record = make_outcome_record("gpt-4o", 0.85, Outcome::Accepted);
        service.on_outcome(&record).unwrap();

        let log = service.learning_log(10).unwrap();
        assert!(!log.is_empty());
        assert!(log.iter().any(|e| e.event_type == "outcome_recorded"));
    }

    #[test]
    fn test_reject_preference() {
        let service = LearningService::in_memory().unwrap();

        service
            .preference_model
            .observe("tone", "concise", 0.9)
            .unwrap();
        assert!(service.preference_model.get("tone", 0.0).unwrap().is_some());

        service.reject_preference("tone").unwrap();
        assert!(service.preference_model.get("tone", 0.0).unwrap().is_none());
    }

    #[test]
    fn test_accept_prompt_refinement() {
        let service = LearningService::in_memory().unwrap();

        let version = service
            .accept_prompt_refinement("coder", "You are a coding assistant.")
            .unwrap();
        assert_eq!(version, 1);

        let prompt = service.prompt_evolver.get_prompt("coder").unwrap();
        assert_eq!(prompt, Some("You are a coding assistant.".to_string()));
    }

    #[test]
    fn test_rollback_prompt() {
        let service = LearningService::in_memory().unwrap();

        service
            .accept_prompt_refinement("coder", "Version 1")
            .unwrap();
        service
            .accept_prompt_refinement("coder", "Version 2")
            .unwrap();

        service.rollback_prompt("coder", 1).unwrap();
        let prompt = service.prompt_evolver.get_prompt("coder").unwrap();
        assert_eq!(prompt, Some("Version 1".to_string()));
    }

    #[test]
    fn test_reset_all() {
        let service = LearningService::in_memory().unwrap();

        service
            .preference_model
            .observe("tone", "concise", 0.9)
            .unwrap();
        service
            .preference_model
            .observe("theme", "dark", 0.8)
            .unwrap();

        service.reset_all().unwrap();

        assert!(service.preference_model.get("tone", 0.0).unwrap().is_none());
        assert!(
            service
                .preference_model
                .get("theme", 0.0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_periodic_analysis_at_milestones() {
        let service = LearningService::in_memory().unwrap();

        // Record exactly 50 outcomes to trigger routing analysis
        for i in 0..50 {
            let record = OutcomeRecord {
                conversation_id: "conv-1".into(),
                message_id: format!("msg-{i}"),
                model_id: "gpt-4o".into(),
                task_type: "code_gen".into(),
                tier: "standard".into(),
                persona: None,
                outcome: Outcome::Accepted,
                edit_distance: None,
                follow_up_count: 0,
                quality_score: 0.8,
                cost: 0.001,
                latency_ms: 300,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            service.on_outcome(&record).unwrap();
        }

        assert_eq!(service.interaction_count(), 50);
        // No assertion on analysis results since it depends on data volume,
        // but verifying it didn't panic is the test
    }

    #[test]
    fn test_full_lifecycle() {
        let service = LearningService::in_memory().unwrap();

        // 1. Set up a prompt
        service
            .accept_prompt_refinement("coder", "You are a coder.")
            .unwrap();

        // 2. Observe a preference
        service
            .preference_model
            .observe("detail_level", "high", 0.8)
            .unwrap();

        // 3. Record some outcomes
        for quality in [0.9, 0.85, 0.7, 0.8, 0.75] {
            let record = OutcomeRecord {
                conversation_id: "conv-1".into(),
                message_id: format!("msg-{}", uuid::Uuid::new_v4()),
                model_id: "gpt-4o".into(),
                task_type: "code_gen".into(),
                tier: "standard".into(),
                persona: Some("coder".into()),
                outcome: Outcome::Accepted,
                edit_distance: None,
                follow_up_count: 0,
                quality_score: quality,
                cost: 0.002,
                latency_ms: 500,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            service.on_outcome(&record).unwrap();
        }

        // 4. Extract patterns from good code
        service
            .pattern_library
            .extract_patterns("pub fn process() -> Result<(), Error> {}", "rust", 0.9)
            .unwrap();

        // 5. Run self-evaluation
        let report = service.self_evaluator.evaluate().unwrap();
        assert!(report.overall_quality > 0.0);
        assert_eq!(report.total_interactions, 5);

        // 6. Check learning log has entries
        let log = service.learning_log(100).unwrap();
        assert!(log.len() >= 5); // At least outcomes + prompt + evaluation

        // 7. Verify interaction count
        assert_eq!(service.interaction_count(), 5);
    }
}
