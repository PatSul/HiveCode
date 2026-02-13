//! Specialist Agent Personas — built-in and custom persona definitions.
//!
//! Six built-in personas (Investigate, Implement, Verify, Critique, Debug,
//! CodeReview) plus support for user-defined custom personas. Each persona
//! carries a system prompt, model tier preference, tool list, and token limit.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use hive_ai::types::{ChatMessage, ChatRequest, MessageRole, ModelTier, TokenUsage};

use crate::hivemind::{AgentOutput, AgentRole, AiExecutor, default_model_for_tier};

// ---------------------------------------------------------------------------
// Persona Kind
// ---------------------------------------------------------------------------

/// The kind of specialist persona. Built-in kinds cover common development
/// workflows; `Custom` allows arbitrary user-defined personas.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaKind {
    Investigate,
    Implement,
    Verify,
    Critique,
    Debug,
    CodeReview,
    Custom(String),
}

impl PersonaKind {
    /// All built-in persona kinds (excludes Custom).
    pub const BUILT_IN: [PersonaKind; 6] = [
        Self::Investigate,
        Self::Implement,
        Self::Verify,
        Self::Critique,
        Self::Debug,
        Self::CodeReview,
    ];
}

impl std::fmt::Display for PersonaKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Investigate => write!(f, "Investigate"),
            Self::Implement => write!(f, "Implement"),
            Self::Verify => write!(f, "Verify"),
            Self::Critique => write!(f, "Critique"),
            Self::Debug => write!(f, "Debug"),
            Self::CodeReview => write!(f, "Code Review"),
            Self::Custom(name) => write!(f, "Custom({name})"),
        }
    }
}

// ---------------------------------------------------------------------------
// Persona
// ---------------------------------------------------------------------------

/// A specialist agent persona with a system prompt, model preference,
/// available tools, and token limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub kind: PersonaKind,
    pub name: String,
    pub system_prompt: String,
    pub model_tier: ModelTier,
    pub description: String,
    pub tools: Vec<String>,
    pub max_tokens: u32,
}

// ---------------------------------------------------------------------------
// Built-in persona constructors
// ---------------------------------------------------------------------------

fn investigate_persona() -> Persona {
    Persona {
        kind: PersonaKind::Investigate,
        name: "Investigator".into(),
        system_prompt: "You are an expert code investigator. Your role is to perform deep \
            codebase analysis: trace dependencies, understand architecture, map call graphs, \
            and identify how components interact. Read broadly before drawing conclusions. \
            Present findings as structured analysis with references to specific files and \
            line numbers. Never guess — trace every claim to source."
            .into(),
        model_tier: ModelTier::Premium,
        description: "Deep codebase analysis, dependency tracing, architecture understanding"
            .into(),
        tools: vec![
            "read_file".into(),
            "search_symbol".into(),
            "find_references".into(),
            "list_directory".into(),
        ],
        max_tokens: 8192,
    }
}

fn implement_persona() -> Persona {
    Persona {
        kind: PersonaKind::Implement,
        name: "Implementer".into(),
        system_prompt: "You are an expert software engineer. Write clean, efficient, \
            well-tested code that follows the project's established conventions. Handle \
            edge cases explicitly. Prefer simple, readable solutions over clever ones. \
            Include proper error handling, documentation, and type annotations. Every \
            change must compile and pass existing tests."
            .into(),
        model_tier: ModelTier::Mid,
        description: "Writes code following project conventions, handles edge cases".into(),
        tools: vec![
            "read_file".into(),
            "write_file".into(),
            "run_command".into(),
            "search_symbol".into(),
        ],
        max_tokens: 8192,
    }
}

fn verify_persona() -> Persona {
    Persona {
        kind: PersonaKind::Verify,
        name: "Verifier".into(),
        system_prompt: "You are a testing and verification expert. Run tests, validate \
            correctness against requirements, and check for regressions. Write new tests \
            for uncovered paths. Report pass/fail status with evidence. Check both happy \
            paths and error conditions. Ensure the build is green before approving."
            .into(),
        model_tier: ModelTier::Mid,
        description: "Runs tests, validates correctness, checks for regressions".into(),
        tools: vec![
            "run_command".into(),
            "read_file".into(),
            "write_file".into(),
        ],
        max_tokens: 4096,
    }
}

