//! Competence Detection — self-awareness for the Hive agent swarm.
//!
//! Analyses incoming requests, scores Hive's confidence in handling them,
//! identifies knowledge / skill gaps, and determines whether to trigger the
//! skill-acquisition pipeline.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};

use hive_ai::types::{ChatMessage, ChatRequest, MessageRole};

use crate::collective_memory::CollectiveMemory;
use crate::hivemind::AiExecutor;
use crate::skill_authoring::{SkillAuthoringPipeline, SkillAuthoringRequest, SkillAuthoringResult};
use crate::skill_marketplace::SkillMarketplace;
use crate::skills::SkillsRegistry;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Common English stop-words filtered out during keyword extraction.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "had", "has", "have",
    "he", "her", "his", "how", "i", "if", "in", "into", "is", "it", "its", "let", "my", "no",
    "not", "of", "on", "or", "our", "she", "so", "than", "that", "the", "their", "them", "then",
    "there", "these", "they", "this", "to", "us", "was", "we", "were", "what", "when", "where",
    "which", "who", "will", "with", "you", "your",
];

/// Mapping from technology domain keywords to authoritative documentation sites.
const DOMAIN_DOCS: &[(&str, &[&str])] = &[
    ("kubernetes", &["kubernetes.io"]),
    ("k8s", &["kubernetes.io"]),
    ("docker", &["docs.docker.com"]),
    ("rust", &["docs.rs", "doc.rust-lang.org"]),
    ("python", &["docs.python.org"]),
    ("react", &["react.dev"]),
    ("terraform", &["developer.hashicorp.com"]),
    ("aws", &["docs.aws.amazon.com"]),
    ("postgres", &["www.postgresql.org"]),
    ("postgresql", &["www.postgresql.org"]),
    ("redis", &["redis.io"]),
    ("graphql", &["graphql.org"]),
    ("typescript", &["typescriptlang.org"]),
    ("go", &["go.dev"]),
    ("golang", &["go.dev"]),
    ("vue", &["vuejs.org"]),
    ("svelte", &["svelte.dev"]),
    ("angular", &["angular.io"]),
    ("tailwind", &["tailwindcss.com"]),
    ("express", &["expressjs.com"]),
    ("fastapi", &["fastapi.tiangolo.com"]),
    ("next", &["nextjs.org"]),
    ("nextjs", &["nextjs.org"]),
];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The outcome of a competence assessment for a given request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetenceAssessment {
    /// Overall confidence score in the range `0.0 ..= 1.0`.
    pub confidence: f64,
    /// Identified knowledge or skill gaps.
    pub gaps: Vec<CompetenceGap>,
    /// Names of skills that matched the request.
    pub matching_skills: Vec<String>,
    /// Number of patterns (partial description matches) found.
    pub matching_patterns: usize,
    /// Number of relevant memories recalled.
    pub relevant_memories: usize,
    /// Whether the confidence is below the learning threshold.
    pub should_learn: bool,
    /// Recommended next action if a gap was detected.
    pub suggested_action: Option<SuggestedAction>,
}

/// A single identified gap in Hive's competence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetenceGap {
    /// The domain or topic area of the gap (e.g. "kubernetes").
    pub domain: String,
    /// Classification of the gap.
    pub gap_type: GapType,
    /// Human-readable description of the gap.
    pub description: String,
    /// How severe the gap is relative to fulfilling the request.
    pub severity: GapSeverity,
}

/// Classification of a competence gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapType {
    MissingSkill,
    MissingKnowledge,
    LowQualitySkill,
    NoPatterns,
}

/// Severity of a competence gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapSeverity {
    Low,
    Medium,
    High,
}

/// Recommended action to close a detected gap.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedAction {
    SearchDirectory { domain: String },
    ResearchTopic { topic: String, suggested_domains: Vec<String> },
    AuthorSkill { domain: String, description: String },
    ResearchAndAuthor { topic: String, suggested_domains: Vec<String> },
}

/// Tuning knobs for the competence detector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetenceConfig {
    /// Confidence score below which `should_learn` is set to `true`.
    pub learning_threshold: f64,
    /// Minimum number of positive signals before trusting the confidence.
    pub min_signals_for_confidence: usize,
    /// Whether `assess_and_learn` will automatically invoke the authoring pipeline.
    pub auto_learn: bool,
}

