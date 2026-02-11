use chrono::{DateTime, Utc};
use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;
use crate::workspace::MonitorRefresh;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// The nine specialized agent roles in a HiveMind orchestration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRole {
    Architect, Coder, Reviewer, Tester, Documenter,
    Debugger, Security, OutputReviewer, TaskVerifier,
}

impl AgentRole {
    /// Returns (emoji, display name, short description).
    fn meta(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Architect      => ("\u{1F3D7}", "Architect",       "System design & planning"),
            Self::Coder          => ("\u{1F4BB}", "Coder",           "Implementation & code gen"),
            Self::Reviewer       => ("\u{1F50D}", "Reviewer",        "Code review & feedback"),
            Self::Tester         => ("\u{1F9EA}", "Tester",          "Test creation & validation"),
            Self::Documenter     => ("\u{1F4DD}", "Documenter",      "Docs & technical writing"),
            Self::Debugger       => ("\u{1F41B}", "Debugger",        "Bug diagnosis & fixes"),
            Self::Security       => ("\u{1F512}", "Security",        "Security audit & hardening"),
            Self::OutputReviewer => ("\u{1F440}", "Output Reviewer", "Output quality checks"),
            Self::TaskVerifier   => ("\u{2705}",  "Task Verifier",   "Task completion verification"),
        }
    }

    pub fn label(self) -> &'static str { self.meta().1 }

    pub fn all() -> [Self; 9] {
        [Self::Architect, Self::Coder, Self::Reviewer, Self::Tester, Self::Documenter,
         Self::Debugger, Self::Security, Self::OutputReviewer, Self::TaskVerifier]
    }
}

/// High-level status of the agent orchestration system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentSystemStatus {
    Idle,
    Running,
    Paused,
    Error,
}

impl AgentSystemStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Running => "Running",
            Self::Paused => "Paused",
            Self::Error => "Error",
        }
    }

    fn dot_color(self, theme: &HiveTheme) -> Hsla {
        match self {
            Self::Idle => theme.text_muted,
            Self::Running => theme.accent_green,
            Self::Paused => theme.accent_yellow,
            Self::Error => theme.accent_red,
        }
    }
}

/// Runtime status of an individual agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Working,
    Waiting,
    Done,
    Failed,
}

impl AgentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Working => "Working",
            Self::Waiting => "Waiting",
            Self::Done => "Done",
            Self::Failed => "Failed",
        }
    }

    fn dot_color(self, theme: &HiveTheme) -> Hsla {
        match self {
            Self::Idle => theme.text_muted,
            Self::Working => theme.accent_green,
            Self::Waiting => theme.accent_yellow,
            Self::Done => theme.accent_cyan,
            Self::Failed => theme.accent_red,
        }
    }
}

/// A single agent currently participating in an orchestration.
pub struct ActiveAgent {
    pub role: String,
    pub status: AgentStatus,
    pub phase: String,
    pub model: String,
    pub started_at: DateTime<Utc>,
}

impl ActiveAgent {
    pub fn new(role: impl Into<String>, status: AgentStatus, phase: impl Into<String>, model: impl Into<String>, started_at: DateTime<Utc>) -> Self {
        Self {
            role: role.into(),
            status,
            phase: phase.into(),
            model: model.into(),
            started_at,
        }
    }
}

/// A completed (or failed/aborted) orchestration run.
pub struct RunHistoryEntry {
    pub id: String,
    pub task_summary: String,
    pub agents_used: usize,
    pub status: AgentSystemStatus,
    pub cost: f64,
    pub started_at: String,
    pub duration_secs: u64,
}

impl RunHistoryEntry {
    pub fn new(id: &str, summary: &str, agents: usize, status: AgentSystemStatus,
           cost: f64, time: &str, dur: u64) -> Self {
        Self {
            id: id.into(), task_summary: summary.into(), agents_used: agents,
            status, cost, started_at: time.into(), duration_secs: dur,
        }
    }
}

// ---------------------------------------------------------------------------
// System resource data types
// ---------------------------------------------------------------------------

/// System resource usage snapshot.
#[derive(Debug, Clone)]
pub struct SystemResources {
    /// CPU usage as a percentage (0.0 to 100.0).
    pub cpu_percent: f64,
    /// Memory used in bytes.
    pub memory_used: u64,
    /// Total memory in bytes.
    pub memory_total: u64,
    /// Disk used in bytes.
    pub disk_used: u64,
    /// Total disk in bytes.
    pub disk_total: u64,
}

