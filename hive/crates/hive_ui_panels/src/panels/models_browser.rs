//! Model Browser panel — browse all available models and curate a project list.

use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use hive_ai::model_registry::MODEL_REGISTRY;
use hive_ai::types::{ModelInfo, ModelTier, ProviderType};

use hive_ui_core::HiveTheme;

use crate::components::model_selector::FetchStatus;

/// Max models to show per provider group before requiring expansion.
const MAX_VISIBLE_PER_GROUP: usize = 10;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when the user adds/removes a model from the project list.
#[derive(Debug, Clone)]
pub struct ProjectModelsChanged(pub Vec<String>);

// ---------------------------------------------------------------------------
// View mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Browse all available models across all providers.
    Browse,
    /// Show only models the user has added to their project.
    Project,
}

// ---------------------------------------------------------------------------
// ModelsBrowserView
// ---------------------------------------------------------------------------

pub struct ModelsBrowserView {
    theme: HiveTheme,

    // Search
    search_query: String,
    search_input: Entity<InputState>,

    // Provider filter (None = all)
    active_provider_filter: Option<ProviderType>,

    // Project model list
    project_models: HashSet<String>,

    // API key gating
    enabled_providers: HashSet<ProviderType>,

    // Cloud catalogs
    fetched_or_models: Vec<ModelInfo>,
    or_fetch_status: FetchStatus,
    openrouter_api_key: Option<String>,

    fetched_openai_models: Vec<ModelInfo>,
    openai_fetch_status: FetchStatus,
    openai_api_key: Option<String>,

    fetched_anthropic_models: Vec<ModelInfo>,
    anthropic_fetch_status: FetchStatus,
    anthropic_api_key: Option<String>,

    fetched_google_models: Vec<ModelInfo>,
    google_fetch_status: FetchStatus,
    google_api_key: Option<String>,

    fetched_groq_models: Vec<ModelInfo>,
    groq_fetch_status: FetchStatus,
    groq_api_key: Option<String>,

    fetched_hf_models: Vec<ModelInfo>,
    hf_fetch_status: FetchStatus,
    huggingface_api_key: Option<String>,

    // Local models
    discovered_local_models: Vec<ModelInfo>,

    // UI state
    collapsed_providers: HashSet<ProviderType>,
    expanded_providers: HashSet<ProviderType>,
    view_mode: ViewMode,
    show_tier_guide_collapsed: bool,
}

impl EventEmitter<ProjectModelsChanged> for ModelsBrowserView {}

