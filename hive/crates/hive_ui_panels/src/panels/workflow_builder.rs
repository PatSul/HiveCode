//! Visual Workflow Builder — drag-and-drop node canvas for wiring agents,
//! steps, and conditions into executable automation workflows.

use gpui::prelude::FluentBuilder;
use gpui::*;
use serde::{Deserialize, Serialize};

use hive_agents::automation::{
    ActionType, Condition, TriggerType, Workflow, WorkflowStatus, WorkflowStep,
};
use hive_agents::personas::PersonaKind;
use hive_ui_core::HiveTheme;

// ---------------------------------------------------------------------------
// Canvas data model
// ---------------------------------------------------------------------------

/// The kind of node on the workflow canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Starting point — defines the trigger that kicks off the workflow.
    Trigger,
    /// A concrete action step (run command, call API, send notification, etc.).
    Action,
    /// A conditional branch — routes execution based on a condition.
    Condition,
    /// Terminal output node — marks the end of a branch.
    Output,
}

/// A visual node on the workflow canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub action: Option<ActionType>,
    pub trigger: Option<TriggerType>,
    pub conditions: Vec<Condition>,
    pub persona: Option<PersonaKind>,
    pub timeout_secs: Option<u64>,
    pub retry_count: u32,
}

impl CanvasNode {
    pub fn new_trigger(x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Trigger,
            label: "Trigger".into(),
            x,
            y,
            width: 160.0,
            height: 60.0,
            action: None,
            trigger: Some(TriggerType::ManualTrigger),
            conditions: Vec::new(),
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }

    pub fn new_action(label: &str, action: ActionType, x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Action,
            label: label.into(),
            x,
            y,
            width: 180.0,
            height: 70.0,
            action: Some(action),
            trigger: None,
            conditions: Vec::new(),
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }

    pub fn new_condition(label: &str, conditions: Vec<Condition>, x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Condition,
            label: label.into(),
            x,
            y,
            width: 160.0,
            height: 70.0,
            action: None,
            trigger: None,
            conditions,
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }

    pub fn new_output(x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Output,
            label: "End".into(),
            x,
            y,
            width: 120.0,
            height: 50.0,
            action: None,
            trigger: None,
            conditions: Vec::new(),
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }
}

/// A port on a node where edges can connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Port {
    Output,
    TrueOutput,
    FalseOutput,
    Input,
}

/// A directed edge between two ports on two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    pub from_node_id: String,
    pub from_port: Port,
    pub to_node_id: String,
    pub to_port: Port,
    pub label: Option<String>,
}

/// Full serialisable state of the workflow canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCanvasState {
    pub workflow_id: String,
    pub name: String,
    pub description: String,
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
    pub canvas_offset_x: f64,
    pub canvas_offset_y: f64,
    pub zoom: f64,
}

impl WorkflowCanvasState {
    pub fn empty(name: &str) -> Self {
        Self {
            workflow_id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            nodes: vec![CanvasNode::new_trigger(100.0, 200.0)],
            edges: Vec::new(),
            canvas_offset_x: 0.0,
            canvas_offset_y: 0.0,
            zoom: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when a workflow is saved from the builder.
#[derive(Debug, Clone)]
pub struct WorkflowSaved(pub String);

/// Emitted when the user wants to run the current workflow.
#[derive(Debug, Clone)]
pub struct WorkflowRunRequested(pub String);

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

/// Workflow list entry for the left sidebar.
#[derive(Debug, Clone)]
pub struct WorkflowListEntry {
    pub id: String,
    pub name: String,
    pub is_builtin: bool,
    pub status: String,
}

struct DragState {
    node_id: String,
    start_x: f64,
    start_y: f64,
    node_start_x: f64,
    node_start_y: f64,
}

pub struct WorkflowBuilderView {
    theme: HiveTheme,

    // Canvas state
    canvas: WorkflowCanvasState,

    // Interaction
    selected_node_id: Option<String>,
    dragging_node: Option<DragState>,
    connecting_from: Option<(String, Port)>,

    // Viewport
    canvas_offset: (f64, f64),
    zoom: f64,

    // UI panels
    show_node_palette: bool,
    show_properties_panel: bool,

    // Workflow list
    workflow_list: Vec<WorkflowListEntry>,
    active_workflow_id: Option<String>,

    // Dirty flag
    is_dirty: bool,
}

impl EventEmitter<WorkflowSaved> for WorkflowBuilderView {}
impl EventEmitter<WorkflowRunRequested> for WorkflowBuilderView {}

impl WorkflowBuilderView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            theme: HiveTheme::dark(),
            canvas: WorkflowCanvasState::empty("New Workflow"),
            selected_node_id: None,
            dragging_node: None,
            connecting_from: None,
            canvas_offset: (0.0, 0.0),
            zoom: 1.0,
            show_node_palette: true,
            show_properties_panel: false,
            workflow_list: Vec::new(),
            active_workflow_id: None,
            is_dirty: false,
        }
    }

