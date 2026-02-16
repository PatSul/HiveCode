use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::{Icon, IconName};

use hive_ui_core::HiveTheme;
use hive_ui_core::{
    SkillsAddSource, SkillsClearSearch, SkillsCreate, SkillsInstall, SkillsRefresh,
    SkillsRemove, SkillsRemoveSource, SkillsSetCategory, SkillsSetTab,
    SkillsToggle,
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Which tab is active in the skills panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillsTab {
    Installed,
    Directory,
    Create,
    AddSource,
}

/// An installed skill with integrity tracking.
#[derive(Debug, Clone)]
pub struct InstalledSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,
    pub integrity_hash: String,
}

/// Skill category for filtering in the directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillCategory {
    CodeQuality,
    Testing,
    DevOps,
    Security,
    Documentation,
    Database,
    Productivity,
    Other,
}

impl SkillCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CodeQuality => "Code Quality",
            Self::Testing => "Testing",
            Self::DevOps => "DevOps",
            Self::Security => "Security",
            Self::Documentation => "Documentation",
            Self::Database => "Database",
            Self::Productivity => "Productivity",
            Self::Other => "Other",
        }
    }

    pub const ALL: [SkillCategory; 8] = [
        Self::CodeQuality,
        Self::Testing,
        Self::DevOps,
        Self::Security,
        Self::Documentation,
        Self::Database,
        Self::Productivity,
        Self::Other,
    ];
}

/// A skill available in the directory for browsing/install.
#[derive(Debug, Clone)]
pub struct DirectorySkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub downloads: usize,
    pub rating: f32,
    pub category: SkillCategory,
    pub installed: bool,
}

/// Data for the Create tab -- a new skill being authored.
#[derive(Debug, Clone)]
pub struct CreateSkillDraft {
    pub name: String,
    pub description: String,
    pub version: String,
    pub instructions: String,
}

impl CreateSkillDraft {
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            version: "0.1.0".to_string(),
            instructions: String::new(),
        }
    }

    /// Whether the draft has enough data to be valid for creation.
    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty()
            && !self.description.trim().is_empty()
            && !self.instructions.trim().is_empty()
    }
}

/// A configured skill source (registry URL).
#[derive(Debug, Clone)]
pub struct SkillSource {
    pub url: String,
    pub name: String,
    pub skill_count: usize,
}

/// All data for the skills panel.
#[derive(Debug, Clone)]
pub struct SkillsData {
    pub installed: Vec<InstalledSkill>,
    pub directory: Vec<DirectorySkill>,
    pub active_tab: SkillsTab,
    pub search_query: String,
    pub selected_category: Option<SkillCategory>,
    pub sources: Vec<SkillSource>,
    pub create_draft: CreateSkillDraft,
}

impl SkillsData {
    /// Create empty skills data (no skills, no sources).
    pub fn empty() -> Self {
        Self {
            installed: Vec::new(),
            directory: Vec::new(),
            active_tab: SkillsTab::Installed,
            search_query: String::new(),
            selected_category: None,
            sources: Vec::new(),
            create_draft: CreateSkillDraft::empty(),
        }
    }