impl ModelsBrowserView {
    pub fn new(
        project_models: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Search models\u{2026}", window, cx);
            state
        });

        cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self, _state, event, _window, cx| {
                if let InputEvent::Change = event {
                    this.search_query = _state.read(cx).value().to_string();
                    cx.notify();
                }
            },
        )
        .detach();

        Self {
            theme: HiveTheme::dark(),
            search_query: String::new(),
            search_input,
            active_provider_filter: None,
            project_models: project_models.into_iter().collect(),
            enabled_providers: HashSet::new(),
            fetched_or_models: Vec::new(),
            or_fetch_status: FetchStatus::Idle,
            openrouter_api_key: None,
            fetched_openai_models: Vec::new(),
            openai_fetch_status: FetchStatus::Idle,
            openai_api_key: None,
            fetched_anthropic_models: Vec::new(),
            anthropic_fetch_status: FetchStatus::Idle,
            anthropic_api_key: None,
            fetched_google_models: Vec::new(),
            google_fetch_status: FetchStatus::Idle,
            google_api_key: None,
            fetched_groq_models: Vec::new(),
            groq_fetch_status: FetchStatus::Idle,
            groq_api_key: None,
            fetched_hf_models: Vec::new(),
            hf_fetch_status: FetchStatus::Idle,
            huggingface_api_key: None,
            discovered_local_models: Vec::new(),
            collapsed_providers: HashSet::new(),
            expanded_providers: HashSet::new(),
            view_mode: ViewMode::Browse,
            show_tier_guide_collapsed: false,
        }
    }

    // -- Public setters (called by workspace) --

    pub fn set_enabled_providers(
        &mut self,
        providers: HashSet<ProviderType>,
        cx: &mut Context<Self>,
    ) {
        if self.enabled_providers != providers {
            self.enabled_providers = providers;
            cx.notify();
        }
    }

    pub fn set_openrouter_api_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        let changed = self.openrouter_api_key != key;
        self.openrouter_api_key = key;
        if changed {
            self.fetched_or_models.clear();
            self.or_fetch_status = FetchStatus::Idle;
            cx.notify();
        }
    }

    pub fn set_openai_api_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        let changed = self.openai_api_key != key;
        self.openai_api_key = key;
        if changed {
            self.fetched_openai_models.clear();
            self.openai_fetch_status = FetchStatus::Idle;
            cx.notify();
        }
    }

    pub fn set_anthropic_api_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        let changed = self.anthropic_api_key != key;
        self.anthropic_api_key = key;
        if changed {
            self.fetched_anthropic_models.clear();
            self.anthropic_fetch_status = FetchStatus::Idle;
            cx.notify();
        }
    }

    pub fn set_google_api_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        let changed = self.google_api_key != key;
        self.google_api_key = key;
        if changed {
            self.fetched_google_models.clear();
            self.google_fetch_status = FetchStatus::Idle;
            cx.notify();
        }
    }

    pub fn set_groq_api_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        let changed = self.groq_api_key != key;
        self.groq_api_key = key;
        if changed {
            self.fetched_groq_models.clear();
            self.groq_fetch_status = FetchStatus::Idle;
            cx.notify();
        }
    }

    pub fn set_huggingface_api_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        let changed = self.huggingface_api_key != key;
        self.huggingface_api_key = key;
        if changed {
            self.fetched_hf_models.clear();
            self.hf_fetch_status = FetchStatus::Idle;
            cx.notify();
        }
    }

    pub fn set_local_models(&mut self, models: Vec<ModelInfo>, cx: &mut Context<Self>) {
        if self.discovered_local_models != models {
            self.discovered_local_models = models;
            cx.notify();
        }
    }

    /// Trigger live catalog fetches for all configured providers.
    pub fn trigger_fetches(&mut self, cx: &mut Context<Self>) {
        self.maybe_fetch_openrouter(cx);
        self.maybe_fetch_openai(cx);
        self.maybe_fetch_anthropic(cx);
        self.maybe_fetch_google(cx);
        self.maybe_fetch_groq(cx);
        self.maybe_fetch_huggingface(cx);
    }

    // -- Project model management --

    fn toggle_project_model(&mut self, model_id: String, cx: &mut Context<Self>) {
        if self.project_models.contains(&model_id) {
            self.project_models.remove(&model_id);
        } else {
            self.project_models.insert(model_id);
        }
        let list: Vec<String> = self.project_models.iter().cloned().collect();
        cx.emit(ProjectModelsChanged(list));
        cx.notify();
    }

    fn is_in_project(&self, model_id: &str) -> bool {
        self.project_models.contains(model_id)
    }

    // -- Model collection --

    /// Build the full model list.
    ///
    /// **Architecture: live catalogs are the primary source.**
    ///
    /// 1. Start with locally-discovered models (Ollama / LM Studio / GenericLocal).
    /// 2. Add every model from the live catalog fetches (OpenRouter, OpenAI,
    ///    Anthropic, Google, Groq, Hugging Face).
    /// 3. Enrich each catalog model with metadata from the static registry
    ///    (pricing, tier, capabilities, context window) where a matching entry
    ///    exists — the registry acts as a metadata overlay, *not* the source of
    ///    truth for which models exist.
    /// 4. While a catalog is still loading, fall back to the static registry
    ///    entries for that provider so the list is never empty for enabled
    ///    providers.
    fn all_models(&self) -> Vec<ModelInfo> {
        let mut models: Vec<ModelInfo> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        // 1. Local models first — always shown regardless of API keys.
        for m in &self.discovered_local_models {
            if seen_ids.insert(m.id.clone()) {
                models.push(m.clone());
            }
        }

        // 2. Live catalog models — the primary source of truth.
        let catalogs: [(&Vec<ModelInfo>, FetchStatus, ProviderType); 6] = [
            (
                &self.fetched_or_models,
                self.or_fetch_status,
                ProviderType::OpenRouter,
            ),
            (
                &self.fetched_openai_models,
                self.openai_fetch_status,
                ProviderType::OpenAI,
            ),
            (
                &self.fetched_anthropic_models,
                self.anthropic_fetch_status,
                ProviderType::Anthropic,
            ),
            (
                &self.fetched_google_models,
                self.google_fetch_status,
                ProviderType::Google,
            ),
            (
                &self.fetched_groq_models,
                self.groq_fetch_status,
                ProviderType::Groq,
            ),
            (
                &self.fetched_hf_models,
                self.hf_fetch_status,
                ProviderType::HuggingFace,
            ),
        ];

        for (catalog, status, ptype) in &catalogs {
            if !self.enabled_providers.contains(ptype) {
                continue; // no API key → skip
            }

            if *status == FetchStatus::Done && !catalog.is_empty() {
                // Catalog loaded — use it as primary, enriched with registry metadata.
                for m in *catalog {
                    if seen_ids.insert(m.id.clone()) {
                        let mut enriched = m.clone();
                        hive_ai::model_registry::enrich_from_registry(&mut enriched);
                        models.push(enriched);
                    }
                }
            } else {
                // Catalog not yet loaded (Idle / Loading / Failed / empty Done).
                // Fall back to the static registry for this provider so the
                // user sees *something* while the fetch is in progress.
                for m in MODEL_REGISTRY.iter() {
                    if m.provider_type == *ptype && seen_ids.insert(m.id.clone()) {
                        models.push(m.clone());
                    }
                }
            }
        }

        models
    }

    fn matches_search(model: &ModelInfo, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        model.name.to_lowercase().contains(&q) || model.id.to_lowercase().contains(&q)
    }

    fn is_local_provider(ptype: ProviderType) -> bool {
        matches!(
            ptype,
            ProviderType::Ollama | ProviderType::LMStudio | ProviderType::GenericLocal
        )
    }

    // -- Catalog fetchers (same tokio bridge pattern) --

    fn maybe_fetch_openrouter(&mut self, cx: &mut Context<Self>) {
        if self.or_fetch_status == FetchStatus::Loading
            || self.or_fetch_status == FetchStatus::Done
        {
            return;
        }
        let Some(api_key) = self.openrouter_api_key.clone() else {
            return;
        };
        if api_key.is_empty() {
            return;
        }
        self.or_fetch_status = FetchStatus::Loading;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(
                    hive_ai::providers::openrouter_catalog::fetch_openrouter_models(&api_key),
                ),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| match result {
                Ok(models) => {
                    this.fetched_or_models = models;
                    this.or_fetch_status = FetchStatus::Done;
                    cx.notify();
                }
                Err(_) => {
                    this.or_fetch_status = FetchStatus::Failed;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn maybe_fetch_openai(&mut self, cx: &mut Context<Self>) {
        if self.openai_fetch_status == FetchStatus::Loading
            || self.openai_fetch_status == FetchStatus::Done
        {
            return;
        }
        let Some(api_key) = self.openai_api_key.clone() else {
            return;
        };
        if api_key.is_empty() {
            return;
        }
        self.openai_fetch_status = FetchStatus::Loading;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => {
                    rt.block_on(hive_ai::providers::openai_catalog::fetch_openai_models(&api_key))
                }
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| match result {
                Ok(models) => {
                    this.fetched_openai_models = models;
                    this.openai_fetch_status = FetchStatus::Done;
                    cx.notify();
                }
                Err(_) => {
                    this.openai_fetch_status = FetchStatus::Failed;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn maybe_fetch_anthropic(&mut self, cx: &mut Context<Self>) {
        if self.anthropic_fetch_status == FetchStatus::Loading
            || self.anthropic_fetch_status == FetchStatus::Done
        {
            return;
        }
        let Some(api_key) = self.anthropic_api_key.clone() else {
            return;
        };
        if api_key.is_empty() {
            return;
        }
        self.anthropic_fetch_status = FetchStatus::Loading;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(
                    hive_ai::providers::anthropic_catalog::fetch_anthropic_models(&api_key),
                ),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| match result {
                Ok(models) => {
                    this.fetched_anthropic_models = models;
                    this.anthropic_fetch_status = FetchStatus::Done;
                    cx.notify();
                }
                Err(_) => {
                    this.anthropic_fetch_status = FetchStatus::Failed;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn maybe_fetch_google(&mut self, cx: &mut Context<Self>) {
        if self.google_fetch_status == FetchStatus::Loading
            || self.google_fetch_status == FetchStatus::Done
        {
            return;
        }
        let Some(api_key) = self.google_api_key.clone() else {
            return;
        };
        if api_key.is_empty() {
            return;
        }
        self.google_fetch_status = FetchStatus::Loading;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(
                    hive_ai::providers::google_catalog::fetch_google_models(&api_key),
                ),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| match result {
                Ok(models) => {
                    this.fetched_google_models = models;
                    this.google_fetch_status = FetchStatus::Done;
                    cx.notify();
                }
                Err(_) => {
                    this.google_fetch_status = FetchStatus::Failed;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn maybe_fetch_groq(&mut self, cx: &mut Context<Self>) {
        if self.groq_fetch_status == FetchStatus::Loading
            || self.groq_fetch_status == FetchStatus::Done
        {
            return;
        }
        let Some(api_key) = self.groq_api_key.clone() else {
            return;
        };
        if api_key.is_empty() {
            return;
        }
        self.groq_fetch_status = FetchStatus::Loading;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => {
                    rt.block_on(hive_ai::providers::groq_catalog::fetch_groq_models(&api_key))
                }
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| match result {
                Ok(models) => {
                    this.fetched_groq_models = models;
                    this.groq_fetch_status = FetchStatus::Done;
                    cx.notify();
                }
                Err(_) => {
                    this.groq_fetch_status = FetchStatus::Failed;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn maybe_fetch_huggingface(&mut self, cx: &mut Context<Self>) {
        if self.hf_fetch_status == FetchStatus::Loading
            || self.hf_fetch_status == FetchStatus::Done
        {
            return;
        }
        let Some(api_key) = self.huggingface_api_key.clone() else {
            return;
        };
        if api_key.is_empty() {
            return;
        }
        self.hf_fetch_status = FetchStatus::Loading;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(
                    hive_ai::providers::huggingface_catalog::fetch_huggingface_models(&api_key),
                ),
                Err(e) => Err(format!("tokio runtime: {e}")),
            };
            let _ = tx.send(result);
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result = rx.await.unwrap_or(Err("channel closed".into()));
            let _ = this.update(app, |this, cx| match result {
                Ok(models) => {
                    this.fetched_hf_models = models;
                    this.hf_fetch_status = FetchStatus::Done;
                    cx.notify();
                }
                Err(_) => {
                    this.hf_fetch_status = FetchStatus::Failed;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn retry_fetch(&mut self, provider: ProviderType, cx: &mut Context<Self>) {
        match provider {
            ProviderType::OpenRouter => self.or_fetch_status = FetchStatus::Idle,
            ProviderType::OpenAI => self.openai_fetch_status = FetchStatus::Idle,
            ProviderType::Anthropic => self.anthropic_fetch_status = FetchStatus::Idle,
            ProviderType::Google => self.google_fetch_status = FetchStatus::Idle,
            ProviderType::Groq => self.groq_fetch_status = FetchStatus::Idle,
            ProviderType::HuggingFace => self.hf_fetch_status = FetchStatus::Idle,
            _ => return,
        }
        match provider {
            ProviderType::OpenRouter => self.maybe_fetch_openrouter(cx),
            ProviderType::OpenAI => self.maybe_fetch_openai(cx),
            ProviderType::Anthropic => self.maybe_fetch_anthropic(cx),
            ProviderType::Google => self.maybe_fetch_google(cx),
            ProviderType::Groq => self.maybe_fetch_groq(cx),
            ProviderType::HuggingFace => self.maybe_fetch_huggingface(cx),
            _ => {}
        }
    }

    /// Force-refresh all catalogs by invalidating their caches and re-fetching.
    fn refresh_all_catalogs(&mut self, cx: &mut Context<Self>) {
        // Invalidate server-side caches so the next fetch hits the API.
        hive_ai::providers::openrouter_catalog::invalidate_cache();
        hive_ai::providers::openai_catalog::invalidate_cache();
        hive_ai::providers::anthropic_catalog::invalidate_cache();
        hive_ai::providers::google_catalog::invalidate_cache();
        hive_ai::providers::groq_catalog::invalidate_cache();
        hive_ai::providers::huggingface_catalog::invalidate_cache();

        // Reset local fetch state so they re-trigger.
        self.or_fetch_status = FetchStatus::Idle;
        self.openai_fetch_status = FetchStatus::Idle;
        self.anthropic_fetch_status = FetchStatus::Idle;
        self.google_fetch_status = FetchStatus::Idle;
        self.groq_fetch_status = FetchStatus::Idle;
        self.hf_fetch_status = FetchStatus::Idle;

        self.fetched_or_models.clear();
        self.fetched_openai_models.clear();
        self.fetched_anthropic_models.clear();
        self.fetched_google_models.clear();
        self.fetched_groq_models.clear();
        self.fetched_hf_models.clear();

        // Re-trigger all fetches.
        self.trigger_fetches(cx);
        cx.notify();
    }

    /// Returns true if any enabled catalog is currently loading.
    fn any_catalog_loading(&self) -> bool {
        let statuses = [
            (self.or_fetch_status, ProviderType::OpenRouter),
            (self.openai_fetch_status, ProviderType::OpenAI),
            (self.anthropic_fetch_status, ProviderType::Anthropic),
            (self.google_fetch_status, ProviderType::Google),
            (self.groq_fetch_status, ProviderType::Groq),
            (self.hf_fetch_status, ProviderType::HuggingFace),
        ];
        statuses.iter().any(|(status, ptype)| {
            self.enabled_providers.contains(ptype) && *status == FetchStatus::Loading
        })
    }

    /// Returns the count of enabled catalogs that have finished loading.
    fn catalogs_done_count(&self) -> (usize, usize) {
        let statuses = [
            (self.or_fetch_status, ProviderType::OpenRouter),
            (self.openai_fetch_status, ProviderType::OpenAI),
            (self.anthropic_fetch_status, ProviderType::Anthropic),
            (self.google_fetch_status, ProviderType::Google),
            (self.groq_fetch_status, ProviderType::Groq),
            (self.hf_fetch_status, ProviderType::HuggingFace),
        ];
        let enabled: Vec<_> = statuses
            .iter()
            .filter(|(_, ptype)| self.enabled_providers.contains(ptype))
            .collect();
        let done = enabled
            .iter()
            .filter(|(status, _)| *status == FetchStatus::Done)
            .count();
        (done, enabled.len())
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for ModelsBrowserView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let all_models = self.all_models();
        let project_count = self.project_models.len();
        let _local_count = all_models
            .iter()
            .filter(|m| Self::is_local_provider(m.provider_type))
            .count();

        let view_mode = self.view_mode;
        let is_loading = self.any_catalog_loading();
        let (done, total) = self.catalogs_done_count();

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
                            .child("Models"),
                    )
                    // Refresh button
                    .child(
                        div()
                            .id("refresh-catalogs-btn")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded(theme.radius_md)
                            .bg(theme.bg_tertiary)
                            .text_size(theme.font_size_sm)
                            .text_color(if is_loading {
                                theme.text_muted
                            } else {
                                theme.accent_cyan
                            })
                            .cursor(if is_loading {
                                CursorStyle::default()
                            } else {
                                CursorStyle::PointingHand
                            })
                            .when(!is_loading, |el| {
                                el.hover(|s| s.bg(theme.bg_surface).text_color(theme.accent_aqua))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _e, _w, cx| {
                                    if !this.any_catalog_loading() {
                                        this.refresh_all_catalogs(cx);
                                    }
                                }),
                            )
                            .child(if is_loading { "\u{21BB}" } else { "\u{21BB}" }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(theme.space_1)
                    .child(self.render_tab("Browse All", ViewMode::Browse, view_mode, theme, cx))
                    .child(self.render_tab(
                        &format!("My Models ({project_count})"),
                        ViewMode::Project,
                        view_mode,
                        theme,
                        cx,
                    )),
            );

        // Search bar + catalog status line
        let catalog_status_text = if is_loading {
            format!(
                "Refreshing catalogs\u{2026} ({done}/{total} providers loaded)",
            )
        } else if total > 0 && done == total {
            "Live catalogs loaded \u{2014} showing latest available models".to_string()
        } else if total > 0 {
            format!("{done}/{total} provider catalogs loaded")
        } else {
            String::new()
        };

        let search_bar = div()
            .flex()
            .flex_col()
            .px(theme.space_4)
            .py(theme.space_2)
            .gap(theme.space_1)
            .child(
                Input::new(&self.search_input)
                    .appearance(true)
                    .cleanable(false),
            )
            .when(!catalog_status_text.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme.space_1)
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(if is_loading {
                                    theme.accent_yellow
                                } else {
                                    theme.accent_green
                                })
                                .child(if is_loading {
                                    "\u{25CF}"
                                } else {
                                    "\u{25CF}"
                                }),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(catalog_status_text),
                        ),
                )
            });

        // Tier coverage guide — convert to AnyElement to release the cx borrow
        // before render_model_list needs it.
        let tier_guide = self
            .render_tier_guide(&all_models, theme, cx)
            .into_any_element();

        // Model list
        let model_list = self.render_model_list(&all_models, theme, cx);

        div()
            .id("models-browser-panel")
            .flex()
            .flex_col()
            .size_full()
            .child(header)
            .child(search_bar)
            .child(tier_guide)
            .child(model_list)
    }
}

impl ModelsBrowserView {
    fn render_tab(
        &self,
        label: &str,
        mode: ViewMode,
        current: ViewMode,
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = mode == current;
        let tab_mode = mode;

        div()
            .id(ElementId::Name(format!("tab-{label}").into()))
            .px(theme.space_3)
            .py(theme.space_1)
            .rounded(theme.radius_full)
            .text_size(theme.font_size_sm)
            .cursor_pointer()
            .when(is_active, |el| {
                el.bg(theme.accent_cyan)
                    .text_color(theme.bg_primary)
                    .font_weight(FontWeight::BOLD)
            })
            .when(!is_active, |el| {
                el.bg(theme.bg_tertiary)
                    .text_color(theme.text_secondary)
                    .hover(|s| s.bg(theme.bg_surface))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _e, _w, cx| {
                    this.view_mode = tab_mode;
                    cx.notify();
                }),
            )
            .child(label.to_string())
    }

    // -- Tier coverage guide --------------------------------------------------

    fn render_tier_guide(
        &self,
        all_models: &[ModelInfo],
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Count project models per tier (include local models automatically).
        let mut premium = 0u32;
        let mut mid = 0u32;
        let mut budget = 0u32;
        let mut free = 0u32;
        for m in all_models {
            let counts = self.is_in_project(&m.id) || Self::is_local_provider(m.provider_type);
            if !counts {
                continue;
            }
            match m.tier {
                ModelTier::Premium => premium += 1,
                ModelTier::Mid => mid += 1,
                ModelTier::Budget => budget += 1,
                ModelTier::Free => free += 1,
            }
        }

        let all_covered = premium > 0 && mid > 0 && budget > 0;
        let show_guide = !self.show_tier_guide_collapsed;

        let tier_rows: [(ModelTier, &str, &str, u32, Hsla); 4] = [
            (
                ModelTier::Premium,
                "Premium",
                "Complex reasoning, code generation, long analysis",
                premium,
                theme.accent_pink,
            ),
            (
                ModelTier::Mid,
                "Mid",
                "Everyday chat, summarisation, Q&A",
                mid,
                theme.accent_cyan,
            ),
            (
                ModelTier::Budget,
                "Budget",
                "Simple tasks, quick lookups, translations",
                budget,
                theme.accent_green,
            ),
            (
                ModelTier::Free,
                "Free",
                "Local models, free API tiers",
                free,
                theme.text_muted,
            ),
        ];

        let mut rows: Vec<AnyElement> = Vec::new();
        if show_guide {
            for (tier, label, desc, count, color) in &tier_rows {
                let has_models = *count > 0;
                let status_icon = if has_models { "\u{2713}" } else { "\u{25CB}" };
                let status_color = if has_models {
                    theme.accent_green
                } else {
                    theme.text_muted
                };
                let needed = match tier {
                    ModelTier::Free => "optional",
                    _ => "need \u{2265}1",
                };
                let needed_label = if has_models {
                    format!("{count}")
                } else {
                    needed.to_string()
                };

                rows.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme.space_2)
                        .py(px(3.0))
                        .child(
                            div()
                                .w(px(16.0))
                                .text_size(theme.font_size_sm)
                                .text_color(status_color)
                                .child(status_icon),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(theme.space_2)
                                .flex_1()
                                .child(
                                    div()
                                        .text_size(theme.font_size_xs)
                                        .text_color(*color)
                                        .font_weight(FontWeight::BOLD)
                                        .w(px(60.0))
                                        .child(label.to_string()),
                                )
                                .child(
                                    div()
                                        .text_size(theme.font_size_xs)
                                        .text_color(theme.text_muted)
                                        .flex_1()
                                        .child(desc.to_string()),
                                ),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(if has_models {
                                    theme.accent_green
                                } else {
                                    theme.text_muted
                                })
                                .min_w(px(48.0))
                                .text_right()
                                .child(needed_label),
                        )
                        .into_any_element(),
                );
            }
        }

        let summary_text = if all_covered {
            format!(
                "\u{2705} Auto Routing ready \u{2014} {premium}P / {mid}M / {budget}B / {free}F"
            )
        } else {
            let mut missing: Vec<&str> = Vec::new();
            if premium == 0 {
                missing.push("Premium");
            }
            if mid == 0 {
                missing.push("Mid");
            }
            if budget == 0 {
                missing.push("Budget");
            }
            format!(
                "Add at least 1 model to: {}",
                missing.join(", ")
            )
        };
        let summary_color = if all_covered {
            theme.accent_green
        } else {
            theme.accent_yellow
        };

        let collapse_icon = if show_guide { "\u{25BC}" } else { "\u{25B6}" };

        div()
            .mx(theme.space_4)
            .my(theme.space_2)
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(if all_covered {
                let mut c = theme.accent_green;
                c.a = 0.25;
                c
            } else {
                let mut c = theme.accent_yellow;
                c.a = 0.25;
                c
            })
            .child(
                div()
                    .id("tier-guide-header")
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _e, _w, cx| {
                            this.show_tier_guide_collapsed = !this.show_tier_guide_collapsed;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(theme.text_muted)
                                    .child(collapse_icon),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text_secondary)
                                    .child("Auto Routing Tiers"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(summary_color)
                            .child(summary_text),
                    ),
            )
            .when(show_guide, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .pt(theme.space_2)
                        .border_t_1()
                        .border_color(theme.border)
                        .mt(theme.space_2)
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .pb(theme.space_1)
                                .child(
                                    "Hive auto-selects the best model for each task based on \
                                     complexity. Add at least one model per tier for full coverage.",
                                ),
                        )
                        .children(rows),
                )
            })
    }

    fn render_model_list(
        &self,
        all_models: &[ModelInfo],
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let provider_order = [
            (ProviderType::Anthropic, "Anthropic"),
            (ProviderType::OpenAI, "OpenAI"),
            (ProviderType::Google, "Google"),
            (ProviderType::OpenRouter, "OpenRouter"),
            (ProviderType::Groq, "Groq"),
            (ProviderType::HuggingFace, "Hugging Face"),
            (ProviderType::LiteLLM, "LiteLLM"),
            (ProviderType::Ollama, "Ollama (Local)"),
            (ProviderType::LMStudio, "LM Studio (Local)"),
            (ProviderType::GenericLocal, "Local AI"),
        ];

        let query = &self.search_query;
        let mut groups: Vec<AnyElement> = Vec::new();

        for (ptype, label) in &provider_order {
            // Provider chip filter
            if let Some(filter) = self.active_provider_filter {
                if filter != *ptype {
                    continue;
                }
            }

            let models: Vec<&ModelInfo> = all_models
                .iter()
                .filter(|m| m.provider_type == *ptype)
                .filter(|m| Self::matches_search(m, query))
                .filter(|m| {
                    if self.view_mode == ViewMode::Project {
                        self.is_in_project(&m.id) || Self::is_local_provider(m.provider_type)
                    } else {
                        true
                    }
                })
                .collect();

            if models.is_empty() {
                continue;
            }

            groups.push(
                self.render_group(*ptype, label, &models, theme, cx)
                    .into_any_element(),
            );
        }

        // Loading / error indicators
        let loading_statuses = [
            (self.or_fetch_status, "OpenRouter", ProviderType::OpenRouter),
            (self.openai_fetch_status, "OpenAI", ProviderType::OpenAI),
            (
                self.anthropic_fetch_status,
                "Anthropic",
                ProviderType::Anthropic,
            ),
            (self.google_fetch_status, "Google", ProviderType::Google),
            (self.groq_fetch_status, "Groq", ProviderType::Groq),
            (
                self.hf_fetch_status,
                "Hugging Face",
                ProviderType::HuggingFace,
            ),
        ];
        for (status, name, ptype) in &loading_statuses {
            match status {
                FetchStatus::Loading => {
                    groups.push(
                        div()
                            .px(theme.space_4)
                            .py(theme.space_2)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(format!("Loading {name} catalog\u{2026}"))
                            .into_any_element(),
                    );
                }
                FetchStatus::Failed => {
                    let retry_ptype = *ptype;
                    groups.push(
                        div()
                            .id(ElementId::Name(format!("retry-{name}").into()))
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .px(theme.space_4)
                            .py(theme.space_2)
                            .text_size(theme.font_size_xs)
                            .child(
                                div()
                                    .text_color(theme.accent_red)
                                    .child(format!("Failed to load {name}")),
                            )
                            .child(
                                div()
                                    .text_color(theme.accent_cyan)
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(theme.accent_aqua))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _e, _w, cx| {
                                            this.retry_fetch(retry_ptype, cx);
                                        }),
                                    )
                                    .child("Retry"),
                            )
                            .into_any_element(),
                    );
                }
                _ => {}
            }
        }

        if groups.is_empty() {
            let has_any_provider = !self.enabled_providers.is_empty();
            let any_loading = [
                self.or_fetch_status,
                self.openai_fetch_status,
                self.anthropic_fetch_status,
                self.google_fetch_status,
                self.groq_fetch_status,
                self.hf_fetch_status,
            ]
            .iter()
            .any(|s| *s == FetchStatus::Loading);

            let message = if self.view_mode == ViewMode::Project {
                "No project models yet. Switch to Browse All to add models.".to_string()
            } else if !has_any_provider {
                "No API keys configured. Go to Settings to add provider keys \
                 (OpenAI, Anthropic, Google, etc.) and models will appear here."
                    .to_string()
            } else if any_loading {
                "Loading model catalogs\u{2026}".to_string()
            } else if !self.search_query.is_empty() {
                "No models match your search.".to_string()
            } else {
                "No models found. Try adding more API keys in Settings.".to_string()
            };

            groups.push(
                div()
                    .px(theme.space_4)
                    .py(theme.space_4)
                    .flex()
                    .flex_col()
                    .gap(theme.space_2)
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_muted)
                            .child(message),
                    )
                    .when(!has_any_provider && self.view_mode == ViewMode::Browse, |el| {
                        el.child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(
                                    "Supported providers: Anthropic, OpenAI, Google, \
                                     OpenRouter, Groq, Hugging Face, Ollama, LM Studio",
                                ),
                        )
                    })
                    .into_any_element(),
            );
        }

        div()
            .id("models-browser-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .py(theme.space_1)
            .children(groups)
    }

    fn render_group(
        &self,
        provider: ProviderType,
        label: &str,
        models: &[&ModelInfo],
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_collapsed = self.collapsed_providers.contains(&provider);
        let is_expanded = self.expanded_providers.contains(&provider);
        let total_count = models.len();
        let is_searching = !self.search_query.is_empty();

        let visible_models: Vec<&&ModelInfo> = if is_collapsed {
            vec![]
        } else if is_expanded || is_searching || total_count <= MAX_VISIBLE_PER_GROUP {
            models.iter().collect()
        } else {
            models.iter().take(MAX_VISIBLE_PER_GROUP).collect()
        };

        let has_more =
            !is_collapsed && !is_expanded && !is_searching && total_count > MAX_VISIBLE_PER_GROUP;
        let remaining = total_count.saturating_sub(MAX_VISIBLE_PER_GROUP);

        let mut entries: Vec<AnyElement> = Vec::new();
        for model in &visible_models {
            entries.push(self.render_entry(model, theme, cx).into_any_element());
        }

        if has_more {
            let ptype = provider;
            entries.push(
                div()
                    .id(ElementId::Name(format!("show-more-{label}").into()))
                    .px(theme.space_4)
                    .py(theme.space_1)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.accent_cyan)
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.accent_aqua))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e, _w, cx| {
                            if this.expanded_providers.contains(&ptype) {
                                this.expanded_providers.remove(&ptype);
                            } else {
                                this.expanded_providers.insert(ptype);
                            }
                            cx.notify();
                        }),
                    )
                    .child(format!("Show {remaining} more\u{2026}"))
                    .into_any_element(),
            );
        }

        if is_expanded && !is_searching && total_count > MAX_VISIBLE_PER_GROUP {
            let ptype = provider;
            entries.push(
                div()
                    .id(ElementId::Name(format!("show-less-{label}").into()))
                    .px(theme.space_4)
                    .py(theme.space_1)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.accent_cyan)
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.accent_aqua))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e, _w, cx| {
                            this.expanded_providers.remove(&ptype);
                            cx.notify();
                        }),
                    )
                    .child("Show less")
                    .into_any_element(),
            );
        }

        let collapse_icon = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };
        let header_ptype = provider;

        // Count how many are in project
        let in_project = models.iter().filter(|m| self.is_in_project(&m.id)).count();
        let project_badge = if in_project > 0 {
            format!("{in_project}/{total_count} in project")
        } else {
            format!("{total_count}")
        };

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(
                div()
                    .id(ElementId::Name(format!("group-hdr-{label}").into()))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(theme.space_4)
                    .py(theme.space_2)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_tertiary))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e, _w, cx| {
                            if this.collapsed_providers.contains(&header_ptype) {
                                this.collapsed_providers.remove(&header_ptype);
                            } else {
                                this.collapsed_providers.insert(header_ptype);
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(theme.text_muted)
                                    .child(collapse_icon),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(theme.text_primary)
                                    .font_weight(FontWeight::BOLD)
                                    .child(label.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .px(theme.space_2)
                            .py(px(1.0))
                            .rounded(theme.radius_full)
                            .bg(theme.bg_primary)
                            .child(project_badge),
                    ),
            )
            .children(entries)
    }

    fn render_entry(
        &self,
        model: &ModelInfo,
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let in_project = self.is_in_project(&model.id);
        let is_local = Self::is_local_provider(model.provider_type);
        let model_id = model.id.clone();

        let (tier_color, tier_label) = tier_style(model.tier, theme);
        let mut tier_bg = tier_color;
        tier_bg.a = 0.15;

        let ctx_k = model.context_window / 1000;
        let price = format!(
            "{}K ctx \u{00B7} ${:.2}/M in \u{00B7} ${:.2}/M out",
            ctx_k, model.input_price_per_mtok, model.output_price_per_mtok
        );

        let toggle_label = if in_project { "\u{2713}" } else { "+" };
        let toggle_color = if in_project {
            theme.accent_green
        } else {
            theme.text_muted
        };
        let mut toggle_bg = toggle_color;
        toggle_bg.a = 0.15;

        div()
            .id(ElementId::Name(format!("model-{}", model.id).into()))
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px(theme.space_4)
            .py(theme.space_2)
            .hover(|s| s.bg(theme.bg_tertiary))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(theme.text_primary)
                                    .child(model.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(tier_color)
                                    .bg(tier_bg)
                                    .px(theme.space_2)
                                    .py(px(2.0))
                                    .rounded(theme.radius_full)
                                    .child(tier_label),
                            )
                            .when(is_local, |el| {
                                el.child(
                                    div()
                                        .text_size(theme.font_size_xs)
                                        .text_color(theme.accent_aqua)
                                        .bg({
                                            let mut c = theme.accent_aqua;
                                            c.a = 0.15;
                                            c
                                        })
                                        .px(theme.space_2)
                                        .py(px(2.0))
                                        .rounded(theme.radius_full)
                                        .child("Local"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(price),
                    ),
            )
            // Toggle button
            .child(
                div()
                    .id(ElementId::Name(format!("toggle-{}", model.id).into()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(28.0))
                    .h(px(28.0))
                    .rounded(theme.radius_md)
                    .bg(toggle_bg)
                    .text_color(toggle_color)
                    .text_size(theme.font_size_sm)
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .hover(|s| {
                        if in_project {
                            s.bg(theme.accent_red).text_color(theme.text_primary)
                        } else {
                            s.bg(theme.accent_green).text_color(theme.text_primary)
                        }
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e, _w, cx| {
                            this.toggle_project_model(model_id.clone(), cx);
                        }),
                    )
                    .child(toggle_label),
            )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tier_style(tier: ModelTier, theme: &HiveTheme) -> (Hsla, &'static str) {
    match tier {
        ModelTier::Free => (theme.text_muted, "Free"),
        ModelTier::Budget => (theme.accent_green, "Budget"),
        ModelTier::Mid => (theme.accent_cyan, "Mid"),
        ModelTier::Premium => (theme.accent_pink, "Premium"),
    }
}
