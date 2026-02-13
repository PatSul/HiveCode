use crate::storage::LearningStorage;
use crate::types::*;
use std::sync::Arc;

/// Gradual, user-approved prompt refinement system.
///
/// Tracks prompt versions per persona, records quality scores over time,
/// and suggests refinements when quality is consistently low. All changes
/// require explicit user approval.
pub struct PromptEvolver {
    storage: Arc<LearningStorage>,
}

impl PromptEvolver {
    pub fn new(storage: Arc<LearningStorage>) -> Self {
        Self { storage }
    }

    /// Return the active evolved prompt text for a persona, or None if no evolved version exists.
    pub fn get_prompt(&self, persona: &str) -> Result<Option<String>, String> {
        match self.storage.get_active_prompt(persona)? {
            Some(pv) => Ok(Some(pv.prompt_text)),
            None => Ok(None),
        }
    }

    /// Update avg_quality for the active version of a persona using a running average.
    ///
    /// The running average is: `new_avg = (old_avg * sample_count + quality_score) / (sample_count + 1)`
    pub fn record_quality(&self, persona: &str, quality_score: f64) -> Result<(), String> {
        let active = match self.storage.get_active_prompt(persona)? {
            Some(pv) => pv,
            None => return Ok(()), // No active prompt to update
        };

        let count = active.sample_count as f64;
        let new_avg = (active.avg_quality * count + quality_score) / (count + 1.0);
        let new_count = active.sample_count + 1;

        self.storage
            .update_prompt_quality(persona, new_avg, new_count)?;

        Ok(())
    }

    /// Suggest refinements for personas with poor performance.
    ///
    /// Only triggers when a persona has 20+ outcomes with avg quality < 0.6.
    /// Does NOT auto-apply; returns suggestions for user review.
    pub fn suggest_refinements(&self) -> Result<Vec<PromptRefinement>, String> {
        let active_prompts = self.storage.all_active_prompts()?;
        let mut suggestions = Vec::new();

        for pv in &active_prompts {
            if pv.sample_count >= 20 && pv.avg_quality < 0.6 {
                let suggested_prompt =
                    generate_refinement_suggestion(&pv.prompt_text, pv.avg_quality);
                let refinement = PromptRefinement {
                    persona: pv.persona.clone(),
                    current_version: pv.version,
                    suggested_prompt,
                    reason: format!(
                        "Average quality {:.2} over {} samples is below 0.6 threshold",
                        pv.avg_quality, pv.sample_count
                    ),
                };
                suggestions.push(refinement);
            }
        }

        if !suggestions.is_empty() {
            self.storage.log_learning(&LearningLogEntry {
                id: 0,
                event_type: "refinement_suggested".into(),
                description: format!(
                    "Suggested {} prompt refinement(s) for review",
                    suggestions.len()
                ),
                details: serde_json::to_string(&suggestions).unwrap_or_default(),
                reversible: false,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })?;
        }

        Ok(suggestions)
    }

