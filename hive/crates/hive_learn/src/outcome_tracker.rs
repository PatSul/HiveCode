use crate::storage::LearningStorage;
use crate::types::*;
use std::collections::HashSet;
use std::sync::Arc;

/// Detects and records what happened after each AI response.
pub struct OutcomeTracker {
    storage: Arc<LearningStorage>,
}

impl OutcomeTracker {
    pub fn new(storage: Arc<LearningStorage>) -> Self {
        Self { storage }
    }

    /// Detect outcome by comparing the previous assistant message with the new user message.
    ///
    /// Uses keyword detection for explicit regeneration requests, and Jaccard token
    /// similarity for distinguishing between ignored, corrected, and accepted outcomes.
    pub fn detect_outcome(previous_assistant_msg: &str, new_user_msg: &str) -> Outcome {
        let lower = new_user_msg.to_lowercase();

        // Explicit regeneration keywords
        if lower.contains("try again")
            || lower.contains("redo")
            || lower.contains("that's wrong")
            || lower.contains("regenerate")
            || lower.contains("not what i")
            || lower.contains("no, ")
        {
            return Outcome::Regenerated;
        }

        // Compute Jaccard token similarity
        let sim = jaccard_similarity(previous_assistant_msg, new_user_msg);

        if sim < 0.05 {
            return Outcome::Ignored; // completely unrelated followup
        }
        if sim > 0.3 && sim < 0.9 {
            return Outcome::Corrected;
        }

        Outcome::Accepted
    }

    /// Compute quality score from outcome and context.
    ///
    /// Applies penalties for multiple follow-ups (diminishing returns) and large
    /// edit distances. The result is clamped to [0.0, 1.0].
    pub fn compute_quality_score(
        outcome: Outcome,
        follow_ups: u32,
        edit_distance: Option<f64>,
        _response_length: usize,
        _time_to_accept_ms: Option<u64>,
    ) -> f64 {
        let mut score = outcome.base_quality_score();

        // Penalty for many follow-ups (diminishing)
        if follow_ups > 0 {
            score -= (follow_ups as f64 * 0.05).min(0.3);
        }

        // Penalty for large edit distance
        if let Some(ed) = edit_distance {
            if ed > 0.5 {
                score -= 0.1;
            }
            if ed > 0.8 {
                score -= 0.1;
            }
        }

        score.clamp(0.0, 1.0)
    }

    /// Record an outcome and log it to the transparent learning log.
    pub fn record(&self, record: &OutcomeRecord) -> Result<(), String> {
        self.storage.record_outcome(record)?;
        self.storage.log_learning(&LearningLogEntry {
            id: 0,
            event_type: "outcome_recorded".into(),
            description: format!(
                "Outcome {:?} for model {} (quality: {:.2})",
                record.outcome, record.model_id, record.quality_score
            ),
            details: serde_json::to_string(record).unwrap_or_default(),
            reversible: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })?;
        Ok(())
    }

    /// Rolling average quality for a model over N days.
    pub fn model_quality(&self, model_id: &str, days: u32) -> Result<f64, String> {
        self.storage.model_quality(model_id, days)
    }

    /// Rolling average quality for a task_type + tier combination.
    pub fn task_tier_quality(
        &self,
        task_type: &str,
        tier: &str,
        days: u32,
    ) -> Result<f64, String> {
        self.storage.task_tier_quality(task_type, tier, days)
    }
}