    /// Return installed skills filtered by the current search query.
    /// Matches against name and description (case-insensitive).
    pub fn filtered_installed(&self) -> Vec<&InstalledSkill> {
        if self.search_query.is_empty() {
            return self.installed.iter().collect();
        }
        let query = self.search_query.to_lowercase();
        self.installed
            .iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query)
                    || s.description.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Return directory skills filtered by the current search query and category.
    /// Matches against name, description, and author (case-insensitive).
    pub fn filtered_directory(&self) -> Vec<&DirectorySkill> {
        self.directory
            .iter()
            .filter(|s| {
                // Category filter
                if let Some(cat) = self.selected_category
                    && s.category != cat {
                        return false;
                    }
                // Text search filter
                if self.search_query.is_empty() {
                    return true;
                }
                let query = self.search_query.to_lowercase();
                s.name.to_lowercase().contains(&query)
                    || s.description.to_lowercase().contains(&query)
                    || s.author.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Return a sample dataset for previewing the panel.
    #[allow(dead_code)]
    pub fn sample() -> Self {
        Self {
            installed: vec![
                InstalledSkill {
                    id: "code_review".into(),
                    name: "code_review".into(),
                    description: "Analyzes code for bugs, style issues, and improvement opportunities with detailed suggestions.".into(),
                    version: "1.2.0".into(),
                    enabled: true,
                    integrity_hash: "sha256:a1b2c3d4e5f6".into(),
                },
                InstalledSkill {
                    id: "test_gen".into(),
                    name: "test_gen".into(),
                    description: "Generates comprehensive unit and integration tests for functions and modules.".into(),
                    version: "0.9.1".into(),
                    enabled: true,
                    integrity_hash: "sha256:b2c3d4e5f6a1".into(),
                },
                InstalledSkill {
                    id: "doc_writer".into(),
                    name: "doc_writer".into(),
                    description: "Produces structured documentation including API references, guides, and inline comments.".into(),
                    version: "1.0.3".into(),
                    enabled: true,
                    integrity_hash: "sha256:c3d4e5f6a1b2".into(),
                },
                InstalledSkill {
                    id: "security_scan".into(),
                    name: "security_scan".into(),
                    description: "Scans source code for common vulnerabilities, secrets, and insecure patterns.".into(),
                    version: "2.1.0".into(),
                    enabled: false,
                    integrity_hash: "sha256:d4e5f6a1b2c3".into(),
                },
                InstalledSkill {
                    id: "refactor".into(),
                    name: "refactor".into(),
                    description: "Suggests and applies refactoring patterns to improve code clarity and reduce duplication.".into(),
                    version: "0.4.2".into(),
                    enabled: true,
                    integrity_hash: "sha256:e5f6a1b2c3d4".into(),
                },
            ],
            directory: vec![
                DirectorySkill {
                    id: "api_designer".into(),
                    name: "api_designer".into(),
                    description: "Design REST and GraphQL APIs from natural language descriptions.".into(),
                    author: "Hive Team".into(),
                    version: "1.3.0".into(),
                    downloads: 12_400,
                    rating: 4.7,
                    category: SkillCategory::CodeQuality,
                    installed: false,
                },
                DirectorySkill {
                    id: "perf_profiler".into(),
                    name: "perf_profiler".into(),
                    description: "Identify performance bottlenecks and suggest optimizations.".into(),
                    author: "PerfLabs".into(),
                    version: "2.0.1".into(),
                    downloads: 8_750,
                    rating: 4.5,
                    category: SkillCategory::Testing,
                    installed: false,
                },
                DirectorySkill {
                    id: "changelog_gen".into(),
                    name: "changelog_gen".into(),
                    description: "Automatically generate changelogs from git history and PR descriptions.".into(),
                    author: "community".into(),
                    version: "0.8.2".into(),
                    downloads: 6_300,
                    rating: 4.2,
                    category: SkillCategory::Documentation,
                    installed: false,
                },
                DirectorySkill {
                    id: "dependency_audit".into(),
                    name: "dependency_audit".into(),
                    description: "Audit project dependencies for known CVEs and license issues.".into(),
                    author: "SecureAI Labs".into(),
                    version: "3.1.0".into(),
                    downloads: 15_200,
                    rating: 4.9,
                    category: SkillCategory::Security,
                    installed: false,
                },
                DirectorySkill {
                    id: "db_migrate".into(),
                    name: "db_migrate".into(),
                    description: "Generate and validate database migration scripts from schema changes.".into(),
                    author: "DataTools".into(),
                    version: "1.0.0".into(),
                    downloads: 4_100,
                    rating: 4.0,
                    category: SkillCategory::Database,
                    installed: false,
                },
                DirectorySkill {
                    id: "i18n_helper".into(),
                    name: "i18n_helper".into(),
                    description: "Extract translatable strings and manage localization files.".into(),
                    author: "community".into(),
                    version: "0.5.3".into(),
                    downloads: 3_600,
                    rating: 3.8,
                    category: SkillCategory::Productivity,
                    installed: false,
                },
                DirectorySkill {
                    id: "ci_pipeline".into(),
                    name: "ci_pipeline".into(),
                    description: "Generate CI/CD pipeline configs for GitHub Actions, GitLab CI, and more.".into(),
                    author: "DevOpsKit".into(),
                    version: "2.2.0".into(),
                    downloads: 9_800,
                    rating: 4.6,
                    category: SkillCategory::DevOps,
                    installed: false,
                },
                DirectorySkill {
                    id: "load_tester".into(),
                    name: "load_tester".into(),
                    description: "Create and run load test scenarios with detailed performance reports.".into(),
                    author: "PerfLabs".into(),
                    version: "1.4.0".into(),
                    downloads: 5_500,
                    rating: 4.3,
                    category: SkillCategory::Testing,
                    installed: false,
                },
            ],
            sources: vec![
                SkillSource {
                    url: "https://clawdhub.hive.dev/registry".into(),
                    name: "ClawdHub Official".into(),
                    skill_count: 42,
                },
                SkillSource {
                    url: "https://github.com/hive-community/skills".into(),
                    name: "Community Hub".into(),
                    skill_count: 128,
                },
            ],
            active_tab: SkillsTab::Installed,
            search_query: String::new(),
            selected_category: None,
            create_draft: CreateSkillDraft::empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// SkillsPanel
// ---------------------------------------------------------------------------

/// Skills marketplace: installed skills, directory, add sources.
pub struct SkillsPanel;

impl SkillsPanel {
    pub fn render(data: &SkillsData, theme: &HiveTheme) -> impl IntoElement {
        let enabled_count = data.installed.iter().filter(|s| s.enabled).count();

        div()
            .id("skills-panel")
            .flex()
            .flex_col()
            .size_full()
            .child(render_header(enabled_count, data.installed.len(), theme))
            .child(render_tab_bar(&data.active_tab, theme))
            .child(render_search_field(&data.search_query, theme))
            .child(render_tab_content(data, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(enabled_count: usize, total_count: usize, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .p(theme.space_4)
        .gap(theme.space_3)
        .border_b_1()
        .border_color(theme.border)
        .child(header_icon(theme))
        .child(header_title_block(enabled_count, total_count, theme))
        .child(div().flex_1())
        .child(refresh_button(theme))
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
        .child(Icon::new(IconName::Star).size_4())
}

fn header_title_block(enabled_count: usize, total_count: usize, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(header_title_row(enabled_count, total_count, theme))
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Hive's skill marketplace \u{2014} install, manage, and discover skills"),
        )
}

fn header_title_row(enabled_count: usize, total_count: usize, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_xl)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("ClawdHub"),
        )
        .child(installed_count_badge(enabled_count, total_count, theme))
}

fn installed_count_badge(enabled: usize, total: usize, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_green)
        .child(format!("{enabled}/{total} active"))
        .into_any_element()
}

fn refresh_button(theme: &HiveTheme) -> AnyElement {
    div()
        .id("skills-refresh")
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_sm)
        .text_color(theme.accent_cyan)
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.bg(theme.bg_tertiary))
        .child("\u{21BB} Refresh")
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            window.dispatch_action(Box::new(SkillsRefresh), cx);
        })
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tab bar (pill-style buttons)
// ---------------------------------------------------------------------------

fn render_tab_bar(active: &SkillsTab, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .px(theme.space_4)
        .py(theme.space_2)
        .gap(theme.space_2)
        .border_b_1()
        .border_color(theme.border)
        .child(tab_pill(
            "Installed",
            *active == SkillsTab::Installed,
            theme,
        ))
        .child(tab_pill(
            "Directory",
            *active == SkillsTab::Directory,
            theme,
        ))
        .child(tab_pill("Create", *active == SkillsTab::Create, theme))
        .child(tab_pill(
            "Add Source",
            *active == SkillsTab::AddSource,
            theme,
        ))
        .into_any_element()
}

fn tab_pill(label: &str, active: bool, theme: &HiveTheme) -> AnyElement {
    let (bg, text_color, border_color) = if active {
        (theme.accent_aqua, theme.text_on_accent, theme.accent_aqua)
    } else {
        (theme.bg_surface, theme.text_secondary, theme.border)
    };

    let tab_name = label.to_string();
    let id_str = format!("skills-tab-{}", label.to_lowercase().replace(' ', "-"));

    div()
        .id(SharedString::from(id_str))
        .px(theme.space_3)
        .py(theme.space_1)
        .rounded(theme.radius_full)
        .bg(bg)
        .border_1()
        .border_color(border_color)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::MEDIUM)
        .text_color(text_color)
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.opacity(0.85))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(SkillsSetTab {
                    tab: tab_name.clone(),
                }),
                cx,
            );
        })
        .child(label.to_string())
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Search field (shows current query or placeholder)
// ---------------------------------------------------------------------------