    /// Refresh the workflow list from the automation service.
    pub fn refresh_workflow_list(&mut self, workflows: Vec<WorkflowListEntry>, cx: &mut Context<Self>) {
        self.workflow_list = workflows;
        cx.notify();
    }

    /// Load a workflow canvas state.
    pub fn load_canvas(&mut self, canvas: WorkflowCanvasState, cx: &mut Context<Self>) {
        self.canvas = canvas;
        self.active_workflow_id = Some(self.canvas.workflow_id.clone());
        self.selected_node_id = None;
        self.is_dirty = false;
        cx.notify();
    }

    /// Add a node to the canvas.
    pub fn add_node(&mut self, node: CanvasNode, cx: &mut Context<Self>) {
        self.canvas.nodes.push(node);
        self.is_dirty = true;
        cx.notify();
    }

    /// Remove a node and its connected edges.
    pub fn delete_node(&mut self, node_id: &str, cx: &mut Context<Self>) {
        self.canvas.nodes.retain(|n| n.id != node_id);
        self.canvas
            .edges
            .retain(|e| e.from_node_id != node_id && e.to_node_id != node_id);
        if self.selected_node_id.as_deref() == Some(node_id) {
            self.selected_node_id = None;
        }
        self.is_dirty = true;
        cx.notify();
    }

    /// Connect two nodes via an edge.
    pub fn connect_nodes(
        &mut self,
        from_id: &str,
        from_port: Port,
        to_id: &str,
        to_port: Port,
        cx: &mut Context<Self>,
    ) {
        let edge = CanvasEdge {
            id: uuid::Uuid::new_v4().to_string(),
            from_node_id: from_id.into(),
            from_port,
            to_node_id: to_id.into(),
            to_port,
            label: None,
        };
        self.canvas.edges.push(edge);
        self.is_dirty = true;
        cx.notify();
    }

    /// Convert the current canvas to an executable automation `Workflow`.
    pub fn to_executable_workflow(&self) -> Workflow {
        let mut steps: Vec<WorkflowStep> = Vec::new();

        // Walk nodes in topological order (simplified: just iterate non-trigger
        // action nodes in the order they appear).
        for node in &self.canvas.nodes {
            if node.kind == NodeKind::Action {
                if let Some(ref action) = node.action {
                    steps.push(WorkflowStep {
                        id: node.id.clone(),
                        name: node.label.clone(),
                        action: action.clone(),
                        conditions: node.conditions.clone(),
                        timeout_secs: node.timeout_secs,
                        retry_count: node.retry_count,
                    });
                }
            }
        }

        // Find trigger
        let trigger = self
            .canvas
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Trigger)
            .and_then(|n| n.trigger.clone())
            .unwrap_or(TriggerType::ManualTrigger);

