use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use hive_ai::model_registry::MODEL_REGISTRY;
use hive_ai::types::{ModelInfo, ModelTier, ProviderType};

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when the user picks a model from the dropdown.
#[derive(Debug, Clone)]
pub struct ModelSelected(pub String);

// ---------------------------------------------------------------------------
// Fetch status for OpenRouter catalog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStatus {
    Idle,
    Loading,
    Done,
    Failed,
}

// ---------------------------------------------------------------------------
// ModelSelectorView
// ---------------------------------------------------------------------------

/// Interactive dropdown that lists all models from the static registry,
/// grouped by provider. Emits `ModelSelected` on pick.
///
/// Supports API-key gating (grayed-out providers), search filtering,
/// and live OpenRouter catalog fetching.
pub struct ModelSelectorView {
    current_model: String,
    is_open: bool,
    theme: HiveTheme,

    // API key gating
    enabled_providers: HashSet<ProviderType>,

    // Search / filter
    search_query: String,
    search_input: Entity<InputState>,

    // OpenRouter live catalog
    fetched_or_models: Vec<ModelInfo>,
    or_fetch_status: FetchStatus,
    openrouter_api_key: Option<String>,
}

impl EventEmitter<ModelSelected> for ModelSelectorView {}

impl ModelSelectorView {
    pub fn new(current_model: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Search models\u{2026}", window, cx);
            state
        });

        cx.subscribe_in(&search_input, window, |this: &mut Self, _state, event, _window, cx| {
            if let InputEvent::Change = event {
                this.search_query = _state.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();

        Self {
            current_model,
            is_open: false,
            theme: HiveTheme::dark(),
            enabled_providers: HashSet::new(),
            search_query: String::new(),
            search_input,
            fetched_or_models: Vec::new(),
            or_fetch_status: FetchStatus::Idle,
            openrouter_api_key: None,
        }
    }

    pub fn current_model(&self) -> &str {
        &self.current_model
    }

    /// Update which providers have valid API keys configured.
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

    /// Store the OpenRouter API key; invalidate cached catalog on change.
    pub fn set_openrouter_api_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        let changed = self.openrouter_api_key != key;
        self.openrouter_api_key = key;
        if changed {
            hive_ai::providers::openrouter_catalog::invalidate_cache();
            self.fetched_or_models.clear();
            self.or_fetch_status = FetchStatus::Idle;
            cx.notify();
        }
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_open = !self.is_open;
        if !self.is_open {
            // Clear search on close
            self.search_query.clear();
            self.search_input.update(cx, |state, cx| {
                state.set_value(String::new(), window, cx);
            });
        } else {
            // Trigger OpenRouter catalog fetch if needed
            self.maybe_fetch_openrouter(cx);
        }
        cx.notify();
    }

    fn select(&mut self, model_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.current_model = model_id.clone();
        self.is_open = false;
        self.search_query.clear();
        self.search_input.update(cx, |state, cx| {
            state.set_value(String::new(), window, cx);
        });
        cx.emit(ModelSelected(model_id));
        cx.notify();
    }

    fn maybe_fetch_openrouter(&mut self, cx: &mut Context<Self>) {
        if !self.enabled_providers.contains(&ProviderType::OpenRouter) {
            return;
        }
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

        cx.spawn(async move |this, app: &mut AsyncApp| {
            let result =
                hive_ai::providers::openrouter_catalog::fetch_openrouter_models(&api_key).await;

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

    fn is_provider_enabled(&self, provider: ProviderType) -> bool {
        self.enabled_providers.contains(&provider)
    }

    /// Collect all models: static registry + fetched OpenRouter catalog (deduplicated).
    fn all_models(&self) -> Vec<ModelInfo> {
        let mut models: Vec<ModelInfo> = MODEL_REGISTRY.iter().cloned().collect();

        // Merge fetched OpenRouter models, deduplicating by id
        let existing_ids: HashSet<String> = models.iter().map(|m| m.id.clone()).collect();
        for m in &self.fetched_or_models {
            if !existing_ids.contains(&m.id) {
                models.push(m.clone());
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
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for ModelSelectorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let is_open = self.is_open;

        let all_models = self.all_models();
        let info = all_models.iter().find(|m| m.id == self.current_model);
        let display = info.map(|m| m.name.as_str()).unwrap_or(
            if self.current_model.is_empty() {
                "Select a model"
            } else {
                self.current_model.as_str()
            },
        );
        let tier = info.map(|m| m.tier);

        let (tier_color, tier_label) = tier_style(tier, theme);
        let mut tier_bg = tier_color;
        tier_bg.a = 0.15;

        let arrow = if is_open { "\u{25B2}" } else { "\u{25BC}" };

        div()
            .id("model-selector")
            .w_full()
            .flex()
            .flex_col()
            // Trigger
            .child(
                div()
                    .id("model-selector-trigger")
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_md)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _e, w, cx| this.toggle(w, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(theme.text_primary)
                                    .child(display.to_string()),
                            )
                            .when(tier.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(theme.font_size_xs)
                                        .text_color(tier_color)
                                        .bg(tier_bg)
                                        .px(theme.space_2)
                                        .py(px(2.0))
                                        .rounded(theme.radius_full)
                                        .child(tier_label),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(arrow),
                    ),
            )
            // Dropdown (inline, pushes content down)
            .when(is_open, |el| {
                el.child(self.render_dropdown(theme, cx))
            })
    }
}

impl ModelSelectorView {
    fn render_dropdown(
        &self,
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let provider_order = [
            (ProviderType::Anthropic, "Anthropic"),
            (ProviderType::OpenAI, "OpenAI"),
            (ProviderType::OpenRouter, "OpenRouter"),
            (ProviderType::Groq, "Groq"),
            (ProviderType::HuggingFace, "Hugging Face"),
            (ProviderType::LiteLLM, "LiteLLM"),
        ];

        let all_models = self.all_models();
        let query = &self.search_query;

        let mut groups: Vec<AnyElement> = Vec::new();
        let mut total_visible = 0usize;

        for (ptype, label) in &provider_order {
            let enabled = self.is_provider_enabled(*ptype);
            let models: Vec<&ModelInfo> = all_models
                .iter()
                .filter(|m| m.provider_type == *ptype)
                .filter(|m| Self::matches_search(m, query))
                .collect();
            if models.is_empty() {
                continue;
            }
            total_visible += models.len();
            groups.push(
                self.render_group(label, &models, enabled, theme, cx)
                    .into_any_element(),
            );
        }

        // "OpenRouter Catalog" group for fetched models not in static registry
        if self.or_fetch_status == FetchStatus::Loading {
            groups.push(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("Loading OpenRouter catalog\u{2026}")
                    .into_any_element(),
            );
        }

        // Empty state
        if total_visible == 0 && self.or_fetch_status != FetchStatus::Loading {
            groups.push(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child("No models match your search")
                    .into_any_element(),
            );
        }

        div()
            .id("model-selector-dropdown")
            .mt(px(4.0))
            .w_full()
            .min_w(px(320.0))
            .max_h(px(400.0))
            .overflow_y_scroll()
            .rounded(theme.radius_md)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .py(theme.space_1)
            // Search input at top
            .child(
                div()
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .child(
                        Input::new(&self.search_input)
                            .appearance(true)
                            .cleanable(false),
                    ),
            )
            .children(groups)
    }

    fn render_group(
        &self,
        label: &str,
        models: &[&ModelInfo],
        enabled: bool,
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut entries: Vec<AnyElement> = Vec::new();
        for model in models {
            entries.push(
                self.render_entry(model, enabled, theme, cx)
                    .into_any_element(),
            );
        }

        let header_suffix = if !enabled { " \u{2014} No API key" } else { "" };

        div()
            .flex()
            .flex_col()
            .w_full()
            // Provider header
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_1)
                    .text_size(theme.font_size_xs)
                    .text_color(if enabled {
                        theme.text_muted
                    } else {
                        dimmed(theme.text_muted)
                    })
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{label}{header_suffix}")),
            )
            .children(entries)
    }

    fn render_entry(
        &self,
        model: &ModelInfo,
        enabled: bool,
        theme: &HiveTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_selected = model.id == self.current_model;
        let bg = if is_selected && enabled {
            theme.bg_tertiary
        } else {
            Hsla::transparent_black()
        };

        let (tier_color, tier_label) = tier_style(Some(model.tier), theme);
        let mut tier_bg = tier_color;
        tier_bg.a = 0.15;

        let model_id = model.id.clone();
        let price = format!(
            "${:.2}/M in \u{00B7} ${:.2}/M out",
            model.input_price_per_mtok, model.output_price_per_mtok
        );

        let text_color = if enabled {
            theme.text_primary
        } else {
            dimmed(theme.text_muted)
        };
        let sub_color = if enabled {
            theme.text_muted
        } else {
            dimmed(theme.text_muted)
        };

        let mut el = div()
            .id(ElementId::Name(model.id.clone().into()))
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px(theme.space_3)
            .py(theme.space_2)
            .bg(bg);

        if enabled {
            el = el
                .cursor_pointer()
                .hover(|s| s.bg(theme.bg_tertiary))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _e, w, cx| {
                        this.select(model_id.clone(), w, cx);
                    }),
                );
        }

        el.child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(text_color)
                                .child(model.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(if enabled { tier_color } else { dimmed(theme.text_muted) })
                                .bg(if enabled { tier_bg } else { Hsla::transparent_black() })
                                .px(theme.space_2)
                                .py(px(2.0))
                                .rounded(theme.radius_full)
                                .child(tier_label),
                        )
                        .when(!enabled, |el| {
                            el.child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.accent_red)
                                    .bg({
                                        let mut c = theme.accent_red;
                                        c.a = 0.15;
                                        c
                                    })
                                    .px(theme.space_2)
                                    .py(px(2.0))
                                    .rounded(theme.radius_full)
                                    .child("No API key"),
                            )
                        }),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(sub_color)
                        .child(price),
                ),
        )
        .when(is_selected && enabled, |el| {
            el.child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.accent_cyan)
                    .child("\u{2713}"),
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Reduce alpha for disabled/dimmed elements.
fn dimmed(mut color: Hsla) -> Hsla {
    color.a *= 0.4;
    color
}

fn tier_style(tier: Option<ModelTier>, theme: &HiveTheme) -> (Hsla, &'static str) {
    match tier {
        Some(ModelTier::Free) => (theme.text_muted, "Free"),
        Some(ModelTier::Budget) => (theme.accent_green, "Budget"),
        Some(ModelTier::Mid) => (theme.accent_cyan, "Mid"),
        Some(ModelTier::Premium) => (theme.accent_pink, "Premium"),
        None => (theme.text_muted, ""),
    }
}

/// Compact model badge for display in chat bubbles and headers.
/// Kept for backwards compatibility with `ChatPanel`.
pub fn render_model_badge(model: &str, tier: &str, theme: &HiveTheme) -> impl IntoElement {
    let parsed_tier = match tier.to_lowercase().as_str() {
        "budget" => Some(ModelTier::Budget),
        "mid" => Some(ModelTier::Mid),
        "premium" => Some(ModelTier::Premium),
        "free" => Some(ModelTier::Free),
        _ => None,
    };
    let (color, label) = tier_style(parsed_tier, theme);
    let mut bg = color;
    bg.a = 0.15;

    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_1)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius_md)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(model.to_string()),
        )
        .when(!label.is_empty(), |el| {
            el.child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(color)
                    .bg(bg)
                    .px(theme.space_2)
                    .py(px(2.0))
                    .rounded(theme.radius_full)
                    .child(label),
            )
        })
}
