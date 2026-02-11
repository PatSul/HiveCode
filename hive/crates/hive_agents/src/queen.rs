//! Queen Meta-Coordinator -- swarm orchestration across multiple teams.
//!
//! The Queen sits above HiveMind and Coordinator. She decomposes a high-level
//! goal into team objectives, dispatches each team using the appropriate
//! orchestration mode (HiveMind, Coordinator, NativeProvider, or SingleShot),
//! enforces budget and time limits, shares cross-team insights, synthesizes
//! a final output, and records learnings to collective memory.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use hive_ai::types::{ChatMessage, ChatRequest, ChatResponse, MessageRole, ModelTier};

use crate::collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry};
use crate::coordinator::{Coordinator, CoordinatorConfig, CoordinatorResult};
use crate::hivemind::{default_model_for_tier, AiExecutor, HiveMind, HiveMindConfig, OrchestrationResult};
use crate::swarm::{
    InnerResult, OrchestrationMode, SwarmConfig, SwarmPlan, SwarmResult, SwarmStatus,
    SwarmStatusCallback, TeamObjective, TeamResult, TeamStatus,
};

// ---------------------------------------------------------------------------
// ArcExecutor -- bridge to pass Arc<E> where E: AiExecutor is expected
// ---------------------------------------------------------------------------

/// Thin wrapper that lets us pass `Arc<E>` to APIs that consume an `E: AiExecutor`
/// by value (HiveMind::new, Coordinator::new).
struct ArcExecutor<E: AiExecutor>(Arc<E>);

impl<E: AiExecutor> AiExecutor for ArcExecutor<E> {
    async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
        self.0.execute(request).await
    }
}

// ---------------------------------------------------------------------------
// Queen
// ---------------------------------------------------------------------------

/// The Queen meta-coordinator for swarm orchestration.
///
/// Given a high-level goal, the Queen:
/// 1. Plans -- decomposes the goal into team objectives with dependency ordering.
/// 2. Executes -- dispatches teams in dependency waves, enforcing budget/time limits.
/// 3. Synthesizes -- merges team outputs into a coherent final result.
/// 4. Learns -- records success/failure patterns to collective memory.
pub struct Queen<E: AiExecutor> {
    config: SwarmConfig,
    executor: Arc<E>,
    memory: Option<Arc<CollectiveMemory>>,
    status_callback: Option<SwarmStatusCallback>,
    /// Accumulated cost stored as the bit-pattern of an f64 so we can use
    /// atomic operations without a mutex.
    accumulated_cost: AtomicU64,
}

