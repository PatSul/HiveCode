use crate::storage::LearningStorage;
use crate::types::*;
use std::sync::Arc;

/// Tracks and learns user style/behavior preferences via Bayesian confidence updates.
///
/// Each preference is a key-value pair with a confidence score that increases
/// with repeated consistent observations. Preferences are only surfaced when
/// confidence exceeds a caller-specified threshold.
pub struct PreferenceModel {
    storage: Arc<LearningStorage>,
}

impl PreferenceModel {
    pub fn new(storage: Arc<LearningStorage>) -> Self {
        Self { storage }
    }

    /// Observe a preference signal.
    ///
    /// Uses a Bayesian confidence update: the new confidence is the running average
    /// of all observed signal strengths, weighted equally. This means confidence
    /// converges to the mean signal strength over time.
    ///
    /// Formula: `new_conf = (old_conf * count + signal_strength) / (count + 1)`
    pub fn observe(
        &self,
        key: &str,
        value: &str,
        signal_strength: f64,
    ) -> Result<(), String> {
        let signal = signal_strength.clamp(0.0, 1.0);

        let existing = self.storage.get_preference(key)?;

        let (new_confidence, new_count) = match &existing {
            Some(pref) => {
                let count = pref.observation_count as f64;
                let new_conf = (pref.confidence * count + signal) / (count + 1.0);
                (new_conf, pref.observation_count + 1)
            }
            None => (signal, 1),
        };

        let pref = UserPreference {
            key: key.to_string(),
            value: value.to_string(),
            confidence: new_confidence,
            observation_count: new_count,
            last_updated: chrono::Utc::now().to_rfc3339(),
        };

        self.storage.set_preference(&pref)?;

        self.storage.log_learning(&LearningLogEntry {
            id: 0,
            event_type: "preference_observed".into(),
            description: format!(
                "Observed preference {key}={value} (signal={signal:.2}, confidence={new_confidence:.2}, count={new_count})"
            ),
            details: serde_json::to_string(&pref).unwrap_or_default(),
            reversible: true,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })?;

        Ok(())
    }

    /// Get a preference value only if its confidence exceeds the given threshold.
    ///
    /// Returns `None` if the preference does not exist or its confidence is below
    /// `min_confidence`.
    pub fn get(
        &self,
        key: &str,
        min_confidence: f64,
    ) -> Result<Option<String>, String> {
        match self.storage.get_preference(key)? {
            Some(pref) if pref.confidence >= min_confidence => Ok(Some(pref.value)),
            _ => Ok(None),
        }
    }

    /// Generate a system prompt addendum from all confident preferences.
    ///
    /// Only includes preferences with confidence >= 0.6. The output is a formatted
    /// paragraph suitable for injection into a system prompt.
    pub fn prompt_addendum(&self) -> Result<String, String> {
        let all = self.storage.all_preferences()?;

        let confident: Vec<&UserPreference> =
            all.iter().filter(|p| p.confidence >= 0.6).collect();

        if confident.is_empty() {
            return Ok(String::new());
        }

        let mut parts = Vec::new();
        parts.push("Based on observed user preferences:".to_string());

        for pref in &confident {
            parts.push(format!(
                "- {}: {} (confidence: {:.0}%)",
                pref.key,
                pref.value,
                pref.confidence * 100.0
            ));
        }

        Ok(parts.join("\n"))
    }

    /// Delete a specific preference by key.
    pub fn delete(&self, key: &str) -> Result<(), String> {
        self.storage.delete_preference(key)?;

        self.storage.log_learning(&LearningLogEntry {
            id: 0,
            event_type: "preference_deleted".into(),
            description: format!("User rejected preference: {key}"),
            details: String::new(),
            reversible: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })?;

        Ok(())
    }

    /// Return all preferences as (key, value, confidence) triples.
    pub fn all(&self) -> Result<Vec<(String, String, f64)>, String> {
        let prefs = self.storage.all_preferences()?;
        Ok(prefs
            .into_iter()
            .map(|p| (p.key, p.value, p.confidence))
            .collect())
    }