fn render_search_field(search_query: &str, theme: &HiveTheme) -> AnyElement {
    let display_text = if search_query.is_empty() {
        "Search skills...".to_string()
    } else {
        search_query.to_string()
    };

    let text_color = if search_query.is_empty() {
        theme.text_muted
    } else {
        theme.text_primary
    };

    let has_query = !search_query.is_empty();

    div()
        .px(theme.space_4)
        .py(theme.space_2)
        .child(
            div()
                .flex()
                .items_center()
                .px(theme.space_2)
                .py(theme.space_1)
                .rounded(theme.radius_md)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .hover(|style: StyleRefinement| style.border_color(theme.border_focus))
                .child(
                    div()
                        .mr(theme.space_1)
                        .child(Icon::new(IconName::Search).size_3p5()),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(theme.font_size_sm)
                        .text_color(text_color)
                        .child(display_text),
                )
                .when(has_query, |el: Div| {
                    el.child(
                        div()
                            .id("skills-clear-search")
                            .ml(theme.space_1)
                            .px(theme.space_1)
                            .rounded(theme.radius_sm)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .hover(|style: StyleRefinement| style.text_color(theme.text_primary))
                            .on_mouse_down(
                                MouseButton::Left,
                                |_event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                    window.dispatch_action(Box::new(SkillsClearSearch), cx);
                                },
                            )
                            .child("\u{2715}"),
                    )
                }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tab content router
// ---------------------------------------------------------------------------

fn render_tab_content(data: &SkillsData, theme: &HiveTheme) -> AnyElement {
    match data.active_tab {
        SkillsTab::Installed => {
            let filtered = data.filtered_installed();
            render_installed_tab(&filtered, theme)
        }
        SkillsTab::Directory => {
            let filtered = data.filtered_directory();
            render_directory_tab(&filtered, &data.selected_category, theme)
        }
        SkillsTab::Create => render_create_tab(&data.create_draft, theme),
        SkillsTab::AddSource => render_add_source_tab(&data.sources, theme),
    }
}

// ---------------------------------------------------------------------------
// Installed tab
// ---------------------------------------------------------------------------

fn render_installed_tab(skills: &[&InstalledSkill], theme: &HiveTheme) -> AnyElement {
    if skills.is_empty() {
        return render_empty_state(
            "\u{1F4E6}",
            "No skills installed",
            "Browse the Directory tab to find and install skills.",
            theme,
        );
    }

    let mut list = div()
        .id("installed-skills-list")
        .flex()
        .flex_col()
        .flex_1()
        .overflow_y_scroll()
        .p(theme.space_4)
        .gap(theme.space_3);

    for skill in skills {
        list = list.child(render_installed_card(skill, theme));
    }

    list.into_any_element()
}

fn render_installed_card(skill: &InstalledSkill, theme: &HiveTheme) -> AnyElement {
    let has_hash = !skill.integrity_hash.is_empty();

    div()
        .flex()
        .flex_col()
        .p(theme.space_3)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(installed_card_top_row(skill, has_hash, theme))
        .child(installed_card_description(skill, theme))
        .child(installed_card_bottom_row(skill, theme))
        .into_any_element()
}

fn installed_card_top_row(skill: &InstalledSkill, has_hash: bool, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.accent_aqua)
                .font_weight(FontWeight::BOLD)
                .child(skill.name.clone()),
        )
        .child(version_badge(&skill.version, theme))
        .child(div().flex_1())
        .child(integrity_badge(has_hash, theme))
}

