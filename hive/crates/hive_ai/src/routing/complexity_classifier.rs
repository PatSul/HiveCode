//! Complexity Classifier
//!
//! Analyzes user requests to determine the optimal model tier using a
//! 12-factor scoring system. Based on RouteLLM research for intelligent
//! model routing.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::types::{ChatMessage, MessageRole, ModelTier};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The detected type of task the user is requesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    SimpleQuestion,
    CodeExplanation,
    CodeGen,
    BugFix,
    Refactoring,
    Architecture,
    Security,
    Documentation,
    Testing,
    Debugging,
    Research,
    CreativeWriting,
    General,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::SimpleQuestion => "simple question",
            Self::CodeExplanation => "code explanation",
            Self::CodeGen => "code generation",
            Self::BugFix => "bug fix",
            Self::Refactoring => "refactoring",
            Self::Architecture => "architecture",
            Self::Security => "security review",
            Self::Documentation => "documentation",
            Self::Testing => "testing",
            Self::Debugging => "debugging",
            Self::Research => "research",
            Self::CreativeWriting => "creative writing",
            Self::General => "general",
        };
        f.write_str(s)
    }
}

/// How much reasoning the task requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningDepth {
    Shallow,
    Moderate,
    Deep,
}

/// How specialized the domain knowledge required is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DomainSpecificity {
    General,
    Specialized,
    Expert,
}

/// The individual factors that contribute to the complexity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityFactors {
    pub token_count: u32,
    pub context_size: u32,
    pub file_count: u32,
    pub has_errors: bool,
    pub reasoning_depth: ReasoningDepth,
    pub domain_specificity: DomainSpecificity,
    pub task_type: TaskType,
    pub code_complexity: f32,
}

/// The result of classifying request complexity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityResult {
    pub tier: ModelTier,
    pub score: f32,
    pub factors: ComplexityFactors,
    pub recommended_model: Option<String>,
    pub reasoning: String,
}

/// Optional context provided alongside messages to improve classification.
#[derive(Debug, Clone, Default)]
pub struct ClassificationContext {
    pub context_size: Option<u32>,
    pub file_count: Option<u32>,
}

// ---------------------------------------------------------------------------
// Compiled regex patterns (Lazy statics)
// ---------------------------------------------------------------------------

/// Task-type detection patterns. Each entry is (TaskType, Vec<Regex>).
static TASK_PATTERNS: Lazy<Vec<(TaskType, Vec<Regex>)>> = Lazy::new(|| {
    // ORDER MATTERS: more-specific / higher-priority task types must come
    // first so they match before generic patterns like "code for" or "error".
    vec![
        // --- High-priority types (always-Premium hard rules depend on these) ---
        (
            TaskType::Security,
            compile_patterns(&[
                r"(?i)security|vulnerability|exploit|injection|xss|csrf",
                r"(?i)is this secure|audit|penetration",
            ]),
        ),
        (
            TaskType::Architecture,
            compile_patterns(&[
                r"(?i)architect|design|structure|pattern|system design",
                r"(?i)how should i organize|best approach for",
            ]),
        ),
        (
            TaskType::Debugging,
            compile_patterns(&[
                r"(?i)debug|trace|investigate|diagnose|why is",
                r"(?i)stack trace|exception|crash",
            ]),
        ),
        // --- Medium-priority types ---
        (
            TaskType::SimpleQuestion,
            compile_patterns(&[
                r"(?i)what is|what's|how do i|can you explain|tell me about",
                r"^\s*\?",
                r"(?i)quick question",
            ]),
        ),
        (
            TaskType::CodeExplanation,
            compile_patterns(&[
                r"(?i)explain this code|what does this do|how does this work",
                r"(?i)walk me through|break down",
            ]),
        ),
        (
            TaskType::BugFix,
            compile_patterns(&[
                r"(?i)fix|bug|error|issue|problem|broken|not working",
                r"(?i)doesn't work|won't compile|fails",
            ]),
        ),
        (
            TaskType::Refactoring,
            compile_patterns(&[
                r"(?i)refactor|improve|optimize|clean up|simplify",
                r"(?i)make.*better|more efficient|readable",
            ]),
        ),
        (
            TaskType::CodeGen,
            compile_patterns(&[
                r"(?i)write|create|generate|implement|build|make",
                r"(?i)code for|function that|class that",
            ]),
        ),
        (
            TaskType::Documentation,
            compile_patterns(&[
                r"(?i)document|readme|jsdoc|docstring|comment",
                r"(?i)write docs|api documentation",
            ]),
        ),
        (
            TaskType::Testing,
            compile_patterns(&[
                r"(?i)test|spec|jest|mocha|pytest|unittest",
                r"(?i)write tests|test cases|coverage",
            ]),
        ),
        (
            TaskType::Research,
            compile_patterns(&[
                r"(?i)research|compare|evaluate|alternatives|options",
                r"(?i)pros and cons|tradeoffs|which.*should",
            ]),
        ),
        (
            TaskType::CreativeWriting,
            compile_patterns(&[
                r"(?i)creative|brainstorm|ideas|suggest|innovative",
                r"(?i)come up with|think of",
            ]),
        ),
    ]
});

