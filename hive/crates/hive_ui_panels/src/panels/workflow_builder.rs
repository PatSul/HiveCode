//! Visual Workflow Builder — drag-and-drop node canvas for wiring agents,
//! steps, and conditions into executable automation workflows.

use gpui::prelude::FluentBuilder;
use gpui::*;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

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

    /// Save this canvas state to ~/.hive/workflows/{workflow_id}.canvas.json
    pub fn save_to_disk(&self) -> anyhow::Result<()> {
        let dir = hive_core::config::HiveConfig::base_dir()?.join("workflows");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.canvas.json", self.workflow_id));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a canvas state from disk by workflow_id.
    pub fn load_from_disk(workflow_id: &str) -> anyhow::Result<Self> {
        let dir = hive_core::config::HiveConfig::base_dir()?.join("workflows");
        let path = dir.join(format!("{workflow_id}.canvas.json"));
        let json = std::fs::read_to_string(path)?;
        let state: Self = serde_json::from_str(&json)?;
        Ok(state)
    }

    /// List all saved canvas workflow IDs on disk.
    pub fn list_saved() -> Vec<String> {
        let dir = match hive_core::config::HiveConfig::base_dir() {
            Ok(d) => d.join("workflows"),
            Err(_) => return Vec::new(),
        };
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(id) = name.strip_suffix(".canvas.json") {
                    ids.push(id.to_string());
                }
            }
        }
        ids
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
    /// Mouse position at start of drag.
    start_x: f64,
    start_y: f64,
    /// Node position at start of drag.
    node_start_x: f64,
    node_start_y: f64,
}

/// State for panning the canvas background.
struct PanState {
    start_mouse_x: f64,
    start_mouse_y: f64,
    start_offset_x: f64,
    start_offset_y: f64,
}

pub struct WorkflowBuilderView {
    theme: HiveTheme,

    // Canvas state
    canvas: WorkflowCanvasState,

    // Interaction
    selected_node_id: Option<String>,
    dragging_node: Option<DragState>,
    connecting_from: Option<(String, Port)>,
    panning: Option<PanState>,

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
            panning: None,
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

    // -- Drag/pan/connect interaction handlers --------------------------------

    /// Start dragging a node.
    fn start_drag(&mut self, node_id: &str, mouse_x: f64, mouse_y: f64) {
        if let Some(node) = self.canvas.nodes.iter().find(|n| n.id == node_id) {
            self.dragging_node = Some(DragState {
                node_id: node_id.to_string(),
                start_x: mouse_x,
                start_y: mouse_y,
                node_start_x: node.x,
                node_start_y: node.y,
            });
        }
    }

    /// Update dragged node position based on mouse movement.
    fn update_drag(&mut self, mouse_x: f64, mouse_y: f64, cx: &mut Context<Self>) {
        if let Some(ref drag) = self.dragging_node {
            let dx = mouse_x - drag.start_x;
            let dy = mouse_y - drag.start_y;
            let new_x = (drag.node_start_x + dx).max(0.0);
            let new_y = (drag.node_start_y + dy).max(0.0);
            let nid = drag.node_id.clone();
            if let Some(node) = self.canvas.nodes.iter_mut().find(|n| n.id == nid) {
                node.x = new_x;
                node.y = new_y;
            }
            self.is_dirty = true;
            cx.notify();
        }
    }

    /// Finish dragging a node.
    fn end_drag(&mut self) {
        self.dragging_node = None;
    }

    /// Start panning the canvas.
    fn start_pan(&mut self, mouse_x: f64, mouse_y: f64) {
        self.panning = Some(PanState {
            start_mouse_x: mouse_x,
            start_mouse_y: mouse_y,
            start_offset_x: self.canvas_offset.0,
            start_offset_y: self.canvas_offset.1,
        });
    }