fn installed_card_description(skill: &InstalledSkill, theme: &HiveTheme) -> Div {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_secondary)
        .overflow_hidden()
        .max_h(px(36.0))
        .child(skill.description.clone())
}

fn installed_card_bottom_row(skill: &InstalledSkill, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(toggle_switch(skill.enabled, &skill.id, theme))
        .child(div().flex_1())
        .child(remove_button(&skill.id, theme))
}

// ---------------------------------------------------------------------------
// Directory tab
// ---------------------------------------------------------------------------

fn render_directory_tab(
    skills: &[&DirectorySkill],
    selected_category: &Option<SkillCategory>,
    theme: &HiveTheme,
) -> AnyElement {
    let mut container = div()
        .id("directory-skills-tab")
        .flex()
        .flex_col()
        .flex_1()
        .overflow_y_scroll();

    // Category filter bar
    container = container.child(render_category_bar(selected_category, theme));

    if skills.is_empty() {
        return container
            .child(render_empty_state(
                "\u{1F310}",
                "No skills available",
                "Add a skill source or change the category filter.",
                theme,
            ))
            .into_any_element();
    }

    let mut grid = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .p(theme.space_4)
        .gap(theme.space_3);

    for skill in skills {
        grid = grid.child(render_directory_card(skill, theme));
    }

    container.child(grid).into_any_element()
}

