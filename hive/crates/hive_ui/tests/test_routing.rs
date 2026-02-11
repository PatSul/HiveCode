use hive_ui::panels::routing::*;
use hive_ai::routing::{ModelRouter, ProviderType};
use hive_ai::ModelTier;

#[test]
fn empty_data_has_no_content() {
    let data = RoutingData::empty();
    assert!(data.task_mappings.is_empty());
    assert!(data.custom_rules.is_empty());
    assert!(data.provider_status.is_empty());
}

#[test]
fn sample_data_has_content() {
    let data = RoutingData::sample();
    assert!(!data.task_mappings.is_empty());
    assert!(!data.custom_rules.is_empty());
    assert!(!data.provider_status.is_empty());
}

#[test]
fn health_status_labels() {
    assert_eq!(ProviderHealth::Healthy.label(), "Healthy");
    assert_eq!(ProviderHealth::Degraded.label(), "Degraded");
    assert_eq!(ProviderHealth::Down.label(), "Down");
}

#[test]
fn from_router_produces_task_mappings() {
    let router = ModelRouter::new();
    router
        .fallback_manager()
        .set_available(ProviderType::Anthropic, true);

    let data = RoutingData::from_router(&router);

    // Should have mappings for all known task types
    assert_eq!(data.task_mappings.len(), 12);

    // Verify a known mapping
    let arch = data
        .task_mappings
        .iter()
        .find(|m| m.task_type == "Architecture")
        .expect("Architecture mapping should exist");
    assert_eq!(arch.tier, "Premium");
    assert_eq!(arch.model_id, "claude-opus-4-20250514");
}

#[test]
fn from_router_populates_provider_status() {
    let router = ModelRouter::new();
    router
        .fallback_manager()
        .set_available(ProviderType::Anthropic, true);
    router
        .fallback_manager()
        .set_available(ProviderType::OpenAI, true);

    let data = RoutingData::from_router(&router);

    let anthropic = data
        .provider_status
        .iter()
        .find(|p| p.name == "Anthropic")
        .expect("Anthropic should be present");
    assert_eq!(anthropic.status, ProviderHealth::Healthy);

    let openai = data
        .provider_status
        .iter()
        .find(|p| p.name == "OpenAI")
        .expect("OpenAI should be present");
    assert_eq!(openai.status, ProviderHealth::Healthy);
}

#[test]
fn from_router_shows_degraded_on_failures() {
    let router = ModelRouter::new();
    router
        .fallback_manager()
        .set_available(ProviderType::OpenRouter, true);
    router.record_result(
        ProviderType::OpenRouter,
        false,
        Some(hive_ai::routing::FallbackReason::ServerError),
    );

    let data = RoutingData::from_router(&router);

    let or_entry = data
        .provider_status
        .iter()
        .find(|p| p.name == "OpenRouter")
        .expect("OpenRouter should be present");
    assert_eq!(or_entry.status, ProviderHealth::Degraded);
    assert_eq!(or_entry.failures, 1);
}

#[test]
fn from_router_shows_down_when_unavailable() {
    let router = ModelRouter::new();
    // By default providers are unavailable
    let data = RoutingData::from_router(&router);

    for entry in &data.provider_status {
        assert_eq!(entry.status, ProviderHealth::Down);
    }
}

#[test]
fn tier_for_task_covers_all_categories() {
    assert_eq!(tier_for_task("Simple Question"), ModelTier::Budget);
    assert_eq!(tier_for_task("Code Explanation"), ModelTier::Budget);
    assert_eq!(tier_for_task("Documentation"), ModelTier::Budget);
    assert_eq!(tier_for_task("Code Generation"), ModelTier::Mid);
    assert_eq!(tier_for_task("Bug Fix"), ModelTier::Mid);
    assert_eq!(tier_for_task("Refactoring"), ModelTier::Mid);
    assert_eq!(tier_for_task("Testing"), ModelTier::Mid);
    assert_eq!(tier_for_task("Research"), ModelTier::Mid);
    assert_eq!(tier_for_task("Creative Writing"), ModelTier::Mid);
    assert_eq!(tier_for_task("Architecture"), ModelTier::Premium);
    assert_eq!(tier_for_task("Security Review"), ModelTier::Premium);
    assert_eq!(tier_for_task("Debugging"), ModelTier::Premium);
    // Unknown defaults to Mid
    assert_eq!(tier_for_task("Unknown Task"), ModelTier::Mid);
}