impl<E: AiExecutor + 'static> Queen<E> {
    /// Create a new Queen with the given config and shared executor.
    pub fn new(config: SwarmConfig, executor: Arc<E>) -> Self {
        Self {
            config,
            executor,
            memory: None,
            status_callback: None,
            accumulated_cost: AtomicU64::new(0f64.to_bits()),
        }
    }

    /// Attach collective memory for learning across runs.
    pub fn with_memory(mut self, memory: Arc<CollectiveMemory>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Register a callback for swarm-level status updates.
    pub fn with_status_callback(mut self, cb: SwarmStatusCallback) -> Self {
        self.status_callback = Some(cb);
        self
    }

    // -----------------------------------------------------------------------
    // Phase 1: Planning
    // -----------------------------------------------------------------------

    /// Decompose a high-level goal into a validated `SwarmPlan`.
    ///
    /// Queries collective memory for relevant past patterns, builds a planning
    /// prompt, sends it to the queen model, and parses the response into a set
    /// of team objectives with dependency ordering.
    pub async fn plan(&self, goal: &str) -> Result<SwarmPlan, String> {
        self.emit_status(SwarmStatus::Planning, "Decomposing goal into team objectives");

        // Gather memory context if available.
        let memory_context = self.gather_memory_context(goal);

        // Build the planning prompt.
        let prompt = format!(
            "You are a Queen coordinator decomposing a goal into team objectives.\n\
             Given the goal, create a JSON array of team objectives. Each team should have:\n\
             - \"id\": unique string id like \"team-1\"\n\
             - \"name\": short descriptive name\n\
             - \"description\": detailed description of what this team should do\n\
             - \"dependencies\": array of team ids that must complete first (empty for independent teams)\n\
             - \"orchestration_mode\": one of \"hivemind\", \"coordinator\", \"native_provider\", \"single_shot\"\n\
             - \"scope_paths\": array of relevant file/directory paths\n\
             - \"priority\": 0-9 (0 = highest priority)\n\n\
             Goal: {goal}\n\
             {memory_context}\n\n\
             Respond with ONLY a JSON array."
        );

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: prompt,
                timestamp: chrono::Utc::now(),
            }],
            model: self.config.queen_model.clone(),
            max_tokens: 4096,
            temperature: Some(0.3),
            system_prompt: Some(
                "You are a swarm orchestration planner. Produce valid JSON only.".into(),
            ),
        };

        let response = self.executor.execute(&request).await?;
        let plan = self.parse_plan_response(&response.content)?;

        // Track the cost of the planning call.
        self.add_cost(estimate_cost(&self.config.queen_model, &response));

        plan.validate()?;
        Ok(plan)
    }

    /// Parse the AI response into a `SwarmPlan`.
    ///
    /// Extracts the JSON array from the response (handling markdown fences
    /// and surrounding text), deserializes into `Vec<TeamObjective>`, and
    /// wraps in a `SwarmPlan`.
    fn parse_plan_response(&self, response: &str) -> Result<SwarmPlan, String> {
        let content = response.trim();

        // Find the first '[' and last ']' to extract the JSON array.
        let start = content
            .find('[')
            .ok_or_else(|| "No JSON array found in planning response".to_string())?;
        let end = content
            .rfind(']')
            .ok_or_else(|| "No closing bracket found in planning response".to_string())?;

        if end <= start {
            return Err("Malformed JSON array in planning response".into());
        }

        let json_str = &content[start..=end];
        let teams: Vec<TeamObjective> = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse team objectives: {e}"))?;

        if teams.is_empty() {
            return Err("Planning produced zero team objectives".into());
        }

        Ok(SwarmPlan { teams })
    }

    /// Query collective memory for patterns relevant to the current goal.
    ///
    /// Searches per-term (not the full phrase) so that partial matches work
    /// against the SQLite LIKE operator.
    fn gather_memory_context(&self, goal: &str) -> String {
        let memory = match &self.memory {
            Some(m) => m,
            None => return String::new(),
        };

        // Extract key terms from the goal for memory search.
        let terms: Vec<&str> = goal
            .split_whitespace()
            .filter(|w| w.len() >= 4)
            .take(5)
            .collect();

        if terms.is_empty() {
            return String::new();
        }

        let mut context_parts: Vec<String> = Vec::new();
        let mut seen_ids: HashSet<i64> = HashSet::new();

        // Search each term individually so LIKE matching works on partial words.
        for term in &terms {
            // Success patterns.
            if let Ok(entries) =
                memory.recall(term, Some(MemoryCategory::SuccessPattern), None, 3)
            {
                for entry in entries {
                    if seen_ids.insert(entry.id) {
                        context_parts.push(format!("- Success pattern: {}", entry.content));
                    }
                }
            }

            // Failure patterns.
            if let Ok(entries) =
                memory.recall(term, Some(MemoryCategory::FailurePattern), None, 2)
            {
                for entry in entries {
                    if seen_ids.insert(entry.id) {
                        context_parts.push(format!("- Failure to avoid: {}", entry.content));
                    }
                }
            }

            // Model insights.
            if let Ok(entries) =
                memory.recall(term, Some(MemoryCategory::ModelInsight), None, 2)
            {
                for entry in entries {
                    if seen_ids.insert(entry.id) {
                        context_parts.push(format!("- Model insight: {}", entry.content));
                    }
                }
            }
        }

        if context_parts.is_empty() {
            return String::new();
        }

        format!(
            "\nRelevant past learnings:\n{}",
            context_parts.join("\n")
        )
    }

    // -----------------------------------------------------------------------
    // Phase 2: Execution
    // -----------------------------------------------------------------------

    /// Execute the full swarm pipeline: plan, execute teams, synthesize, learn.
    pub async fn execute(&self, goal: &str) -> Result<SwarmResult, String> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let overall_start = Instant::now();

        // Phase 1: Plan.
        let plan = self.plan(goal).await?;

        // Phase 2: Execute teams in dependency waves.
        self.emit_status(SwarmStatus::Executing, "Executing team objectives");
        let team_results = self.execute_plan(&plan).await?;

        // Phase 3: Synthesize outputs.
        self.emit_status(SwarmStatus::Synthesizing, "Synthesizing team outputs");
        let synthesized = self.synthesize(&plan, &team_results).await;

        // Phase 4: Record learnings.
        let learnings_recorded = self.record_learnings(&plan, &team_results);

        // Determine final status.
        let completed = team_results
            .iter()
            .filter(|r| r.status == TeamStatus::Completed)
            .count();
        let failed = team_results
            .iter()
            .filter(|r| r.status == TeamStatus::Failed)
            .count();

        let status = if failed == 0 {
            SwarmStatus::Complete
        } else if completed > 0 {
            SwarmStatus::PartialSuccess
        } else {
            SwarmStatus::Failed
        };

        let total_cost = self.current_cost();
        let total_duration_ms = overall_start.elapsed().as_millis() as u64;

        self.emit_status(status, "Swarm execution finished");

        Ok(SwarmResult {
            run_id,
            goal: goal.to_string(),
            status,
            plan,
            team_results,
            synthesized_output: synthesized,
            total_cost,
            total_duration_ms,
            learnings_recorded,
        })
    }

    /// Execute a swarm plan in dependency-ordered waves.
    ///
    /// Teams within a wave are independent (all dependencies satisfied) and
    /// are executed sequentially within the wave. Across waves, dependency
    /// ordering is enforced. Budget and time limits are checked before each
    /// wave. Failed teams cause their dependents to be skipped.
    async fn execute_plan(&self, plan: &SwarmPlan) -> Result<Vec<TeamResult>, String> {
        let start = Instant::now();
        let mut results: Vec<TeamResult> = Vec::new();
        let mut completed_ids: HashSet<String> = HashSet::new();
        let mut failed_ids: HashSet<String> = HashSet::new();
        let mut remaining: Vec<TeamObjective> = plan.teams.clone();

        while !remaining.is_empty() {
            // Time enforcement.
            if start.elapsed().as_secs() >= self.config.total_time_limit_secs {
                self.emit_status(SwarmStatus::TimedOut, "Swarm time limit reached");
                // Mark all remaining teams as skipped.
                for team in &remaining {
                    results.push(TeamResult {
                        team_id: team.id.clone(),
                        team_name: team.name.clone(),
                        status: TeamStatus::Skipped,
                        inner: None,
                        cost: 0.0,
                        duration_ms: 0,
                        insights: vec![],
                        error: Some("Swarm time limit reached".into()),
                    });
                }
                break;
            }

            // Budget enforcement.
            if self.current_cost() >= self.config.total_cost_limit_usd {
                self.emit_status(SwarmStatus::BudgetExceeded, "Swarm budget exceeded");
                for team in &remaining {
                    results.push(TeamResult {
                        team_id: team.id.clone(),
                        team_name: team.name.clone(),
                        status: TeamStatus::Skipped,
                        inner: None,
                        cost: 0.0,
                        duration_ms: 0,
                        insights: vec![],
                        error: Some("Swarm budget exceeded".into()),
                    });
                }
                break;
            }

            // Partition into ready teams (all deps satisfied, no failed deps)
            // and not-ready teams.
            let (ready, mut still_waiting): (Vec<TeamObjective>, Vec<TeamObjective>) =
                remaining.into_iter().partition(|t| {
                    t.dependencies.iter().all(|d| completed_ids.contains(d))
                });

            // Check for teams whose dependencies have failed -- mark them skipped.
            let mut skipped_this_wave: Vec<TeamObjective> = Vec::new();
            still_waiting.retain(|t| {
                let has_failed_dep = t.dependencies.iter().any(|d| failed_ids.contains(d));
                if has_failed_dep {
                    skipped_this_wave.push(t.clone());
                    false
                } else {
                    true
                }
            });
            for team in &skipped_this_wave {
                failed_ids.insert(team.id.clone());
                results.push(TeamResult {
                    team_id: team.id.clone(),
                    team_name: team.name.clone(),
                    status: TeamStatus::Skipped,
                    inner: None,
                    cost: 0.0,
                    duration_ms: 0,
                    insights: vec![],
                    error: Some("Dependency failed".into()),
                });
            }

            remaining = still_waiting;

            if ready.is_empty() {
                if remaining.is_empty() {
                    break;
                }
                // No ready teams but some remain -- unresolvable deps (should not
                // happen after validation, but be safe).
                for team in &remaining {
                    results.push(TeamResult {
                        team_id: team.id.clone(),
                        team_name: team.name.clone(),
                        status: TeamStatus::Skipped,
                        inner: None,
                        cost: 0.0,
                        duration_ms: 0,
                        insights: vec![],
                        error: Some("Unresolvable dependency".into()),
                    });
                }
                break;
            }

            // Collect cross-team insights from completed teams for context sharing.
            let prior_results: Vec<TeamResult> = results
                .iter()
                .filter(|r| r.status == TeamStatus::Completed)
                .cloned()
                .collect();

            // Limit batch size to max_parallel_teams.
            let batch_size = ready.len().min(self.config.max_parallel_teams);
            let batch: Vec<TeamObjective> = ready.into_iter().take(batch_size).collect();

            // Emit cross-team sync if sharing insights.
            if !prior_results.is_empty() && !batch.is_empty() {
                self.emit_status(
                    SwarmStatus::CrossTeamSync,
                    &format!(
                        "Sharing {} insights with {} teams",
                        prior_results.len(),
                        batch.len()
                    ),
                );
            }

            // Execute teams in this wave sequentially.
            for objective in &batch {
                self.emit_status(
                    SwarmStatus::TeamStarted,
                    &format!("Starting team '{}' ({})", objective.name, objective.id),
                );

                let result = self.execute_team(objective, &prior_results).await;

                match result.status {
                    TeamStatus::Completed => {
                        completed_ids.insert(result.team_id.clone());
                        self.emit_status(
                            SwarmStatus::TeamCompleted,
                            &format!("Team '{}' completed", objective.name),
                        );
                    }
                    TeamStatus::Failed => {
                        failed_ids.insert(result.team_id.clone());
                        self.emit_status(
                            SwarmStatus::TeamFailed,
                            &format!(
                                "Team '{}' failed: {}",
                                objective.name,
                                result.error.as_deref().unwrap_or("unknown error")
                            ),
                        );
                    }
                    _ => {}
                }

                self.add_cost(result.cost);
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Execute a single team objective, choosing the orchestration mode.
    ///
    /// Builds enriched context from prior team results and dispatches to
    /// the appropriate orchestrator.
    async fn execute_team(
        &self,
        objective: &TeamObjective,
        prior_results: &[TeamResult],
    ) -> TeamResult {
        let team_start = Instant::now();

        // Build enriched context from prior team results.
        let cross_team_context = self.build_cross_team_context(prior_results);
        let enriched_description = if cross_team_context.is_empty() {
            objective.description.clone()
        } else {
            format!(
                "{}\n\nContext from prior teams:\n{}",
                objective.description, cross_team_context
            )
        };

        let result = match objective.orchestration_mode {
            OrchestrationMode::HiveMind => {
                self.execute_team_hivemind(objective, &enriched_description)
                    .await
            }
            OrchestrationMode::Coordinator => {
                self.execute_team_coordinator(objective, &enriched_description)
                    .await
            }
            OrchestrationMode::NativeProvider => {
                self.execute_team_native(objective, &enriched_description)
                    .await
            }
            OrchestrationMode::SingleShot => {
                self.execute_team_singleshot(objective, &enriched_description)
                    .await
            }
        };

        let duration_ms = team_start.elapsed().as_millis() as u64;

        match result {
            Ok((inner, cost, insights)) => TeamResult {
                team_id: objective.id.clone(),
                team_name: objective.name.clone(),
                status: TeamStatus::Completed,
                inner: Some(inner),
                cost,
                duration_ms,
                insights,
                error: None,
            },
            Err(err) => TeamResult {
                team_id: objective.id.clone(),
                team_name: objective.name.clone(),
                status: TeamStatus::Failed,
                inner: None,
                cost: 0.0,
                duration_ms,
                insights: vec![],
                error: Some(err),
            },
        }
    }

    /// Execute a team using the full HiveMind multi-agent pipeline.
    async fn execute_team_hivemind(
        &self,
        objective: &TeamObjective,
        description: &str,
    ) -> Result<(InnerResult, f64, Vec<String>), String> {
        let model = objective
            .preferred_model
            .clone()
            .unwrap_or_else(|| default_model_for_tier(ModelTier::Mid));

        let config = HiveMindConfig {
            max_agents: 5,
            cost_limit_usd: self.config.per_team_cost_limit_usd,
            time_limit_secs: self.config.per_team_time_limit_secs,
            auto_scale: true,
            consensus_threshold: 0.7,
            model_overrides: {
                let mut m = std::collections::HashMap::new();
                // Use the preferred model for the architect role (most important).
                m.insert(
                    crate::hivemind::AgentRole::Architect,
                    model,
                );
                m
            },
        };

        let arc_exec = ArcExecutor(Arc::clone(&self.executor));
        let hm = HiveMind::new(config, arc_exec);
        let result = hm.execute(description).await;

        let cost = result.total_cost;
        let insights = extract_insights_from_orchestration(&result);

        Ok((InnerResult::HiveMind { result }, cost, insights))
    }

    /// Execute a team using the Coordinator with dependency-ordered task dispatch.
    async fn execute_team_coordinator(
        &self,
        objective: &TeamObjective,
        description: &str,
    ) -> Result<(InnerResult, f64, Vec<String>), String> {
        let config = CoordinatorConfig {
            max_parallel: 4,
            cost_limit: self.config.per_team_cost_limit_usd,
            time_limit_secs: self.config.per_team_time_limit_secs,
            model_for_coordination: objective
                .preferred_model
                .clone()
                .unwrap_or_else(|| default_model_for_tier(ModelTier::Mid)),
        };

        // Build a simple TaskPlan from the objective description.
        // The coordinator's plan_from_spec uses AI to decompose, but here we
        // construct a minimal plan that the coordinator can execute directly.
        let plan = build_coordinator_plan_from_objective(objective, description);

        let arc_exec = ArcExecutor(Arc::clone(&self.executor));
        let coordinator = Coordinator::new(config, arc_exec);
        let result = coordinator.execute_plan(&plan).await;

        let cost = result.total_cost;
        let insights = extract_insights_from_coordinator(&result);

        Ok((InnerResult::Coordinator { result }, cost, insights))
    }

    /// Execute a team using a single rich AI call with a detailed system prompt.
    async fn execute_team_native(
        &self,
        objective: &TeamObjective,
        description: &str,
    ) -> Result<(InnerResult, f64, Vec<String>), String> {
        let model = objective
            .preferred_model
            .clone()
            .unwrap_or_else(|| self.config.queen_model.clone());

        let system_prompt = format!(
            "You are a specialized team working on: {}\n\n\
             Your team name is '{}'. Scope paths: {}\n\n\
             Provide a thorough, complete solution. Include:\n\
             1. Analysis of the problem\n\
             2. Detailed implementation or plan\n\
             3. Key decisions and trade-offs\n\
             4. Potential risks and mitigations",
            objective.name,
            objective.name,
            if objective.scope_paths.is_empty() {
                "(entire project)".to_string()
            } else {
                objective.scope_paths.join(", ")
            }
        );

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: description.to_string(),
                timestamp: chrono::Utc::now(),
            }],
            model: model.clone(),
            max_tokens: 4096,
            temperature: Some(0.3),
            system_prompt: Some(system_prompt),
        };

        let response = self.executor.execute(&request).await?;
        let cost = estimate_cost(&model, &response);
        let insights = extract_insights_from_text(&response.content);

        Ok((
            InnerResult::Native {
                content: response.content,
                model,
            },
            cost,
            insights,
        ))
    }

    /// Execute a team using a single AI call with enriched context.
    async fn execute_team_singleshot(
        &self,
        objective: &TeamObjective,
        description: &str,
    ) -> Result<(InnerResult, f64, Vec<String>), String> {
        let model = objective
            .preferred_model
            .clone()
            .unwrap_or_else(|| default_model_for_tier(ModelTier::Budget));

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: description.to_string(),
                timestamp: chrono::Utc::now(),
            }],
            model: model.clone(),
            max_tokens: 4096,
            temperature: Some(0.3),
            system_prompt: Some(format!(
                "You are team '{}'. Complete the following objective thoroughly and concisely.",
                objective.name
            )),
        };

        let response = self.executor.execute(&request).await?;
        let cost = estimate_cost(&model, &response);
        let insights = extract_insights_from_text(&response.content);

        Ok((
            InnerResult::SingleShot {
                content: response.content,
                model,
            },
            cost,
            insights,
        ))
    }

    /// Build a context string from completed prior team results.
    fn build_cross_team_context(&self, prior_results: &[TeamResult]) -> String {
        if prior_results.is_empty() {
            return String::new();
        }

        let mut sections: Vec<String> = Vec::new();

        for result in prior_results {
            let output_summary = match &result.inner {
                Some(InnerResult::HiveMind { result: r }) => {
                    truncate_str(&r.synthesized_output, 500)
                }
                Some(InnerResult::Coordinator { result: r }) => {
                    let summaries: Vec<String> = r
                        .results
                        .iter()
                        .filter(|t| t.success)
                        .map(|t| truncate_str(&t.output, 200))
                        .collect();
                    summaries.join("\n")
                }
                Some(InnerResult::Native { content, .. })
                | Some(InnerResult::SingleShot { content, .. }) => truncate_str(content, 500),
                None => continue,
            };

            if !output_summary.is_empty() {
                sections.push(format!(
                    "[Team '{}'] {}\nInsights: {}",
                    result.team_name,
                    output_summary,
                    if result.insights.is_empty() {
                        "none".to_string()
                    } else {
                        result.insights.join("; ")
                    }
                ));
            }
        }

        sections.join("\n\n")
    }

    // -----------------------------------------------------------------------
    // Phase 3: Synthesis
    // -----------------------------------------------------------------------

    /// Synthesize all team outputs into a coherent final result.
    async fn synthesize(&self, plan: &SwarmPlan, results: &[TeamResult]) -> String {
        if results.is_empty() {
            return "No team outputs to synthesize.".into();
        }

        // Build a synthesis prompt with all team outputs.
        let mut team_sections: Vec<String> = Vec::new();

        for team in &plan.teams {
            let result = results.iter().find(|r| r.team_id == team.id);
            match result {
                Some(r) if r.status == TeamStatus::Completed => {
                    let content = match &r.inner {
                        Some(InnerResult::HiveMind { result: hr }) => {
                            hr.synthesized_output.clone()
                        }
                        Some(InnerResult::Coordinator { result: cr }) => {
                            cr.results
                                .iter()
                                .filter(|t| t.success)
                                .map(|t| t.output.clone())
                                .collect::<Vec<_>>()
                                .join("\n\n")
                        }
                        Some(InnerResult::Native { content, .. })
                        | Some(InnerResult::SingleShot { content, .. }) => content.clone(),
                        None => "(no output)".into(),
                    };
                    team_sections.push(format!(
                        "## Team: {} ({})\nMode: {:?}\n\n{}",
                        team.name, team.id, team.orchestration_mode, content
                    ));
                }
                Some(r) if r.status == TeamStatus::Failed => {
                    team_sections.push(format!(
                        "## Team: {} ({}) [FAILED]\nError: {}",
                        team.name,
                        team.id,
                        r.error.as_deref().unwrap_or("unknown")
                    ));
                }
                Some(r) if r.status == TeamStatus::Skipped => {
                    team_sections.push(format!(
                        "## Team: {} ({}) [SKIPPED]\nReason: {}",
                        team.name,
                        team.id,
                        r.error.as_deref().unwrap_or("dependency failed")
                    ));
                }
                _ => {}
            }
        }

        let all_outputs = team_sections.join("\n\n---\n\n");

        let synthesis_prompt = format!(
            "Synthesize the following team outputs into a coherent summary.\n\
             Combine key findings, resolve any conflicts, and produce a unified result.\n\n\
             {all_outputs}"
        );

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: synthesis_prompt,
                timestamp: chrono::Utc::now(),
            }],
            model: self.config.queen_model.clone(),
            max_tokens: 4096,
            temperature: Some(0.3),
            system_prompt: Some(
                "You are a synthesis agent. Merge multiple team outputs into a single, \
                 coherent, well-structured result. Preserve important details from each team."
                    .into(),
            ),
        };

        match self.executor.execute(&request).await {
            Ok(response) => {
                self.add_cost(estimate_cost(&self.config.queen_model, &response));
                response.content
            }
            Err(err) => {
                // Fall back to simple concatenation if synthesis AI call fails.
                format!(
                    "Synthesis failed ({err}). Raw team outputs:\n\n{all_outputs}"
                )
            }
        }
    }

    // -----------------------------------------------------------------------
    // Phase 4: Learning
    // -----------------------------------------------------------------------

    /// Record success and failure patterns to collective memory.
    ///
    /// Returns the number of memory entries created.
    fn record_learnings(&self, plan: &SwarmPlan, results: &[TeamResult]) -> usize {
        let memory = match &self.memory {
            Some(m) => m,
            None => return 0,
        };

        let mut count = 0;

        for result in results {
            let team = plan.teams.iter().find(|t| t.id == result.team_id);
            let team_name = team
                .map(|t| t.name.as_str())
                .unwrap_or(&result.team_name);

            match result.status {
                TeamStatus::Completed => {
                    let mut entry = MemoryEntry::new(
                        MemoryCategory::SuccessPattern,
                        format!(
                            "Team '{}' ({:?}) completed successfully in {}ms with cost ${:.4}",
                            team_name,
                            team.map(|t| t.orchestration_mode)
                                .unwrap_or(OrchestrationMode::SingleShot),
                            result.duration_ms,
                            result.cost,
                        ),
                    );
                    entry.source_team_id = Some(result.team_id.clone());
                    entry.tags = vec![
                        "swarm".to_string(),
                        "success".to_string(),
                        team_name.to_string(),
                    ];

                    if memory.remember(&entry).is_ok() {
                        count += 1;
                    }
                }
                TeamStatus::Failed => {
                    let mut entry = MemoryEntry::new(
                        MemoryCategory::FailurePattern,
                        format!(
                            "Team '{}' ({:?}) failed: {}",
                            team_name,
                            team.map(|t| t.orchestration_mode)
                                .unwrap_or(OrchestrationMode::SingleShot),
                            result.error.as_deref().unwrap_or("unknown error"),
                        ),
                    );
                    entry.source_team_id = Some(result.team_id.clone());
                    entry.tags = vec![
                        "swarm".to_string(),
                        "failure".to_string(),
                        team_name.to_string(),
                    ];

                    if memory.remember(&entry).is_ok() {
                        count += 1;
                    }
                }
                _ => {}
            }

            // Store any extracted insights as model insights.
            for insight in &result.insights {
                if insight.trim().is_empty() {
                    continue;
                }
                let mut entry = MemoryEntry::new(
                    MemoryCategory::ModelInsight,
                    format!("Insight from team '{}': {}", team_name, insight),
                );
                entry.source_team_id = Some(result.team_id.clone());
                entry.tags = vec![
                    "swarm".to_string(),
                    "insight".to_string(),
                    team_name.to_string(),
                ];

                if memory.remember(&entry).is_ok() {
                    count += 1;
                }
            }
        }

        count
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Emit a status update to the registered callback.
    fn emit_status(&self, status: SwarmStatus, detail: &str) {
        if let Some(ref cb) = self.status_callback {
            cb(status, detail);
        }
    }

    /// Atomically add to the accumulated cost.
    fn add_cost(&self, cost: f64) {
        loop {
            let old_bits = self.accumulated_cost.load(Ordering::Relaxed);
            let old_val = f64::from_bits(old_bits);
            let new_val = old_val + cost;
            let new_bits = new_val.to_bits();

            if self
                .accumulated_cost
                .compare_exchange_weak(old_bits, new_bits, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }

    /// Read the current accumulated cost.
    fn current_cost(&self) -> f64 {
        f64::from_bits(self.accumulated_cost.load(Ordering::SeqCst))
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Build a minimal `TaskPlan` from a team objective for the Coordinator.
///
/// Creates a three-phase plan: investigate, implement, verify.
fn build_coordinator_plan_from_objective(
    objective: &TeamObjective,
    description: &str,
) -> crate::coordinator::TaskPlan {
    use crate::coordinator::PlannedTask;
    use crate::personas::PersonaKind;

    crate::coordinator::TaskPlan {
        tasks: vec![
            PlannedTask {
                id: format!("{}-investigate", objective.id),
                description: format!("Investigate: {}", description),
                persona: PersonaKind::Investigate,
                dependencies: vec![],
                priority: 1,
            },
            PlannedTask {
                id: format!("{}-implement", objective.id),
                description: format!("Implement: {}", description),
                persona: PersonaKind::Implement,
                dependencies: vec![format!("{}-investigate", objective.id)],
                priority: 2,
            },
            PlannedTask {
                id: format!("{}-verify", objective.id),
                description: format!("Verify: {}", description),
                persona: PersonaKind::Verify,
                dependencies: vec![format!("{}-implement", objective.id)],
                priority: 3,
            },
        ],
    }
}

/// Estimate cost from a response based on model name and token usage.
fn estimate_cost(model: &str, response: &ChatResponse) -> f64 {
    let (input_rate, output_rate) = match model {
        m if m.contains("opus") => (15.0, 75.0),
        m if m.contains("sonnet") => (3.0, 15.0),
        m if m.contains("haiku") => (0.80, 4.0),
        m if m.contains("gpt-4o") => (2.50, 10.0),
        m if m.contains("gpt-4") => (10.0, 30.0),
        m if m.contains("gpt-3.5") => (0.50, 1.50),
        _ => (0.0, 0.0),
    };

    let input_cost = (response.usage.prompt_tokens as f64 / 1_000_000.0) * input_rate;
    let output_cost = (response.usage.completion_tokens as f64 / 1_000_000.0) * output_rate;
    input_cost + output_cost
}

/// Extract insights from an OrchestrationResult.
fn extract_insights_from_orchestration(result: &OrchestrationResult) -> Vec<String> {
    let mut insights = Vec::new();

    if result.total_cost > 0.0 {
        insights.push(format!(
            "Total cost: ${:.4}, {} agents succeeded, {} failed",
            result.total_cost,
            result.successful_agents(),
            result.failed_agents(),
        ));
    }

    if let Some(score) = result.consensus_score {
        if score < 0.5 {
            insights.push(format!("Low consensus ({score:.2}) -- agents disagreed significantly"));
        } else if score > 0.9 {
            insights.push(format!("High consensus ({score:.2}) -- strong agreement"));
        }
    }

    insights
}

/// Extract insights from a CoordinatorResult.
fn extract_insights_from_coordinator(result: &CoordinatorResult) -> Vec<String> {
    let mut insights = Vec::new();

    let successful = result.successful_tasks();
    let failed = result.failed_tasks();

    if successful > 0 || failed > 0 {
        insights.push(format!(
            "Coordinator: {successful} tasks succeeded, {failed} failed, cost ${:.4}",
            result.total_cost,
        ));
    }

    insights
}

/// Extract high-level insights from raw text output.
///
/// Looks for sentences containing key indicator phrases.
fn extract_insights_from_text(text: &str) -> Vec<String> {
    let indicators = [
        "important",
        "critical",
        "key finding",
        "recommendation",
        "warning",
        "risk",
        "trade-off",
        "tradeoff",
        "decision",
        "lesson learned",
    ];

    let mut insights = Vec::new();

    // Split into sentences (rough heuristic: split on ". " or ".\n").
    for sentence in text.split(|c| c == '.' || c == '\n') {
        let trimmed = sentence.trim();
        if trimmed.len() < 10 || trimmed.len() > 500 {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if indicators.iter().any(|ind| lower.contains(ind)) {
            insights.push(trimmed.to_string());
            if insights.len() >= 5 {
                break;
            }
        }
    }

    insights
}

/// Truncate a string to at most `max_len` characters, appending "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = s
            .char_indices()
            .take_while(|(i, _)| *i < max_len.saturating_sub(3))
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}...", &s[..boundary])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collective_memory::CollectiveMemory;
    use crate::swarm::{SwarmConfig, SwarmPlan, TeamObjective};
    use hive_ai::types::{ChatResponse, FinishReason, TokenUsage};
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    // -- Mock Executor -------------------------------------------------------

    struct MockExecutor {
        response: String,
        call_count: AtomicUsize,
    }

    impl MockExecutor {
        fn new(response: &str) -> Self {
            Self {
                response: response.into(),
                call_count: AtomicUsize::new(0),
            }
        }

        #[allow(dead_code)]
        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl AiExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(ChatResponse {
                content: self.response.clone(),
                model: "mock".into(),
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                thinking: None,
            })
        }
    }

    // -- Construction --------------------------------------------------------

    #[test]
    fn queen_new_creates_with_defaults() {
        let executor = Arc::new(MockExecutor::new("test"));
        let queen = Queen::new(SwarmConfig::default(), executor);
        assert_eq!(queen.config.max_parallel_teams, 3);
        assert_eq!(queen.config.total_cost_limit_usd, 25.0);
        assert!(queen.memory.is_none());
        assert!(queen.status_callback.is_none());
        assert!((queen.current_cost() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn queen_with_memory() {
        let executor = Arc::new(MockExecutor::new("test"));
        let memory = Arc::new(CollectiveMemory::in_memory().unwrap());
        let queen = Queen::new(SwarmConfig::default(), executor).with_memory(memory);
        assert!(queen.memory.is_some());
    }

    #[test]
    fn queen_with_status_callback() {
        let executor = Arc::new(MockExecutor::new("test"));
        let cb: SwarmStatusCallback = Arc::new(|_status, _detail| {});
        let queen = Queen::new(SwarmConfig::default(), executor).with_status_callback(cb);
        assert!(queen.status_callback.is_some());
    }

    // -- Plan parsing --------------------------------------------------------

    #[tokio::test]
    async fn plan_parses_valid_json_array() {
        let json_response = r#"[
            {
                "id": "team-1",
                "name": "Research",
                "description": "Research the codebase",
                "dependencies": [],
                "orchestration_mode": "single_shot",
                "scope_paths": ["src/"],
                "priority": 0
            },
            {
                "id": "team-2",
                "name": "Implement",
                "description": "Implement the feature",
                "dependencies": ["team-1"],
                "orchestration_mode": "hive_mind",
                "scope_paths": ["src/lib.rs"],
                "priority": 3
            }
        ]"#;

        let executor = Arc::new(MockExecutor::new(json_response));
        let queen = Queen::new(SwarmConfig::default(), executor);
        let plan = queen.plan("Build a caching layer").await.unwrap();

        assert_eq!(plan.teams.len(), 2);
        assert_eq!(plan.teams[0].id, "team-1");
        assert_eq!(plan.teams[0].name, "Research");
        assert_eq!(
            plan.teams[0].orchestration_mode,
            OrchestrationMode::SingleShot
        );
        assert_eq!(plan.teams[1].dependencies, vec!["team-1"]);
        assert_eq!(
            plan.teams[1].orchestration_mode,
            OrchestrationMode::HiveMind
        );
    }

    #[tokio::test]
    async fn plan_parses_json_with_surrounding_text() {
        let response = r#"Here is the plan:
        [
            {
                "id": "team-1",
                "name": "Analyze",
                "description": "Analyze requirements",
                "dependencies": [],
                "orchestration_mode": "single_shot",
                "scope_paths": [],
                "priority": 1
            }
        ]
        That should work well."#;

        let executor = Arc::new(MockExecutor::new(response));
        let queen = Queen::new(SwarmConfig::default(), executor);
        let plan = queen.plan("Analyze the requirements").await.unwrap();
        assert_eq!(plan.teams.len(), 1);
        assert_eq!(plan.teams[0].id, "team-1");
    }

    #[tokio::test]
    async fn plan_rejects_empty_array() {
        let executor = Arc::new(MockExecutor::new("[]"));
        let queen = Queen::new(SwarmConfig::default(), executor);
        let result = queen.plan("Do nothing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("zero team objectives"));
    }

    #[tokio::test]
    async fn plan_rejects_no_json() {
        let executor = Arc::new(MockExecutor::new("This has no JSON at all."));
        let queen = Queen::new(SwarmConfig::default(), executor);
        let result = queen.plan("No JSON").await;
        assert!(result.is_err());
    }

    // -- SingleShot execution ------------------------------------------------

    #[tokio::test]
    async fn execute_team_singleshot_returns_result() {
        let executor = Arc::new(MockExecutor::new("SingleShot output for the team."));
        let queen = Queen::new(SwarmConfig::default(), executor);

        let objective = TeamObjective {
            id: "team-1".into(),
            name: "Quick Task".into(),
            description: "Do a quick thing".into(),
            dependencies: vec![],
            orchestration_mode: OrchestrationMode::SingleShot,
            scope_paths: vec![],
            priority: 0,
            preferred_model: None,
        };

        let result = queen.execute_team(&objective, &[]).await;

        assert_eq!(result.status, TeamStatus::Completed);
        assert_eq!(result.team_id, "team-1");
        assert_eq!(result.team_name, "Quick Task");
        assert!(result.error.is_none());
        assert!(result.inner.is_some());
        match result.inner.unwrap() {
            InnerResult::SingleShot { content, .. } => {
                assert_eq!(content, "SingleShot output for the team.");
            }
            other => panic!("Expected SingleShot, got {other:?}"),
        }
    }

    // -- Cost tracking -------------------------------------------------------

    #[test]
    fn cost_tracking_add_and_read() {
        let executor = Arc::new(MockExecutor::new("test"));
        let queen = Queen::new(SwarmConfig::default(), executor);

        assert!((queen.current_cost() - 0.0).abs() < f64::EPSILON);

        queen.add_cost(1.5);
        assert!((queen.current_cost() - 1.5).abs() < f64::EPSILON);

        queen.add_cost(0.75);
        assert!((queen.current_cost() - 2.25).abs() < f64::EPSILON);

        queen.add_cost(0.0);
        assert!((queen.current_cost() - 2.25).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_tracking_multiple_small_additions() {
        let executor = Arc::new(MockExecutor::new("test"));
        let queen = Queen::new(SwarmConfig::default(), executor);

        for _ in 0..100 {
            queen.add_cost(0.01);
        }

        // Should be approximately 1.0 (within floating-point tolerance).
        assert!((queen.current_cost() - 1.0).abs() < 1e-10);
    }

    // -- Status callback invocation ------------------------------------------

    #[tokio::test]
    async fn status_callback_is_invoked_during_plan() {
        let json_response = r#"[{
            "id": "team-1",
            "name": "Task",
            "description": "Do it",
            "dependencies": [],
            "orchestration_mode": "single_shot",
            "scope_paths": [],
            "priority": 0
        }]"#;

        let statuses: Arc<Mutex<Vec<(SwarmStatus, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let statuses_clone = statuses.clone();

        let executor = Arc::new(MockExecutor::new(json_response));
        let queen = Queen::new(SwarmConfig::default(), executor)
            .with_status_callback(Arc::new(move |status, detail| {
                if let Ok(mut s) = statuses_clone.lock() {
                    s.push((status, detail.to_string()));
                }
            }));

        let _plan = queen.plan("Do something").await.unwrap();

        let recorded = statuses.lock().unwrap();
        assert!(
            !recorded.is_empty(),
            "Status callback should have been invoked"
        );
        assert_eq!(
            recorded[0].0,
            SwarmStatus::Planning,
            "First status should be Planning"
        );
    }

    #[tokio::test]
    async fn status_callback_tracks_full_execution() {
        let json_response = r#"[{
            "id": "team-1",
            "name": "Task",
            "description": "Do it",
            "dependencies": [],
            "orchestration_mode": "single_shot",
            "scope_paths": [],
            "priority": 0
        }]"#;

        let statuses: Arc<Mutex<Vec<SwarmStatus>>> = Arc::new(Mutex::new(Vec::new()));
        let statuses_clone = statuses.clone();

        let executor = Arc::new(MockExecutor::new(json_response));
        let queen = Queen::new(SwarmConfig::default(), executor)
            .with_status_callback(Arc::new(move |status, _detail| {
                if let Ok(mut s) = statuses_clone.lock() {
                    s.push(status);
                }
            }));

        let result = queen.execute("Do something").await.unwrap();
        assert_eq!(result.status, SwarmStatus::Complete);

        let recorded = statuses.lock().unwrap();
        // Should see: Planning, Executing, TeamStarted, TeamCompleted, Synthesizing, Complete
        assert!(
            recorded.len() >= 4,
            "Expected at least 4 status updates, got {}",
            recorded.len()
        );
        assert!(recorded.contains(&SwarmStatus::Planning));
        assert!(recorded.contains(&SwarmStatus::Executing));
        assert!(recorded.contains(&SwarmStatus::Synthesizing));
        assert!(recorded.contains(&SwarmStatus::Complete));
    }

    // -- Full execution pipeline ---------------------------------------------

    #[tokio::test]
    async fn full_execute_with_singleshot_team() {
        // Mock returns a valid plan on first call, then team output, then synthesis.
        // Since we use the same mock for all calls, the plan JSON must parse correctly.
        let json_response = r#"[{
            "id": "team-1",
            "name": "Only Team",
            "description": "Do the thing",
            "dependencies": [],
            "orchestration_mode": "single_shot",
            "scope_paths": [],
            "priority": 0
        }]"#;

        let executor = Arc::new(MockExecutor::new(json_response));
        let queen = Queen::new(SwarmConfig::default(), executor);

        let result = queen.execute("Build a feature").await.unwrap();

        assert_eq!(result.goal, "Build a feature");
        assert!(!result.run_id.is_empty());
        assert_eq!(result.plan.teams.len(), 1);
        assert_eq!(result.team_results.len(), 1);
        assert_eq!(result.team_results[0].status, TeamStatus::Completed);
        assert!(result.total_duration_ms > 0 || result.total_duration_ms == 0);
    }

    // -- Memory recording ----------------------------------------------------

    #[test]
    fn record_learnings_stores_entries() {
        let executor = Arc::new(MockExecutor::new("test"));
        let memory = Arc::new(CollectiveMemory::in_memory().unwrap());
        let queen = Queen::new(SwarmConfig::default(), executor).with_memory(memory.clone());

        let plan = SwarmPlan {
            teams: vec![
                TeamObjective {
                    id: "team-1".into(),
                    name: "Success Team".into(),
                    description: "Succeeds".into(),
                    dependencies: vec![],
                    orchestration_mode: OrchestrationMode::SingleShot,
                    scope_paths: vec![],
                    priority: 0,
                    preferred_model: None,
                },
                TeamObjective {
                    id: "team-2".into(),
                    name: "Failure Team".into(),
                    description: "Fails".into(),
                    dependencies: vec![],
                    orchestration_mode: OrchestrationMode::HiveMind,
                    scope_paths: vec![],
                    priority: 0,
                    preferred_model: None,
                },
            ],
        };

        let results = vec![
            TeamResult {
                team_id: "team-1".into(),
                team_name: "Success Team".into(),
                status: TeamStatus::Completed,
                inner: None,
                cost: 0.5,
                duration_ms: 1000,
                insights: vec!["Key finding about caching".into()],
                error: None,
            },
            TeamResult {
                team_id: "team-2".into(),
                team_name: "Failure Team".into(),
                status: TeamStatus::Failed,
                inner: None,
                cost: 0.0,
                duration_ms: 200,
                insights: vec![],
                error: Some("API timeout".into()),
            },
        ];

        let count = queen.record_learnings(&plan, &results);

        // Should record: 1 success pattern + 1 insight + 1 failure pattern = 3
        assert_eq!(count, 3);

        let all = memory.recall("", None, None, 100).unwrap();
        assert_eq!(all.len(), 3);

        // Verify categories.
        let success_entries = memory
            .recall("", Some(MemoryCategory::SuccessPattern), None, 10)
            .unwrap();
        assert_eq!(success_entries.len(), 1);

        let failure_entries = memory
            .recall("", Some(MemoryCategory::FailurePattern), None, 10)
            .unwrap();
        assert_eq!(failure_entries.len(), 1);
        assert!(failure_entries[0].content.contains("API timeout"));

        let insight_entries = memory
            .recall("", Some(MemoryCategory::ModelInsight), None, 10)
            .unwrap();
        assert_eq!(insight_entries.len(), 1);
        assert!(insight_entries[0].content.contains("caching"));
    }

    #[test]
    fn record_learnings_without_memory_returns_zero() {
        let executor = Arc::new(MockExecutor::new("test"));
        let queen = Queen::new(SwarmConfig::default(), executor);

        let plan = SwarmPlan {
            teams: vec![TeamObjective {
                id: "team-1".into(),
                name: "Test".into(),
                description: "Test".into(),
                dependencies: vec![],
                orchestration_mode: OrchestrationMode::SingleShot,
                scope_paths: vec![],
                priority: 0,
                preferred_model: None,
            }],
        };

        let results = vec![TeamResult {
            team_id: "team-1".into(),
            team_name: "Test".into(),
            status: TeamStatus::Completed,
            inner: None,
            cost: 0.1,
            duration_ms: 100,
            insights: vec!["An insight".into()],
            error: None,
        }];

        let count = queen.record_learnings(&plan, &results);
        assert_eq!(count, 0);
    }

    // -- Helper function tests -----------------------------------------------

    #[test]
    fn truncate_str_short_string() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_exact_length() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_str_long_string() {
        let result = truncate_str("hello world this is long", 10);
        assert!(result.len() <= 13); // 10 - 3 + 3 for "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn estimate_cost_known_models() {
        let response = ChatResponse {
            content: "test".into(),
            model: "claude-sonnet".into(),
            usage: TokenUsage {
                prompt_tokens: 1_000_000,
                completion_tokens: 1_000_000,
                total_tokens: 2_000_000,
            },
            finish_reason: FinishReason::Stop,
            thinking: None,
        };

        let cost = estimate_cost("claude-sonnet", &response);
        // Sonnet: $3 input + $15 output = $18
        assert!((cost - 18.0).abs() < 0.01);
    }

    #[test]
    fn estimate_cost_unknown_model() {
        let response = ChatResponse {
            content: "test".into(),
            model: "local-llama".into(),
            usage: TokenUsage {
                prompt_tokens: 1000,
                completion_tokens: 1000,
                total_tokens: 2000,
            },
            finish_reason: FinishReason::Stop,
            thinking: None,
        };

        let cost = estimate_cost("local-llama", &response);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn extract_insights_from_text_finds_indicators() {
        let text = "The system is working well. \
                    An important finding is that caching reduces latency by 50%. \
                    The recommendation is to use Redis. \
                    Normal text here. \
                    There is a risk of data loss without backups.";

        let insights = extract_insights_from_text(text);
        assert!(!insights.is_empty());
        assert!(insights.len() <= 5);
        // At least one should contain "important" or "recommendation" or "risk".
        let has_indicator = insights.iter().any(|i| {
            let lower = i.to_lowercase();
            lower.contains("important")
                || lower.contains("recommendation")
                || lower.contains("risk")
        });
        assert!(has_indicator);
    }

    #[test]
    fn extract_insights_from_text_empty() {
        let insights = extract_insights_from_text("Nothing special here at all.");
        assert!(insights.is_empty());
    }

    #[test]
    fn build_coordinator_plan_produces_three_tasks() {
        let objective = TeamObjective {
            id: "team-5".into(),
            name: "Test Objective".into(),
            description: "Test description".into(),
            dependencies: vec![],
            orchestration_mode: OrchestrationMode::Coordinator,
            scope_paths: vec![],
            priority: 0,
            preferred_model: None,
        };

        let plan = build_coordinator_plan_from_objective(&objective, "Test description");
        assert_eq!(plan.tasks.len(), 3);
        assert_eq!(plan.tasks[0].id, "team-5-investigate");
        assert_eq!(plan.tasks[1].id, "team-5-implement");
        assert_eq!(plan.tasks[2].id, "team-5-verify");

        // Verify dependency chain.
        assert!(plan.tasks[0].dependencies.is_empty());
        assert_eq!(plan.tasks[1].dependencies, vec!["team-5-investigate"]);
        assert_eq!(plan.tasks[2].dependencies, vec!["team-5-implement"]);

        // Should validate successfully.
        assert!(plan.validate().is_ok());
    }

    // -- Cross-team context --------------------------------------------------

    #[test]
    fn build_cross_team_context_empty_results() {
        let executor = Arc::new(MockExecutor::new("test"));
        let queen = Queen::new(SwarmConfig::default(), executor);
        let context = queen.build_cross_team_context(&[]);
        assert!(context.is_empty());
    }

    #[test]
    fn build_cross_team_context_includes_completed_teams() {
        let executor = Arc::new(MockExecutor::new("test"));
        let queen = Queen::new(SwarmConfig::default(), executor);

        let results = vec![TeamResult {
            team_id: "team-1".into(),
            team_name: "Research".into(),
            status: TeamStatus::Completed,
            inner: Some(InnerResult::SingleShot {
                content: "Found that the codebase uses async/await extensively.".into(),
                model: "mock".into(),
            }),
            cost: 0.1,
            duration_ms: 500,
            insights: vec!["Uses async patterns".into()],
            error: None,
        }];

        let context = queen.build_cross_team_context(&results);
        assert!(context.contains("Research"));
        assert!(context.contains("async/await"));
        assert!(context.contains("async patterns"));
    }

    // -- Dependency skip on failure ------------------------------------------

    #[tokio::test]
    async fn execute_plan_skips_dependents_of_failed_team() {
        // First call returns plan JSON, second call will be used for team execution.
        // We need a mock that fails for team execution but succeeds for planning.
        // Since we call execute_plan directly, we just need an executor.

        struct FailingExecutor;
        impl AiExecutor for FailingExecutor {
            async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
                Err("Always fails".into())
            }
        }

        let executor = Arc::new(FailingExecutor);
        let queen = Queen::new(SwarmConfig::default(), executor);

        let plan = SwarmPlan {
            teams: vec![
                TeamObjective {
                    id: "team-1".into(),
                    name: "First".into(),
                    description: "First task".into(),
                    dependencies: vec![],
                    orchestration_mode: OrchestrationMode::SingleShot,
                    scope_paths: vec![],
                    priority: 0,
                    preferred_model: None,
                },
                TeamObjective {
                    id: "team-2".into(),
                    name: "Second".into(),
                    description: "Depends on first".into(),
                    dependencies: vec!["team-1".into()],
                    orchestration_mode: OrchestrationMode::SingleShot,
                    scope_paths: vec![],
                    priority: 0,
                    preferred_model: None,
                },
            ],
        };

        let results = queen.execute_plan(&plan).await.unwrap();
        assert_eq!(results.len(), 2);

        // First team should have failed.
        assert_eq!(results[0].team_id, "team-1");
        assert_eq!(results[0].status, TeamStatus::Failed);

        // Second team should be skipped because its dependency failed.
        assert_eq!(results[1].team_id, "team-2");
        assert_eq!(results[1].status, TeamStatus::Skipped);
        assert!(results[1].error.as_deref().unwrap().contains("Dependency failed"));
    }

    // -- Budget enforcement --------------------------------------------------

    #[tokio::test]
    async fn execute_plan_stops_on_budget_exceeded() {
        // Use a config with zero budget.
        let executor = Arc::new(MockExecutor::new("output"));
        let config = SwarmConfig {
            total_cost_limit_usd: 0.0,
            ..Default::default()
        };
        let queen = Queen::new(config, executor);

        // Pre-load some cost so the budget is already exceeded.
        queen.add_cost(0.01);

        let plan = SwarmPlan {
            teams: vec![TeamObjective {
                id: "team-1".into(),
                name: "Task".into(),
                description: "Will not run".into(),
                dependencies: vec![],
                orchestration_mode: OrchestrationMode::SingleShot,
                scope_paths: vec![],
                priority: 0,
                preferred_model: None,
            }],
        };

        let results = queen.execute_plan(&plan).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TeamStatus::Skipped);
        assert!(results[0]
            .error
            .as_deref()
            .unwrap()
            .contains("budget"));
    }

    // -- Memory context gathering --------------------------------------------

    #[test]
    fn gather_memory_context_without_memory() {
        let executor = Arc::new(MockExecutor::new("test"));
        let queen = Queen::new(SwarmConfig::default(), executor);
        let ctx = queen.gather_memory_context("build a caching system");
        assert!(ctx.is_empty());
    }

    #[test]
    fn gather_memory_context_with_matching_entries() {
        let executor = Arc::new(MockExecutor::new("test"));
        let memory = Arc::new(CollectiveMemory::in_memory().unwrap());

        // Store some relevant memories.
        let mut entry = MemoryEntry::new(
            MemoryCategory::SuccessPattern,
            "Using Redis for caching improved latency significantly",
        );
        entry.tags = vec!["caching".into()];
        memory.remember(&entry).unwrap();

        let queen = Queen::new(SwarmConfig::default(), executor).with_memory(memory);
        let ctx = queen.gather_memory_context("Build a caching layer for the API");

        assert!(ctx.contains("Success pattern"));
        assert!(ctx.contains("Redis") || ctx.contains("caching"));
    }

    #[test]
    fn gather_memory_context_with_short_goal() {
        let executor = Arc::new(MockExecutor::new("test"));
        let memory = Arc::new(CollectiveMemory::in_memory().unwrap());
        let queen = Queen::new(SwarmConfig::default(), executor).with_memory(memory);

        // Goal with only short words should return empty context.
        let ctx = queen.gather_memory_context("do it");
        assert!(ctx.is_empty());
    }
}