    /// Update pan offset based on mouse movement.
    fn update_pan(&mut self, mouse_x: f64, mouse_y: f64, cx: &mut Context<Self>) {
        if let Some(ref pan) = self.panning {
            let dx = mouse_x - pan.start_mouse_x;
            let dy = mouse_y - pan.start_mouse_y;
            self.canvas_offset.0 = pan.start_offset_x + dx;
            self.canvas_offset.1 = pan.start_offset_y + dy;
            cx.notify();
        }
    }

    /// Finish panning.
    fn end_pan(&mut self) {
        self.panning = None;
    }

    /// Start connecting from a port.
    fn start_connect(&mut self, node_id: &str, port: Port, cx: &mut Context<Self>) {
        self.connecting_from = Some((node_id.to_string(), port));
        cx.notify();
    }

    /// Finish connection at a target port.
    fn finish_connect(&mut self, target_node_id: &str, target_port: Port, cx: &mut Context<Self>) {
        if let Some((from_id, from_port)) = self.connecting_from.take() {
            // Don't connect a node to itself
            if from_id != target_node_id {
                self.connect_nodes(&from_id, from_port, target_node_id, target_port, cx);
            }
        }
        cx.notify();
    }

    /// Cancel connection.
    fn cancel_connect(&mut self, cx: &mut Context<Self>) {
        self.connecting_from = None;
        cx.notify();
    }

    /// Persist the current canvas state to disk, clear the dirty flag, and emit
    /// a [`WorkflowSaved`] event.
    pub fn save_workflow(&mut self, cx: &mut Context<Self>) {
        // Sync viewport state into the serialisable canvas model.
        self.canvas.canvas_offset_x = self.canvas_offset.0;
        self.canvas.canvas_offset_y = self.canvas_offset.1;
        self.canvas.zoom = self.zoom;

        match self.canvas.save_to_disk() {
            Ok(()) => {
                self.is_dirty = false;
                info!(
                    workflow_id = %self.canvas.workflow_id,
                    name = %self.canvas.name,
                    "Workflow canvas saved to disk"
                );
                cx.emit(WorkflowSaved(self.canvas.workflow_id.clone()));
            }
            Err(e) => {
                error!(
                    workflow_id = %self.canvas.workflow_id,
                    err = %e,
                    "Failed to save workflow canvas to disk"
                );
            }
        }
        cx.notify();
    }

    /// Port position for a node (relative to canvas). Returns (x, y) center of port.
    fn port_position(node: &CanvasNode, port: Port) -> (f64, f64) {
        match port {
            Port::Input => (node.x, node.y + node.height / 2.0),
            Port::Output => (node.x + node.width, node.y + node.height / 2.0),
            Port::TrueOutput => (node.x + node.width, node.y + node.height * 0.33),
            Port::FalseOutput => (node.x + node.width, node.y + node.height * 0.67),
        }
    }

