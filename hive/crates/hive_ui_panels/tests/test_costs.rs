use hive_ai::CostTracker;
use hive_ui_panels::panels::costs::{CostData, ModelCostEntry};

// ---------------------------------------------------------------------------
// CostData::empty
// ---------------------------------------------------------------------------

#[test]
fn empty_has_zero_values() {
    let data = CostData::empty();
    assert_eq!(data.today_cost, 0.0);
    assert_eq!(data.all_time_cost, 0.0);
    assert_eq!(data.total_requests, 0);
    assert_eq!(data.total_input_tokens, 0);
    assert_eq!(data.total_output_tokens, 0);
    assert!(data.by_model.is_empty());
}

// ---------------------------------------------------------------------------
// CostData::from_tracker
// ---------------------------------------------------------------------------

#[test]
fn from_tracker_empty() {
    let tracker = CostTracker::default();
    let data = CostData::from_tracker(&tracker);
    assert_eq!(data.total_requests, 0);
    assert!(data.by_model.is_empty());
}

#[test]
fn from_tracker_single_record() {
    let mut tracker = CostTracker::default();
    tracker.record("claude-sonnet-4-5", 100, 50);
    let data = CostData::from_tracker(&tracker);
    assert_eq!(data.total_requests, 1);
    assert_eq!(data.by_model.len(), 1);
    assert_eq!(data.by_model[0].model_id, "claude-sonnet-4-5");
    assert_eq!(data.by_model[0].requests, 1);
    assert_eq!(data.by_model[0].input_tokens, 100);
    assert_eq!(data.by_model[0].output_tokens, 50);
}

#[test]
fn from_tracker_multi_model_sorted_by_cost() {
    let mut tracker = CostTracker::default();
    // Record smaller model first
    tracker.record("gpt-4o-mini", 10, 5);
    // Record larger model second (more tokens = more cost)
    tracker.record("claude-opus-4", 10000, 5000);
    let data = CostData::from_tracker(&tracker);

    assert_eq!(data.by_model.len(), 2);
    // Most expensive model should be first
    assert!(data.by_model[0].cost >= data.by_model[1].cost);
}

#[test]
fn from_tracker_same_model_aggregated() {
    let mut tracker = CostTracker::default();
    tracker.record("gpt-4o", 100, 50);
    tracker.record("gpt-4o", 200, 100);
    let data = CostData::from_tracker(&tracker);

    assert_eq!(data.by_model.len(), 1);
    assert_eq!(data.by_model[0].requests, 2);
    assert_eq!(data.by_model[0].input_tokens, 300);
    assert_eq!(data.by_model[0].output_tokens, 150);
}

// ---------------------------------------------------------------------------
// ModelCostEntry construction
// ---------------------------------------------------------------------------

#[test]
fn model_cost_entry_construction() {
    let entry = ModelCostEntry {
        model_id: "test-model".into(),
        requests: 42,
        input_tokens: 1000,
        output_tokens: 500,
        cost: 0.15,
    };
    assert_eq!(entry.model_id, "test-model");
    assert_eq!(entry.requests, 42);
    assert!((entry.cost - 0.15).abs() < f64::EPSILON);
}

#[test]
fn from_tracker_total_tokens_match() {
    let mut tracker = CostTracker::default();
    tracker.record("m1", 100, 50);
    tracker.record("m2", 200, 100);
    let data = CostData::from_tracker(&tracker);
    assert_eq!(data.total_input_tokens, 300);
    assert_eq!(data.total_output_tokens, 150);
}