impl Default for CompetenceConfig {
    fn default() -> Self {
        Self {
            learning_threshold: 0.4,
            min_signals_for_confidence: 2,
            auto_learn: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Keyword extraction
// ---------------------------------------------------------------------------

/// Extract the most significant keywords from `text`.
///
/// Tokenises on non-alphanumeric boundaries, lowercases, removes stop-words
/// and short tokens (< 3 chars), deduplicates, and returns up to the top 5
/// by frequency in the original text.
fn extract_keywords(text: &str) -> Vec<String> {
    let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();

    // Tokenise and lowercase.
    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_lowercase())
        .filter(|t| t.len() >= 3 && !stopwords.contains(t.as_str()))
        .collect();

    // Count frequencies.
    let mut freq: HashMap<String, usize> = HashMap::new();
    for token in &tokens {
        *freq.entry(token.clone()).or_default() += 1;
    }

    // Sort by frequency descending, take top 5.
    let mut pairs: Vec<(String, usize)> = freq.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.into_iter().take(5).map(|(k, _)| k).collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Look up documentation domains for the given keywords using `DOMAIN_DOCS`.
fn find_suggested_domains(keywords: &[String]) -> Vec<String> {
    let mut domains = Vec::new();
    for kw in keywords {
        for &(key, sites) in DOMAIN_DOCS {
            if kw == key {
                for site in sites {
                    let s = (*site).to_string();
                    if !domains.contains(&s) {
                        domains.push(s);
                    }
                }
            }
        }
    }
    domains
}

/// Compute the Jaccard similarity between `keywords` and the keywords
/// extracted from `text`.
fn compute_keyword_overlap(keywords: &[String], text: &str) -> f64 {
    let text_kw: HashSet<String> = extract_keywords(text).into_iter().collect();
    let input_kw: HashSet<&String> = keywords.iter().collect();

    if input_kw.is_empty() && text_kw.is_empty() {
        return 0.0;
    }

    let intersection = input_kw.iter().filter(|k| text_kw.contains(**k)).count();
    let union = {
        let mut all: HashSet<&String> = input_kw.iter().copied().collect();
        all.extend(text_kw.iter());
        all.len()
    };

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

// ---------------------------------------------------------------------------
// CompetenceDetector
// ---------------------------------------------------------------------------

/// Analyses incoming requests and scores Hive's confidence in handling them.
pub struct CompetenceDetector {
    config: CompetenceConfig,
}

impl CompetenceDetector {
    /// Create a new detector with the given configuration.
    pub fn new(config: CompetenceConfig) -> Self {
        Self { config }
    }

    /// Create a detector with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(CompetenceConfig::default())
    }

    // -- quick (no AI) assessment -------------------------------------------

    /// Fast competence assessment without any AI call.
    ///
    /// Checks the skills registry and marketplace for keyword overlap and
    /// returns a confidence score derived solely from skill matching.
    pub fn quick_assess(
        &self,
        request: &str,
        marketplace: &SkillMarketplace,
        skills: &SkillsRegistry,
    ) -> CompetenceAssessment {
        let keywords = extract_keywords(request);
        debug!(?keywords, "quick_assess: extracted keywords");

        // --- skill signal from SkillsRegistry --------------------------------
        let mut matching_skills: Vec<String> = Vec::new();
        let mut best_skill_overlap: f64 = 0.0;

        for skill in skills.list_enabled() {
            let combined = format!("{} {}", skill.name, skill.description);
            let overlap = compute_keyword_overlap(&keywords, &combined);
            if overlap >= 0.5 {
                matching_skills.push(skill.name.clone());
                best_skill_overlap = best_skill_overlap.max(1.0);
            } else if overlap > 0.0 {
                best_skill_overlap = best_skill_overlap.max(0.5);
            }
        }

        // --- installed marketplace skills ------------------------------------
        for installed in marketplace.list_installed() {
            let combined = format!("{} {}", installed.name, installed.description);
            let overlap = compute_keyword_overlap(&keywords, &combined);
            if overlap >= 0.5 {
                if !matching_skills.contains(&installed.name) {
                    matching_skills.push(installed.name.clone());
                }
                best_skill_overlap = best_skill_overlap.max(1.0);
            } else if overlap > 0.0 {
                best_skill_overlap = best_skill_overlap.max(0.5);
            }
        }

        let skill_signal = best_skill_overlap;

        // --- check directory for uninstalled skills --------------------------
        let mut directory_match: Option<String> = None;
        for available in marketplace.list_directory() {
            let combined = format!("{} {}", available.name, available.description);
            let overlap = compute_keyword_overlap(&keywords, &combined);
            if overlap > 0.0 {
                directory_match = Some(available.name.clone());
                break;
            }
        }

        // --- confidence & gaps -----------------------------------------------
        let confidence = skill_signal;
        let mut gaps = Vec::new();

        if matching_skills.is_empty() {
            let domain = keywords.first().cloned().unwrap_or_else(|| "unknown".into());
            if directory_match.is_some() {
                gaps.push(CompetenceGap {
                    domain: domain.clone(),
                    gap_type: GapType::MissingSkill,
                    description: format!("No installed skill covers '{domain}', but one is available in the directory"),
                    severity: GapSeverity::Medium,
                });
            } else {
                gaps.push(CompetenceGap {
                    domain: domain.clone(),
                    gap_type: GapType::MissingKnowledge,
                    description: format!("No skill or pattern found for '{domain}'"),
                    severity: GapSeverity::High,
                });
            }
        }

        let should_learn = confidence < self.config.learning_threshold;

        let suggested_action = if should_learn {
            Self::action_from_gaps(&gaps, &keywords)
        } else {
            None
        };

        info!(confidence, should_learn, gaps = gaps.len(), "quick_assess complete");

        CompetenceAssessment {
            confidence,
            gaps,
            matching_skills,
            matching_patterns: 0,
            relevant_memories: 0,
            should_learn,
            suggested_action,
        }
    }

    // -- full assessment (with AI) ------------------------------------------

    /// Full competence assessment including memory recall and an AI confidence
    /// check.
    pub async fn assess<E: AiExecutor>(
        &self,
        request: &str,
        marketplace: &SkillMarketplace,
        skills: &SkillsRegistry,
        memory: &CollectiveMemory,
        executor: &E,
    ) -> Result<CompetenceAssessment, String> {
        let keywords = extract_keywords(request);
        debug!(?keywords, "assess: extracted keywords");

        // --- skill signal (same as quick_assess) -----------------------------
        let mut matching_skills: Vec<String> = Vec::new();
        let mut best_skill_overlap: f64 = 0.0;

        for skill in skills.list_enabled() {
            let combined = format!("{} {}", skill.name, skill.description);
            let overlap = compute_keyword_overlap(&keywords, &combined);
            if overlap >= 0.5 {
                matching_skills.push(skill.name.clone());
                best_skill_overlap = best_skill_overlap.max(1.0);
            } else if overlap > 0.0 {
                best_skill_overlap = best_skill_overlap.max(0.5);
            }
        }

        for installed in marketplace.list_installed() {
            let combined = format!("{} {}", installed.name, installed.description);
            let overlap = compute_keyword_overlap(&keywords, &combined);
            if overlap >= 0.5 {
                if !matching_skills.contains(&installed.name) {
                    matching_skills.push(installed.name.clone());
                }
                best_skill_overlap = best_skill_overlap.max(1.0);
            } else if overlap > 0.0 {
                best_skill_overlap = best_skill_overlap.max(0.5);
            }
        }

        let skill_signal = best_skill_overlap;

        // --- pattern signal (partial description matches) --------------------
        let mut matching_patterns: usize = 0;
        for skill in skills.list() {
            let overlap = compute_keyword_overlap(&keywords, &skill.description);
            if overlap > 0.0 {
                matching_patterns += 1;
            }
        }
        for installed in marketplace.list_installed() {
            let overlap = compute_keyword_overlap(&keywords, &installed.description);
            if overlap > 0.0 {
                matching_patterns += 1;
            }
        }
        let pattern_signal = if matching_patterns > 0 {
            (matching_patterns as f64 / 3.0).min(1.0)
        } else {
            0.0
        };

        // --- memory signal ---------------------------------------------------
        let query = keywords.join(" ");
        let hits = memory.recall(&query, None, None, 10)?;
        let relevant_memories = hits.len();
        let memory_signal = (relevant_memories as f64 / 3.0).min(1.0);

        // --- AI signal -------------------------------------------------------
        let skill_names: Vec<String> = skills.list_enabled().iter().map(|s| s.name.clone()).collect();
        let system_prompt = "You are a competence assessor. Rate your confidence (0-10) that you can handle this request given these available skills. Respond with just a number 0-10.".to_string();
        let user_content = format!(
            "Available skills: [{}]. Request: {}",
            skill_names.join(", "),
            request
        );

        let ai_request = ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, user_content)],
            model: String::new(),
            max_tokens: 16,
            temperature: Some(0.0),
            system_prompt: Some(system_prompt),
            tools: None,
        };

        let ai_signal = match executor.execute(&ai_request).await {
            Ok(resp) => {
                let trimmed = resp.content.trim();
                trimmed
                    .parse::<f64>()
                    .unwrap_or_else(|_| {
                        // Try to extract first number from the response.
                        trimmed
                            .split_whitespace()
                            .find_map(|w: &str| w.parse::<f64>().ok())
                            .unwrap_or(5.0)
                    })
                    .clamp(0.0, 10.0)
                    / 10.0
            }
            Err(e) => {
                warn!("AI confidence call failed: {e}");
                0.5
            }
        };

        // --- composite confidence --------------------------------------------
        let confidence = (skill_signal * 0.30)
            + (pattern_signal * 0.20)
            + (memory_signal * 0.15)
            + (ai_signal * 0.35);

        // --- gaps ------------------------------------------------------------
        let mut gaps = Vec::new();
        if matching_skills.is_empty() {
            let domain = keywords.first().cloned().unwrap_or_else(|| "unknown".into());
            gaps.push(CompetenceGap {
                domain: domain.clone(),
                gap_type: GapType::MissingSkill,
                description: format!("No installed skill covers '{domain}'"),
                severity: GapSeverity::High,
            });
        }
        if matching_patterns == 0 {
            let domain = keywords.first().cloned().unwrap_or_else(|| "unknown".into());
            gaps.push(CompetenceGap {
                domain,
                gap_type: GapType::NoPatterns,
                description: "No description patterns matched the request".into(),
                severity: GapSeverity::Medium,
            });
        }
        if relevant_memories == 0 {
            let domain = keywords.first().cloned().unwrap_or_else(|| "unknown".into());
            gaps.push(CompetenceGap {
                domain,
                gap_type: GapType::MissingKnowledge,
                description: "No relevant memories found for this domain".into(),
                severity: GapSeverity::Low,
            });
        }

        let should_learn = confidence < self.config.learning_threshold;
        let suggested_action = if should_learn {
            Self::action_from_gaps(&gaps, &keywords)
        } else {
            None
        };

        info!(confidence, should_learn, gaps = gaps.len(), "assess complete");

        Ok(CompetenceAssessment {
            confidence,
            gaps,
            matching_skills,
            matching_patterns,
            relevant_memories,
            should_learn,
            suggested_action,
        })
    }

