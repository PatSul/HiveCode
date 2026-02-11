use gpui::*;
use hive_ai::routing::{AutoFallbackManager, ModelRouter, ProviderType};
use hive_ai::ModelTier;

use crate::theme::HiveTheme;
use crate::workspace::RoutingAddRule;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Health state of a provider for display purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderHealth {
    Healthy,
    Degraded,
    Down,
}

impl ProviderHealth {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::Degraded => "Degraded",
            Self::Down => "Down",
        }
    }
}

/// A task-type to model-tier mapping entry.
#[derive(Debug, Clone)]
pub struct TaskMapping {
    pub task_type: String,
    pub model_id: String,
    pub tier: String,
    pub performance_score: f64,
}

/// A custom routing rule override.
#[derive(Debug, Clone)]
pub struct RoutingRule {
    pub name: String,
    pub condition: String,
    pub target_model: String,
    pub enabled: bool,
}

/// Provider status entry for the performance tracking table.
#[derive(Debug, Clone)]
pub struct ProviderStatusEntry {
    pub name: String,
    pub status: ProviderHealth,
    pub requests: usize,
    pub failures: usize,
    pub avg_latency_ms: f64,
}

/// Aggregated performance metrics across all providers.
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_requests: usize,
    pub total_failures: usize,
    pub avg_latency_ms: f64,
    pub healthy_providers: usize,
    pub total_providers: usize,
    pub active_rules: usize,
    pub total_rules: usize,
}

impl PerformanceMetrics {
    /// Compute aggregate metrics from the routing data.
    pub fn from_data(data: &RoutingData) -> Self {
        let total_requests: usize = data.provider_status.iter().map(|p| p.requests).sum();
        let total_failures: usize = data.provider_status.iter().map(|p| p.failures).sum();

        let providers_with_latency: Vec<f64> = data
            .provider_status
            .iter()
            .filter(|p| p.avg_latency_ms > 0.0)
            .map(|p| p.avg_latency_ms)
            .collect();
        let avg_latency_ms = if providers_with_latency.is_empty() {
            0.0
        } else {
            providers_with_latency.iter().sum::<f64>() / providers_with_latency.len() as f64
        };

        let healthy_providers = data
            .provider_status
            .iter()
            .filter(|p| p.status == ProviderHealth::Healthy)
            .count();

        let active_rules = data.custom_rules.iter().filter(|r| r.enabled).count();

        Self {
            total_requests,
            total_failures,
            avg_latency_ms,
            healthy_providers,
            total_providers: data.provider_status.len(),
            active_rules,
            total_rules: data.custom_rules.len(),
        }
    }

    /// Success rate as a percentage (0.0 to 100.0).
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            100.0
        } else {
            (1.0 - self.total_failures as f64 / self.total_requests as f64) * 100.0
        }
    }
}

/// All data for the routing panel.
#[derive(Debug, Clone)]
pub struct RoutingData {
    pub task_mappings: Vec<TaskMapping>,
    pub custom_rules: Vec<RoutingRule>,
    pub provider_status: Vec<ProviderStatusEntry>,
}

impl RoutingData {
    /// Create empty routing data (no mappings, no rules, no providers).
    pub fn empty() -> Self {
        Self {
            task_mappings: Vec::new(),
            custom_rules: Vec::new(),
            provider_status: Vec::new(),
        }
    }

    /// Populate routing data from a live ModelRouter instance.
    ///
    /// Reads provider health from the fallback manager and builds default
    /// task-type mappings based on the classifier's known task categories.
    pub fn from_router(router: &ModelRouter) -> Self {
        let task_mappings = build_task_mappings_from_router(router);
        let provider_status = build_provider_status(router.fallback_manager());

        // Custom rules are user-configured; the ModelRouter does not store
        // them, so we start with an empty list. The caller can populate this
        // from persisted config.
        Self {
            task_mappings,
            custom_rules: Vec::new(),
            provider_status,
        }
    }