    /// Apply a user-approved refinement.
    ///
    /// Creates a new version with the given prompt text, activates it, and
    /// returns the new version number.
    pub fn apply_refinement(&self, persona: &str, new_prompt: &str) -> Result<u32, String> {
        let max_version = self.storage.max_prompt_version(persona)?;
        let new_version = max_version + 1;

        // Deactivate all existing versions for this persona by activating the new one
        let pv = PromptVersion {
            persona: persona.to_string(),
            version: new_version,
            prompt_text: new_prompt.to_string(),
            avg_quality: 0.0,
            sample_count: 0,
            is_active: false, // will be activated below
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.storage.save_prompt_version(&pv)?;
        self.storage.activate_prompt_version(persona, new_version)?;

        self.storage.log_learning(&LearningLogEntry {
            id: 0,
            event_type: "prompt_refinement_applied".into(),
            description: format!(
                "Applied prompt refinement for persona '{persona}': version {new_version}"
            ),
            details: format!(
                "{{\"persona\":\"{persona}\",\"version\":{new_version},\"prompt_length\":{}}}",
                new_prompt.len()
            ),
            reversible: true,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })?;

        Ok(new_version)
    }

    /// Rollback to a specific prompt version.
    ///
    /// Activates the specified version and deactivates all others for the persona.
    pub fn rollback(&self, persona: &str, to_version: u32) -> Result<(), String> {
        // Verify the version exists
        let versions = self.storage.get_prompt_versions(persona)?;
        let exists = versions.iter().any(|pv| pv.version == to_version);
        if !exists {
            return Err(format!(
                "Version {to_version} does not exist for persona '{persona}'"
            ));
        }

        self.storage.activate_prompt_version(persona, to_version)?;

        self.storage.log_learning(&LearningLogEntry {
            id: 0,
            event_type: "prompt_rollback".into(),
            description: format!("Rolled back persona '{persona}' to version {to_version}"),
            details: format!("{{\"persona\":\"{persona}\",\"to_version\":{to_version}}}"),
            reversible: true,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })?;

        Ok(())
    }
}

/// Generate a refinement suggestion based on current prompt and quality.
///
/// This is a rule-based generator that adds guidance clauses to the existing prompt
/// when quality is low. In a production system, this could be replaced with an
/// AI-powered prompt rewriter.
fn generate_refinement_suggestion(current_prompt: &str, avg_quality: f64) -> String {
    let mut suggestion = current_prompt.to_string();

    if avg_quality < 0.3 {
        suggestion.push_str(
            "\n\nIMPORTANT: Previous responses were frequently rejected. \
             Focus on accuracy and completeness. Double-check all code before responding. \
             Ask clarifying questions when the request is ambiguous.",
        );
    } else if avg_quality < 0.5 {
        suggestion.push_str(
            "\n\nNote: Response quality has been below expectations. \
             Be more thorough in your responses and provide working examples. \
             Verify code compiles and handles edge cases.",
        );
    } else {
        suggestion.push_str(
            "\n\nNote: There is room for improvement. \
             Pay attention to user corrections and adapt your style accordingly.",
        );
    }

    suggestion
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_evolver() -> PromptEvolver {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        PromptEvolver::new(storage)
    }

    fn make_evolver_with_storage() -> (PromptEvolver, Arc<LearningStorage>) {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let evolver = PromptEvolver::new(Arc::clone(&storage));
        (evolver, storage)
    }

    // ── get_prompt tests ─────────────────────────────────────────────

    #[test]
    fn test_get_prompt_none_when_empty() {
        let evolver = make_evolver();
        assert_eq!(evolver.get_prompt("coder").unwrap(), None);
    }

    #[test]
    fn test_get_prompt_returns_active() {
        let (evolver, _) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "You are a coding assistant.")
            .unwrap();

        let prompt = evolver.get_prompt("coder").unwrap();
        assert_eq!(prompt, Some("You are a coding assistant.".to_string()));
    }

    #[test]
    fn test_get_prompt_returns_latest_active() {
        let (evolver, _) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "Version 1 prompt.")
            .unwrap();
        evolver
            .apply_refinement("coder", "Version 2 prompt.")
            .unwrap();

