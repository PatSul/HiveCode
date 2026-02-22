use hive_agents::automation::{ActionType, Condition, ConditionOp};
use hive_ui_panels::panels::workflow_builder::{
    CanvasEdge, CanvasNode, NodeKind, Port, WorkflowCanvasState,
};

// ---------------------------------------------------------------------------
// WorkflowCanvasState::empty
// ---------------------------------------------------------------------------

#[test]
fn empty_state_has_name() {
    let state = WorkflowCanvasState::empty("My Workflow");
    assert_eq!(state.name, "My Workflow");
}

#[test]
fn empty_state_has_one_trigger_node() {
    let state = WorkflowCanvasState::empty("test");
    assert_eq!(state.nodes.len(), 1);
    assert_eq!(state.nodes[0].kind, NodeKind::Trigger);
}

#[test]
fn empty_state_has_no_edges() {
    let state = WorkflowCanvasState::empty("test");
    assert!(state.edges.is_empty());
}

#[test]
fn empty_state_zoom_is_one() {
    let state = WorkflowCanvasState::empty("test");
    assert!((state.zoom - 1.0).abs() < f64::EPSILON);
}

#[test]
fn empty_state_has_uuid() {
    let state = WorkflowCanvasState::empty("test");
    assert!(!state.workflow_id.is_empty());
    // UUID v4 has hyphens
    assert!(state.workflow_id.contains('-'));
}

#[test]
fn two_empty_states_have_different_ids() {
    let a = WorkflowCanvasState::empty("a");
    let b = WorkflowCanvasState::empty("b");
    assert_ne!(a.workflow_id, b.workflow_id);
}

// ---------------------------------------------------------------------------
// CanvasNode constructors
// ---------------------------------------------------------------------------

#[test]
fn new_trigger_kind_and_label() {
    let node = CanvasNode::new_trigger(100.0, 200.0);
    assert_eq!(node.kind, NodeKind::Trigger);
    assert_eq!(node.x, 100.0);
    assert_eq!(node.y, 200.0);
}

#[test]
fn new_trigger_dimensions() {
    let node = CanvasNode::new_trigger(0.0, 0.0);
    assert_eq!(node.width, 160.0);
    assert_eq!(node.height, 60.0);
}

#[test]
fn new_action_kind_and_label() {
    let action = ActionType::RunCommand {
        command: "echo hi".into(),
    };
    let node = CanvasNode::new_action("Run Echo", action, 50.0, 100.0);
    assert_eq!(node.kind, NodeKind::Action);
    assert_eq!(node.label, "Run Echo");
    assert_eq!(node.x, 50.0);
    assert_eq!(node.y, 100.0);
}

#[test]
fn new_action_dimensions() {
    let action = ActionType::RunCommand {
        command: "ls".into(),
    };
    let node = CanvasNode::new_action("LS", action, 0.0, 0.0);
    assert_eq!(node.width, 180.0);
    assert_eq!(node.height, 70.0);
}

#[test]
fn new_condition_kind() {
    let conds = vec![Condition {
        field: "status".into(),
        operator: ConditionOp::Equals,
        value: "ok".into(),
    }];
    let node = CanvasNode::new_condition("Check Status", conds, 0.0, 0.0);
    assert_eq!(node.kind, NodeKind::Condition);
    assert_eq!(node.label, "Check Status");
    assert_eq!(node.width, 160.0);
    assert_eq!(node.height, 70.0);
}

#[test]
fn new_output_kind_and_label() {
    let node = CanvasNode::new_output(300.0, 400.0);
    assert_eq!(node.kind, NodeKind::Output);
    assert_eq!(node.label, "End");
    assert_eq!(node.width, 120.0);
    assert_eq!(node.height, 50.0);
}

#[test]
fn node_ids_are_unique() {
    let a = CanvasNode::new_trigger(0.0, 0.0);
    let b = CanvasNode::new_trigger(0.0, 0.0);
    assert_ne!(a.id, b.id);
}

// ---------------------------------------------------------------------------
// CanvasEdge
// ---------------------------------------------------------------------------

#[test]
fn canvas_edge_construction() {
    let edge = CanvasEdge {
        id: "e1".into(),
        from_node_id: "n1".into(),
        from_port: Port::Output,
        to_node_id: "n2".into(),
        to_port: Port::Input,
        label: Some("yes".into()),
    };
    assert_eq!(edge.from_port, Port::Output);
    assert_eq!(edge.to_port, Port::Input);
    assert_eq!(edge.label.as_deref(), Some("yes"));
}

#[test]
fn canvas_edge_no_label() {
    let edge = CanvasEdge {
        id: "e2".into(),
        from_node_id: "n1".into(),
        from_port: Port::TrueOutput,
        to_node_id: "n3".into(),
        to_port: Port::Input,
        label: None,
    };
    assert!(edge.label.is_none());
}

// ---------------------------------------------------------------------------
// Port and NodeKind equality
// ---------------------------------------------------------------------------

#[test]
fn port_equality() {
    assert_eq!(Port::Output, Port::Output);
    assert_ne!(Port::Output, Port::Input);
    assert_ne!(Port::TrueOutput, Port::FalseOutput);
}

#[test]
fn node_kind_equality() {
    assert_eq!(NodeKind::Trigger, NodeKind::Trigger);
    assert_ne!(NodeKind::Action, NodeKind::Condition);
}

// ---------------------------------------------------------------------------
// JSON serde roundtrip
// ---------------------------------------------------------------------------

#[test]
fn workflow_state_serde_roundtrip() {
    let state = WorkflowCanvasState::empty("Serde Test");
    let json = serde_json::to_string(&state).unwrap();
    let restored: WorkflowCanvasState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.name, "Serde Test");
    assert_eq!(restored.workflow_id, state.workflow_id);
    assert_eq!(restored.nodes.len(), 1);
    assert_eq!(restored.nodes[0].kind, NodeKind::Trigger);
}

#[test]
fn canvas_node_serde_roundtrip() {
    let node = CanvasNode::new_output(10.0, 20.0);
    let json = serde_json::to_string(&node).unwrap();
    let restored: CanvasNode = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.kind, NodeKind::Output);
    assert_eq!(restored.label, "End");
    assert_eq!(restored.x, 10.0);
}

#[test]
fn canvas_edge_serde_roundtrip() {
    let edge = CanvasEdge {
        id: "e1".into(),
        from_node_id: "n1".into(),
        from_port: Port::TrueOutput,
        to_node_id: "n2".into(),
        to_port: Port::Input,
        label: Some("branch".into()),
    };
    let json = serde_json::to_string(&edge).unwrap();
    let restored: CanvasEdge = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.from_port, Port::TrueOutput);
    assert_eq!(restored.label.as_deref(), Some("branch"));
}
