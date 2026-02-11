use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Display information for an agent persona.
#[derive(Debug, Clone)]
pub struct PersonaDisplay {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub model_tier: String,
    pub active: bool,
}

impl PersonaDisplay {
    /// Icon name mapped from the persona kind.
    pub fn icon(&self) -> IconName {
        match self.kind.as_str() {
            "investigate" => IconName::Search,
            "implement" => IconName::File,
            "verify" => IconName::CircleCheck,
            "critique" => IconName::Info,
            "debug" => IconName::TriangleAlert,
            "code_review" => IconName::Eye,
            _ => IconName::Bot,
        }
    }
}

/// Display information for an orchestration run.
#[derive(Debug, Clone)]
pub struct RunDisplay {
    pub id: String,
    pub spec_title: String,
    pub status: String,
    pub progress: f32,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub cost: f64,
    pub elapsed: String,
}

impl RunDisplay {
    /// Whether this run is still in progress.
    pub fn is_active(&self) -> bool {
        self.status == "Running" || self.status == "Pending"
    }
}

/// All data needed to render the agents panel.
#[derive(Debug, Clone)]
pub struct AgentsPanelData {
    pub personas: Vec<PersonaDisplay>,
    pub active_runs: Vec<RunDisplay>,
    pub run_history: Vec<RunDisplay>,
}

impl AgentsPanelData {
    /// Create an empty state.
    pub fn empty() -> Self {
        Self {
            personas: Vec::new(),
            active_runs: Vec::new(),
            run_history: Vec::new(),
        }
    }

    /// Return a sample dataset with the six default personas.
    pub fn sample() -> Self {
        Self {
            personas: vec![
                PersonaDisplay {
                    name: "Investigator".into(),
                    kind: "investigate".into(),
                    description: "Analyzes codebases, traces bugs, and gathers context for tasks."
                        .into(),
                    model_tier: "Tier 1".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Implementer".into(),
                    kind: "implement".into(),
                    description:
                        "Writes production code, applies patches, and implements features.".into(),
                    model_tier: "Tier 1".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Verifier".into(),
                    kind: "verify".into(),
                    description: "Runs tests, validates behavior, and checks correctness.".into(),
                    model_tier: "Tier 2".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Critic".into(),
                    kind: "critique".into(),
                    description:
                        "Reviews plans and code for flaws, edge cases, and improvements.".into(),
                    model_tier: "Tier 2".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Debugger".into(),
                    kind: "debug".into(),
                    description:
                        "Isolates failures, inspects stack traces, and proposes fixes.".into(),
                    model_tier: "Tier 1".into(),
                    active: false,
                },
                PersonaDisplay {
                    name: "Code Reviewer".into(),
                    kind: "code_review".into(),
                    description:
                        "Performs detailed code review with style, correctness, and security checks."
                            .into(),
                    model_tier: "Tier 2".into(),
                    active: false,
                },
            ],
            active_runs: vec![RunDisplay {
                id: "run-001".into(),
                spec_title: "Authentication Overhaul".into(),
                status: "Running".into(),
                progress: 0.58,
                tasks_done: 7,
                tasks_total: 12,
                cost: 0.42,
                elapsed: "3m 22s".into(),
            }],
            run_history: vec![
                RunDisplay {
                    id: "run-000".into(),
                    spec_title: "Database Migration v2".into(),
                    status: "Complete".into(),
                    progress: 1.0,
                    tasks_done: 5,
                    tasks_total: 5,
                    cost: 0.18,
                    elapsed: "1m 47s".into(),
                },
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Multi-agent orchestration panel: active runs, persona grid.
pub struct AgentsPanel;

impl AgentsPanel {
    pub fn render(data: &AgentsPanelData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("agents-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(theme))
            .child(render_active_runs_section(&data.active_runs, theme))
            .child(render_personas_section(&data.personas, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(header_icon(theme))
        .child(header_title(theme))
        .child(div().flex_1())
        .child(run_spec_button(theme))
        .into_any_element()
}

fn header_icon(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(40.0))
        .h(px(40.0))
        .rounded(theme.radius_lg)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(Icon::new(IconName::Bot).size_4())
}

fn header_title(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(theme.font_size_xl)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("Agent Orchestration"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Coordinate multi-agent runs on specifications"),
        )
}

fn run_spec_button(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.accent_cyan)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_on_accent)
        .child("Run Spec")
}

// ---------------------------------------------------------------------------
// Active runs
// ---------------------------------------------------------------------------

fn render_active_runs_section(runs: &[RunDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Active Runs", runs.len(), theme));

    if runs.is_empty() {
        section = section.child(render_empty_runs(theme));
    } else {
        for run in runs {
            section = section.child(render_run_card(run, theme));
        }
    }

    section.into_any_element()
}

fn render_run_card(run: &RunDisplay, theme: &HiveTheme) -> AnyElement {
    let status_color = match run.status.as_str() {
        "Running" => theme.accent_aqua,
        "Complete" => theme.accent_green,
        "Failed" => theme.accent_red,
        "Pending" => theme.accent_yellow,
        _ => theme.text_muted,
    };

    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(run_card_top_row(run, status_color, theme))
        .child(run_progress_bar(run, status_color, theme))
        .child(run_card_stats(run, theme))
        .into_any_element()
}

fn run_card_top_row(run: &RunDisplay, status_color: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(run.spec_title.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .px(theme.space_2)
                .py(px(2.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .font_weight(FontWeight::MEDIUM)
                .text_color(status_color)
                .child(run.status.clone()),
        )
}

fn run_progress_bar(run: &RunDisplay, bar_color: Hsla, theme: &HiveTheme) -> Div {
    let progress = run.progress.clamp(0.0, 1.0);

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("{}/{} tasks", run.tasks_done, run.tasks_total)),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(bar_color)
                        .child(format!("{}%", (progress * 100.0) as u32)),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(6.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .child(
                    div()
                        .h(px(6.0))
                        .rounded(theme.radius_full)
                        .bg(bar_color)
                        .w(relative(progress)),
                ),
        )
}

fn run_card_stats(run: &RunDisplay, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_4)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Cost:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_yellow)
                        .child(format!("${:.2}", run.cost)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Elapsed:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child(run.elapsed.clone()),
                ),
        )
}

fn render_empty_runs(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(theme.space_6)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("No active runs. Click \"Run Spec\" to start one."),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Personas
// ---------------------------------------------------------------------------

fn render_personas_section(personas: &[PersonaDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Agent Personas", personas.len(), theme));

    if personas.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .py(theme.space_6)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("No personas configured."),
                ),
        );
    } else {
        let mut grid = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(theme.space_3);

        for persona in personas {
            grid = grid.child(render_persona_card(persona, theme));
        }

        section = section.child(grid);
    }

    // Custom personas section
    section = section.child(custom_personas_header(theme));

    section.into_any_element()
}

fn render_persona_card(persona: &PersonaDisplay, theme: &HiveTheme) -> AnyElement {
    let border_color = if persona.active {
        theme.accent_aqua
    } else {
        theme.border
    };

    let name_color = if persona.active {
        theme.text_primary
    } else {
        theme.text_secondary
    };

    div()
        .flex()
        .flex_col()
        .w(px(200.0))
        .p(theme.space_3)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(border_color)
        .child(persona_card_header(persona, name_color, theme))
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .overflow_hidden()
                .max_h(px(32.0))
                .child(persona.description.clone()),
        )
        .child(persona_card_footer(persona, theme))
        .into_any_element()
}

fn persona_card_header(
    persona: &PersonaDisplay,
    name_color: Hsla,
    theme: &HiveTheme,
) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(Icon::new(persona.icon()).size_4().text_color(theme.accent_cyan))
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(name_color)
                .font_weight(FontWeight::SEMIBOLD)
                .child(persona.name.clone()),
        )
}