fn render_category_bar(selected: &Option<SkillCategory>, theme: &HiveTheme) -> AnyElement {
    let mut bar = div()
        .flex()
        .flex_row()
        .items_center()
        .px(theme.space_4)
        .py(theme.space_2)
        .gap(theme.space_1);

    // "All" pill
    let all_active = selected.is_none();
    bar = bar.child(category_pill("All", all_active, theme));

    for cat in &SkillCategory::ALL {
        let active = selected.is_some_and(|s| s == *cat);
        bar = bar.child(category_pill(cat.label(), active, theme));
    }

    bar.into_any_element()
}

fn category_pill(label: &str, active: bool, theme: &HiveTheme) -> AnyElement {
    let (bg, text_color) = if active {
        (theme.accent_cyan, theme.text_on_accent)
    } else {
        (theme.bg_tertiary, theme.text_secondary)
    };

    let cat_name = label.to_string();
    let id_str = format!("skills-cat-{}", label.to_lowercase().replace(' ', "-"));

    div()
        .id(SharedString::from(id_str))
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_full)
        .bg(bg)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::MEDIUM)
        .text_color(text_color)
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.opacity(0.85))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(SkillsSetCategory {
                    category: cat_name.clone(),
                }),
                cx,
            );
        })
        .child(label.to_string())
        .into_any_element()
}

fn render_directory_card(skill: &DirectorySkill, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w(px(280.0))
        .p(theme.space_3)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .hover(|style: StyleRefinement| style.border_color(theme.border_focus))
        .child(directory_card_top_row(skill, theme))
        .child(directory_card_metadata(skill, theme))
        .child(directory_card_description(skill, theme))
        .child(directory_card_action(skill, theme))
        .into_any_element()
}

fn directory_card_top_row(skill: &DirectorySkill, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(skill.name.clone()),
        )
        .child(version_badge(&skill.version, theme))
        .child(div().flex_1())
        .child(if skill.installed {
            installed_badge(theme).into_any_element()
        } else {
            div().into_any_element()
        })
}

fn directory_card_metadata(skill: &DirectorySkill, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .flex_wrap()
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("by {}", skill.author)),
        )
        .child(metadata_dot(theme))
        .child(download_count_display(skill.downloads, theme))
        .child(metadata_dot(theme))
        .child(rating_display(skill.rating, theme))
        .child(metadata_dot(theme))
        .child(category_badge(skill.category.label(), theme))
}

fn download_count_display(downloads: usize, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(3.0))
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("\u{2B07}"),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(format_download_count(downloads)),
        )
}

fn directory_card_description(skill: &DirectorySkill, theme: &HiveTheme) -> Div {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_secondary)
        .overflow_hidden()
        .max_h(px(36.0))
        .flex_1()
        .child(skill.description.clone())
}

fn directory_card_action(skill: &DirectorySkill, theme: &HiveTheme) -> AnyElement {
    if skill.installed {
        div()
            .flex()
            .items_center()
            .justify_center()
            .px(theme.space_3)
            .py(theme.space_1)
            .rounded(theme.radius_md)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_sm)
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.accent_green)
            .child("Installed")
            .into_any_element()
    } else {
        install_button(&skill.id, theme)
    }
}

// ---------------------------------------------------------------------------
// Create tab
// ---------------------------------------------------------------------------

fn render_create_tab(draft: &CreateSkillDraft, theme: &HiveTheme) -> AnyElement {
    div()
        .id("create-skill-tab")
        .flex()
        .flex_col()
        .flex_1()
        .overflow_y_scroll()
        .p(theme.space_4)
        .gap(theme.space_4)
        // Header card
        .child(
            div()
                .flex()
                .flex_col()
                .p(theme.space_4)
                .gap(theme.space_3)
                .rounded(theme.radius_md)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(theme.font_size_lg)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Create New Skill"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Define a custom skill with instructions for the AI."),
                ),
        )
        // Form fields
        .child(render_create_form(draft, theme))
        // Preview section
        .child(render_create_preview(draft, theme))
        .into_any_element()
}