/// High complexity indicator patterns.
static HIGH_COMPLEXITY: Lazy<Vec<Regex>> = Lazy::new(|| {
    compile_patterns(&[
        r"(?i)multi-?step|complex|sophisticated|advanced",
        r"(?i)entire|whole|complete|full",
        r"(?i)from scratch|ground up",
        r"(?i)production|enterprise|scale",
        r"(?i)security|authentication|authorization",
        r"(?i)distributed|microservice|concurrent",
        r"(?i)optimize|performance|efficient",
        r"(?i)architecture|design pattern|system",
    ])
});

/// Medium complexity indicator patterns.
static MEDIUM_COMPLEXITY: Lazy<Vec<Regex>> = Lazy::new(|| {
    compile_patterns(&[
        r"(?i)update|modify|change|add|extend",
        r"(?i)integrate|connect|combine",
        r"(?i)handle|manage|process",
        r"(?i)validate|check|verify",
    ])
});

/// Deep reasoning indicator patterns.
static DEEP_REASONING_INDICATORS: Lazy<Vec<Regex>> = Lazy::new(|| {
    compile_patterns(&[
        r"(?i)why",
        r"(?i)how does",
        r"(?i)explain the reasoning",
        r"(?i)step by step",
        r"(?i)trade-?offs",
    ])
});

/// Expert domain patterns.
static EXPERT_DOMAINS: Lazy<Vec<Regex>> = Lazy::new(|| {
    compile_patterns(&[
        r"(?i)cryptograph|blockchain|quantum",
        r"(?i)machine learning|neural network|deep learning",
        r"(?i)compiler|parser|ast|lexer",
        r"(?i)kernel|driver|embedded",
        r"(?i)distributed system|consensus|raft|paxos",
    ])
});

/// Specialized domain patterns.
static SPECIALIZED_DOMAINS: Lazy<Vec<Regex>> = Lazy::new(|| {
    compile_patterns(&[
        r"(?i)react|vue|angular|svelte",
        r"(?i)kubernetes|docker|terraform",
        r"(?i)postgresql|mongodb|redis",
        r"(?i)graphql|rest api|websocket",
        r"(?i)typescript|rust|go|python",
    ])
});

/// Error detection patterns.
static ERROR_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    compile_patterns(&[
        r"(?i)error:|exception:|failed:|crash",
        r"(?i)stack trace|traceback",
        r"(?i)undefined is not|cannot read property",
        r"(?i)typeerror|referenceerror|syntaxerror",
        r"(?i)\bat line \d+",
        r"(?i)exit code [1-9]",
    ])
});

/// Code-complexity pattern indicators.
static CODE_COMPLEXITY_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    compile_patterns(&[
        r"(?i)async|await|promise",
        r"(?i)class\s+\w+.*extends",
        r"(?i)interface|type\s+\w+\s*=",
        r"(?i)useEffect|useState|useCallback",
        r"(?i)try\s*\{[\s\S]*catch",
        r"(?i)for\s*\(.*\)[\s\S]*for\s*\(",
    ])
});

/// Regex for extracting code blocks.
static CODE_BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"```[\s\S]*?```").expect("code block regex")
});

/// Default model recommendations per tier.
static BUDGET_MODELS: &[&str] = &[
    "deepseek/deepseek-chat",
    "qwen/qwen-2.5-72b-instruct",
    "meta-llama/llama-3.3-70b-instruct",
];
static MID_MODELS: &[&str] = &[
    "claude-sonnet-4-20250514",
    "gpt-4o-mini",
    "gemini-1.5-flash",
];
static PREMIUM_MODELS: &[&str] = &[
    "claude-opus-4-20250514",
    "gpt-4o",
    "o1",
    "gemini-1.5-pro",
];

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn compile_patterns(patterns: &[&str]) -> Vec<Regex> {
    patterns
        .iter()
        .map(|p| Regex::new(p).unwrap_or_else(|e| panic!("Bad regex pattern `{p}`: {e}")))
        .collect()
}