        Workflow {
            id: self.canvas.workflow_id.clone(),
            name: self.canvas.name.clone(),
            description: self.canvas.description.clone(),
            trigger,
            steps,
            status: WorkflowStatus::Active,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_run: None,
            run_count: 0,
        }
    }

    // -- Render helpers -------------------------------------------------------

    fn node_color(&self, kind: NodeKind) -> Hsla {
        match kind {
            NodeKind::Trigger => self.theme.accent_green,
            NodeKind::Action => self.theme.accent_cyan,
            NodeKind::Condition => self.theme.accent_yellow,
            NodeKind::Output => self.theme.accent_pink,
        }
    }

    fn render_node_palette(&self, theme: &HiveTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let palette_items = [
            ("Trigger", NodeKind::Trigger),
            ("Run Command", NodeKind::Action),
            ("Call API", NodeKind::Action),
            ("Send Notification", NodeKind::Action),
            ("Execute Skill", NodeKind::Action),
            ("Condition", NodeKind::Condition),
            ("End", NodeKind::Output),
        ];

        let mut items: Vec<AnyElement> = Vec::new();
        for (label, kind) in &palette_items {
            let color = self.node_color(*kind);
            let mut bg = color;
            bg.a = 0.15;
            let label_str = label.to_string();
            let kind_copy = *kind;

            items.push(
                div()
                    .id(ElementId::Name(format!("palette-{label}").into()))
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_md)
                    .bg(bg)
                    .text_size(theme.font_size_xs)
                    .text_color(color)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_surface))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e, _w, cx| {
                            let node = match kind_copy {
                                NodeKind::Trigger => CanvasNode::new_trigger(300.0, 200.0),
                                NodeKind::Action => CanvasNode::new_action(
                                    &label_str,
                                    ActionType::RunCommand {
                                        command: String::new(),
                                    },
                                    300.0,
                                    200.0,
                                ),
                                NodeKind::Condition => {
                                    CanvasNode::new_condition(&label_str, Vec::new(), 300.0, 200.0)
                                }
                                NodeKind::Output => CanvasNode::new_output(300.0, 200.0),
                            };
                            this.add_node(node, cx);
                        }),
                    )
                    .child(label.to_string())
                    .into_any_element(),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(theme.space_1)
            .w(px(200.0))
            .min_w(px(200.0))
            .border_r_1()
            .border_color(theme.border)
            .p(theme.space_3)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::BOLD)
                    .pb(theme.space_2)
                    .child("NODE PALETTE"),
            )
            .children(items)
            .child(
                div()
                    .mt(theme.space_4)
                    .border_t_1()
                    .border_color(theme.border)
                    .pt(theme.space_3)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .font_weight(FontWeight::BOLD)
                            .pb(theme.space_2)
                            .child("WORKFLOWS"),
                    )
                    .children(self.workflow_list.iter().map(|wf| {
                        let is_active = self.active_workflow_id.as_deref() == Some(&wf.id);
                        div()
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .text_size(theme.font_size_xs)
                            .text_color(if is_active {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .when(is_active, |el| el.bg(theme.bg_surface))
                            .child(wf.name.clone())
                            .into_any_element()
                    })),
            )
    }

    fn render_canvas_nodes(&self, theme: &HiveTheme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut elements: Vec<AnyElement> = Vec::new();

        for node in &self.canvas.nodes {
            let color = self.node_color(node.kind);
            let mut bg = color;
            bg.a = 0.12;
            let is_selected = self.selected_node_id.as_deref() == Some(&node.id);
            let node_id = node.id.clone();

            let node_el = div()
                .id(ElementId::Name(format!("node-{}", node.id).into()))
                .absolute()
                .left(px(node.x as f32))
                .top(px(node.y as f32))
                .w(px(node.width as f32))
                .h(px(node.height as f32))
                .rounded(theme.radius_md)
                .bg(bg)
                .border_1()
                .border_color(if is_selected { color } else { theme.border })
                .when(is_selected, |el| el.border_2())
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _e, _w, cx| {
                        this.selected_node_id = Some(node_id.clone());
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(color)
                                .font_weight(FontWeight::BOLD)
                                .child(match node.kind {
                                    NodeKind::Trigger => "\u{25B6}",
                                    NodeKind::Action => "\u{2699}",
                                    NodeKind::Condition => "\u{2747}",
                                    NodeKind::Output => "\u{2713}",
                                }),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_primary)
                                .child(node.label.clone()),
                        )
                        .when(node.persona.is_some(), |el| {
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(theme.text_muted)
                                    .child(format!(
                                        "{:?}",
                                        node.persona.as_ref().unwrap()
                                    )),
                            )
                        }),
                )
                .into_any_element();

            elements.push(node_el);
        }

        // Render edges as simple colored lines using positioned divs
        for edge in &self.canvas.edges {
            let from_node = self.canvas.nodes.iter().find(|n| n.id == edge.from_node_id);
            let to_node = self.canvas.nodes.iter().find(|n| n.id == edge.to_node_id);
            if let (Some(from), Some(to)) = (from_node, to_node) {
                let from_x = (from.x + from.width) as f32;
                let from_y = (from.y + from.height / 2.0) as f32;
                let to_x = to.x as f32;
                let to_y = (to.y + to.height / 2.0) as f32;

                let mid_x = (from_x + to_x) / 2.0;

                // Horizontal segment from source
                let h1_x = from_x.min(mid_x);
                let h1_w = (mid_x - from_x).abs().max(1.0);
                elements.push(
                    div()
                        .absolute()
                        .left(px(h1_x))
                        .top(px(from_y - 1.0))
                        .w(px(h1_w))
                        .h(px(2.0))
                        .bg(self.theme.accent_cyan)
                        .into_any_element(),
                );

                // Vertical connector
                let v_top = from_y.min(to_y);
                let v_h = (to_y - from_y).abs().max(1.0);
                elements.push(
                    div()
                        .absolute()
                        .left(px(mid_x - 1.0))
                        .top(px(v_top))
                        .w(px(2.0))
                        .h(px(v_h))
                        .bg(self.theme.accent_cyan)
                        .into_any_element(),
                );

                // Horizontal segment to target
                let h2_x = mid_x.min(to_x);
                let h2_w = (to_x - mid_x).abs().max(1.0);
                elements.push(
                    div()
                        .absolute()
                        .left(px(h2_x))
                        .top(px(to_y - 1.0))
                        .w(px(h2_w))
                        .h(px(2.0))
                        .bg(self.theme.accent_cyan)
                        .into_any_element(),
                );
            }
        }

        elements
    }

    fn render_properties_panel(&self, theme: &HiveTheme) -> impl IntoElement {
        let Some(ref node_id) = self.selected_node_id else {
            return div()
                .w(px(280.0))
                .min_w(px(280.0))
                .border_l_1()
                .border_color(theme.border)
                .p(theme.space_3)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Select a node to view properties"),
                );
        };

        let node = self.canvas.nodes.iter().find(|n| n.id == *node_id);

        div()
            .w(px(280.0))
            .min_w(px(280.0))
            .border_l_1()
            .border_color(theme.border)
            .p(theme.space_3)
            .flex()
            .flex_col()
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::BOLD)
                    .child("PROPERTIES"),
            )
            .when_some(node, |el, node| {
                el.child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child(node.label.clone()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("Type: {:?}", node.kind)),
                )
                .when(node.action.is_some(), |el| {
                    el.child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .child(format!("Action: {:?}", node.action.as_ref().unwrap())),
                    )
                })
                .when(node.persona.is_some(), |el| {
                    el.child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_aqua)
                            .child(format!(
                                "Agent: {:?}",
                                node.persona.as_ref().unwrap()
                            )),
                    )
                })
            })
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for WorkflowBuilderView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let node_count = self.canvas.nodes.len();
        let edge_count = self.canvas.edges.len();

        // Header
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px(theme.space_4)
            .py(theme.space_3)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_3)
                    .child(
                        div()
                            .text_size(theme.font_size_lg)
                            .text_color(theme.text_primary)
                            .font_weight(FontWeight::BOLD)
                            .child("Workflow Builder"),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(format!(
                                "{} \u{2014} {} nodes \u{00B7} {} edges",
                                self.canvas.name, node_count, edge_count
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(theme.space_2)
                    // Save button
                    .child(
                        div()
                            .id("wf-save-btn")
                            .px(theme.space_3)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .bg(if self.is_dirty {
                                theme.accent_cyan
                            } else {
                                theme.bg_tertiary
                            })
                            .text_size(theme.font_size_sm)
                            .text_color(if self.is_dirty {
                                theme.bg_primary
                            } else {
                                theme.text_muted
                            })
                            .cursor_pointer()
                            .child("Save"),
                    )
                    // Run button
                    .child(
                        div()
                            .id("wf-run-btn")
                            .px(theme.space_3)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .bg(theme.accent_green)
                            .text_size(theme.font_size_sm)
                            .text_color(theme.bg_primary)
                            .font_weight(FontWeight::BOLD)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _e, _w, cx| {
                                    let wf_id = this.canvas.workflow_id.clone();
                                    cx.emit(WorkflowRunRequested(wf_id));
                                }),
                            )
                            .child("\u{25B6} Run"),
                    ),
            );

        // Canvas area with nodes
        let canvas_elements = self.render_canvas_nodes(theme, cx);
        let canvas_area = div()
            .id("wf-canvas")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .relative()
            .overflow_hidden()
            .bg(theme.bg_primary)
            .children(canvas_elements);

        // Node palette (left)
        let palette = self
            .render_node_palette(theme, cx)
            .into_any_element();

        // Properties (right)
        let properties = self.render_properties_panel(theme).into_any_element();

        div()
            .id("workflow-builder-panel")
            .flex()
            .flex_col()
            .size_full()
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(palette)
                    .child(canvas_area)
                    .when(self.show_properties_panel || self.selected_node_id.is_some(), |el| {
                        el.child(properties)
                    }),
            )
    }
}