fn render_create_form(draft: &CreateSkillDraft, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(form_section_title("Skill Details", theme))
        .child(form_field_display("Name", &draft.name, "my_skill", theme))
        .child(form_field_display(
            "Version",
            &draft.version,
            "0.1.0",
            theme,
        ))
        .child(form_field_display(
            "Description",
            &draft.description,
            "A brief description of what this skill does...",
            theme,
        ))
        .child(instructions_field(draft, theme))
        .child(create_button_row(draft, theme))
        .into_any_element()
}

fn form_section_title(text: &str, theme: &HiveTheme) -> Div {
    div()
        .text_size(theme.font_size_base)
        .text_color(theme.text_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
}

fn instructions_field(draft: &CreateSkillDraft, theme: &HiveTheme) -> Div {
    let (text_color, content) = if draft.instructions.is_empty() {
        (
            theme.text_muted,
            "You are a helpful assistant that... (write detailed instructions for the AI)"
                .to_string(),
        )
    } else {
        (theme.text_primary, draft.instructions.clone())
    };

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .font_weight(FontWeight::MEDIUM)
                .child("Instructions"),
        )
        .child(instructions_textarea(text_color, &content, theme))
}

fn instructions_textarea(text_color: Hsla, content: &str, theme: &HiveTheme) -> Div {
    div()
        .w_full()
        .min_h(px(120.0))
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_sm)
        .font_family(SharedString::from("JetBrains Mono"))
        .text_color(text_color)
        .child(content.to_string())
}

fn create_button_row(draft: &CreateSkillDraft, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .mt(theme.space_2)
        .child(div().flex_1())
        .child(create_skill_button(draft, theme))
}

fn create_skill_button(draft: &CreateSkillDraft, theme: &HiveTheme) -> AnyElement {
    let valid = draft.is_valid();
    let (bg, text_color) = if valid {
        (theme.accent_cyan, theme.text_on_accent)
    } else {
        (theme.bg_tertiary, theme.text_muted)
    };

    let name = draft.name.clone();
    let description = draft.description.clone();
    let instructions = draft.instructions.clone();

    let mut btn = div()
        .id("skills-create-btn")
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_4)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(bg)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::BOLD)
        .text_color(text_color);

    if valid {
        btn = btn.cursor_pointer().on_mouse_down(
            MouseButton::Left,
            move |_event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                window.dispatch_action(
                    Box::new(SkillsCreate {
                        name: name.clone(),
                        description: description.clone(),
                        instructions: instructions.clone(),
                    }),
                    cx,
                );
            },
        );
    }

    btn.child("Create Skill").into_any_element()
}

fn form_field_display(
    label: &str,
    value: &str,
    placeholder: &str,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .font_weight(FontWeight::MEDIUM)
                .child(label.to_string()),
        )
        .child(
            div()
                .w_full()
                .px(theme.space_3)
                .py(theme.space_2)
                .rounded(theme.radius_md)
                .bg(theme.bg_primary)
                .border_1()
                .border_color(theme.border)
                .text_size(theme.font_size_sm)
                .text_color(if value.is_empty() {
                    theme.text_muted
                } else {
                    theme.text_primary
                })
                .child(if value.is_empty() {
                    placeholder.to_string()
                } else {
                    value.to_string()
                }),
        )
        .into_any_element()
}

fn render_create_preview(draft: &CreateSkillDraft, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(form_section_title("Preview", theme))
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("How your skill will appear once created."),
        )
        .child(separator(theme))
        .child(preview_card_inner(draft, theme))
        .into_any_element()
}

fn preview_card_inner(draft: &CreateSkillDraft, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .p(theme.space_3)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .child(preview_name_row(draft, theme))
        .child(preview_description(draft, theme))
        .child(preview_footer(draft, theme))
}

fn preview_name_row(draft: &CreateSkillDraft, theme: &HiveTheme) -> Div {
    let display_name = if draft.name.is_empty() {
        "my_skill".to_string()
    } else {
        draft.name.clone()
    };
    let display_version = if draft.version.is_empty() {
        "0.1.0"
    } else {
        &draft.version
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.accent_aqua)
                .font_weight(FontWeight::BOLD)
                .child(display_name),
        )
        .child(version_badge(display_version, theme))
}