impl SystemResources {
    /// Returns a placeholder snapshot with sample values.
    pub fn placeholder() -> Self {
        Self {
            cpu_percent: 34.2,
            memory_used: 8_589_934_592,   // 8 GB
            memory_total: 32_212_254_720, // 30 GB
            disk_used: 214_748_364_800,   // 200 GB
            disk_total: 536_870_912_000,  // 500 GB
        }
    }

    pub fn memory_percent(&self) -> f64 {
        if self.memory_total == 0 { return 0.0; }
        (self.memory_used as f64 / self.memory_total as f64) * 100.0
    }

    pub fn disk_percent(&self) -> f64 {
        if self.disk_total == 0 { return 0.0; }
        (self.disk_used as f64 / self.disk_total as f64) * 100.0
    }
}

/// Status of an AI provider connection.
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub name: String,
    pub online: bool,
    pub latency_ms: Option<u32>,
}

impl ProviderStatus {
    pub fn new(name: impl Into<String>, online: bool, latency_ms: Option<u32>) -> Self {
        Self { name: name.into(), online, latency_ms }
    }
}

/// All data needed to render the Monitor panel.
pub struct MonitorData {
    // Agent orchestration state (existing)
    pub status: AgentSystemStatus,
    pub active_agents: Vec<ActiveAgent>,
    pub completed_tasks: usize,
    pub total_runs: usize,
    pub current_run_id: Option<String>,
    pub run_history: Vec<RunHistoryEntry>,

    // System resource monitoring (new)
    pub resources: SystemResources,
    pub providers: Vec<ProviderStatus>,
    pub request_queue_length: usize,
    pub active_streams: usize,
    pub uptime_secs: u64,
}

impl MonitorData {
    /// Returns a default idle state with no agents or history.
    pub fn empty() -> Self {
        Self {
            status: AgentSystemStatus::Idle,
            active_agents: Vec::new(),
            completed_tasks: 0,
            total_runs: 0,
            current_run_id: None,
            run_history: Vec::new(),
            resources: SystemResources::placeholder(),
            providers: Vec::new(),
            request_queue_length: 0,
            active_streams: 0,
            uptime_secs: 0,
        }
    }

    /// Realistic sample data so the panel is visible before live wiring.
    pub fn sample() -> Self {
        use AgentStatus::*;
        let now = Utc::now();
        Self {
            status: AgentSystemStatus::Running,
            completed_tasks: 14,
            total_runs: 7,
            current_run_id: Some("run-007".into()),
            active_agents: vec![
                ActiveAgent::new("Architect", Done,    "Design complete",       "claude-opus-4-6",   now - chrono::Duration::minutes(5)),
                ActiveAgent::new("Coder",     Working, "Implementing module",   "claude-sonnet-4-5", now - chrono::Duration::minutes(3)),
                ActiveAgent::new("Tester",    Waiting, "Awaiting code",         "claude-haiku-4-5",  now - chrono::Duration::minutes(1)),
                ActiveAgent::new("Security",  Working, "Scanning dependencies", "claude-sonnet-4-5", now - chrono::Duration::minutes(2)),
            ],
            run_history: vec![
                RunHistoryEntry::new("run-007", "Implement Agent Monitor panel with full UI",           4, AgentSystemStatus::Running,   0.48, "14:32",          0),
                RunHistoryEntry::new("run-006", "Add cost tracking dashboard with model breakdown",     3, AgentSystemStatus::Idle,      0.92, "13:15",        187),
                RunHistoryEntry::new("run-005", "Fix token launch wizard wallet creation flow",          5, AgentSystemStatus::Idle,      1.34, "11:42",        312),
                RunHistoryEntry::new("run-004", "Security audit of IPC handlers and preload bridge",    6, AgentSystemStatus::Idle,      2.10, "09:30",        540),
                RunHistoryEntry::new("run-003", "Refactor messaging hub for Telegram provider",         3, AgentSystemStatus::Error,     0.67, "Yesterday",     95),
            ],
            resources: SystemResources {
                cpu_percent: 42.7,
                memory_used: 12_884_901_888,  // 12 GB
                memory_total: 32_212_254_720, // 30 GB
                disk_used: 257_698_037_760,   // 240 GB
                disk_total: 536_870_912_000,  // 500 GB
            },
            providers: vec![
                ProviderStatus::new("Anthropic", true, Some(45)),
                ProviderStatus::new("OpenAI", true, Some(62)),
                ProviderStatus::new("Ollama (local)", true, Some(8)),
                ProviderStatus::new("OpenRouter", true, Some(120)),
                ProviderStatus::new("Google Gemini", false, None),
                ProviderStatus::new("LM Studio", false, None),
            ],
            request_queue_length: 3,
            active_streams: 2,
            uptime_secs: 7834,
        }
    }