    /// Convert the current canvas to an executable automation `Workflow`.
    pub fn to_executable_workflow(&self) -> Workflow {
        let mut steps: Vec<WorkflowStep> = Vec::new();

        // Walk nodes in topological order (simplified: just iterate non-trigger
        // action nodes in the order they appear).
        for node in &self.canvas.nodes {
            if node.kind == NodeKind::Action
                && let Some(ref action) = node.action {
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
        let offset_x = self.canvas_offset.0 as f32;
        let offset_y = self.canvas_offset.1 as f32;
        let zoom = self.zoom as f32;

        for node in &self.canvas.nodes {
            let color = self.node_color(node.kind);
            let mut bg = color;
            bg.a = 0.12;
            let is_selected = self.selected_node_id.as_deref() == Some(&node.id);
            let node_id = node.id.clone();
            let node_id2 = node.id.clone();
            let node_id_input = node.id.clone();

            // Compute display position with canvas offset, scaled by zoom
            let display_x = (node.x as f32 + offset_x) * zoom;
            let display_y = (node.y as f32 + offset_y) * zoom;
            let node_w = node.width as f32 * zoom;
            let node_h = node.height as f32 * zoom;

            // Determine which ports to show based on node kind
            let has_input = node.kind != NodeKind::Trigger;
            let has_output = node.kind == NodeKind::Trigger || node.kind == NodeKind::Action;
            let is_condition = node.kind == NodeKind::Condition;

            // Build port circles
            let mut port_elements: Vec<AnyElement> = Vec::new();

            // Input port (left side)
            if has_input {
                let nid = node_id_input.clone();
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-in-{}", node.id).into()))
                        .absolute()
                        .left(px(-5.0))
                        .top(px(node_h / 2.0 - 5.0))
                        .w(px(10.0))
                        .h(px(10.0))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_aqua)
                        .border_1()
                        .border_color(theme.bg_primary)
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                // If connecting from another node, finish the connection here
                                if this.connecting_from.is_some() {
                                    this.finish_connect(&nid, Port::Input, cx);
                                }
                                // Event handled by port — parent node handler will also fire
                                // but we accept the last-handler-wins behavior in GPUI.
                            }),
                        )
                        .into_any_element(),
                );
            }

            // Output port (right side)
            if has_output {
                let nid = node.id.clone();
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-out-{}", node.id).into()))
                        .absolute()
                        .right(px(-5.0))
                        .top(px(node_h / 2.0 - 5.0))
                        .w(px(10.0))
                        .h(px(10.0))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_cyan)
                        .border_1()
                        .border_color(theme.bg_primary)
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                this.start_connect(&nid, Port::Output, cx);
                            }),
                        )
                        .into_any_element(),
                );
            }

            // Condition node: True (top-right) and False (bottom-right) output ports
            if is_condition {
                let nid_true = node.id.clone();
                let nid_false = node.id.clone();
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-true-{}", node.id).into()))
                        .absolute()
                        .right(px(-5.0))
                        .top(px(node_h * 0.25 - 5.0))
                        .w(px(10.0))
                        .h(px(10.0))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_green)
                        .border_1()
                        .border_color(theme.bg_primary)
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                this.start_connect(&nid_true, Port::TrueOutput, cx);
                            }),
                        )
                        .into_any_element(),
                );
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-false-{}", node.id).into()))
                        .absolute()
                        .right(px(-5.0))
                        .top(px(node_h * 0.75 - 5.0))
                        .w(px(10.0))
                        .h(px(10.0))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_red)
                        .border_1()
                        .border_color(theme.bg_primary)
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                this.start_connect(&nid_false, Port::FalseOutput, cx);
                            }),
                        )
                        .into_any_element(),
                );
            }

            let node_el = div()
                .id(ElementId::Name(format!("node-{}", node.id).into()))
                .absolute()
                .left(px(display_x))
                .top(px(display_y))
                .w(px(node_w))
                .h(px(node_h))
                .rounded(theme.radius_md)
                .bg(bg)
                .border_1()
                .border_color(if is_selected { color } else { theme.border })
                .when(is_selected, |el| el.border_2())
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                        // If we're in connect mode and click a node body, finish connect
                        // to its input port
                        if this.connecting_from.is_some() {
                            this.finish_connect(&node_id2, Port::Input, cx);
                            return;
                        }
                        this.selected_node_id = Some(node_id.clone());
                        let pos = event.position;
                        this.start_drag(&node_id, f64::from(pos.x), f64::from(pos.y));
                        cx.notify();
                    }),
                )
                // Port circles
                .children(port_elements)
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
                                        node.persona.as_ref().expect("guarded by is_some check")
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
                let (fp_x, fp_y) = Self::port_position(from, edge.from_port);
                let (tp_x, tp_y) = Self::port_position(to, edge.to_port);
                let from_x = (fp_x as f32 + offset_x) * zoom;
                let from_y = (fp_y as f32 + offset_y) * zoom;
                let to_x = (tp_x as f32 + offset_x) * zoom;
                let to_y = (tp_y as f32 + offset_y) * zoom;

                // Edge color based on port type
                let edge_color = match edge.from_port {
                    Port::TrueOutput => self.theme.accent_green,
                    Port::FalseOutput => self.theme.accent_red,
                    _ => self.theme.accent_cyan,
                };

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
                        .bg(edge_color)
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
                        .bg(edge_color)
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
                        .bg(edge_color)
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
                            .child(format!("Action: {:?}", node.action.as_ref().expect("guarded by is_some check"))),
                    )
                })
                .when(node.persona.is_some(), |el| {
                    el.child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_aqua)
                            .child(format!(
                                "Agent: {:?}",
                                node.persona.as_ref().expect("guarded by is_some check")
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
                    .items_center()
                    .gap(theme.space_2)
                    // Palette toggle button
                    .child({
                        let palette_bg = if self.show_node_palette {
                            let mut c = theme.accent_cyan;
                            c.a = 0.15;
                            c
                        } else {
                            theme.bg_tertiary
                        };
                        div()
                            .id("toggle-palette-btn")
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_sm)
                            .bg(palette_bg)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.show_node_palette = !this.show_node_palette;
                                    cx.notify();
                                }),
                            )
                            .child("Palette")
                    })
                    // Zoom controls
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .child(
                                div()
                                    .id("zoom-out-btn")
                                    .px(theme.space_2)
                                    .py(theme.space_1)
                                    .rounded(theme.radius_sm)
                                    .bg(theme.bg_tertiary)
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_secondary)
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.zoom = (this.zoom - 0.1).max(0.3);
                                            cx.notify();
                                        }),
                                    )
                                    .child("\u{2212}"),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(format!("{:.0}%", self.zoom * 100.0)),
                            )
                            .child(
                                div()
                                    .id("zoom-in-btn")
                                    .px(theme.space_2)
                                    .py(theme.space_1)
                                    .rounded(theme.radius_sm)
                                    .bg(theme.bg_tertiary)
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_secondary)
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.zoom = (this.zoom + 0.1).min(3.0);
                                            cx.notify();
                                        }),
                                    )
                                    .child("+"),
                            ),
                    )
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
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _e, _w, cx| {
                                    this.save_workflow(cx);
                                }),
                            )
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

        // Canvas area with nodes + interaction handlers
        let canvas_elements = self.render_canvas_nodes(theme, cx);
        let is_connecting = self.connecting_from.is_some();

        let canvas_area = div()
            .id("wf-canvas")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .relative()
            .overflow_hidden()
            .bg(theme.bg_primary)
            .when(is_connecting, |el| el.cursor(CursorStyle::Crosshair))
            // Mouse down on canvas background → start panning
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                    // If we're in connect mode and click the background, cancel
                    if this.connecting_from.is_some() {
                        this.cancel_connect(cx);
                        return;
                    }
                    // Start panning
                    let pos = event.position;
                    this.start_pan(f64::from(pos.x), f64::from(pos.y));
                    // Deselect node
                    this.selected_node_id = None;
                    cx.notify();
                }),
            )
            // Mouse move → update drag or pan
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _w, cx| {
                let pos = event.position;
                let mx = f64::from(pos.x);
                let my = f64::from(pos.y);
                if this.dragging_node.is_some() {
                    this.update_drag(mx, my, cx);
                } else if this.panning.is_some() {
                    this.update_pan(mx, my, cx);
                }
            }))
            // Mouse up → end drag or pan
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _w, _cx| {
                    this.end_drag();
                    this.end_pan();
                }),
            )
            .children(canvas_elements);

        // Node palette (left)
        let palette = self
            .render_node_palette(theme, cx)
            .into_any_element();

        // Properties (right)
        let properties = self.render_properties_panel(theme).into_any_element();

        let show_palette = self.show_node_palette;

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
                    .when(show_palette, |el| el.child(palette))
                    .child(canvas_area)
                    .when(self.show_properties_panel || self.selected_node_id.is_some(), |el| {
                        el.child(properties)
                    }),
            )
    }
}