    /// Reset all learned preferences.
    pub fn reset_all(&self) -> Result<(), String> {
        self.storage.reset_preferences()?;

        self.storage.log_learning(&LearningLogEntry {
            id: 0,
            event_type: "preferences_reset".into(),
            description: "All preferences reset by user".into(),
            details: String::new(),
            reversible: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_model() -> PreferenceModel {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        PreferenceModel::new(storage)
    }

    fn make_model_with_storage() -> (PreferenceModel, Arc<LearningStorage>) {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let model = PreferenceModel::new(Arc::clone(&storage));
        (model, storage)
    }

    // ── observe tests ────────────────────────────────────────────────

    #[test]
    fn test_observe_single() {
        let model = make_model();
        model.observe("tone", "concise", 0.8).unwrap();

        let val = model.get("tone", 0.5).unwrap();
        assert_eq!(val, Some("concise".to_string()));
    }

    #[test]
    fn test_observe_updates_confidence() {
        let (model, storage) = make_model_with_storage();

        // First observation: confidence = 0.8
        model.observe("tone", "concise", 0.8).unwrap();
        let pref = storage.get_preference("tone").unwrap().unwrap();
        assert!((pref.confidence - 0.8).abs() < f64::EPSILON);
        assert_eq!(pref.observation_count, 1);

        // Second observation: confidence = (0.8*1 + 0.6) / 2 = 0.7
        model.observe("tone", "concise", 0.6).unwrap();
        let pref = storage.get_preference("tone").unwrap().unwrap();
        assert!((pref.confidence - 0.7).abs() < f64::EPSILON);
        assert_eq!(pref.observation_count, 2);

        // Third observation: confidence = (0.7*2 + 0.9) / 3 = 2.3/3 ~= 0.7667
        model.observe("tone", "concise", 0.9).unwrap();
        let pref = storage.get_preference("tone").unwrap().unwrap();
        let expected = (0.7 * 2.0 + 0.9) / 3.0;
        assert!((pref.confidence - expected).abs() < 1e-10);
        assert_eq!(pref.observation_count, 3);
    }

    #[test]
    fn test_observe_clamps_signal_strength() {
        let (model, storage) = make_model_with_storage();

        model.observe("theme", "dark", 1.5).unwrap(); // clamped to 1.0
        let pref = storage.get_preference("theme").unwrap().unwrap();
        assert!((pref.confidence - 1.0).abs() < f64::EPSILON);

        model.observe("indent", "tabs", -0.5).unwrap(); // clamped to 0.0
        let pref = storage.get_preference("indent").unwrap().unwrap();
        assert!((pref.confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_observe_updates_value() {
        let model = make_model();

        model.observe("theme", "dark", 0.8).unwrap();
        assert_eq!(model.get("theme", 0.0).unwrap(), Some("dark".to_string()));

        model.observe("theme", "light", 0.9).unwrap();
        assert_eq!(model.get("theme", 0.0).unwrap(), Some("light".to_string()));
    }

    // ── get tests ────────────────────────────────────────────────────

    #[test]
    fn test_get_returns_none_below_threshold() {
        let model = make_model();
        model.observe("tone", "concise", 0.3).unwrap();

        assert_eq!(model.get("tone", 0.5).unwrap(), None);
        assert_eq!(model.get("tone", 0.3).unwrap(), Some("concise".to_string()));
    }

    #[test]
    fn test_get_returns_none_for_missing_key() {
        let model = make_model();
        assert_eq!(model.get("nonexistent", 0.0).unwrap(), None);
    }

    #[test]
    fn test_get_at_exact_threshold() {
        let model = make_model();
        model.observe("tone", "concise", 0.7).unwrap();
        // Confidence is exactly 0.7, threshold is 0.7 -> should return
        assert_eq!(model.get("tone", 0.7).unwrap(), Some("concise".to_string()));
    }

    // ── prompt_addendum tests ────────────────────────────────────────

    #[test]
    fn test_prompt_addendum_empty() {
        let model = make_model();
        let addendum = model.prompt_addendum().unwrap();
        assert!(addendum.is_empty());
    }

    #[test]
    fn test_prompt_addendum_only_confident() {
        let model = make_model();

        model.observe("tone", "concise", 0.8).unwrap();
        model.observe("format", "markdown", 0.3).unwrap(); // below 0.6 threshold

        let addendum = model.prompt_addendum().unwrap();
        assert!(addendum.contains("tone"));
        assert!(addendum.contains("concise"));
        assert!(!addendum.contains("format"));
        assert!(!addendum.contains("markdown"));
    }

    #[test]
    fn test_prompt_addendum_multiple_preferences() {
        let model = make_model();

        model.observe("tone", "concise", 0.9).unwrap();
        model.observe("language", "english", 0.8).unwrap();
        model.observe("detail", "high", 0.7).unwrap();

        let addendum = model.prompt_addendum().unwrap();
        assert!(addendum.contains("Based on observed user preferences:"));
        assert!(addendum.contains("tone: concise"));
        assert!(addendum.contains("language: english"));
        assert!(addendum.contains("detail: high"));
    }

    #[test]
    fn test_prompt_addendum_includes_confidence_percentage() {
        let model = make_model();
        model.observe("tone", "concise", 0.8).unwrap();

        let addendum = model.prompt_addendum().unwrap();
        assert!(addendum.contains("80%"));
    }

    // ── delete tests ─────────────────────────────────────────────────

    #[test]
    fn test_delete_removes_preference() {
        let model = make_model();
        model.observe("tone", "concise", 0.8).unwrap();
        assert!(model.get("tone", 0.0).unwrap().is_some());

        model.delete("tone").unwrap();
        assert!(model.get("tone", 0.0).unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent_ok() {
        let model = make_model();
        // Should not error on deleting a non-existent key
        model.delete("nonexistent").unwrap();
    }

    #[test]
    fn test_delete_logs_to_learning_log() {
        let (model, storage) = make_model_with_storage();
        model.observe("tone", "concise", 0.8).unwrap();
        model.delete("tone").unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert!(log.iter().any(|e| e.event_type == "preference_deleted"));
    }

    // ── reset_all tests ──────────────────────────────────────────────

    #[test]
    fn test_reset_all() {
        let model = make_model();

        model.observe("tone", "concise", 0.8).unwrap();
        model.observe("theme", "dark", 0.9).unwrap();

        model.reset_all().unwrap();

        assert!(model.get("tone", 0.0).unwrap().is_none());
        assert!(model.get("theme", 0.0).unwrap().is_none());
    }

    #[test]
    fn test_reset_all_logs() {
        let (model, storage) = make_model_with_storage();
        model.observe("tone", "concise", 0.8).unwrap();
        model.reset_all().unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert!(log.iter().any(|e| e.event_type == "preferences_reset"));
    }

    #[test]
    fn test_reset_all_empty_is_ok() {
        let model = make_model();
        model.reset_all().unwrap();
    }

    // ── observe logs to learning_log ─────────────────────────────────

    #[test]
    fn test_observe_logs() {
        let (model, storage) = make_model_with_storage();
        model.observe("tone", "concise", 0.8).unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].event_type, "preference_observed");
        assert!(log[0].description.contains("tone=concise"));
    }
}