fn any_match(patterns: &[Regex], text: &str) -> bool {
    patterns.iter().any(|re| re.is_match(text))
}

// ---------------------------------------------------------------------------
// ComplexityClassifier
// ---------------------------------------------------------------------------

/// Analyzes user requests using a 12-factor scoring system to determine the
/// optimal [`ModelTier`] for handling the request.
pub struct ComplexityClassifier {
    // All regex state is in Lazy statics, so this struct is zero-sized for now.
    // Keeping it as a struct allows future configuration (custom model lists, etc.).
    _private: (),
}

impl Default for ComplexityClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ComplexityClassifier {
    /// Create a new classifier. All regex patterns are compiled once (lazily)
    /// and shared across instances.
    pub fn new() -> Self {
        // Force-initialize the lazy statics on first construction so any regex
        // compilation errors surface early.
        let _ = &*TASK_PATTERNS;
        let _ = &*HIGH_COMPLEXITY;
        let _ = &*MEDIUM_COMPLEXITY;
        let _ = &*DEEP_REASONING_INDICATORS;
        let _ = &*EXPERT_DOMAINS;
        let _ = &*SPECIALIZED_DOMAINS;
        let _ = &*ERROR_PATTERNS;
        let _ = &*CODE_COMPLEXITY_PATTERNS;
        let _ = &*CODE_BLOCK_RE;

        Self { _private: () }
    }

    /// Classify the complexity of a conversation and return a [`ComplexityResult`].
    pub fn classify(
        &self,
        messages: &[ChatMessage],
        context: Option<&ClassificationContext>,
    ) -> ComplexityResult {
        let user_message = self.latest_user_message(messages);
        let factors = self.analyze_factors(messages, &user_message, context);
        let score = self.calculate_score(&factors);
        let tier = self.determine_tier(score, &factors);
        let recommended_model = self.recommended_model(tier);
        let reasoning = self.generate_reasoning(&factors, tier);

        ComplexityResult {
            tier,
            score,
            factors,
            recommended_model,
            reasoning,
        }
    }

    // ------------------------------------------------------------------
    // Factor analysis
    // ------------------------------------------------------------------

    fn analyze_factors(
        &self,
        messages: &[ChatMessage],
        user_message: &str,
        context: Option<&ClassificationContext>,
    ) -> ComplexityFactors {
        let token_count = self.estimate_tokens(messages);
        let task_type = self.detect_task_type(user_message);
        let reasoning_depth = self.assess_reasoning_depth(user_message, task_type);
        let domain_specificity = self.assess_domain_specificity(user_message);
        let code_complexity = self.assess_code_complexity(user_message, context);
        let file_count = context.and_then(|c| c.file_count).unwrap_or(0);

        ComplexityFactors {
            token_count,
            context_size: context
                .and_then(|c| c.context_size)
                .unwrap_or(token_count),
            file_count,
            has_errors: self.detect_errors(user_message),
            reasoning_depth,
            domain_specificity,
            task_type,
            code_complexity,
        }
    }

    // ------------------------------------------------------------------
    // 12-factor score calculation (0.0 - 1.0)
    // ------------------------------------------------------------------

    fn calculate_score(&self, factors: &ComplexityFactors) -> f32 {
        let mut score: f32 = 0.0;

        // 1. Token count contribution (0 - 0.20)
        score += if factors.token_count > 50_000 {
            0.20
        } else if factors.token_count > 10_000 {
            0.15
        } else if factors.token_count > 2_000 {
            0.10
        } else {
            0.05
        };

        // 2. File count contribution (0 - 0.15)
        score += if factors.file_count > 10 {
            0.15
        } else if factors.file_count > 5 {
            0.10
        } else if factors.file_count > 1 {
            0.05
        } else {
            0.0
        };

        // 3. Error presence (0 - 0.15)
        if factors.has_errors {
            score += 0.15;
        }

        // 4. Reasoning depth (0 - 0.20)
        score += match factors.reasoning_depth {
            ReasoningDepth::Deep => 0.20,
            ReasoningDepth::Moderate => 0.10,
            ReasoningDepth::Shallow => 0.0,
        };

        // 5. Domain specificity (0 - 0.15)
        score += match factors.domain_specificity {
            DomainSpecificity::Expert => 0.15,
            DomainSpecificity::Specialized => 0.08,
            DomainSpecificity::General => 0.0,
        };

        // 6. Task type contribution (0 - 0.15)
        score += match factors.task_type {
            TaskType::Architecture | TaskType::Security | TaskType::Debugging => 0.15,
            TaskType::CodeGen | TaskType::BugFix | TaskType::Refactoring | TaskType::Testing => {
                0.08
            }
            _ => 0.0,
        };

        // 7. Code complexity (0 - 0.15)
        score += factors.code_complexity * 0.15;

        score.min(1.0)
    }