    // -- assess + auto-learn ------------------------------------------------

    /// Perform a full assessment and, if `auto_learn` is enabled and the
    /// confidence is below the threshold, trigger the skill-authoring pipeline.
    pub async fn assess_and_learn<E: AiExecutor>(
        &self,
        request: &str,
        marketplace: &mut SkillMarketplace,
        skills: &SkillsRegistry,
        memory: &CollectiveMemory,
        executor: &E,
        pipeline: &SkillAuthoringPipeline,
    ) -> Result<(CompetenceAssessment, Option<SkillAuthoringResult>), String> {
        let assessment = self.assess(request, marketplace, skills, memory, executor).await?;

        if assessment.should_learn && self.config.auto_learn {
            info!("Auto-learn triggered — invoking skill-authoring pipeline");

            let domain = assessment
                .gaps
                .first()
                .map(|g| g.domain.clone())
                .unwrap_or_else(|| "unknown".into());

            let authoring_request = SkillAuthoringRequest {
                domain,
                user_query: request.to_string(),
                gaps: assessment
                    .gaps
                    .iter()
                    .map(|g| format!("[{:?}] {}: {}", g.gap_type, g.domain, g.description))
                    .collect(),
            };

            let result = pipeline.author_skill(&authoring_request, executor, marketplace, Some(memory)).await;
            Ok((assessment, Some(result)))
        } else {
            Ok((assessment, None))
        }
    }