    /// Return a sample dataset for previewing the panel.
    pub fn sample() -> Self {
        Self {
            task_mappings: vec![
                TaskMapping {
                    task_type: "Simple Question".into(),
                    model_id: "deepseek/deepseek-chat".into(),
                    tier: "Budget".into(),
                    performance_score: 0.95,
                },
                TaskMapping {
                    task_type: "Code Generation".into(),
                    model_id: "claude-sonnet-4-20250514".into(),
                    tier: "Mid".into(),
                    performance_score: 0.92,
                },
                TaskMapping {
                    task_type: "Architecture".into(),
                    model_id: "claude-opus-4-20250514".into(),
                    tier: "Premium".into(),
                    performance_score: 0.98,
                },
                TaskMapping {
                    task_type: "Bug Fix".into(),
                    model_id: "claude-sonnet-4-20250514".into(),
                    tier: "Mid".into(),
                    performance_score: 0.90,
                },
                TaskMapping {
                    task_type: "Security Review".into(),
                    model_id: "claude-opus-4-20250514".into(),
                    tier: "Premium".into(),
                    performance_score: 0.97,
                },
                TaskMapping {
                    task_type: "Documentation".into(),
                    model_id: "claude-3-5-haiku-20241022".into(),
                    tier: "Budget".into(),
                    performance_score: 0.88,
                },
                TaskMapping {
                    task_type: "Testing".into(),
                    model_id: "gpt-4o-mini".into(),
                    tier: "Mid".into(),
                    performance_score: 0.91,
                },
                TaskMapping {
                    task_type: "Debugging".into(),
                    model_id: "claude-opus-4-20250514".into(),
                    tier: "Premium".into(),
                    performance_score: 0.96,
                },
                TaskMapping {
                    task_type: "Research".into(),
                    model_id: "claude-sonnet-4-20250514".into(),
                    tier: "Mid".into(),
                    performance_score: 0.89,
                },
                TaskMapping {
                    task_type: "Creative Writing".into(),
                    model_id: "claude-sonnet-4-20250514".into(),
                    tier: "Mid".into(),
                    performance_score: 0.93,
                },
                TaskMapping {
                    task_type: "Refactoring".into(),
                    model_id: "claude-sonnet-4-20250514".into(),
                    tier: "Mid".into(),
                    performance_score: 0.91,
                },
                TaskMapping {
                    task_type: "Code Explanation".into(),
                    model_id: "claude-3-5-haiku-20241022".into(),
                    tier: "Budget".into(),
                    performance_score: 0.87,
                },
            ],
            custom_rules: vec![
                RoutingRule {
                    name: "Security Override".into(),
                    condition: "Message contains 'security'".into(),
                    target_model: "claude-opus-4-20250514".into(),
                    enabled: true,
                },
                RoutingRule {
                    name: "Short Message Budget".into(),
                    condition: "Token count < 50".into(),
                    target_model: "deepseek/deepseek-chat".into(),
                    enabled: true,
                },
                RoutingRule {
                    name: "Night Mode Local".into(),
                    condition: "After 10pm local time".into(),
                    target_model: "llama3.2".into(),
                    enabled: false,
                },
            ],
            provider_status: vec![
                ProviderStatusEntry {
                    name: "Anthropic".into(),
                    status: ProviderHealth::Healthy,
                    requests: 170,
                    failures: 2,
                    avg_latency_ms: 1850.0,
                },
                ProviderStatusEntry {
                    name: "OpenAI".into(),
                    status: ProviderHealth::Healthy,
                    requests: 67,
                    failures: 1,
                    avg_latency_ms: 890.0,
                },
                ProviderStatusEntry {
                    name: "OpenRouter".into(),
                    status: ProviderHealth::Degraded,
                    requests: 8,
                    failures: 1,
                    avg_latency_ms: 4500.0,
                },
                ProviderStatusEntry {
                    name: "Ollama".into(),
                    status: ProviderHealth::Healthy,
                    requests: 23,
                    failures: 0,
                    avg_latency_ms: 3200.0,
                },
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// from_router helpers
// ---------------------------------------------------------------------------

/// Default model ID for a given tier (matches model_router.rs defaults).
fn default_model_for_tier(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Premium => "claude-opus-4-20250514",
        ModelTier::Mid => "claude-sonnet-4-20250514",
        ModelTier::Budget => "deepseek/deepseek-chat",
        ModelTier::Free => "llama3.2",
    }
}

/// Map a task type string to its default tier (mirrors complexity classifier logic).
pub fn tier_for_task(task: &str) -> ModelTier {
    match task {
        "Simple Question" | "Code Explanation" | "Documentation" => ModelTier::Budget,
        "Code Generation" | "Bug Fix" | "Refactoring" | "Testing" | "Research"
        | "Creative Writing" | "General" => ModelTier::Mid,
        "Architecture" | "Security Review" | "Debugging" => ModelTier::Premium,
        _ => ModelTier::Mid,
    }
}

fn tier_label(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Free => "Free",
        ModelTier::Budget => "Budget",
        ModelTier::Mid => "Mid",
        ModelTier::Premium => "Premium",
    }
}

/// Build task mappings by classifying each known task type through the router.
fn build_task_mappings_from_router(router: &ModelRouter) -> Vec<TaskMapping> {
    let task_types = [
        "Simple Question",
        "Code Explanation",
        "Code Generation",
        "Bug Fix",
        "Refactoring",
        "Architecture",
        "Security Review",
        "Documentation",
        "Testing",
        "Debugging",
        "Research",
        "Creative Writing",
    ];

    task_types
        .iter()
        .map(|task| {
            let tier = tier_for_task(task);

            // Use the classifier to get a performance score for this task type.
            // We simulate a representative message for classification.
            let messages = [hive_ai::ChatMessage {
                role: hive_ai::MessageRole::User,
                content: task.to_string(),
                timestamp: chrono::Utc::now(),
            }];
            let result = router.classify(&messages, None);

            TaskMapping {
                task_type: task.to_string(),
                model_id: default_model_for_tier(tier).to_string(),
                tier: tier_label(tier).to_string(),
                performance_score: result.score as f64,
            }
        })
        .collect()
}

/// Build provider status entries from the fallback manager's health data.
fn build_provider_status(fallback_manager: &AutoFallbackManager) -> Vec<ProviderStatusEntry> {
    let providers = [
        (ProviderType::Anthropic, "Anthropic"),
        (ProviderType::OpenAI, "OpenAI"),
        (ProviderType::OpenRouter, "OpenRouter"),
        (ProviderType::Google, "Google"),
        (ProviderType::Groq, "Groq"),
        (ProviderType::HuggingFace, "Hugging Face"),
        (ProviderType::LiteLLM, "LiteLLM"),
        (ProviderType::Ollama, "Ollama"),
        (ProviderType::LMStudio, "LM Studio"),
    ];

    let statuses = fallback_manager.provider_statuses();

    providers
        .iter()
        .filter_map(|(provider_type, name)| {
            let status = statuses.get(provider_type)?;

            let health = if !status.available {
                ProviderHealth::Down
            } else if status.consecutive_failures > 0 {
                ProviderHealth::Degraded
            } else {
                ProviderHealth::Healthy
            };

            Some(ProviderStatusEntry {
                name: name.to_string(),
                status: health,
                requests: 0,
                failures: status.consecutive_failures as usize,
                avg_latency_ms: 0.0,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Model routing rules, task mapping, performance tracking, and custom rules.
pub struct RoutingPanel;

impl RoutingPanel {
    pub fn render(data: &RoutingData, theme: &HiveTheme) -> impl IntoElement {
        let metrics = PerformanceMetrics::from_data(data);

        div()
            .id("routing-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(Self::header(theme))
            .child(Self::metrics_summary(&metrics, data, theme))
            .child(Self::task_mapping_card(data, theme))
            .child(Self::tier_models_card(theme))
            .child(Self::performance_card(data, theme))
            .child(Self::fallback_chain_card(theme))
            .child(Self::custom_rules_card(data, theme))
            .child(Self::hard_rules_card(theme))
    }

    // ------------------------------------------------------------------
    // Header
    // ------------------------------------------------------------------

    fn header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_2xl)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::BOLD)
                    .child("Model Routing"),
            )
    }

    // ------------------------------------------------------------------
    // Performance metrics summary (4 stat cards)
    // ------------------------------------------------------------------

    fn metrics_summary(
        metrics: &PerformanceMetrics,
        data: &RoutingData,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        let latency_text = Self::format_latency(metrics.avg_latency_ms);
        let success_rate = metrics.success_rate();
        let success_color = Self::rate_color(success_rate, theme);
        let provider_color = Self::provider_health_color(metrics, theme);

        let success_subtitle = format!(
            "{}/{} requests ok",
            metrics.total_requests - metrics.total_failures,
            metrics.total_requests
        );
        let provider_subtitle = format!(
            "{} rules active, {} mappings",
            metrics.active_rules,
            data.task_mappings.len()
        );

        div()
            .flex()
            .flex_row()
            .gap(theme.space_3)
            .child(Self::stat_card(
                "Total Requests",
                &format!("{}", metrics.total_requests),
                &format!("{} failures", metrics.total_failures),
                theme.accent_cyan,
                theme,
            ))
            .child(Self::stat_card(
                "Avg Latency",
                &latency_text,
                "across all providers",
                theme.accent_aqua,
                theme,
            ))
            .child(Self::stat_card(
                "Success Rate",
                &format!("{:.1}%", success_rate),
                &success_subtitle,
                success_color,
                theme,
            ))
            .child(Self::stat_card(
                "Providers",
                &format!("{}/{}", metrics.healthy_providers, metrics.total_providers),
                &provider_subtitle,
                provider_color,
                theme,
            ))
    }

    fn format_latency(ms: f64) -> String {
        if ms >= 1000.0 {
            format!("{:.1}s", ms / 1000.0)
        } else if ms > 0.0 {
            format!("{:.0}ms", ms)
        } else {
            "-".to_string()
        }
    }

    fn rate_color(rate: f64, theme: &HiveTheme) -> Hsla {
        if rate >= 99.0 {
            theme.accent_green
        } else if rate >= 95.0 {
            theme.accent_yellow
        } else {
            theme.accent_red
        }
    }

    fn provider_health_color(metrics: &PerformanceMetrics, theme: &HiveTheme) -> Hsla {
        if metrics.healthy_providers == metrics.total_providers {
            theme.accent_green
        } else if metrics.healthy_providers > 0 {
            theme.accent_yellow
        } else {
            theme.accent_red
        }
    }

    fn stat_card(
        label: &str,
        value: &str,
        subtitle: &str,
        accent: Hsla,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .p(theme.space_3)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .gap(theme.space_1)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(theme.font_size_xl)
                    .text_color(accent)
                    .font_weight(FontWeight::BOLD)
                    .child(value.to_string()),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(subtitle.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Task-type -> Tier mapping table (now data-driven)
    // ------------------------------------------------------------------

    fn task_mapping_card(data: &RoutingData, theme: &HiveTheme) -> impl IntoElement {
        let mut card = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::section_title("Task Type Mapping", theme))
            .child(Self::section_desc(
                "How incoming requests are classified and routed to model tiers",
                theme,
            ))
            .child(Self::mapping_header(theme));

        if data.task_mappings.is_empty() {
            card = card.child(
                div()
                    .py(theme.space_4)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_muted)
                            .child("No task mappings configured"),
                    ),
            );
        } else {
            for mapping in &data.task_mappings {
                card = card.child(Self::task_row_from_mapping(mapping, theme));
            }
        }

        card
    }

    fn mapping_header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .pb(theme.space_1)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Task Type"),
            )
            .child(
                div()
                    .w(px(80.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Tier"),
            )
            .child(
                div()
                    .w(px(60.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Score"),
            )
    }

    fn task_row_from_mapping(mapping: &TaskMapping, theme: &HiveTheme) -> impl IntoElement {
        let tier_color = match mapping.tier.as_str() {
            "Budget" => theme.accent_green,
            "Mid" => theme.accent_yellow,
            "Premium" => theme.accent_red,
            "Free" => theme.accent_aqua,
            _ => theme.text_muted,
        };

        let score_color = if mapping.performance_score >= 0.95 {
            theme.accent_green
        } else if mapping.performance_score >= 0.85 {
            theme.accent_yellow
        } else {
            theme.accent_red
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_primary)
                    .child(mapping.task_type.clone()),
            )
            .child(
                div()
                    .w(px(80.0))
                    .child(Self::tier_badge(&mapping.tier, tier_color, theme)),
            )
            .child(
                div()
                    .w(px(60.0))
                    .text_size(theme.font_size_xs)
                    .text_color(score_color)
                    .child(format!("{:.0}%", mapping.performance_score * 100.0)),
            )
    }

    fn tier_badge(tier: &str, color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        div()
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_xs)
            .text_color(color)
            .font_weight(FontWeight::MEDIUM)
            .child(tier.to_string())
    }

    // ------------------------------------------------------------------
    // Tier -> Models registry
    // ------------------------------------------------------------------

    fn tier_models_card(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::section_title("Model Registry", theme))
            .child(Self::section_desc(
                "Available models grouped by performance tier",
                theme,
            ))
            .child(Self::tier_premium_section(theme))
            .child(Self::tier_mid_section(theme))
            .child(Self::tier_budget_section(theme))
            .child(Self::tier_free_section(theme))
    }

    fn tier_premium_section(theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_col()
            .child(Self::tier_section_header("Premium", theme.accent_red, theme))
            .child(Self::model_row("Claude Opus 4.6", "anthropic", "$15/$75 per MTok", theme))
            .child(Self::model_row("GPT-4o", "openai", "$2.50/$10 per MTok", theme))
            .child(Self::model_row("DeepSeek R1", "deepseek", "$0.55/$2.19 per MTok", theme))
    }

    fn tier_mid_section(theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_col()
            .child(Self::tier_section_header("Mid", theme.accent_yellow, theme))
            .child(Self::model_row("Claude Sonnet 4.5", "anthropic", "$3/$15 per MTok", theme))
            .child(Self::model_row("GPT-4o Mini", "openai", "$0.15/$0.60 per MTok", theme))
    }

    fn tier_budget_section(theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_col()
            .child(Self::tier_section_header("Budget", theme.accent_green, theme))
            .child(Self::model_row("Claude Haiku 4.5", "anthropic", "$0.80/$4 per MTok", theme))
            .child(Self::model_row("DeepSeek V3", "deepseek", "$0.27/$1.10 per MTok", theme))
    }

    fn tier_free_section(theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_col()
            .child(Self::tier_section_header("Free (Local)", theme.accent_aqua, theme))
            .child(Self::model_row("Ollama models", "local", "Free", theme))
            .child(Self::model_row("LM Studio models", "local", "Free", theme))
    }

    fn tier_section_header(label: &str, color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .mt(theme.space_2)
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(theme.radius_full)
                    .bg(color),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label.to_string()),
            )
    }