    /// Total number of online providers.
    pub fn online_provider_count(&self) -> usize {
        self.providers.iter().filter(|p| p.online).count()
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// System & Agent Monitor: resource usage, provider status, agent state,
/// request queue, streaming sessions, uptime, and orchestration history.
pub struct MonitorPanel;

impl MonitorPanel {
    /// Main entry point -- renders the full monitor panel.
    pub fn render(data: &MonitorData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("monitor-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(Self::header(data, theme))
            .child(Self::system_resources_section(data, theme))
            .child(Self::provider_status_section(data, theme))
            .child(Self::runtime_stats_section(data, theme))
            .child(Self::agent_roles_section(theme))
            .child(Self::active_agents_section(data, theme))
            .child(Self::run_history_section(data, theme))
    }

    // ------------------------------------------------------------------
    // Header
    // ------------------------------------------------------------------

    fn header(data: &MonitorData, theme: &HiveTheme) -> impl IntoElement {
        let (badge_label, badge_color) = match data.status {
            AgentSystemStatus::Running => ("Live", theme.accent_green),
            AgentSystemStatus::Paused => ("Paused", theme.accent_yellow),
            AgentSystemStatus::Error => ("Error", theme.accent_red),
            AgentSystemStatus::Idle => ("Idle", theme.text_muted),
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(Icon::new(IconName::Loader).size_6())
            .child(Self::header_title(theme))
            .child(div().flex_1())
            .child(Self::uptime_label(data, theme))
            .child(Self::refresh_btn(theme))
            .child(Self::status_badge(badge_label, badge_color, theme))
    }

    fn header_title(theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_2xl)
            .text_color(theme.text_primary)
            .font_weight(FontWeight::BOLD)
            .child("System Monitor".to_string())
    }

    /// Refresh button -- dispatches `MonitorRefresh`.
    fn refresh_btn(theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("monitor-refresh-btn")
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_secondary)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.dispatch_action(&MonitorRefresh);
            })
            .child("Refresh".to_string())
    }

    fn uptime_label(data: &MonitorData, theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .mr(theme.space_2)
            .child(format!("Uptime: {}", Self::fmt_uptime(data.uptime_secs)))
    }

    fn status_badge(label: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .px(theme.space_3)
            .py(theme.space_1)
            .rounded(theme.radius_full)
            .bg(theme.bg_tertiary)
            .child(Self::dot(px(6.0), color, theme))
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // System Resources (CPU, Memory, Disk)
    // ------------------------------------------------------------------

    fn system_resources_section(data: &MonitorData, theme: &HiveTheme) -> impl IntoElement {
        let res = &data.resources;

        div()
            .flex()
            .flex_row()
            .gap(theme.space_3)
            .child(Self::resource_card(
                "CPU Usage",
                &format!("{:.1}%", res.cpu_percent),
                res.cpu_percent,
                Self::usage_color(res.cpu_percent, theme),
                theme,
            ))
            .child(Self::resource_card(
                "Memory",
                &format!(
                    "{} / {}",
                    Self::fmt_bytes(res.memory_used),
                    Self::fmt_bytes(res.memory_total)
                ),
                res.memory_percent(),
                Self::usage_color(res.memory_percent(), theme),
                theme,
            ))
            .child(Self::resource_card(
                "Disk",
                &format!(
                    "{} / {}",
                    Self::fmt_bytes(res.disk_used),
                    Self::fmt_bytes(res.disk_total)
                ),
                res.disk_percent(),
                Self::usage_color(res.disk_percent(), theme),
                theme,
            ))
    }

    fn resource_card(
        label: &str,
        value: &str,
        percent: f64,
        color: Hsla,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        Self::card_shell(theme)
            .child(Self::card_label(label, theme))
            .child(
                div()
                    .text_size(theme.font_size_xl)
                    .text_color(color)
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{:.0}%", percent)),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(value.to_string()),
            )
            .child(Self::progress_bar(percent, color, theme))
    }

    fn progress_bar(percent: f64, color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        let clamped = percent.clamp(0.0, 100.0);
        // We render a track with a filled portion. The width is relative
        // to the card, so we use a fixed height bar that fills proportionally.
        div()
            .w_full()
            .h(px(4.0))
            .rounded(theme.radius_full)
            .bg(theme.bg_primary)
            .mt(theme.space_1)
            .child(
                div()
                    .h(px(4.0))
                    .rounded(theme.radius_full)
                    .bg(color)
                    .w(relative(clamped as f32 / 100.0)),
            )
    }

    fn usage_color(percent: f64, theme: &HiveTheme) -> Hsla {
        if percent >= 90.0 {
            theme.accent_red
        } else if percent >= 70.0 {
            theme.accent_yellow
        } else {
            theme.accent_green
        }
    }

    // ------------------------------------------------------------------
    // Provider Status
    // ------------------------------------------------------------------

    fn provider_status_section(data: &MonitorData, theme: &HiveTheme) -> impl IntoElement {
        let online = data.online_provider_count();
        let total = data.providers.len();

        let mut container = Self::section("AI Provider Status", theme)
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .mb(theme.space_2)
                    .child(format!("{online} of {total} providers online")),
            );

        if data.providers.is_empty() {
            container = container.child(Self::empty_state(
                "No AI providers configured. Add API keys in Settings.", theme));
        } else {
            let mut grid = div().flex().flex_row().flex_wrap().gap(theme.space_2);
            for provider in &data.providers {
                grid = grid.child(Self::provider_card(provider, theme));
            }
            container = container.child(grid);
        }

        container
    }