/// Jaccard similarity between two strings based on word tokens.
///
/// Returns a value in [0.0, 1.0] where 1.0 means identical token sets.
/// Both empty strings yield 1.0 (identical nothingness).
/// One empty and one non-empty yields 0.0.
fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let tokens_a: HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    if tokens_a.is_empty() || tokens_b.is_empty() {
        return 0.0;
    }
    let intersection = tokens_a.intersection(&tokens_b).count() as f64;
    let union = tokens_a.union(&tokens_b).count() as f64;
    intersection / union
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── jaccard_similarity tests ─────────────────────────────────────

    #[test]
    fn test_jaccard_identical() {
        let sim = jaccard_similarity("hello world", "hello world");
        assert!((sim - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaccard_completely_different() {
        let sim = jaccard_similarity("alpha beta gamma", "delta epsilon zeta");
        assert!((sim - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaccard_partial_overlap() {
        // tokens_a = {hello, world, foo}, tokens_b = {hello, world, bar}
        // intersection = 2, union = 4
        let sim = jaccard_similarity("hello world foo", "hello world bar");
        assert!((sim - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaccard_both_empty() {
        let sim = jaccard_similarity("", "");
        assert!((sim - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaccard_one_empty() {
        assert!((jaccard_similarity("hello", "") - 0.0).abs() < f64::EPSILON);
        assert!((jaccard_similarity("", "hello") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaccard_subset() {
        // tokens_a = {a, b, c}, tokens_b = {a, b, c, d, e}
        // intersection = 3, union = 5
        let sim = jaccard_similarity("a b c", "a b c d e");
        assert!((sim - 0.6).abs() < f64::EPSILON);
    }

    // ── detect_outcome tests ─────────────────────────────────────────

    #[test]
    fn test_detect_regeneration_try_again() {
        assert_eq!(
            OutcomeTracker::detect_outcome("Here is the code.", "try again please"),
            Outcome::Regenerated
        );
    }

    #[test]
    fn test_detect_regeneration_redo() {
        assert_eq!(
            OutcomeTracker::detect_outcome("Here is the code.", "Redo this completely"),
            Outcome::Regenerated
        );
    }

    #[test]
    fn test_detect_regeneration_thats_wrong() {
        assert_eq!(
            OutcomeTracker::detect_outcome("Some answer.", "that's wrong, fix it"),
            Outcome::Regenerated
        );
    }

    #[test]
    fn test_detect_regeneration_regenerate() {
        assert_eq!(
            OutcomeTracker::detect_outcome("Some code.", "regenerate the response"),
            Outcome::Regenerated
        );
    }

    #[test]
    fn test_detect_regeneration_not_what_i() {
        assert_eq!(
            OutcomeTracker::detect_outcome("result", "not what i asked for"),
            Outcome::Regenerated
        );
    }

    #[test]
    fn test_detect_regeneration_no_comma() {
        assert_eq!(
            OutcomeTracker::detect_outcome("result", "no, I want something else"),
            Outcome::Regenerated
        );
    }

    #[test]
    fn test_detect_ignored_completely_unrelated() {
        // Completely different topics, low similarity
        assert_eq!(
            OutcomeTracker::detect_outcome(
                "The Rust borrow checker ensures memory safety without garbage collection.",
                "What time is the movie tonight?"
            ),
            Outcome::Ignored
        );
    }

    #[test]
    fn test_detect_corrected_moderate_overlap() {
        // Construct strings with ~40% Jaccard overlap (shared + unique tokens)
        // shared: the function should handle errors and return a result type
        // assistant adds: gracefully with proper logging
        // user adds: by using a custom error enum
        let assistant = "the function should handle errors and return a result type gracefully with proper logging";
        let user = "the function should handle errors and return a result type by using a custom error enum";
        let sim = jaccard_similarity(assistant, user);
        assert!(sim > 0.3 && sim < 0.9, "sim was {sim}");
        assert_eq!(
            OutcomeTracker::detect_outcome(assistant, user),
            Outcome::Corrected
        );
    }

    #[test]
    fn test_detect_accepted_high_similarity() {
        // Near-identical or very high overlap
        let msg = "Here is the implementation of the sorting algorithm with O(n log n) complexity";
        assert_eq!(
            OutcomeTracker::detect_outcome(msg, msg),
            Outcome::Accepted
        );
    }

    #[test]
    fn test_detect_accepted_low_but_above_threshold() {
        // Similarity between 0.05 and 0.3 (or >= 0.9) => Accepted
        // With just a bit of overlap
        let assistant = "cat dog";
        let user = "cat fish bird snake elephant";
        let sim = jaccard_similarity(assistant, user);
        assert!(
            sim > 0.05 && sim <= 0.3,
            "expected sim in (0.05, 0.3], got {sim}"
        );
        assert_eq!(
            OutcomeTracker::detect_outcome(assistant, user),
            Outcome::Accepted
        );
    }

    // ── compute_quality_score tests ──────────────────────────────────

    #[test]
    fn test_quality_score_accepted_no_penalties() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Accepted, 0, None, 100, None);
        assert!((score - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_regenerated() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Regenerated, 0, None, 100, None);
        assert!((score - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_follow_up_penalty() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Accepted, 3, None, 100, None);
        // 0.9 - 3*0.05 = 0.75
        assert!((score - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_follow_up_penalty_caps_at_0_3() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Accepted, 100, None, 100, None);
        // 0.9 - 0.3 (capped) = 0.6
        assert!((score - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_edit_distance_penalty_medium() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Accepted, 0, Some(0.6), 100, None);
        // 0.9 - 0.1 (ed > 0.5) = 0.8
        assert!((score - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_edit_distance_penalty_high() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Accepted, 0, Some(0.9), 100, None);
        // 0.9 - 0.1 (ed > 0.5) - 0.1 (ed > 0.8) = 0.7
        assert!((score - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_combined_penalties() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Corrected, 4, Some(0.9), 100, None);
        // 0.5 - 0.2 (4*0.05) - 0.1 - 0.1 = 0.1
        assert!((score - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_clamps_to_zero() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Ignored, 100, Some(0.95), 100, None);
        // 0.1 - 0.3 - 0.1 - 0.1 = -0.4 → clamped to 0.0
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_no_edit_distance_penalty_below_threshold() {
        let score =
            OutcomeTracker::compute_quality_score(Outcome::Accepted, 0, Some(0.3), 100, None);
        // 0.9 - 0 (ed <= 0.5) = 0.9
        assert!((score - 0.9).abs() < f64::EPSILON);
    }

    // ── record and model_quality tests ───────────────────────────────

    #[test]
    fn test_record_and_model_quality() {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let tracker = OutcomeTracker::new(Arc::clone(&storage));

        let record = OutcomeRecord {
            conversation_id: "conv-1".into(),
            message_id: "msg-1".into(),
            model_id: "gpt-4o".into(),
            task_type: "code_gen".into(),
            tier: "premium".into(),
            persona: None,
            outcome: Outcome::Accepted,
            edit_distance: None,
            follow_up_count: 0,
            quality_score: 0.85,
            cost: 0.002,
            latency_ms: 500,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        tracker.record(&record).unwrap();
        let quality = tracker.model_quality("gpt-4o", 30).unwrap();
        assert!((quality - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_record_logs_to_learning_log() {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let tracker = OutcomeTracker::new(Arc::clone(&storage));

        let record = OutcomeRecord {
            conversation_id: "conv-1".into(),
            message_id: "msg-1".into(),
            model_id: "claude-3".into(),
            task_type: "chat".into(),
            tier: "standard".into(),
            persona: Some("assistant".into()),
            outcome: Outcome::Corrected,
            edit_distance: Some(0.4),
            follow_up_count: 1,
            quality_score: 0.5,
            cost: 0.001,
            latency_ms: 300,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        tracker.record(&record).unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].event_type, "outcome_recorded");
        assert!(log[0].description.contains("Corrected"));
        assert!(log[0].description.contains("claude-3"));
    }

    #[test]
    fn test_task_tier_quality() {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let tracker = OutcomeTracker::new(Arc::clone(&storage));

        for quality in [0.7, 0.8, 0.9] {
            let record = OutcomeRecord {
                conversation_id: "conv-1".into(),
                message_id: format!("msg-{}", uuid::Uuid::new_v4()),
                model_id: "gpt-4o".into(),
                task_type: "debugging".into(),
                tier: "standard".into(),
                persona: None,
                outcome: Outcome::Accepted,
                edit_distance: None,
                follow_up_count: 0,
                quality_score: quality,
                cost: 0.001,
                latency_ms: 400,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            tracker.record(&record).unwrap();
        }

        let avg = tracker.task_tier_quality("debugging", "standard", 30).unwrap();
        assert!((avg - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_model_quality_no_data() {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let tracker = OutcomeTracker::new(storage);
        let quality = tracker.model_quality("nonexistent", 30).unwrap();
        assert!((quality - 0.0).abs() < f64::EPSILON);
    }
}