    fn model_row(name: &str, provider: &str, price: &str, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .pl(px(20.0))
            .py(theme.space_1)
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_primary)
                    .child(name.to_string()),
            )
            .child(
                div()
                    .w(px(80.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(provider.to_string()),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(price.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Performance tracking (data-driven)
    // ------------------------------------------------------------------

    fn performance_card(data: &RoutingData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::section_title("Provider Performance", theme))
            .child(Self::section_desc(
                "Real-time metrics for each active provider",
                theme,
            ))
            .child(Self::perf_header(theme));

        if data.provider_status.is_empty() {
            container = container.child(
                div()
                    .py(theme.space_4)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_muted)
                            .child("No provider data available"),
                    ),
            );
        } else {
            for entry in &data.provider_status {
                container = container.child(Self::perf_row(entry, theme));
            }
        }

        container
    }

    fn perf_header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .pb(theme.space_1)
            .border_b_1()
            .border_color(theme.border)
            .child(Self::col_header_flex("Provider", theme))
            .child(Self::col_header_fixed("Reqs", px(60.0), theme))
            .child(Self::col_header_fixed("Failures", px(72.0), theme))
            .child(Self::col_header_fixed("Latency", px(72.0), theme))
            .child(Self::col_header_fixed("Status", px(80.0), theme))
    }

    fn col_header_flex(label: &str, theme: &HiveTheme) -> Div {
        div()
            .flex_1()
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .font_weight(FontWeight::SEMIBOLD)
            .child(label.to_string())
    }

    fn col_header_fixed(label: &str, width: Pixels, theme: &HiveTheme) -> Div {
        div()
            .w(width)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .font_weight(FontWeight::SEMIBOLD)
            .child(label.to_string())
    }

    fn perf_row(entry: &ProviderStatusEntry, theme: &HiveTheme) -> impl IntoElement {
        let (status_color, status_label) = match entry.status {
            ProviderHealth::Healthy => (theme.accent_green, entry.status.label()),
            ProviderHealth::Degraded => (theme.accent_yellow, entry.status.label()),
            ProviderHealth::Down => (theme.accent_red, entry.status.label()),
        };

        let latency_text = Self::format_latency(entry.avg_latency_ms);

        let failures_color = if entry.failures > 0 {
            theme.accent_red
        } else {
            theme.text_secondary
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            .child(Self::cell_flex(&entry.name, theme.text_primary, theme))
            .child(Self::cell_fixed(&format!("{}", entry.requests), px(60.0), theme.text_secondary, theme))
            .child(Self::cell_fixed(&format!("{}", entry.failures), px(72.0), failures_color, theme))
            .child(Self::cell_fixed(&latency_text, px(72.0), theme.text_secondary, theme))
            .child(Self::perf_status_badge(status_label, status_color, theme))
    }

    fn cell_flex(text: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex_1()
            .text_size(theme.font_size_sm)
            .text_color(color)
            .child(text.to_string())
    }

    fn cell_fixed(text: &str, width: Pixels, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .w(width)
            .text_size(theme.font_size_sm)
            .text_color(color)
            .child(text.to_string())
    }

    fn perf_status_badge(label: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .w(px(80.0))
            .child(
                div()
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(color)
                    .child(label.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Fallback chain
    // ------------------------------------------------------------------

    fn fallback_chain_card(theme: &HiveTheme) -> impl IntoElement {
        let card_shell = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::section_title("Fallback Chains", theme))
            .child(Self::section_desc(
                "When a provider fails, requests automatically fall through to the next tier",
                theme,
            ));

        Self::build_fallback_chains(card_shell, theme)
    }

    fn build_fallback_chains(container: Div, theme: &HiveTheme) -> Div {
        container
            .child(Self::chain_row(
                "Premium",
                theme.accent_red,
                &["Anthropic Opus", "OpenAI GPT-4o", "Mid tier"],
                theme,
            ))
            .child(Self::chain_row(
                "Mid",
                theme.accent_yellow,
                &["Anthropic Sonnet", "OpenAI Mini", "Budget tier"],
                theme,
            ))
            .child(Self::chain_row(
                "Budget",
                theme.accent_green,
                &["Anthropic Haiku", "DeepSeek V3", "Local (free)"],
                theme,
            ))
            .child(Self::chain_row(
                "Free",
                theme.accent_aqua,
                &["Ollama", "LM Studio", "Generic Local"],
                theme,
            ))
    }

    fn chain_row(
        tier: &str,
        color: Hsla,
        chain: &[&str],
        theme: &HiveTheme,
    ) -> impl IntoElement {
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            .child(
                div()
                    .w(px(70.0))
                    .text_size(theme.font_size_sm)
                    .text_color(color)
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("{tier}:")),
            );

        for (i, step) in chain.iter().enumerate() {
            if i > 0 {
                row = row.child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("\u{2192}"),
                );
            }
            row = row.child(
                div()
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(step.to_string()),
            );
        }

        row
    }

    // ------------------------------------------------------------------
    // Custom rules (data-driven)
    // ------------------------------------------------------------------

    fn custom_rules_card(data: &RoutingData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::custom_rules_header(theme))
            .child(Self::section_desc(
                "Override automatic routing with custom conditions",
                theme,
            ));

        if data.custom_rules.is_empty() {
            container = container.child(Self::centered_empty("No custom rules configured", theme));
        } else {
            for (i, rule) in data.custom_rules.iter().enumerate() {
                container = container.child(Self::rule_entry(rule, i + 1, theme));
            }
        }

        container
    }

    fn custom_rules_header(theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .child(Self::section_title("Custom Rules", theme)),
            )
            .child(Self::add_rule_button(theme))
    }

    fn add_rule_button(theme: &HiveTheme) -> Stateful<Div> {
        div()
            .id("btn-add-rule")
            .px(theme.space_3)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_xs)
            .text_color(theme.accent_cyan)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                window.dispatch_action(Box::new(RoutingAddRule), cx);
            })
            .child("+ Add Rule")
    }

    fn centered_empty(msg: &str, theme: &HiveTheme) -> Div {
        div()
            .py(theme.space_4)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child(msg.to_string()),
            )
    }

    fn rule_entry(rule: &RoutingRule, priority: usize, theme: &HiveTheme) -> impl IntoElement {
        let enabled_color = if rule.enabled {
            theme.accent_green
        } else {
            theme.text_muted
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            .px(theme.space_2)
            .rounded(theme.radius_sm)
            .hover(|style: StyleRefinement| style.bg(theme.bg_tertiary))
            .child(Self::priority_badge(priority, theme))
            .child(Self::rule_name_cell(&rule.name, theme))
            .child(Self::rule_condition_cell(&rule.condition, theme))
            .child(Self::arrow_indicator(theme))
            .child(Self::rule_target_cell(&rule.target_model, theme))
            .child(Self::enabled_badge(rule.enabled, enabled_color, theme))
    }

    fn priority_badge(priority: usize, theme: &HiveTheme) -> Div {
        div()
            .w(px(24.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .child(format!("{}", priority))
    }

    fn rule_name_cell(name: &str, theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_sm)
            .text_color(theme.accent_aqua)
            .font_weight(FontWeight::MEDIUM)
            .child(name.to_string())
    }

    fn rule_condition_cell(condition: &str, theme: &HiveTheme) -> Div {
        div()
            .flex_1()
            .text_size(theme.font_size_sm)
            .text_color(theme.text_primary)
            .child(condition.to_string())
    }

    fn arrow_indicator(theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .child("\u{2192}")
    }

    fn rule_target_cell(target: &str, theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_sm)
            .text_color(theme.accent_cyan)
            .child(target.to_string())
    }

    fn enabled_badge(enabled: bool, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_xs)
            .text_color(color)
            .child(if enabled { "ON" } else { "OFF" })
    }

    // ------------------------------------------------------------------
    // Hard rules
    // ------------------------------------------------------------------

    fn hard_rules_card(theme: &HiveTheme) -> impl IntoElement {
        let card_shell = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::section_title("Hard Rules", theme))
            .child(Self::section_desc(
                "These rules override scoring and force specific tiers",
                theme,
            ));

        Self::build_hard_rules_list(card_shell, theme)
    }

    fn build_hard_rules_list(container: Div, theme: &HiveTheme) -> Div {
        container
            .child(Self::hard_rule_row(
                "\u{1F512}",
                "Security tasks",
                "Always Premium",
                theme.accent_red,
                theme,
            ))
            .child(Self::hard_rule_row(
                "\u{1F3D7}\u{FE0F}",
                "Architecture tasks",
                "Always Premium",
                theme.accent_red,
                theme,
            ))
            .child(Self::hard_rule_row(
                "\u{1F41B}",
                "Debugging with errors",
                "Always Premium",
                theme.accent_red,
                theme,
            ))
            .child(Self::hard_rule_row(
                "\u{2753}",
                "Simple questions (<50 tok)",
                "Always Budget",
                theme.accent_green,
                theme,
            ))
            .child(Self::hard_rule_row(
                "\u{1F510}",
                "Privacy mode enabled",
                "Local only (Free)",
                theme.accent_aqua,
                theme,
            ))
    }

    fn hard_rule_row(
        icon: &str,
        condition: &str,
        result: &str,
        color: Hsla,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            .child(
                div()
                    .w(px(20.0))
                    .text_size(theme.font_size_sm)
                    .child(icon.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_primary)
                    .child(condition.to_string()),
            )
            .child(Self::tier_badge(result, color, theme))
    }

    // ------------------------------------------------------------------
    // Shared helpers
    // ------------------------------------------------------------------

    fn section_title(text: &str, theme: &HiveTheme) -> impl IntoElement {
        div()
            .text_size(theme.font_size_lg)
            .text_color(theme.text_primary)
            .font_weight(FontWeight::SEMIBOLD)
            .child(text.to_string())
    }

    fn section_desc(text: &str, theme: &HiveTheme) -> impl IntoElement {
        div()
            .text_size(theme.font_size_sm)
            .text_color(theme.text_muted)
            .mb(theme.space_1)
            .child(text.to_string())
    }
}