fn critique_persona() -> Persona {
    Persona {
        kind: PersonaKind::Critique,
        name: "Critic".into(),
        system_prompt: "You are a senior engineering critic. Review code and designs for \
            quality, maintainability, and adherence to best practices. Identify potential \
            issues, anti-patterns, unnecessary complexity, and missing error handling. Be \
            thorough but constructive — every criticism must include a specific suggestion \
            for improvement."
            .into(),
        model_tier: ModelTier::Premium,
        description: "Reviews for quality, patterns, and potential issues".into(),
        tools: vec!["read_file".into(), "search_symbol".into()],
        max_tokens: 4096,
    }
}

fn debug_persona() -> Persona {
    Persona {
        kind: PersonaKind::Debug,
        name: "Debugger".into(),
        system_prompt: "You are a systematic debugging expert. Analyze error messages, \
            stack traces, and logs to identify root causes. Reproduce issues when possible. \
            Use binary search and hypothesis testing to narrow down problems. Propose \
            targeted fixes that address the root cause, not symptoms. Document the \
            debugging process for future reference."
            .into(),
        model_tier: ModelTier::Mid,
        description: "Root cause analysis, systematic debugging, fix proposals".into(),
        tools: vec![
            "read_file".into(),
            "run_command".into(),
            "search_symbol".into(),
            "find_references".into(),
        ],
        max_tokens: 8192,
    }
}

fn code_review_persona() -> Persona {
    Persona {
        kind: PersonaKind::CodeReview,
        name: "Code Reviewer".into(),
        system_prompt: "You are a meticulous code reviewer focused on three areas: \
            (1) Style — naming conventions, formatting, documentation standards. \
            (2) Security — injection vulnerabilities, data leaks, insecure defaults, \
            input validation gaps. (3) Performance — unnecessary allocations, O(n^2) \
            algorithms, blocking calls in async contexts, missing caching opportunities. \
            Rate severity (info/warning/critical) for each finding."
            .into(),
        model_tier: ModelTier::Premium,
        description: "Style, security, and performance review".into(),
        tools: vec!["read_file".into(), "search_symbol".into()],
        max_tokens: 4096,
    }
}

// ---------------------------------------------------------------------------
// Prompt Override (for prompt evolver integration)
// ---------------------------------------------------------------------------

/// Trait for external prompt overrides (e.g. from a prompt evolver).
///
/// Implementations can return an evolved/refined prompt for a given persona
/// kind, or `None` to use the built-in default.
pub trait PromptOverride: Send + Sync {
    fn get_prompt(&self, kind: &PersonaKind) -> Option<String>;
}

// ---------------------------------------------------------------------------
// Persona Registry
// ---------------------------------------------------------------------------

/// Registry of available personas — both built-in and user-defined custom ones.
#[derive(Clone, Serialize, Deserialize)]
pub struct PersonaRegistry {
    built_in: HashMap<PersonaKind, Persona>,
    custom: Vec<Persona>,
    #[serde(skip)]
    prompt_override: Option<std::sync::Arc<dyn PromptOverride>>,
}

impl std::fmt::Debug for PersonaRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersonaRegistry")
            .field("built_in", &self.built_in)
            .field("custom", &self.custom)
            .field("prompt_override", &self.prompt_override.is_some())
            .finish()
    }
}

impl PersonaRegistry {
    /// Create a new registry pre-populated with all built-in personas.
    pub fn new() -> Self {
        let mut built_in = HashMap::new();
        built_in.insert(PersonaKind::Investigate, investigate_persona());
        built_in.insert(PersonaKind::Implement, implement_persona());
        built_in.insert(PersonaKind::Verify, verify_persona());
        built_in.insert(PersonaKind::Critique, critique_persona());
        built_in.insert(PersonaKind::Debug, debug_persona());
        built_in.insert(PersonaKind::CodeReview, code_review_persona());

        Self {
            built_in,
            custom: Vec::new(),
            prompt_override: None,
        }
    }

    /// Set a prompt override provider (e.g. from a prompt evolver).
    ///
    /// When set, `get()` will check the override before returning the
    /// built-in prompt, allowing learned/evolved prompts to take precedence.
    pub fn set_prompt_override(&mut self, provider: std::sync::Arc<dyn PromptOverride>) {
        self.prompt_override = Some(provider);
    }