    // ------------------------------------------------------------------
    // Tier determination with hard rules
    // ------------------------------------------------------------------

    fn determine_tier(&self, score: f32, factors: &ComplexityFactors) -> ModelTier {
        // Hard rule: Architecture & Security tasks -> always Premium
        if matches!(
            factors.task_type,
            TaskType::Architecture | TaskType::Security
        ) {
            return ModelTier::Premium;
        }

        // Hard rule: Debugging with errors -> always Premium
        if factors.has_errors && factors.task_type == TaskType::Debugging {
            return ModelTier::Premium;
        }

        // Hard rule: Simple documentation/general under 500 tokens -> always Budget
        if matches!(
            factors.task_type,
            TaskType::Documentation | TaskType::General | TaskType::SimpleQuestion
        ) && factors.token_count < 500
        {
            return ModelTier::Budget;
        }

        // Score-based tier
        if score >= 0.65 {
            ModelTier::Premium
        } else if score >= 0.35 {
            ModelTier::Mid
        } else {
            ModelTier::Budget
        }
    }

    // ------------------------------------------------------------------
    // Task type detection
    // ------------------------------------------------------------------

    fn detect_task_type(&self, text: &str) -> TaskType {
        for (task_type, patterns) in TASK_PATTERNS.iter() {
            if any_match(patterns, text) {
                return *task_type;
            }
        }
        TaskType::General
    }

    // ------------------------------------------------------------------
    // Reasoning depth assessment
    // ------------------------------------------------------------------

    fn assess_reasoning_depth(&self, text: &str, task_type: TaskType) -> ReasoningDepth {
        // Tasks that inherently require deep reasoning
        if matches!(
            task_type,
            TaskType::Architecture | TaskType::Debugging | TaskType::Research
        ) {
            return ReasoningDepth::Deep;
        }

        // Check for deep reasoning keyword indicators
        if any_match(&DEEP_REASONING_INDICATORS, text) {
            return ReasoningDepth::Deep;
        }

        // High complexity indicators
        if any_match(&HIGH_COMPLEXITY, text) {
            return ReasoningDepth::Deep;
        }

        // Medium complexity indicators
        if any_match(&MEDIUM_COMPLEXITY, text) {
            return ReasoningDepth::Moderate;
        }

        ReasoningDepth::Shallow
    }

    // ------------------------------------------------------------------
    // Domain specificity assessment
    // ------------------------------------------------------------------

    fn assess_domain_specificity(&self, text: &str) -> DomainSpecificity {
        if any_match(&EXPERT_DOMAINS, text) {
            return DomainSpecificity::Expert;
        }
        if any_match(&SPECIALIZED_DOMAINS, text) {
            return DomainSpecificity::Specialized;
        }
        DomainSpecificity::General
    }

    // ------------------------------------------------------------------
    // Code complexity assessment (0.0 - 1.0)
    // ------------------------------------------------------------------

    fn assess_code_complexity(
        &self,
        message: &str,
        context: Option<&ClassificationContext>,
    ) -> f32 {
        let mut complexity: f32 = 0.0;

        // Check for code blocks and their total length
        let total_code_len: usize = CODE_BLOCK_RE
            .find_iter(message)
            .map(|m| m.as_str().len())
            .sum();
        if total_code_len > 0 {
            complexity += (total_code_len as f32 / 3000.0).min(0.3);
        }

        // Context file count contribution
        if let Some(ctx) = context {
            if let Some(fc) = ctx.file_count {
                complexity += (fc as f32 / 20.0).min(0.3);
            }
        }

        // Complex code patterns
        for re in CODE_COMPLEXITY_PATTERNS.iter() {
            if re.is_match(message) {
                complexity += 0.05;
            }
        }

        complexity.min(1.0)
    }

    // ------------------------------------------------------------------
    // Error detection
    // ------------------------------------------------------------------

    fn detect_errors(&self, message: &str) -> bool {
        any_match(&ERROR_PATTERNS, message)
    }

    // ------------------------------------------------------------------
    // Token estimation
    // ------------------------------------------------------------------

    fn estimate_tokens(&self, messages: &[ChatMessage]) -> u32 {
        let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
        // Rough estimate: ~4 characters per token
        (total_chars / 4).max(1) as u32
    }

    // ------------------------------------------------------------------
    // Latest user message extraction
    // ------------------------------------------------------------------

