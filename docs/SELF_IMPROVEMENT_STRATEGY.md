# Hive Self-Improvement Strategy

> A comprehensive analysis of Hive's learning and self-improvement capabilities,
> with a phased roadmap for achieving autonomous quality improvement.

---

## Table of Contents

1. [What Exists (Current State Audit)](#1-what-exists-current-state-audit)
2. [Self-Improvement Pillars](#2-self-improvement-pillars)
3. [Implementation Roadmap](#3-implementation-roadmap)
4. [Safety Guardrails](#4-safety-guardrails)
5. [Competitive Edge](#5-competitive-edge)

---

## 1. What Exists (Current State Audit)

### 1.1 hive_learn Crate -- The Learning Engine

**Status: Fully implemented with SQLite persistence and comprehensive test coverage.**

The `hive_learn` crate (`hive/crates/hive_learn/src/`) is the central learning subsystem.
It is structured as six cooperating modules coordinated by `LearningService`:

| Module | File | Purpose | Maturity |
|---|---|---|---|
| `LearningService` | `lib.rs` | Central coordinator; owns all subsystems, triggers periodic analysis | Complete |
| `OutcomeTracker` | `outcome_tracker.rs` | Detects what happened after each AI response (Accepted/Corrected/Regenerated/Ignored) | Complete |
| `RoutingLearner` | `routing_learner.rs` | Analyzes (task_type, tier) quality via EMA; recommends tier changes | Complete |
| `PreferenceModel` | `preference_model.rs` | Bayesian confidence updates for user preferences; generates prompt addendums | Complete |
| `PromptEvolver` | `prompt_evolver.rs` | Versioned prompts per persona; suggests refinements when quality drops | Complete |
| `PatternLibrary` | `pattern_library.rs` | Extracts reusable code patterns from high-quality responses | Complete |
| `SelfEvaluator` | `self_evaluator.rs` | Computes overall quality, trends, weak areas, cost efficiency | Complete |
| `LearningStorage` | `storage.rs` | SQLite persistence with 6 tables and ~40 query methods | Complete |
| Types | `types.rs` | `OutcomeRecord`, `RoutingAdjustment`, `UserPreference`, `PromptVersion`, `CodePattern`, `SelfEvaluationReport`, etc. | Complete |

**Key data flow:**
```
User interaction
    |
    v
LearningService::on_outcome()
    |-- OutcomeTracker::record()           --> learning_outcomes table
    |-- LearningStorage::record_routing()  --> routing_history table
    |-- PromptEvolver::record_quality()    --> prompt_versions table (quality stats)
    |-- Every 50 interactions: RoutingLearner::analyze()
    |-- Every 200 interactions: SelfEvaluator::evaluate()
```

**Storage schema (6 tables):**
- `learning_outcomes` -- per-message outcome records with quality scores, cost, latency
- `routing_history` -- task_type/tier/model routing decisions and their quality
- `user_preferences` -- key/value preferences with Bayesian confidence
- `prompt_versions` -- versioned system prompts per persona with quality tracking
- `code_patterns` -- extracted code patterns with language/quality/frequency metadata
- `learning_log` -- transparent audit trail of all learning decisions

### 1.2 hive_agents Crate -- Collective Intelligence

**Status: Fully implemented with multi-agent orchestration and shared memory.**

| Module | File | Purpose | Maturity |
|---|---|---|---|
| `CollectiveMemory` | `collective_memory.rs` | SQLite-backed shared memory with 7 categories, relevance decay, pruning | Complete |
| `Queen` | `queen.rs` | Meta-coordinator: Plan -> Execute -> Synthesize -> Learn lifecycle | Complete |
| Swarm Types | `swarm.rs` | Orchestration modes, team objectives, config, cycle-detection validation | Complete |
| `Coordinator` | `coordinator.rs` | Task decomposition and dependency planning | Complete |
| `Personas` | `personas.rs` | Named persona definitions with prompt overrides | Complete |
| `AutoCommitService` | `auto_commit.rs` | Git commit automation from agent work | Complete |
| `WorktreeManager` | `worktree.rs` | Git worktree management for parallel agent execution | Complete |

**CollectiveMemory categories:**
- `SuccessPattern` -- approaches that worked well
- `FailurePattern` -- approaches that failed (avoid repeating)
- `ModelInsight` -- observations about specific model strengths/weaknesses
- `ConflictResolution` -- how merge conflicts and disagreements were resolved
- `CodePattern` -- reusable code structures
- `UserPreference` -- learned user preferences
- `General` -- uncategorized knowledge

**Queen lifecycle (4 phases):**
1. **Plan** -- AI-driven goal decomposition into `TeamObjective` items, enriched with collective memory context
2. **Execute** -- Dependency-wave execution with budget ($25 default) and time (1800s default) enforcement
3. **Synthesize** -- AI-powered output merging across team results
4. **Learn** -- Records success/failure/insight patterns to `CollectiveMemory`

### 1.3 hive_ai Crate -- Routing and Context Intelligence

**Status: Fully implemented with provider fallback, complexity-based routing, and TF-IDF context curation.**

| Module | File | Purpose | Maturity |
|---|---|---|---|
| `AutoFallbackManager` | `routing/auto_fallback.rs` | Provider health tracking, intelligent fallback chains across 10 providers | Complete |
| `ModelRouter` | `routing/model_router.rs` | Complexity classification + tier adjustment + model selection | Complete |
| `ContextEngine` | `context_engine.rs` | TF-IDF scoring with heuristic boosts, greedy budget packing | Complete |
| `FleetLearningService` | `fleet_learning.rs` | In-memory pattern tracking, model performance, fleet insights | Complete (in-memory only) |

**Routing pipeline:**
```
User message
    |
    v
ModelRouter::route()
    |-- If explicit model: validate + fallback via OpenRouter proxy
    |-- If auto:
        |-- ComplexityClassifier::classify() --> (task_type, tier)
        |-- TierAdjuster::adjust_tier()     --> (optional learned override)
        |-- AutoFallbackManager::get_chain() --> ordered provider list
        |-- Pick first healthy provider in chain
```

**Provider health tracking (`AutoFallbackManager`):**
- 10 provider types: Anthropic, OpenAI, OpenRouter, Google, Groq, LiteLLM, HuggingFace, Ollama, LMStudio, GenericLocal
- Default 14-entry fallback chain across Premium/Mid/Budget/Free tiers
- Auto-disable after 3 consecutive failures
- Rate-limit cooldown tracking
- Budget exhaustion detection
- History capped at 1000 events

**Critical integration point -- `TierAdjuster` trait:**
```rust
// hive_ai/src/routing/model_router.rs
pub trait TierAdjuster: Send + Sync {
    fn adjust_tier(&self, task_type: &str, classified_tier: &str) -> Option<String>;
}
```
This trait is implemented by `hive_learn::LearnerTierAdjuster`, connecting
the learning engine to live routing decisions. **This wiring is now live** -- `LearnerTierAdjuster` is set on the `ModelRouter` during app startup in `main.rs::init_services()`.

### 1.4 Integration Status

| Item | Status | Details |
|---|---|---|
| `TierAdjuster` wired | **DONE** | `LearnerTierAdjuster` in `hive_learn::lib` implements `TierAdjuster`. Wired in `main.rs` at startup via `model_router.set_tier_adjuster()`. Routing now benefits from learned quality data. |
| `LearningService` instantiated at startup | **DONE** | Created via `LearningService::open("~/.hive/learning.db")` in `init_services()`. Stored as `AppLearning(Arc<LearningService>)` GPUI global. |
| `HiveShield` wired into chat pipeline | **DONE** | `HiveShield::process_outgoing()` called in `workspace.rs::handle_send_text()`. Block/CloakAndAllow/Warn/Allow actions before AI provider calls. |
| Learning outcome recording | **DONE** | `ChatService` emits `StreamCompleted` events. `HiveWorkspace` subscribes and calls `LearningService::on_outcome()` with model, cost, and token data. |
| Learning panel live data | **DONE** | `refresh_learning_data()` queries `AppLearning` global for preferences, self-evaluation reports, and routing adjustments. |
| Shield panel live data | **DONE** | `refresh_shield_data()` queries `AppShield` global for shield configuration and status. |
| `FleetLearningService` is in-memory only | **Open** | No persistence across restarts. Fleet insights lost on app restart. |
| `ContextEngine` does not use `PatternLibrary` | **Open** | Context curation ignores learned code patterns. Pattern library data collected but unused in context. |
| `PreferenceModel::prompt_addendum()` not injected into prompts | **Open** | Preferences tracked but not applied to AI requests. User preference learning has no effect on output quality. |
| `Queen::record_learnings()` and `LearningService` are separate | **Open** | Two parallel learning systems with no cross-pollination. Collective memory and individual learning are siloed. |

---

## 2. Self-Improvement Pillars

### 2.1 Outcome Tracking

**What exists:**
- `OutcomeTracker::detect_outcome()` -- keyword detection for regeneration requests + Jaccard token similarity for distinguishing ignored/corrected/accepted
- `OutcomeTracker::compute_quality_score()` -- base score per outcome type (Accepted=0.9, Corrected=0.5, Regenerated=0.2, Ignored=0.1) with follow-up and edit-distance penalties
- `OutcomeTracker::record()` -- persists to SQLite + writes to transparent learning log
- `OutcomeTracker::model_quality()` and `task_tier_quality()` -- rolling average queries

**What needs improvement:**

1. **Richer signal detection.** The current Jaccard similarity is a blunt instrument. A user who says "Thanks, now do X" is marked Accepted, but the "now do X" may indicate the response was only partially useful. Add:
   - Sentiment analysis on follow-up messages (positive/negative/neutral)
   - Time-to-next-message as a quality proxy (fast follow-up = possible dissatisfaction)
   - Explicit feedback buttons in the UI (thumbs up/down) for unambiguous signal

2. **Edit distance from code diffs.** The `edit_distance` field on `OutcomeRecord` exists but is `Option<f64>` and rarely populated. When a user accepts AI-generated code and then edits it, the diff distance is a strong quality signal. Wire this to:
   - `hive_fs` file watchers that detect changes to AI-written files
   - Git diff analysis post-commit via `AutoCommitService`

3. **Conversation-level quality.** Currently each message gets an independent quality score. Add conversation-level aggregation:
   - Did the conversation achieve its goal? (user explicitly confirms or moves on)
   - How many turns did it take? (fewer = better)
   - Was the final code accepted without modification?

**Concrete actions:**
- Add `OutcomeTracker::detect_outcome_v2()` with sentiment + timing signals
- Wire `AutoCommitService` post-commit diffs back to `OutcomeTracker` as edit distance data
- Add `ConversationOutcome` type to `types.rs` with conversation-level quality metrics
- Add thumbs-up/down buttons to `hive_ui` chat messages that call `LearningService::on_explicit_feedback()`

### 2.2 Prompt Evolution

**What exists:**
- `PromptEvolver` -- versioned prompts per persona, stored in `prompt_versions` table
- `PromptEvolver::record_quality()` -- running average quality per persona
- `PromptEvolver::suggest_refinements()` -- triggers when quality < 0.6 with 20+ outcomes; generates rule-based suggestions
- `PromptEvolver::apply_refinement()` -- creates new version, logs to learning log
- `PromptEvolver::rollback()` -- reverts to any previous version
- `PreferenceModel::prompt_addendum()` -- generates preference-based additions to system prompts

**What needs improvement:**

1. **AI-assisted prompt refinement.** The current `generate_refinement_suggestion()` is purely rule-based (adds static phrases like "Be more precise" or "Include code examples"). Replace with:
   - Use a cheap model (e.g., free-tier Groq) to analyze the low-quality outcomes and generate targeted prompt improvements
   - A/B test refined prompts: send 50% of requests through the new prompt, 50% through the old, and compare quality scores
   - Auto-promote winning prompts after statistically significant improvement (>5% quality lift over 30+ interactions)

2. **Preference injection.** `PreferenceModel::prompt_addendum()` generates text like "User prefers concise responses" but this is never injected into actual AI requests. Wire it into:
   - `ModelRouter::route()` should append preference addendums to the system prompt
   - Or better: `ContextEngine::curate()` should include a "learned preferences" context source

3. **Cross-persona learning.** If the "coder" persona improves by adding "Always include error handling", that insight may also benefit the "reviewer" persona. Add:
   - `PromptEvolver::cross_pollinate()` -- identify high-quality prompt fragments and suggest them for related personas

**Concrete actions:**
- Replace `generate_refinement_suggestion()` body with AI-driven analysis using `AiService::complete()`
- Add `PromptEvolver::ab_test()` method that tracks prompt version performance in parallel
- Wire `PreferenceModel::prompt_addendum()` into the `ContextEngine::curate()` output as a reserved context source
- Add `PromptEvolver::cross_pollinate(source_persona, target_persona)` method

### 2.3 Routing Intelligence

**What exists:**
- `RoutingLearner::analyze()` -- EMA analysis of (task_type, tier) combinations; recommends upgrades when quality < 0.5, downgrades when quality > 0.85
- `RoutingLearner::adjust_tier()` -- returns learned tier override for a given (task_type, tier)
- `TierAdjuster` trait in `ModelRouter` -- designed for exactly this integration
- `AutoFallbackManager` -- provider health tracking with automatic disable/re-enable
- `ComplexityClassifier` -- regex-based task classification into tiers

**What needs improvement:**

1. **Wire the integration.** The `TierAdjuster` trait exists and `RoutingLearner` implements the right interface, but they are not connected. This is the single highest-impact improvement:
   ```rust
   // In app initialization:
   let learning = Arc::new(LearningService::open("~/.hive/learning.db")?);
   let adjuster = Arc::new(LearnerTierAdjuster(learning.routing_learner));
   model_router.set_tier_adjuster(adjuster);
   ```

2. **Model-level routing, not just tier-level.** Currently routing picks a tier, then picks the first available model in that tier. Add model-level quality tracking:
   - Track quality per (task_type, model_id) not just (task_type, tier)
   - When multiple models are available in a tier, prefer the one with highest quality for that task type
   - Use `FleetLearningService::best_model_by_quality()` data to influence selection

3. **Cost-quality Pareto optimization.** The current system either upgrades or downgrades tiers. Add a cost-quality optimizer:
   - For each task type, compute the cost-per-quality-point for each model
   - Prefer models on the Pareto frontier (best quality for their cost level)
   - `SelfEvaluator` already computes `cost_per_quality_point` -- feed this back into routing

4. **Latency-aware routing.** Add latency as a routing signal:
   - Track p50/p95 latency per provider via `AutoFallbackManager`
   - For real-time chat, prefer lower-latency models; for background tasks, prefer higher-quality models
   - `OutcomeRecord` already stores `latency_ms` -- aggregate this in `RoutingLearner`

**Concrete actions:**
- Implement `LearnerTierAdjuster` wrapper struct that implements `TierAdjuster` by delegating to `RoutingLearner::adjust_tier()`
- Wire it in `hive_app/src/main.rs` during startup
- Add `model_quality_for_task()` method to `RoutingLearner` and use it in `ModelRouter::route()` for model-level selection within a tier
- Add `latency_percentile()` method to `AutoFallbackManager` and incorporate it into fallback chain ordering

### 2.4 Pattern Library Growth

**What exists:**
- `PatternLibrary::extract_patterns()` -- heuristic extraction of code patterns from AI responses with quality > 0.8
- Language-specific classifiers for Rust, Python, JS/TS, Go, Java/Kotlin, C/C++ plus generic fallback
- Pattern types: function/method signatures, structs, classes, enums, traits, interfaces, type aliases, decorators/attributes
- `PatternLibrary::search()` and `popular_patterns()` -- query APIs
- Stored in `code_patterns` SQLite table with language, pattern_type, content, quality_score, frequency

**What needs improvement:**

1. **Semantic pattern extraction.** The current extractor is line-based and misses multi-line patterns, doc comments, and pattern context. Improve by:
   - Using tree-sitter or similar for AST-aware extraction (long-term)
   - Short-term: extend regex patterns to capture doc comments and multi-line signatures
   - Group related patterns (e.g., struct + impl block) as composite patterns

2. **Pattern application.** Patterns are collected but never used. Wire them into:
   - `ContextEngine::curate()` -- include relevant patterns as context ("Here are patterns that worked well for similar tasks")
   - `PromptEvolver` -- reference popular patterns in prompt refinements ("When writing Rust, prefer the Result<T, Error> return pattern")
   - Code completion suggestions in the UI

3. **Pattern quality feedback loop.** When a pattern is included in context and the resulting response scores well, boost that pattern's relevance. When it scores poorly, decay it:
   - Add `PatternLibrary::boost(pattern_id, factor)` and `PatternLibrary::decay(pattern_id, factor)` methods
   - Track which patterns were included in which requests via a junction table

4. **Cross-language pattern transfer.** Some patterns are language-agnostic (error handling, resource cleanup, builder pattern). Tag these and suggest them across languages:
   - Add `is_universal` flag to `CodePattern`
   - `PatternLibrary::universal_patterns()` query

**Concrete actions:**
- Add multi-line pattern extraction for doc-comment + signature combinations
- Add `PatternLibrary::relevant_for_task(task_type, language, limit)` and wire it into `ContextEngine`
- Add `PatternLibrary::boost()` and `decay()` with a `pattern_usage` tracking table
- Add `is_universal` column to `code_patterns` and a classification heuristic

### 2.5 Self-Evaluation Loops

**What exists:**
- `SelfEvaluator::evaluate()` computes a comprehensive `SelfEvaluationReport`:
  - `overall_quality` -- average of last 100 outcomes
  - `trend` -- Improving/Stable/Declining based on comparing last 50 vs previous 50 outcomes (+/-0.05 threshold)
  - `best_model` / `worst_model` -- from models with 5+ outcomes
  - `misroute_rate` -- percentage of outcomes where classified tier did not match actual tier needed
  - `cost_per_quality_point` -- total cost / total quality
  - `weak_areas` -- task types with average quality < 0.5
  - `correction_rate` / `regeneration_rate` -- from outcome distribution
- Triggered automatically every 200 interactions via `LearningService::on_outcome()`
- Results logged to `learning_log` table

**What needs improvement:**

1. **Actionable recommendations.** The current report is descriptive but not prescriptive. Add:
   - `SelfEvaluationReport::recommendations` -- a `Vec<Recommendation>` with concrete actions
   - Example: "Model X has quality 0.3 for code_gen tasks. Recommend: stop routing code_gen to Model X."
   - Example: "Routing tier 'standard' has quality 0.4 for 'debugging' tasks. Recommend: upgrade to 'premium'."

2. **Auto-remediation.** When weak areas are detected, trigger automatic corrective actions:
   - If a model's quality drops below 0.4: auto-disable it via `AutoFallbackManager::mark_budget_exhausted()`
   - If a task type consistently underperforms: auto-trigger `RoutingLearner::analyze()` and apply adjustments
   - If `correction_rate` > 0.3: trigger `PromptEvolver::suggest_refinements()` for all active personas

3. **Trend visualization data.** Expose time-series data for the learning UI panel:
   - `SelfEvaluator::quality_history(days)` -- daily average quality scores
   - `SelfEvaluator::cost_history(days)` -- daily cost totals
   - `SelfEvaluator::model_comparison(days)` -- per-model quality over time

4. **Comparative evaluation.** Periodically send the same prompt to multiple models and compare:
   - Shadow requests to secondary models (at low cost with free-tier models)
   - Compare response quality using the `OutcomeTracker` quality metrics
   - Build a ground-truth model ranking that updates continuously

**Concrete actions:**
- Add `Recommendation` struct to `types.rs` with `action: RecommendationAction` enum (DisableModel, UpgradeTier, RefinePrompt, InvestigateTaskType)
- Add `SelfEvaluator::generate_recommendations()` that produces actionable items from report data
- Add `SelfEvaluator::auto_remediate()` that executes safe, reversible recommendations
- Add time-series query methods to `LearningStorage` for UI charting

### 2.6 Collective Intelligence

**What exists:**
- `CollectiveMemory` -- SQLite-backed shared memory with 7 categories, relevance scoring, access tracking
- `CollectiveMemory::remember()` -- stores insights with tags, source attribution, relevance score
- `CollectiveMemory::recall()` -- query by content/category/tags with relevance ordering
- `CollectiveMemory::touch()` -- bumps access count and relevance (1.01x) on useful recall
- `CollectiveMemory::decay_scores(factor)` -- global relevance decay for memory aging
- `CollectiveMemory::prune(min_relevance)` -- removes stale low-relevance memories
- `Queen::record_learnings()` -- stores success/failure/insight patterns after each swarm execution
- `FleetLearningService` -- in-memory tracking of patterns, model performance, fleet insights

**What needs improvement:**

1. **Bidirectional learning bridge.** `CollectiveMemory` (hive_agents) and `LearningStorage` (hive_learn) are parallel systems. Unify them:
   - `CollectiveMemory::ModelInsight` entries should feed into `RoutingLearner` tier adjustments
   - `CollectiveMemory::SuccessPattern` entries should feed into `PatternLibrary`
   - `LearningService` quality data should inform `CollectiveMemory` relevance scores
   - Create a `LearningBridge` service that synchronizes data between both systems

2. **Memory-informed context.** When `Queen::plan()` queries collective memory for planning context, it should also incorporate:
   - `PatternLibrary::popular_patterns()` for the relevant languages
   - `PreferenceModel::prompt_addendum()` for user preferences
   - `SelfEvaluator` weak areas to avoid known failure modes

3. **Fleet persistence.** `FleetLearningService` is in-memory only. Persist to SQLite:
   - Add `fleet_patterns`, `fleet_model_performance`, `fleet_insights` tables to `LearningStorage`
   - Sync on shutdown, restore on startup
   - Enable cross-session fleet learning

4. **Swarm learning aggregation.** When multiple agents in a swarm produce results:
   - Compare agent outputs for consensus (multiple agents agreeing = higher confidence)
   - Record disagreements as `ConflictResolution` memories when resolved
   - Track which orchestration mode (`HiveMind`/`Coordinator`/`NativeProvider`) works best for which task types

**Concrete actions:**
- Create `LearningBridge` in a new `hive_learn/src/bridge.rs` that holds references to both `LearningService` and `CollectiveMemory`
- Add `FleetLearningService::persist()` and `restore()` methods backed by SQLite
- Add `Queen::plan()` integration with `PatternLibrary` and `PreferenceModel`
- Add orchestration mode quality tracking to `CollectiveMemory`

---

## 3. Implementation Roadmap

### Phase 1: Telemetry + Outcome Logging (Weeks 1-3)

**Goal:** Every AI interaction produces a learning signal that is persisted.

**Tasks:**

- [x] **Wire `LearningService` into app startup** (`hive_app/src/main.rs`)
  - `LearningService::open("~/.hive/learning.db")` in `init_services()`
  - Stored as `AppLearning(Arc<LearningService>)` GPUI global
  - Accessible to all subsystems via `cx.global::<AppLearning>()`

- [x] **Instrument `AiService` to call `LearningService::on_outcome()`**
  - `ChatService` emits `StreamCompleted` events (model, cost, tokens)
  - `HiveWorkspace` subscribes and calls `LearningService::on_outcome()`
  - `AiService::router_mut()` added for runtime adjuster wiring

- [ ] **Add explicit feedback UI** (`hive_ui/src/components/feedback.rs`)
  - Thumbs up/down buttons on each AI message in chat
  - "Report issue" button that opens a feedback dialog
  - Wire to `LearningService::on_explicit_feedback()` (new method)

- [ ] **Wire edit distance tracking**
  - When user edits AI-generated code in the editor, compute character-level edit distance
  - Feed back to `OutcomeTracker` as `edit_distance` field
  - Use file watchers from `hive_fs` to detect changes

- [x] **Add Learning panel to UI** (`hive_ui/src/panels/learning.rs`)
  - Panel exists with sections for preferences, self-evaluation reports, routing adjustments
  - `refresh_learning_data()` queries live `AppLearning` global
  - Displays active preferences, quality trends, routing adjustments

**Milestone:** Every AI interaction generates a persisted `OutcomeRecord`. Users can see the learning log in the UI.

### Phase 2: Feedback Loops (Weeks 4-7)

**Goal:** Learning data actively improves routing, prompts, and context selection.

**Tasks:**

- [x] **Wire `TierAdjuster` integration**
  - `LearnerTierAdjuster` struct in `hive_learn::lib` wraps `Arc<LearningService>`, delegates to `RoutingLearner::adjust_tier()`
  - `ModelRouter::set_tier_adjuster()` called in `main.rs::init_services()` at startup
  - `AiService::router_mut()` method added for runtime access to `ModelRouter`

- [ ] **Inject learned preferences into prompts**
  - `ModelRouter::route()` appends `PreferenceModel::prompt_addendum()` to system prompt
  - Or: add a "learned preferences" `SourceType` to `ContextEngine`
  - Verify preference text appears in actual AI requests

- [ ] **Wire `PatternLibrary` into `ContextEngine`**
  - Add `PatternLibrary::relevant_for_task()` method
  - `ContextEngine::curate()` includes top-K relevant patterns as context sources
  - Patterns are scored by (relevance to query * pattern quality * frequency)

- [ ] **Connect `CollectiveMemory` and `LearningService`**
  - Create `LearningBridge` service
  - `CollectiveMemory::ModelInsight` -> `RoutingLearner` tier data
  - `CollectiveMemory::SuccessPattern` -> `PatternLibrary` patterns
  - Bidirectional sync on configurable interval

- [ ] **Persist `FleetLearningService`**
  - Add SQLite tables for fleet patterns and model performance
  - Sync to disk on app shutdown, restore on startup
  - Add `fleet_learning` module to `LearningStorage`

- [ ] **Self-evaluation recommendations**
  - Add `Recommendation` type and `generate_recommendations()` to `SelfEvaluator`
  - Display recommendations in the Learning panel UI
  - Allow user to accept/reject recommendations with one click

**Milestone:** Routing improves automatically based on quality data. Users see and control learned behaviors. Collective memory and individual learning are connected.

### Phase 3: Active Experimentation (Weeks 8-12)

**Goal:** The system actively experiments to discover better configurations.

**Tasks:**

- [ ] **Prompt A/B testing**
  - `PromptEvolver::ab_test(persona, variant_a, variant_b)` -- splits traffic 50/50
  - Track quality per variant over configurable sample size (default: 30 interactions)
  - Auto-promote winner after statistical significance threshold (p < 0.05 via two-proportion z-test)
  - Log all A/B test results to `learning_log`

- [ ] **AI-driven prompt refinement**
  - Replace rule-based `generate_refinement_suggestion()` with AI-powered analysis
  - Send low-quality outcome samples + current prompt to a cheap model
  - Parse structured refinement suggestions
  - Present to user for approval before applying

- [ ] **Model-level routing within tiers**
  - Track quality per (task_type, model_id) in addition to (task_type, tier)
  - When multiple models available in a tier, prefer highest-quality model for that task type
  - Add `model_preference_for_task()` to `RoutingLearner`

- [ ] **Latency-aware routing**
  - Add `LatencyTracker` to `AutoFallbackManager` with p50/p95 per provider
  - For real-time chat: weight latency 30% in routing score
  - For background tasks (swarm agents): weight quality 90%, latency 10%
  - Configurable via `RoutingConfig`

- [ ] **Pattern quality feedback loop**
  - Track which patterns were included in context for each request
  - After outcome, boost patterns that correlated with high quality
  - Decay patterns that correlated with low quality
  - Add `pattern_usage` tracking table to `LearningStorage`

- [ ] **Comparative model evaluation**
  - Periodically shadow-request the same prompt to a secondary model
  - Compare quality metrics between primary and shadow response
  - Update model rankings based on head-to-head comparisons
  - Use free-tier models for shadow requests to minimize cost

**Milestone:** The system actively experiments with prompts, models, and routing configurations. Improvements are data-driven and statistically validated.

### Phase 4: Autonomous Improvement (Weeks 13-20)

**Goal:** The system improves itself with minimal human intervention, within safety bounds.

**Tasks:**

- [ ] **Auto-remediation engine**
  - `SelfEvaluator::auto_remediate()` executes safe, reversible recommendations:
    - Auto-disable models with quality < 0.3 (reversible via `AutoFallbackManager`)
    - Auto-upgrade tiers for task types with quality < 0.4 (reversible via `RoutingLearner::clear_adjustments()`)
    - Auto-trigger prompt refinement for personas with quality < 0.5
  - All auto-actions logged to `learning_log` with `reversible: true`
  - User can undo any auto-action from the Learning panel

- [ ] **Swarm learning optimization**
  - Track orchestration mode effectiveness per task type
  - `Queen::plan()` selects orchestration mode based on historical success rates
  - Swarm budget allocation based on task complexity and historical cost data
  - Record and replay successful swarm strategies

- [ ] **Cross-session learning continuity**
  - On app startup: load and replay recent `SelfEvaluationReport`
  - Resume any in-progress A/B tests
  - Re-apply routing adjustments from previous sessions
  - Display "What I learned last session" summary in UI

- [ ] **Learning dashboard**
  - Time-series charts: quality over time, cost over time, model comparison
  - Heatmap: task_type x model quality matrix
  - Funnel: interaction -> outcome type distribution
  - Export learning data as JSON/CSV for external analysis

- [ ] **Meta-learning: learning about learning**
  - Track which learning mechanisms produce the most improvement
  - If routing adjustments consistently improve quality, increase analysis frequency
  - If prompt refinements rarely help, reduce suggestion frequency
  - Adaptive milestone thresholds (currently fixed at 50/200 interactions)

- [ ] **Privacy-preserving fleet sync** (optional, future)
  - Share anonymized learning insights between Hive instances
  - Federated learning: aggregate model quality rankings across users
  - Differential privacy guarantees on shared data
  - Opt-in only with clear user consent UI

**Milestone:** The system autonomously improves its routing, prompts, and context selection. All changes are transparent, reversible, and user-controlled. Learning metrics show measurable quality improvement over time.

---

## 4. Safety Guardrails

### 4.1 User Control Principles

Every learned behavior must satisfy these invariants:

| Principle | Implementation |
|---|---|
| **Transparency** | All learning decisions are logged to `learning_log` with human-readable descriptions |
| **Reversibility** | Every learned change (tier adjustment, prompt version, preference) can be individually reverted |
| **Auditability** | The Learning panel UI shows what was learned, when, why, and the evidence supporting it |
| **Opt-out** | `LearningService::reset_all()` wipes all learned state. Individual subsystems have their own reset methods |
| **Consent** | Auto-remediation actions are logged but can be disabled entirely via config |

### 4.2 Preventing Runaway Learning

| Risk | Mitigation |
|---|---|
| Tier upgrade spiral (always escalating to expensive models) | `RoutingLearner` has a 4-tier ceiling (enterprise max). EMA smoothing (alpha=0.1) prevents single bad interactions from triggering upgrades. Minimum 10 outcomes required. |
| Prompt drift (evolved prompts degrade over time) | `PromptEvolver` maintains full version history. Quality tracking per version. Rollback to any previous version. A/B testing validates improvements before promotion. |
| Preference hallucination (learning preferences from noise) | `PreferenceModel` uses Bayesian confidence with minimum threshold (0.6). Users can reject any preference. `reset_all()` available. |
| Pattern pollution (bad patterns in library) | Patterns only extracted from responses with quality > 0.8. Frequency tracking ensures popular patterns rise. Decay mechanism for unused patterns. |
| Cost explosion from experimentation | Shadow requests use free-tier models only. A/B tests have fixed sample sizes. Swarm budget cap ($25 default) enforced by `Queen`. |
| Auto-remediation causing harm | All auto-actions are reversible and logged. User can disable auto-remediation entirely. Safety bounds: only disable models < 0.3 quality, only adjust tiers within +-1 level. |

### 4.3 Data Safety

| Concern | Approach |
|---|---|
| Learning DB corruption | SQLite WAL mode for crash safety. Periodic backup to `~/.hive/learning.db.bak` |
| Sensitive data in learning log | `OutcomeRecord` stores model_id, task_type, tier, quality -- not message content. Pattern library stores code structure, not proprietary code. |
| Learning DB growth | `code_patterns` table has frequency-based pruning. `learning_log` capped at configurable limit. `CollectiveMemory::prune()` removes low-relevance entries. |
| Cross-instance data leakage | Fleet sync (Phase 4) is opt-in only. No data leaves the machine without explicit consent. Differential privacy on any shared data. |

### 4.4 Security Considerations

Per the mandatory Security Gate (CLAUDE.md #7):

- Learning DB path is validated and canonicalized (no path traversal)
- `LearningStorage` uses parameterized SQLite queries (no SQL injection)
- `PatternLibrary` extracted patterns are stored as plain text, never executed
- `PromptEvolver` refinements are user-approved before application (Phase 3 adds AI suggestions, but user still confirms)
- Auto-remediation operates only on Hive's own configuration (model selection, tier routing), never on user code or system resources

---

## 5. Competitive Edge

### 5.1 What Makes Hive's Self-Improvement Unique

| Differentiator | Detail |
|---|---|
| **Full-stack learning** | Most AI coding tools learn about user preferences OR routing efficiency. Hive learns about both simultaneously and cross-pollinates insights (outcome quality informs routing which informs prompt evolution which informs context selection). |
| **Transparent learning** | Every learning decision is logged, auditable, and reversible. Users are not subjects of opaque optimization -- they are partners who can inspect, approve, reject, and rollback any learned behavior. |
| **Multi-agent collective memory** | Individual model quality data combines with swarm-level success/failure patterns. The Queen's `record_learnings()` phase means every multi-agent task improves future planning. |
| **Local-first learning** | All learning data stays on the user's machine in a SQLite database. No cloud dependency. Works with local models (Ollama, LMStudio) and cloud providers alike. |
| **Cost-quality optimization** | Not just "use the best model" but "use the best model for this specific task type at the lowest cost." The Pareto frontier approach means users get better results for less money over time. |
| **Pattern library as institutional knowledge** | Over time, the pattern library becomes a curated knowledge base of what works well in the user's codebase, language, and domain. This is transferable institutional knowledge that persists across model changes. |

### 5.2 Competitive Comparison

| Capability | Hive (Current) | Hive (Roadmap) | Cursor | GitHub Copilot | Cline/Aider |
|---|---|---|---|---|---|
| Outcome tracking | Implemented (Jaccard + keyword) | + Sentiment, edit distance, explicit feedback | Limited (accept/reject) | Implicit (accept/dismiss) | None |
| Prompt evolution | Implemented (versioned, rule-based) | + AI-driven, A/B tested | None | None | None |
| Routing learning | Implemented (EMA + tier adjustment) | + Model-level, latency-aware, cost-optimized | Basic (model selection) | Fixed routing | Manual model selection |
| Pattern library | Implemented (6 languages) | + Semantic extraction, feedback loops | None | None | None |
| Self-evaluation | Implemented (comprehensive report) | + Auto-remediation, recommendations | None | Internal analytics only | None |
| Collective intelligence | Implemented (CollectiveMemory + Queen) | + Cross-system bridge, fleet sync | None | None | None |
| User control | Full (log, reject, reset, rollback) | + Dashboard, export, one-click undo | Limited | None | None |
| Local-first | Yes (SQLite) | Yes (SQLite + optional fleet sync) | Cloud-dependent | Cloud-dependent | Local but no learning |

### 5.3 Long-Term Vision

Hive's self-improvement system creates a compounding advantage:

1. **Week 1-4:** Basic telemetry shows which models and task types need attention.
2. **Month 1-2:** Routing automatically steers tasks to the right models. Prompts evolve based on real quality data.
3. **Month 3-6:** The pattern library becomes rich enough to meaningfully improve context. A/B testing validates prompt improvements. Cost drops as the system learns when cheaper models suffice.
4. **Month 6-12:** The collective memory from multi-agent tasks creates institutional knowledge. The system avoids past mistakes and repeats past successes. Users spend less time correcting AI output.
5. **Year 1+:** The learning database becomes the user's most valuable asset -- a personalized, continuously-improving AI configuration tuned to their codebase, preferences, and workflow. Switching to a competitor means losing months of accumulated learning.

This flywheel effect is Hive's deepest competitive moat: **the longer you use Hive, the better it gets at helping you specifically.**

---

## Appendix A: Key Types Reference

```rust
// hive_learn/src/types.rs
pub enum Outcome { Accepted, Corrected, Regenerated, Ignored, Unknown }
pub struct OutcomeRecord { conversation_id, message_id, model_id, task_type, tier, persona, outcome, edit_distance, follow_up_count, quality_score, cost, latency_ms, timestamp }
pub struct RoutingAdjustment { task_type, from_tier, to_tier, confidence, reason }
pub struct UserPreference { key, value, confidence, sample_count, last_updated }
pub struct PromptVersion { persona, version, prompt_text, quality_avg, quality_count, created_at }
pub struct CodePattern { id, language, pattern_type, content, quality_score, frequency, source_file, created_at }
pub struct SelfEvaluationReport { overall_quality, trend, best_model, worst_model, misroute_rate, cost_per_quality_point, weak_areas, correction_rate, regeneration_rate, total_interactions }

// hive_agents/src/collective_memory.rs
pub enum MemoryCategory { SuccessPattern, FailurePattern, ModelInsight, ConflictResolution, CodePattern, UserPreference, General }
pub struct MemoryEntry { id, category, content, tags, source_run_id, source_team_id, relevance_score, created_at, last_accessed, access_count }

// hive_ai/src/routing/model_router.rs
pub trait TierAdjuster: Send + Sync { fn adjust_tier(&self, task_type: &str, classified_tier: &str) -> Option<String>; }
```

## Appendix B: Database Schema

```sql
-- hive_learn/src/storage.rs (LearningStorage)
CREATE TABLE learning_outcomes (id INTEGER PRIMARY KEY, conversation_id TEXT, message_id TEXT UNIQUE, model_id TEXT, task_type TEXT, tier TEXT, persona TEXT, outcome TEXT, edit_distance REAL, follow_up_count INTEGER, quality_score REAL, cost REAL, latency_ms INTEGER, timestamp TEXT);
CREATE TABLE routing_history (id INTEGER PRIMARY KEY, task_type TEXT, classified_tier TEXT, actual_tier_needed TEXT, model_id TEXT, quality_score REAL, cost REAL, timestamp TEXT);
CREATE TABLE user_preferences (key TEXT PRIMARY KEY, value TEXT, confidence REAL, sample_count INTEGER, last_updated TEXT);
CREATE TABLE prompt_versions (persona TEXT, version INTEGER, prompt_text TEXT, quality_avg REAL, quality_count INTEGER, created_at TEXT, PRIMARY KEY (persona, version));
CREATE TABLE code_patterns (id INTEGER PRIMARY KEY, language TEXT, pattern_type TEXT, content TEXT, quality_score REAL, frequency INTEGER, source_file TEXT, created_at TEXT);
CREATE TABLE learning_log (id INTEGER PRIMARY KEY, event_type TEXT, description TEXT, details TEXT, reversible INTEGER, timestamp TEXT);

-- Additional tables for routing adjustments and routing history queries
CREATE TABLE routing_adjustments (id INTEGER PRIMARY KEY, task_type TEXT, from_tier TEXT, to_tier TEXT, confidence REAL, reason TEXT, timestamp TEXT);

-- hive_agents/src/collective_memory.rs (CollectiveMemory)
CREATE TABLE memories (id INTEGER PRIMARY KEY, category TEXT, content TEXT, tags TEXT, source_run_id TEXT, source_team_id TEXT, relevance_score REAL, created_at TEXT, last_accessed TEXT, access_count INTEGER);
```