        let prompt = evolver.get_prompt("coder").unwrap();
        assert_eq!(prompt, Some("Version 2 prompt.".to_string()));
    }

    // ── record_quality tests ─────────────────────────────────────────

    #[test]
    fn test_record_quality_updates_running_average() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "You are a coder.")
            .unwrap();

        // First quality: avg = 0.8
        evolver.record_quality("coder", 0.8).unwrap();
        let active = storage.get_active_prompt("coder").unwrap().unwrap();
        assert!((active.avg_quality - 0.8).abs() < f64::EPSILON);
        assert_eq!(active.sample_count, 1);

        // Second quality: avg = (0.8*1 + 0.6) / 2 = 0.7
        evolver.record_quality("coder", 0.6).unwrap();
        let active = storage.get_active_prompt("coder").unwrap().unwrap();
        assert!((active.avg_quality - 0.7).abs() < f64::EPSILON);
        assert_eq!(active.sample_count, 2);
    }

    #[test]
    fn test_record_quality_no_active_prompt_is_noop() {
        let evolver = make_evolver();
        // Should not error when no active prompt exists
        evolver.record_quality("nonexistent", 0.8).unwrap();
    }

    // ── suggest_refinements tests ────────────────────────────────────

    #[test]
    fn test_suggest_refinements_empty_when_no_prompts() {
        let evolver = make_evolver();
        let suggestions = evolver.suggest_refinements().unwrap();
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_refinements_empty_when_quality_ok() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "You are a coder.")
            .unwrap();

        // Manually set high quality with enough samples
        storage.update_prompt_quality("coder", 0.8, 25).unwrap();

        let suggestions = evolver.suggest_refinements().unwrap();
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_refinements_empty_when_insufficient_samples() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "You are a coder.")
            .unwrap();

        // Low quality but not enough samples
        storage.update_prompt_quality("coder", 0.3, 10).unwrap();

        let suggestions = evolver.suggest_refinements().unwrap();
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_refinements_triggers_on_low_quality() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "You are a coder.")
            .unwrap();

        // Low quality with enough samples
        storage.update_prompt_quality("coder", 0.4, 25).unwrap();

        let suggestions = evolver.suggest_refinements().unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].persona, "coder");
        assert_eq!(suggestions[0].current_version, 1);
        assert!(suggestions[0].reason.contains("0.40"));
        assert!(suggestions[0].suggested_prompt.contains("You are a coder."));
    }

    #[test]
    fn test_suggest_refinements_very_low_quality_prompt() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "You are a coder.")
            .unwrap();

        storage.update_prompt_quality("coder", 0.2, 30).unwrap();

        let suggestions = evolver.suggest_refinements().unwrap();
        assert_eq!(suggestions.len(), 1);
        assert!(
            suggestions[0]
                .suggested_prompt
                .contains("frequently rejected")
        );
    }

    #[test]
    fn test_suggest_refinements_logs() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver
            .apply_refinement("coder", "You are a coder.")
            .unwrap();

        storage.update_prompt_quality("coder", 0.4, 25).unwrap();
        evolver.suggest_refinements().unwrap();

        let log = storage.get_learning_log(20).unwrap();
        assert!(log.iter().any(|e| e.event_type == "refinement_suggested"));
    }

    // ── apply_refinement tests ───────────────────────────────────────

    #[test]
    fn test_apply_refinement_creates_version() {
        let (evolver, storage) = make_evolver_with_storage();

        let v1 = evolver.apply_refinement("coder", "Version 1").unwrap();
        assert_eq!(v1, 1);

        let v2 = evolver.apply_refinement("coder", "Version 2").unwrap();
        assert_eq!(v2, 2);

        let active = storage.get_active_prompt("coder").unwrap().unwrap();
        assert_eq!(active.version, 2);
        assert_eq!(active.prompt_text, "Version 2");
        assert_eq!(active.sample_count, 0);
        assert!((active.avg_quality - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_apply_refinement_deactivates_previous() {
        let (evolver, storage) = make_evolver_with_storage();

        evolver.apply_refinement("coder", "V1").unwrap();
        evolver.apply_refinement("coder", "V2").unwrap();

        let versions = storage.get_prompt_versions("coder").unwrap();
        assert_eq!(versions.len(), 2);

        let v1 = versions.iter().find(|v| v.version == 1).unwrap();
        let v2 = versions.iter().find(|v| v.version == 2).unwrap();
        assert!(!v1.is_active);
        assert!(v2.is_active);
    }

    #[test]
    fn test_apply_refinement_logs() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver.apply_refinement("coder", "V1").unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert!(
            log.iter()
                .any(|e| e.event_type == "prompt_refinement_applied")
        );
    }

    // ── rollback tests ───────────────────────────────────────────────

    #[test]
    fn test_rollback_to_previous_version() {
        let (evolver, storage) = make_evolver_with_storage();

        evolver.apply_refinement("coder", "V1").unwrap();
        evolver.apply_refinement("coder", "V2").unwrap();
        evolver.apply_refinement("coder", "V3").unwrap();

        evolver.rollback("coder", 1).unwrap();

        let active = storage.get_active_prompt("coder").unwrap().unwrap();
        assert_eq!(active.version, 1);
        assert_eq!(active.prompt_text, "V1");
    }

    #[test]
    fn test_rollback_nonexistent_version_errors() {
        let evolver = make_evolver();
        let result = evolver.rollback("coder", 99);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_rollback_logs() {
        let (evolver, storage) = make_evolver_with_storage();
        evolver.apply_refinement("coder", "V1").unwrap();
        evolver.apply_refinement("coder", "V2").unwrap();
        evolver.rollback("coder", 1).unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert!(log.iter().any(|e| e.event_type == "prompt_rollback"));
    }

    // ── generate_refinement_suggestion tests ─────────────────────────

    #[test]
    fn test_refinement_suggestion_very_low() {
        let result = generate_refinement_suggestion("Base prompt.", 0.2);
        assert!(result.contains("Base prompt."));
        assert!(result.contains("frequently rejected"));
    }

    #[test]
    fn test_refinement_suggestion_low() {
        let result = generate_refinement_suggestion("Base prompt.", 0.45);
        assert!(result.contains("below expectations"));
    }

    #[test]
    fn test_refinement_suggestion_moderate() {
        let result = generate_refinement_suggestion("Base prompt.", 0.55);
        assert!(result.contains("room for improvement"));
    }

    // ── multi-persona isolation ──────────────────────────────────────

    #[test]
    fn test_separate_personas() {
        let (evolver, _) = make_evolver_with_storage();

        evolver.apply_refinement("coder", "Coder prompt.").unwrap();
        evolver
            .apply_refinement("writer", "Writer prompt.")
            .unwrap();

        assert_eq!(
            evolver.get_prompt("coder").unwrap(),
            Some("Coder prompt.".to_string())
        );
        assert_eq!(
            evolver.get_prompt("writer").unwrap(),
            Some("Writer prompt.".to_string())
        );
    }
}