fn persona_card_footer(persona: &PersonaDisplay, theme: &HiveTheme) -> Div {
    let active_color = if persona.active {
        theme.accent_aqua
    } else {
        theme.text_muted
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(persona.model_tier.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(theme.radius_full)
                .bg(active_color),
        )
}

fn custom_personas_header(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .font_weight(FontWeight::MEDIUM)
                .child("Custom Personas"),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(24.0))
                .h(px(24.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_sm)
                .text_color(theme.accent_cyan)
                .child("+"),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn section_title(title: &str, count: usize, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .px(theme.space_2)
                .py(px(2.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(format!("{count}")),
        )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persona_icon_mapping() {
        let p = PersonaDisplay {
            name: "Investigator".into(),
            kind: "investigate".into(),
            description: String::new(),
            model_tier: String::new(),
            active: true,
        };
        assert_eq!(p.icon(), IconName::Search);
    }

    #[test]
    fn persona_icon_unknown_kind() {
        let p = PersonaDisplay {
            name: "Custom".into(),
            kind: "custom_thing".into(),
            description: String::new(),
            model_tier: String::new(),
            active: false,
        };
        assert_eq!(p.icon(), IconName::Bot);
    }

    #[test]
    fn run_display_is_active() {
        let running = RunDisplay {
            id: "r1".into(),
            spec_title: "Test".into(),
            status: "Running".into(),
            progress: 0.5,
            tasks_done: 3,
            tasks_total: 6,
            cost: 0.1,
            elapsed: "1m".into(),
        };
        assert!(running.is_active());

        let pending = RunDisplay {
            id: "r2".into(),
            spec_title: "Test".into(),
            status: "Pending".into(),
            progress: 0.0,
            tasks_done: 0,
            tasks_total: 4,
            cost: 0.0,
            elapsed: "0s".into(),
        };
        assert!(pending.is_active());

        let complete = RunDisplay {
            id: "r3".into(),
            spec_title: "Test".into(),
            status: "Complete".into(),
            progress: 1.0,
            tasks_done: 4,
            tasks_total: 4,
            cost: 0.2,
            elapsed: "2m".into(),
        };
        assert!(!complete.is_active());
    }

    #[test]
    fn agents_panel_data_empty() {
        let data = AgentsPanelData::empty();
        assert!(data.personas.is_empty());
        assert!(data.active_runs.is_empty());
        assert!(data.run_history.is_empty());
    }

    #[test]
    fn agents_panel_data_sample() {
        let data = AgentsPanelData::sample();
        assert_eq!(data.personas.len(), 6);
        assert_eq!(data.active_runs.len(), 1);
        assert_eq!(data.run_history.len(), 1);
    }
}