fn preview_description(draft: &CreateSkillDraft, theme: &HiveTheme) -> Div {
    let text = if draft.description.is_empty() {
        "No description yet.".to_string()
    } else {
        draft.description.clone()
    };

    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_secondary)
        .child(text)
}

fn preview_footer(draft: &CreateSkillDraft, theme: &HiveTheme) -> Div {
    let (status_color, status_text) = if draft.instructions.is_empty() {
        (theme.accent_yellow, "Instructions needed")
    } else {
        (theme.accent_green, "Instructions ready")
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("by you"),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(status_color)
                .child(status_text),
        )
}

// ---------------------------------------------------------------------------
// Add Source tab
// ---------------------------------------------------------------------------

fn render_add_source_tab(sources: &[SkillSource], theme: &HiveTheme) -> AnyElement {
    let mut container = div()
        .id("add-source-tab")
        .flex()
        .flex_col()
        .flex_1()
        .overflow_y_scroll()
        .p(theme.space_4)
        .gap(theme.space_4);

    // URL input section
    container = container.child(render_url_input_section(theme));

    // Configured sources list
    container = container.child(render_sources_list(sources, theme));

    container.into_any_element()
}

fn render_url_input_section(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("Add Skill Source"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Enter a URL to a skill registry or GitHub repository."),
        )
        .child(url_input_row(theme))
        .into_any_element()
}

fn url_input_row(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(url_input_placeholder(theme))
        .child(add_source_button(theme))
}

fn url_input_placeholder(theme: &HiveTheme) -> Div {
    div()
        .flex_1()
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_sm)
        .text_color(theme.text_muted)
        .child("https://github.com/org/skills-repo")
}

fn render_sources_list(sources: &[SkillSource], theme: &HiveTheme) -> AnyElement {
    if sources.is_empty() {
        return render_empty_state(
            "\u{1F517}",
            "No sources configured",
            "Add a skill source URL above to get started.",
            theme,
        );
    }

    let mut card = div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("Configured Sources"),
        )
        .child(separator(theme));

    for source in sources {
        card = card.child(render_source_row(source, theme));
    }

    card.into_any_element()
}

fn render_source_row(source: &SkillSource, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .py(theme.space_1)
        .child(source_status_dot(theme))
        .child(source_info_block(source, theme))
        .child(source_count_badge(source, theme))
        .child(source_remove_button(source, theme))
        .into_any_element()
}

fn source_status_dot(theme: &HiveTheme) -> Div {
    div()
        .w(px(8.0))
        .h(px(8.0))
        .rounded(theme.radius_full)
        .bg(theme.accent_green)
}

fn source_info_block(source: &SkillSource, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(px(2.0))
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::MEDIUM)
                .child(source.name.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_family(SharedString::from("JetBrains Mono"))
                .child(source.url.clone()),
        )
}

fn source_count_badge(source: &SkillSource, theme: &HiveTheme) -> Div {
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_cyan)
        .child(format!("{} skills", source.skill_count))
}

fn source_remove_button(source: &SkillSource, theme: &HiveTheme) -> Stateful<Div> {
    let remove_url = source.url.clone();
    div()
        .id(SharedString::from(format!(
            "remove-source-{}",
            source.name
        )))
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_sm)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_red)
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.bg(theme.bg_tertiary))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(SkillsRemoveSource {
                    url: remove_url.clone(),
                }),
                cx,
            );
        })
        .child("Remove")
}

// ---------------------------------------------------------------------------
// Shared UI components
// ---------------------------------------------------------------------------

fn version_badge(version: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_1)
        .py(px(1.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .font_family(SharedString::from("JetBrains Mono"))
        .text_color(theme.text_secondary)
        .child(format!("v{version}"))
        .into_any_element()
}

fn integrity_badge(verified: bool, theme: &HiveTheme) -> AnyElement {
    let (icon, color) = if verified {
        ("\u{2705}", theme.accent_green)
    } else {
        ("\u{26A0}", theme.accent_yellow)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(3.0))
        .child(div().text_size(theme.font_size_xs).child(icon.to_string()))
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(color)
                .child(if verified { "Verified" } else { "Unverified" }),
        )
        .into_any_element()
}

fn metadata_dot(theme: &HiveTheme) -> AnyElement {
    div()
        .w(px(3.0))
        .h(px(3.0))
        .rounded(theme.radius_full)
        .bg(theme.text_muted)
        .into_any_element()
}