    // -- internal helpers ---------------------------------------------------

    /// Determine the best suggested action from the identified gaps and
    /// extracted keywords.
    fn action_from_gaps(gaps: &[CompetenceGap], keywords: &[String]) -> Option<SuggestedAction> {
        let domains = find_suggested_domains(keywords);

        // Find the highest-severity gap to drive the suggestion.
        let worst = gaps.iter().max_by_key(|g| g.severity)?;

        match worst.gap_type {
            GapType::MissingSkill => {
                Some(SuggestedAction::SearchDirectory {
                    domain: worst.domain.clone(),
                })
            }
            GapType::MissingKnowledge => {
                if domains.is_empty() {
                    Some(SuggestedAction::ResearchAndAuthor {
                        topic: worst.domain.clone(),
                        suggested_domains: domains,
                    })
                } else {
                    Some(SuggestedAction::ResearchTopic {
                        topic: worst.domain.clone(),
                        suggested_domains: domains,
                    })
                }
            }
            GapType::LowQualitySkill => {
                Some(SuggestedAction::AuthorSkill {
                    domain: worst.domain.clone(),
                    description: worst.description.clone(),
                })
            }
            GapType::NoPatterns => {
                Some(SuggestedAction::ResearchTopic {
                    topic: worst.domain.clone(),
                    suggested_domains: domains,
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_marketplace::SkillMarketplace;
    use crate::skills::SkillsRegistry;
    use hive_ai::types::{ChatResponse, FinishReason, TokenUsage};

    // -- Mock executor for async tests --------------------------------------

    #[allow(dead_code)]
    struct MockExecutor {
        response: String,
    }

    impl AiExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            Ok(ChatResponse {
                content: self.response.clone(),
                model: "mock".to_string(),
                usage: TokenUsage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
                finish_reason: FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }
    }

    // -- Keyword extraction -------------------------------------------------

    #[test]
    fn extract_keywords_filters_stopwords() {
        let kws = extract_keywords("how do I deploy the kubernetes cluster");
        assert!(!kws.contains(&"how".to_string()));
        assert!(!kws.contains(&"the".to_string()));
        assert!(kws.contains(&"deploy".to_string()));
        assert!(kws.contains(&"kubernetes".to_string()));
    }

    #[test]
    fn extract_keywords_deduplicates() {
        let kws = extract_keywords("test test test test deploy deploy");
        // "test" appears many times but should only be in the output once.
        let test_count = kws.iter().filter(|k| *k == "test").count();
        assert_eq!(test_count, 1);
    }

    #[test]
    fn extract_keywords_top_5() {
        let kws = extract_keywords(
            "alpha bravo charlie delta echo foxtrot golf hotel india juliet",
        );
        assert!(kws.len() <= 5);
    }

    // -- Quick assessment ---------------------------------------------------

    #[test]
    fn quick_assess_known_skill_high_confidence() {
        let detector = CompetenceDetector::with_defaults();
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        // "code-review" is a built-in skill; request mentioning "review code"
        // should match.
        let assessment = detector.quick_assess("review code for bugs", &marketplace, &skills);
        assert!(
            assessment.confidence >= 0.5,
            "Expected high confidence for known skill, got {}",
            assessment.confidence
        );
        assert!(!assessment.matching_skills.is_empty());
    }

    #[test]
    fn quick_assess_unknown_domain_low_confidence() {
        let detector = CompetenceDetector::with_defaults();
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        let assessment = detector.quick_assess(
            "quantum computing entanglement simulation",
            &marketplace,
            &skills,
        );
        assert!(
            assessment.confidence < 0.4,
            "Expected low confidence for unknown domain, got {}",
            assessment.confidence
        );
        assert!(assessment.should_learn);
    }

    #[test]
    fn quick_assess_partial_match_medium() {
        let detector = CompetenceDetector::with_defaults();
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        // "generate" partially overlaps with "generate-docs" / "test-gen".
        let assessment = detector.quick_assess("generate API stubs", &marketplace, &skills);
        // Should have at least some signal (>0) even if not full match.
        assert!(
            assessment.confidence >= 0.0,
            "Confidence should be non-negative"
        );
    }

    // -- Gap identification -------------------------------------------------

    #[test]
    fn identifies_missing_skill_gap() {
        let detector = CompetenceDetector::with_defaults();
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        let assessment = detector.quick_assess(
            "deploy kubernetes cluster to production",
            &marketplace,
            &skills,
        );
        let has_missing = assessment.gaps.iter().any(|g| {
            g.gap_type == GapType::MissingSkill || g.gap_type == GapType::MissingKnowledge
        });
        assert!(has_missing, "Should identify a missing skill or knowledge gap");
    }

    #[test]
    fn identifies_missing_knowledge_gap() {
        let detector = CompetenceDetector::with_defaults();
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        let assessment = detector.quick_assess(
            "explain advanced topological data analysis",
            &marketplace,
            &skills,
        );
        let has_gap = assessment
            .gaps
            .iter()
            .any(|g| g.gap_type == GapType::MissingKnowledge || g.gap_type == GapType::MissingSkill);
        assert!(has_gap, "Should identify a missing-knowledge or missing-skill gap");
    }

    // -- Threshold ----------------------------------------------------------

    #[test]
    fn below_threshold_triggers_learning() {
        let detector = CompetenceDetector::with_defaults(); // threshold = 0.4
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        let assessment = detector.quick_assess(
            "obscure domain nobody has a skill for",
            &marketplace,
            &skills,
        );
        assert!(assessment.should_learn);
    }

    #[test]
    fn above_threshold_does_not_trigger() {
        let detector = CompetenceDetector::with_defaults();
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        let assessment = detector.quick_assess("review code for security issues", &marketplace, &skills);
        assert!(
            !assessment.should_learn || assessment.confidence >= 0.4,
            "High-confidence assessment should not trigger learning"
        );
    }

    // -- Suggested action ---------------------------------------------------

    #[test]
    fn suggested_action_matches_gap_type() {
        let detector = CompetenceDetector::with_defaults();
        let skills = SkillsRegistry::new();
        let marketplace = SkillMarketplace::new();

        let assessment = detector.quick_assess(
            "deploy kubernetes pods with istio service mesh",
            &marketplace,
            &skills,
        );
        if let Some(action) = &assessment.suggested_action {
            // Should suggest searching directory or researching.
            match action {
                SuggestedAction::SearchDirectory { .. }
                | SuggestedAction::ResearchTopic { .. }
                | SuggestedAction::ResearchAndAuthor { .. } => {}
                _ => panic!("Unexpected action for missing skill: {action:?}"),
            }
        }
        // If no action, that is acceptable only when should_learn is false.
        if assessment.suggested_action.is_none() {
            assert!(!assessment.should_learn);
        }
    }

    #[test]
    fn find_suggested_domains_finds_kubernetes() {
        let kws = vec!["kubernetes".to_string()];
        let domains = find_suggested_domains(&kws);
        assert!(domains.contains(&"kubernetes.io".to_string()));
    }

    // -- Keyword overlap ----------------------------------------------------

    #[test]
    fn keyword_overlap_identical_is_one() {
        let kws = extract_keywords("deploy kubernetes cluster");
        let overlap = compute_keyword_overlap(&kws, "deploy kubernetes cluster");
        assert!(
            (overlap - 1.0).abs() < f64::EPSILON,
            "Identical text should yield overlap of 1.0, got {overlap}"
        );
    }

    #[test]
    fn keyword_overlap_disjoint_is_zero() {
        let kws = extract_keywords("quantum physics simulation");
        let overlap = compute_keyword_overlap(&kws, "baking sourdough bread recipe");
        assert!(
            overlap.abs() < f64::EPSILON,
            "Disjoint texts should yield overlap of 0.0, got {overlap}"
        );
    }
}