    fn latest_user_message(&self, messages: &[ChatMessage]) -> String {
        messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------
    // Model recommendation
    // ------------------------------------------------------------------

    fn recommended_model(&self, tier: ModelTier) -> Option<String> {
        let models = match tier {
            ModelTier::Premium => PREMIUM_MODELS,
            ModelTier::Mid => MID_MODELS,
            ModelTier::Budget | ModelTier::Free => BUDGET_MODELS,
        };
        models.first().map(|s| (*s).to_owned())
    }

    // ------------------------------------------------------------------
    // Human-readable reasoning
    // ------------------------------------------------------------------

    fn generate_reasoning(&self, factors: &ComplexityFactors, tier: ModelTier) -> String {
        let mut reasons: Vec<String> = Vec::new();

        reasons.push(format!("Task type: {}", factors.task_type));

        if factors.has_errors {
            reasons.push("Contains error/debugging context".into());
        }

        if factors.reasoning_depth == ReasoningDepth::Deep {
            reasons.push("Requires deep reasoning".into());
        }

        if factors.domain_specificity != DomainSpecificity::General {
            reasons.push(format!(
                "{:?} domain knowledge needed",
                factors.domain_specificity
            ));
        }

        if factors.file_count > 5 {
            reasons.push(format!("Multi-file context ({} files)", factors.file_count));
        }

        if factors.code_complexity > 0.5 {
            reasons.push("Complex code patterns detected".into());
        }

        let tier_label = match tier {
            ModelTier::Premium => "PREMIUM",
            ModelTier::Mid => "MID",
            ModelTier::Budget => "BUDGET",
            ModelTier::Free => "FREE",
        };

        format!("{} tier: {}", tier_label, reasons.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn user_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: Utc::now(),
        }
    }

    fn msgs(content: &str) -> Vec<ChatMessage> {
        vec![user_msg(content)]
    }

    #[test]
    fn architecture_always_premium() {
        let c = ComplexityClassifier::new();
        let result = c.classify(&msgs("Design the system architecture for a microservice"), None);
        assert_eq!(result.tier, ModelTier::Premium);
    }

    #[test]
    fn security_always_premium() {
        let c = ComplexityClassifier::new();
        let result = c.classify(&msgs("Review this code for security vulnerabilities"), None);
        assert_eq!(result.tier, ModelTier::Premium);
    }

    #[test]
    fn debugging_with_errors_premium() {
        let c = ComplexityClassifier::new();
        let result = c.classify(
            &msgs("Debug this crash: TypeError: cannot read property 'foo'"),
            None,
        );
        assert_eq!(result.tier, ModelTier::Premium);
    }

    #[test]
    fn simple_question_budget() {
        let c = ComplexityClassifier::new();
        let result = c.classify(&msgs("What is Rust?"), None);
        assert_eq!(result.tier, ModelTier::Budget);
    }

    #[test]
    fn score_bounded_zero_to_one() {
        let c = ComplexityClassifier::new();
        let result = c.classify(
            &msgs(
                "Design a distributed blockchain system with authentication \
                 from scratch for production enterprise scale with full \
                 security and concurrent microservice architecture",
            ),
            Some(&ClassificationContext {
                file_count: Some(50),
                context_size: Some(100_000),
            }),
        );
        assert!(result.score >= 0.0 && result.score <= 1.0);
        assert_eq!(result.tier, ModelTier::Premium);
    }

    #[test]
    fn expert_domain_detected() {
        let c = ComplexityClassifier::new();
        let result = c.classify(&msgs("Implement a neural network training loop"), None);
        assert_eq!(
            result.factors.domain_specificity,
            DomainSpecificity::Expert
        );
    }

    #[test]
    fn specialized_domain_detected() {
        let c = ComplexityClassifier::new();
        let result = c.classify(&msgs("Add a React component"), None);
        assert_eq!(
            result.factors.domain_specificity,
            DomainSpecificity::Specialized
        );
    }

    #[test]
    fn code_blocks_increase_complexity() {
        let c = ComplexityClassifier::new();
        let short = c.classify(&msgs("Fix this"), None);
        let with_code = c.classify(
            &msgs("Fix this:\n```rust\nfn main() { let x = 1; }\n```"),
            None,
        );
        assert!(with_code.factors.code_complexity > short.factors.code_complexity);
    }

    #[test]
    fn reasoning_includes_tier_label() {
        let c = ComplexityClassifier::new();
        let result = c.classify(&msgs("What is Rust?"), None);
        assert!(result.reasoning.contains("BUDGET") || result.reasoning.contains("MID"));
    }
}