#[test]
fn sample_task_mappings_have_valid_tiers() {
    let data = RoutingData::sample();
    let valid_tiers = ["Free", "Budget", "Mid", "Premium"];
    for mapping in &data.task_mappings {
        assert!(
            valid_tiers.contains(&mapping.tier.as_str()),
            "Invalid tier: {}",
            mapping.tier
        );
        assert!(
            mapping.performance_score >= 0.0 && mapping.performance_score <= 1.0,
            "Score out of range: {}",
            mapping.performance_score
        );
    }
}

// -- PerformanceMetrics tests --

#[test]
fn metrics_from_empty_data() {
    let data = RoutingData::empty();
    let metrics = PerformanceMetrics::from_data(&data);
    assert_eq!(metrics.total_requests, 0);
    assert_eq!(metrics.total_failures, 0);
    assert_eq!(metrics.avg_latency_ms, 0.0);
    assert_eq!(metrics.healthy_providers, 0);
    assert_eq!(metrics.total_providers, 0);
    assert_eq!(metrics.active_rules, 0);
    assert_eq!(metrics.total_rules, 0);
}

#[test]
fn metrics_success_rate_with_no_requests() {
    let data = RoutingData::empty();
    let metrics = PerformanceMetrics::from_data(&data);
    assert_eq!(metrics.success_rate(), 100.0);
}

#[test]
fn metrics_from_sample_data() {
    let data = RoutingData::sample();
    let metrics = PerformanceMetrics::from_data(&data);

    // Sample has 170+67+8+23 = 268 total requests
    assert_eq!(metrics.total_requests, 268);
    // Sample has 2+1+1+0 = 4 total failures
    assert_eq!(metrics.total_failures, 4);
    // 4 providers total
    assert_eq!(metrics.total_providers, 4);
    // Anthropic, OpenAI, Ollama are Healthy; OpenRouter is Degraded
    assert_eq!(metrics.healthy_providers, 3);
    // 2 enabled custom rules out of 3
    assert_eq!(metrics.active_rules, 2);
    assert_eq!(metrics.total_rules, 3);
    // Average latency should be > 0
    assert!(metrics.avg_latency_ms > 0.0);
}

#[test]
fn metrics_success_rate_with_failures() {
    let data = RoutingData {
        task_mappings: Vec::new(),
        custom_rules: Vec::new(),
        provider_status: vec![ProviderStatusEntry {
            name: "Test".into(),
            status: ProviderHealth::Degraded,
            requests: 100,
            failures: 5,
            avg_latency_ms: 500.0,
        }],
    };
    let metrics = PerformanceMetrics::from_data(&data);
    assert_eq!(metrics.success_rate(), 95.0);
}

#[test]
fn metrics_avg_latency_excludes_zero() {
    let data = RoutingData {
        task_mappings: Vec::new(),
        custom_rules: Vec::new(),
        provider_status: vec![
            ProviderStatusEntry {
                name: "A".into(),
                status: ProviderHealth::Healthy,
                requests: 10,
                failures: 0,
                avg_latency_ms: 1000.0,
            },
            ProviderStatusEntry {
                name: "B".into(),
                status: ProviderHealth::Down,
                requests: 0,
                failures: 0,
                avg_latency_ms: 0.0,
            },
        ],
    };
    let metrics = PerformanceMetrics::from_data(&data);
    // Only provider A has latency data, so avg should be 1000.0
    assert_eq!(metrics.avg_latency_ms, 1000.0);
}