    fn provider_card(provider: &ProviderStatus, theme: &HiveTheme) -> impl IntoElement {
        let (dot_color, status_text) = if provider.online {
            (theme.accent_green, "Online")
        } else {
            (theme.accent_red, "Offline")
        };

        let latency_text = provider.latency_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "--".to_string());

        div()
            .flex()
            .flex_col()
            .min_w(px(150.0))
            .flex_1()
            .p(theme.space_3)
            .bg(theme.bg_tertiary)
            .rounded(theme.radius_md)
            .gap(theme.space_1)
            .child(Self::provider_name_row(&provider.name, dot_color, theme))
            .child(Self::provider_status_row(status_text, &latency_text, dot_color, theme))
    }

    fn provider_name_row(name: &str, dot_color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(Self::dot(px(8.0), dot_color, theme))
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::MEDIUM)
                    .child(name.to_string()),
            )
    }

    fn provider_status_row(status: &str, latency: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(color)
                    .child(status.to_string()),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(format!("Latency: {latency}")),
            )
    }

    // ------------------------------------------------------------------
    // Runtime Stats (Queue, Streams, Uptime) — 3 cards across
    // ------------------------------------------------------------------

    fn runtime_stats_section(data: &MonitorData, theme: &HiveTheme) -> impl IntoElement {
        let active_count = data.active_agents.iter()
            .filter(|a| a.status == AgentStatus::Working).count();

        div()
            .flex()
            .flex_row()
            .gap(theme.space_3)
            .child(Self::simple_card(
                "Request Queue",
                &data.request_queue_length.to_string(),
                if data.request_queue_length > 10 { theme.accent_yellow } else { theme.accent_cyan },
                theme,
            ))
            .child(Self::simple_card(
                "Active Streams",
                &data.active_streams.to_string(),
                theme.accent_aqua,
                theme,
            ))
            .child(Self::simple_card(
                "Active Agents",
                &active_count.to_string(),
                theme.accent_cyan,
                theme,
            ))
            .child(Self::simple_card(
                "Tasks Completed",
                &data.completed_tasks.to_string(),
                theme.accent_green,
                theme,
            ))
    }

    fn simple_card(label: &str, value: &str, accent: Hsla, theme: &HiveTheme) -> impl IntoElement {
        Self::card_shell(theme)
            .child(Self::card_label(label, theme))
            .child(Self::card_value(value, accent))
    }

    fn card_shell(theme: &HiveTheme) -> Div {
        div()
            .flex().flex_col().flex_1()
            .p(theme.space_3)
            .bg(theme.bg_surface)
            .border_1().border_color(theme.border)
            .rounded(theme.radius_md)
            .gap(theme.space_1)
    }

    fn card_label(text: &str, theme: &HiveTheme) -> Div {
        div().text_size(theme.font_size_xs).text_color(theme.text_muted).child(text.to_string())
    }

    fn card_value(text: &str, color: Hsla) -> Div {
        div().text_size(px(20.0)).text_color(color).font_weight(FontWeight::BOLD).child(text.to_string())
    }

    // ------------------------------------------------------------------
    // Available Agent Roles (3x3 grid)
    // ------------------------------------------------------------------

    fn agent_roles_section(theme: &HiveTheme) -> impl IntoElement {
        let mut grid = div().flex().flex_row().flex_wrap().gap(theme.space_2);
        for role in AgentRole::all() {
            grid = grid.child(Self::role_card(role, theme));
        }
        Self::section("Available Agent Roles", theme).child(grid)
    }

    fn role_card(role: AgentRole, theme: &HiveTheme) -> impl IntoElement {
        let (icon, name, desc) = role.meta();
        div()
            .flex().flex_col()
            .min_w(px(160.0)).flex_1()
            .p(theme.space_3)
            .bg(theme.bg_tertiary)
            .rounded(theme.radius_md)
            .gap(theme.space_1)
            .child(Self::role_card_header(icon, name, theme))
            .child(div().text_size(theme.font_size_xs).text_color(theme.text_muted).child(desc.to_string()))
            .child(Self::role_ready_indicator(theme))
    }

    fn role_card_header(icon: &str, name: &str, theme: &HiveTheme) -> Div {
        div()
            .flex().flex_row().items_center().gap(theme.space_2)
            .child(div().text_size(theme.font_size_xl).child(icon.to_string()))
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name.to_string()),
            )
    }

    fn role_ready_indicator(theme: &HiveTheme) -> Div {
        div()
            .flex().flex_row().items_center().gap(theme.space_1)
            .child(Self::dot(px(6.0), theme.accent_green, theme))
            .child(div().text_size(theme.font_size_xs).text_color(theme.text_secondary).child("Ready".to_string()))
    }

    // ------------------------------------------------------------------
    // Active Agents
    // ------------------------------------------------------------------

    fn active_agents_section(data: &MonitorData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = Self::section("Active Agents", theme);
        if data.active_agents.is_empty() {
            container = container.child(Self::empty_state(
                "No agents running. Start a HiveMind task to see activity here.", theme));
        } else {
            for agent in &data.active_agents {
                container = container.child(Self::agent_row(agent, theme));
            }
        }
        container
    }

    fn agent_row(agent: &ActiveAgent, theme: &HiveTheme) -> impl IntoElement {
        let color = agent.status.dot_color(theme);
        let status_label = agent.status.label();

        div()
            .flex().flex_row().items_center()
            .gap(theme.space_3)
            .py(theme.space_2).px(theme.space_3)
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .child(Self::agent_name_cell(&agent.role, color, theme))
            .child(Self::agent_phase_cell(&agent.phase, theme))
            .child(div().flex_1())
            .child(Self::agent_status_cell(status_label, color))
            .child(div().text_size(theme.font_size_xs).text_color(theme.text_muted).child(agent.model.clone()))
    }

    fn agent_name_cell(role: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex().flex_row().items_center().gap(theme.space_2).w(px(140.0))
            .child(Self::dot(px(8.0), color, theme))
            .child(
                div().text_size(theme.font_size_sm).text_color(theme.text_primary)
                    .font_weight(FontWeight::MEDIUM).child(role.to_string()),
            )
    }

    fn agent_phase_cell(phase: &str, theme: &HiveTheme) -> Div {
        div()
            .px(theme.space_2).py(px(2.0))
            .rounded(theme.radius_sm).bg(theme.bg_secondary)
            .text_size(theme.font_size_xs).text_color(theme.text_secondary)
            .child(phase.to_string())
    }

    fn agent_status_cell(label: &str, color: Hsla) -> Div {
        div().w(px(56.0)).text_size(px(14.0))
            .text_color(color).font_weight(FontWeight::SEMIBOLD).child(label.to_string())
    }

    // ------------------------------------------------------------------
    // Run History
    // ------------------------------------------------------------------

    fn run_history_section(data: &MonitorData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = Self::section("Run History", theme)
            .child(Self::history_header(theme));

        if data.run_history.is_empty() {
            container = container.child(Self::empty_state(
                "No orchestration runs yet -- history will appear here.", theme));
        } else {
            for entry in &data.run_history {
                container = container.child(Self::history_row(entry, theme));
            }
        }
        container
    }

    fn history_header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex().flex_row().items_center().gap(theme.space_2)
            .pb(theme.space_1).border_b_1().border_color(theme.border)
            .child(Self::col_hdr("Time", px(72.0), theme))
            .child(
                div().flex_1().text_size(theme.font_size_xs).text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD).child("Task".to_string()),
            )
            .child(Self::col_hdr("Agents", px(56.0), theme))
            .child(Self::col_hdr("Status", px(80.0), theme))
            .child(Self::col_hdr("Cost", px(64.0), theme))
            .child(Self::col_hdr("Duration", px(72.0), theme))
    }

    fn col_hdr(label: &str, width: Pixels, theme: &HiveTheme) -> impl IntoElement {
        div().w(width).text_size(theme.font_size_xs).text_color(theme.text_muted)
            .font_weight(FontWeight::SEMIBOLD).child(label.to_string())
    }

    fn history_row(entry: &RunHistoryEntry, theme: &HiveTheme) -> impl IntoElement {
        let color = entry.status.dot_color(theme);
        let summary = if entry.task_summary.len() > 48 {
            format!("{}...", &entry.task_summary[..45])
        } else {
            entry.task_summary.clone()
        };

        div()
            .flex().flex_row().items_center().gap(theme.space_2).py(theme.space_1)
            .child(div().w(px(72.0)).text_size(theme.font_size_sm).text_color(theme.text_muted).child(entry.started_at.clone()))
            .child(div().flex_1().text_size(theme.font_size_sm).text_color(theme.text_primary).child(summary))
            .child(div().w(px(56.0)).text_size(theme.font_size_sm).text_color(theme.text_secondary).child(entry.agents_used.to_string()))
            .child(Self::history_status_cell(entry, color, theme))
            .child(div().w(px(64.0)).text_size(theme.font_size_sm).text_color(theme.accent_aqua)
                .font_weight(FontWeight::MEDIUM).child(format!("${:.2}", entry.cost)))
            .child(div().w(px(72.0)).text_size(theme.font_size_sm).text_color(theme.text_muted)
                .child(Self::fmt_duration(entry.duration_secs)))
    }

    fn history_status_cell(entry: &RunHistoryEntry, color: Hsla, theme: &HiveTheme) -> Div {
        div().w(px(80.0)).child(
            div().flex().flex_row().items_center().gap(theme.space_1)
                .child(Self::dot(px(6.0), color, theme))
                .child(div().text_size(theme.font_size_xs).text_color(color).child(entry.status.label().to_string())),
        )
    }

    // ------------------------------------------------------------------
    // Shared helpers
    // ------------------------------------------------------------------

    fn section(title: &str, theme: &HiveTheme) -> Div {
        div()
            .flex().flex_col()
            .bg(theme.bg_surface)
            .border_1().border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4).gap(theme.space_2)
            .child(
                div().text_size(theme.font_size_lg).text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD).child(title.to_string()),
            )
    }

    fn dot(size: Pixels, color: Hsla, theme: &HiveTheme) -> impl IntoElement {
        div().w(size).h(size).rounded(theme.radius_full).bg(color)
    }

    fn empty_state(message: &str, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex().items_center().justify_center().py(theme.space_6)
            .child(div().text_size(theme.font_size_base).text_color(theme.text_muted).child(message.to_string()))
    }

    pub fn fmt_duration(secs: u64) -> String {
        if secs == 0 { return "Running".to_string(); }
        let mins = secs / 60;
        let rem = secs % 60;
        if mins > 0 { format!("{}m {}s", mins, rem) } else { format!("{}s", rem) }
    }

    /// Format uptime in hours, minutes, seconds.
    pub fn fmt_uptime(secs: u64) -> String {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let rem = secs % 60;
        if hours > 0 {
            format!("{}h {}m {}s", hours, mins, rem)
        } else if mins > 0 {
            format!("{}m {}s", mins, rem)
        } else {
            format!("{}s", rem)
        }
    }

    /// Format a byte count as a human-readable string (KB, MB, GB, TB).
    pub fn fmt_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;
        const TB: u64 = 1024 * GB;

        if bytes >= TB {
            format!("{:.1} TB", bytes as f64 / TB as f64)
        } else if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.0} KB", bytes as f64 / KB as f64)
        } else {
            format!("{bytes} B")
        }
    }
}