    /// Look up a persona by kind. Checks built-in first, then custom.
    ///
    /// If a prompt override provider is set and returns an evolved prompt
    /// for this kind, the returned persona will use the evolved prompt
    /// instead of the built-in one.
    pub fn get(&self, kind: &PersonaKind) -> Option<&Persona> {
        if let Some(persona) = self.built_in.get(kind) {
            return Some(persona);
        }
        self.custom.iter().find(|p| p.kind == *kind)
    }

    /// Like `get()`, but applies any active prompt override, returning
    /// an owned `Persona` with the evolved prompt.
    pub fn get_evolved(&self, kind: &PersonaKind) -> Option<Persona> {
        let persona = self.get(kind)?;
        if let Some(ref provider) = self.prompt_override {
            if let Some(evolved_prompt) = provider.get_prompt(kind) {
                let mut evolved = persona.clone();
                evolved.system_prompt = evolved_prompt;
                return Some(evolved);
            }
        }
        Some(persona.clone())
    }

    /// Register a custom persona. Overwrites any existing custom persona
    /// with the same kind.
    pub fn register_custom(&mut self, persona: Persona) {
        // Remove existing custom persona with the same kind, if any.
        self.custom.retain(|p| p.kind != persona.kind);
        self.custom.push(persona);
    }

    /// Return all personas (built-in followed by custom).
    pub fn all(&self) -> Vec<&Persona> {
        let mut result: Vec<&Persona> = self.built_in.values().collect();
        result.extend(self.custom.iter());
        result
    }

    /// Find a persona by its `name` field (case-insensitive).
    pub fn find_by_name(&self, name: &str) -> Option<&Persona> {
        let lower = name.to_lowercase();
        self.built_in
            .values()
            .chain(self.custom.iter())
            .find(|p| p.name.to_lowercase() == lower)
    }

    /// Number of registered personas (built-in + custom).
    pub fn count(&self) -> usize {
        self.built_in.len() + self.custom.len()
    }
}

impl Default for PersonaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Persona Execution
// ---------------------------------------------------------------------------