fn rating_display(rating: f32, theme: &HiveTheme) -> AnyElement {
    let star_color = if rating >= 4.5 {
        theme.accent_yellow
    } else if rating >= 3.5 {
        theme.accent_green
    } else {
        theme.text_muted
    };

    // Build star string: filled stars + empty stars
    let full_stars = rating.floor() as usize;
    let has_half = (rating - rating.floor()) >= 0.5;
    let mut stars = String::new();
    for _ in 0..full_stars {
        stars.push('\u{2605}'); // filled star
    }
    if has_half {
        stars.push('\u{2606}'); // half star approximated as outline
    }

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(3.0))
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(star_color)
                .child(stars),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(format!("{:.1}", rating)),
        )
        .into_any_element()
}

fn category_badge(label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_1)
        .py(px(1.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_powder)
        .child(label.to_string())
        .into_any_element()
}

fn installed_badge(theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(1.0))
        .rounded(theme.radius_full)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_green)
        .child("Installed")
        .into_any_element()
}

fn toggle_switch(enabled: bool, skill_id: &str, theme: &HiveTheme) -> AnyElement {
    let (track_bg, knob_bg, label, label_color) = if enabled {
        (
            theme.accent_green,
            theme.text_on_accent,
            "Enabled",
            theme.accent_green,
        )
    } else {
        (
            theme.bg_tertiary,
            theme.text_muted,
            "Disabled",
            theme.text_muted,
        )
    };

    let border_color = if enabled {
        theme.accent_green
    } else {
        theme.border
    };

    let toggle_id = skill_id.to_string();
    div()
        .id(SharedString::from(format!("toggle-{skill_id}")))
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(SkillsToggle {
                    skill_id: toggle_id.clone(),
                }),
                cx,
            );
        })
        .child(toggle_track(
            track_bg,
            knob_bg,
            border_color,
            enabled,
            theme,
        ))
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(label_color)
                .child(label),
        )
        .into_any_element()
}

fn toggle_track(
    track_bg: Hsla,
    knob_bg: Hsla,
    border_color: Hsla,
    enabled: bool,
    theme: &HiveTheme,
) -> Div {
    div()
        .w(px(36.0))
        .h(px(20.0))
        .rounded(theme.radius_full)
        .bg(track_bg)
        .border_1()
        .border_color(border_color)
        .flex()
        .items_center()
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .rounded(theme.radius_full)
                .bg(knob_bg)
                .ml(if enabled { px(19.0) } else { px(3.0) }),
        )
}

fn remove_button(skill_id: &str, theme: &HiveTheme) -> AnyElement {
    let remove_id = skill_id.to_string();
    div()
        .id(SharedString::from(format!("remove-{skill_id}")))
        .px(theme.space_2)
        .py(theme.space_1)
        .rounded(theme.radius_sm)
        .text_size(theme.font_size_sm)
        .text_color(theme.accent_red)
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.bg(theme.bg_tertiary))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(SkillsRemove {
                    skill_id: remove_id.clone(),
                }),
                cx,
            );
        })
        .child("Remove")
        .into_any_element()
}

fn install_button(skill_id: &str, theme: &HiveTheme) -> AnyElement {
    let install_id = skill_id.to_string();
    div()
        .id(SharedString::from(format!("install-{skill_id}")))
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
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.bg(theme.accent_aqua))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(SkillsInstall {
                    skill_id: install_id.clone(),
                }),
                cx,
            );
        })
        .child("Install")
        .into_any_element()
}

fn add_source_button(theme: &HiveTheme) -> AnyElement {
    div()
        .id("skills-add-source")
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.accent_cyan)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_on_accent)
        .cursor_pointer()
        .hover(|style: StyleRefinement| style.bg(theme.accent_aqua))
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            window.dispatch_action(
                Box::new(SkillsAddSource {
                    url: String::new(),
                    name: "Custom Source".to_string(),
                }),
                cx,
            );
        })
        .child("Add Source")
        .into_any_element()
}

fn separator(theme: &HiveTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(theme.border)
        .into_any_element()
}

fn render_empty_state(icon: &str, title: &str, subtitle: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_1()
        .gap(theme.space_2)
        .p(theme.space_8)
        .child(
            div()
                .text_size(px(32.0))
                .text_color(theme.text_muted)
                .child(icon.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(subtitle.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn format_download_count(count: usize) -> String {
    if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        format!("{count}")
    }
}