/// Execute a task using a specific persona and AI executor.
///
/// Builds a `ChatRequest` using the persona's system prompt and model tier,
/// then returns an `AgentOutput` compatible with the HiveMind system.
///
/// If `prompt_addendum` is provided (e.g. from a preference model or prompt
/// evolver), it is appended to the persona's system prompt.
pub async fn execute_with_persona<E: AiExecutor>(
    persona: &Persona,
    task: &str,
    executor: &E,
    prompt_addendum: Option<&str>,
) -> AgentOutput {
    let model = default_model_for_tier(persona.model_tier);

    let system_prompt = match prompt_addendum {
        Some(addendum) if !addendum.is_empty() => {
            format!("{}\n\n{}", persona.system_prompt, addendum)
        }
        _ => persona.system_prompt.clone(),
    };

    let request = ChatRequest {
        messages: vec![ChatMessage::text(MessageRole::User, task.to_string())],
        model: model.clone(),
        max_tokens: persona.max_tokens,
        temperature: Some(0.3),
        system_prompt: Some(system_prompt),
        tools: None,
    };

    let start = Instant::now();

    match executor.execute(&request).await {
        Ok(response) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let cost = estimate_persona_cost(&model, &response.usage);

            AgentOutput {
                role: persona_kind_to_role(&persona.kind),
                model_used: model,
                content: response.content,
                cost,
                input_tokens: response.usage.prompt_tokens,
                output_tokens: response.usage.completion_tokens,
                duration_ms,
                success: true,
                error: None,
            }
        }
        Err(err) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            AgentOutput {
                role: persona_kind_to_role(&persona.kind),
                model_used: model,
                content: String::new(),
                cost: 0.0,
                input_tokens: 0,
                output_tokens: 0,
                duration_ms,
                success: false,
                error: Some(err),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a `PersonaKind` to the closest `AgentRole` for compatibility.
fn persona_kind_to_role(kind: &PersonaKind) -> AgentRole {
    match kind {
        PersonaKind::Investigate => AgentRole::Architect,
        PersonaKind::Implement => AgentRole::Coder,
        PersonaKind::Verify => AgentRole::Tester,
        PersonaKind::Critique => AgentRole::Reviewer,
        PersonaKind::Debug => AgentRole::Debugger,
        PersonaKind::CodeReview => AgentRole::Reviewer,
        PersonaKind::Custom(_) => AgentRole::Coder,
    }
}

/// Simple cost estimate for persona execution.
fn estimate_persona_cost(model_id: &str, usage: &TokenUsage) -> f64 {
    let (input_rate, output_rate) = match model_id {
        m if m.contains("opus") => (15.0, 75.0),
        m if m.contains("sonnet") => (3.0, 15.0),
        m if m.contains("haiku") => (0.80, 4.0),
        m if m.contains("gpt-4o") => (2.50, 10.0),
        m if m.contains("gpt-4") => (10.0, 30.0),
        _ => (0.0, 0.0),
    };

    let input_cost = (usage.prompt_tokens as f64 / 1_000_000.0) * input_rate;
    let output_cost = (usage.completion_tokens as f64 / 1_000_000.0) * output_rate;
    input_cost + output_cost
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hive_ai::types::{ChatResponse, FinishReason};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockExecutor {
        response: String,
        should_fail: bool,
        call_count: Arc<AtomicUsize>,
    }

    impl MockExecutor {
        fn new(response: &str) -> Self {
            Self {
                response: response.into(),
                should_fail: false,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn failing() -> Self {
            Self {
                response: String::new(),
                should_fail: true,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl AiExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                return Err("Mock failure".into());
            }
            Ok(ChatResponse {
                content: self.response.clone(),
                model: "mock-model".into(),
                usage: TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 100,
                    total_tokens: 150,
                },
                finish_reason: FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }
    }

    #[test]
    fn registry_has_all_built_in_personas() {
        let registry = PersonaRegistry::new();
        assert_eq!(registry.count(), 6);

        for kind in &PersonaKind::BUILT_IN {
            let persona = registry.get(kind);
            assert!(persona.is_some(), "Missing built-in persona: {kind}");
        }
    }

    #[test]
    fn built_in_personas_have_nonempty_prompts() {
        let registry = PersonaRegistry::new();
        for persona in registry.all() {
            assert!(
                !persona.system_prompt.is_empty(),
                "{} has empty prompt",
                persona.name
            );
            assert!(!persona.name.is_empty());
            assert!(!persona.description.is_empty());
            assert!(!persona.tools.is_empty());
            assert!(persona.max_tokens > 0);
        }
    }

    #[test]
    fn register_custom_persona() {
        let mut registry = PersonaRegistry::new();
        let custom = Persona {
            kind: PersonaKind::Custom("translator".into()),
            name: "Translator".into(),
            system_prompt: "Translate code between languages.".into(),
            model_tier: ModelTier::Mid,
            description: "Cross-language code translation".into(),
            tools: vec!["read_file".into()],
            max_tokens: 4096,
        };
        registry.register_custom(custom);
        assert_eq!(registry.count(), 7);

        let found = registry.get(&PersonaKind::Custom("translator".into()));
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Translator");
    }

    #[test]
    fn register_custom_overwrites_duplicate_kind() {
        let mut registry = PersonaRegistry::new();
        let custom_v1 = Persona {
            kind: PersonaKind::Custom("mybot".into()),
            name: "Bot V1".into(),
            system_prompt: "Version 1".into(),
            model_tier: ModelTier::Budget,
            description: "V1".into(),
            tools: vec![],
            max_tokens: 2048,
        };
        let custom_v2 = Persona {
            kind: PersonaKind::Custom("mybot".into()),
            name: "Bot V2".into(),
            system_prompt: "Version 2".into(),
            model_tier: ModelTier::Mid,
            description: "V2".into(),
            tools: vec![],
            max_tokens: 4096,
        };

        registry.register_custom(custom_v1);
        registry.register_custom(custom_v2);

        // Should still be 7 (6 built-in + 1 custom), not 8.
        assert_eq!(registry.count(), 7);
        let found = registry.get(&PersonaKind::Custom("mybot".into())).unwrap();
        assert_eq!(found.name, "Bot V2");
    }

    #[test]
    fn find_by_name_case_insensitive() {
        let registry = PersonaRegistry::new();
        let found = registry.find_by_name("investigator");
        assert!(found.is_some());
        assert_eq!(found.unwrap().kind, PersonaKind::Investigate);

        let found_upper = registry.find_by_name("DEBUGGER");
        assert!(found_upper.is_some());
        assert_eq!(found_upper.unwrap().kind, PersonaKind::Debug);
    }

    #[test]
    fn find_by_name_not_found() {
        let registry = PersonaRegistry::new();
        assert!(registry.find_by_name("nonexistent").is_none());
    }

    #[test]
    fn persona_kind_display() {
        assert_eq!(PersonaKind::Investigate.to_string(), "Investigate");
        assert_eq!(PersonaKind::CodeReview.to_string(), "Code Review");
        assert_eq!(
            PersonaKind::Custom("my-agent".into()).to_string(),
            "Custom(my-agent)"
        );
    }

    #[test]
    fn persona_kind_to_role_mapping() {
        assert_eq!(
            persona_kind_to_role(&PersonaKind::Investigate),
            AgentRole::Architect
        );
        assert_eq!(
            persona_kind_to_role(&PersonaKind::Implement),
            AgentRole::Coder
        );
        assert_eq!(
            persona_kind_to_role(&PersonaKind::Verify),
            AgentRole::Tester
        );
        assert_eq!(
            persona_kind_to_role(&PersonaKind::Critique),
            AgentRole::Reviewer
        );
        assert_eq!(
            persona_kind_to_role(&PersonaKind::Debug),
            AgentRole::Debugger
        );
        assert_eq!(
            persona_kind_to_role(&PersonaKind::CodeReview),
            AgentRole::Reviewer
        );
        assert_eq!(
            persona_kind_to_role(&PersonaKind::Custom("x".into())),
            AgentRole::Coder
        );
    }

    #[tokio::test]
    async fn execute_with_persona_success() {
        let registry = PersonaRegistry::new();
        let persona = registry.get(&PersonaKind::Implement).unwrap();
        let executor = MockExecutor::new("Here is the implementation.");

        let output =
            execute_with_persona(persona, "Write a sorting function", &executor, None).await;

        assert!(output.success);
        assert_eq!(output.content, "Here is the implementation.");
        assert_eq!(output.role, AgentRole::Coder);
        assert!(output.duration_ms < 5000);
        assert!(output.error.is_none());
    }

    #[tokio::test]
    async fn execute_with_persona_failure() {
        let registry = PersonaRegistry::new();
        let persona = registry.get(&PersonaKind::Debug).unwrap();
        let executor = MockExecutor::failing();

        let output = execute_with_persona(persona, "Debug this crash", &executor, None).await;

        assert!(!output.success);
        assert!(output.content.is_empty());
        assert!(output.error.is_some());
        assert_eq!(output.cost, 0.0);
    }

    #[test]
    fn all_returns_built_in_plus_custom() {
        let mut registry = PersonaRegistry::new();
        let custom = Persona {
            kind: PersonaKind::Custom("extra".into()),
            name: "Extra".into(),
            system_prompt: "Do extra things.".into(),
            model_tier: ModelTier::Free,
            description: "Extra persona".into(),
            tools: vec![],
            max_tokens: 1024,
        };
        registry.register_custom(custom);

        let all = registry.all();
        assert_eq!(all.len(), 7);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut registry = PersonaRegistry::new();
        let custom = Persona {
            kind: PersonaKind::Custom("test".into()),
            name: "Test".into(),
            system_prompt: "Test prompt".into(),
            model_tier: ModelTier::Budget,
            description: "Test desc".into(),
            tools: vec!["tool_a".into()],
            max_tokens: 2048,
        };
        registry.register_custom(custom);

        let json = serde_json::to_string(&registry).unwrap();
        let deserialized: PersonaRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.count(), 7);

        let found = deserialized
            .get(&PersonaKind::Custom("test".into()))
            .unwrap();
        assert_eq!(found.name, "Test");
    }

    #[test]
    fn estimate_persona_cost_known_models() {
        let usage = TokenUsage {
            prompt_tokens: 1_000_000,
            completion_tokens: 1_000_000,
            total_tokens: 2_000_000,
        };
        let cost = estimate_persona_cost("claude-sonnet-4", &usage);
        // $3 input + $15 output = $18
        assert!((cost - 18.0).abs() < 0.01);

        let unknown = estimate_persona_cost("local-model", &usage);
        assert_eq!(unknown, 0.0);
    }
}
