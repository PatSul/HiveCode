use std::collections::HashSet;

use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::switch::Switch;
use gpui_component::{Icon, IconName};
use hive_ai::types::ProviderType;

use crate::components::model_selector::{ModelSelected, ModelSelectorView};
use crate::globals::AppConfig;
use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(
    hive_settings,
    [
        SettingsTogglePrivacy,
        SettingsToggleAutoRouting,
        SettingsToggleAutoUpdate,
        SettingsToggleNotifications,
        SettingsToggleTts,
        SettingsToggleTtsAutoSpeak,
        SettingsToggleClawdTalk,
    ]
);

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when any setting changes. The workspace subscribes to this and
/// persists the values to `AppConfig`.
#[derive(Debug, Clone)]
pub struct SettingsSaved;

// ---------------------------------------------------------------------------
// SettingsData -- read-only snapshot for other panels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SettingsData {
    pub has_anthropic_key: bool,
    pub has_openai_key: bool,
    pub has_openrouter_key: bool,
    pub has_google_key: bool,
    pub has_groq_key: bool,
    pub has_huggingface_key: bool,
    pub has_litellm_key: bool,
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub local_provider_url: Option<String>,
    pub privacy_mode: bool,
    pub default_model: String,
    pub auto_routing: bool,
    pub daily_budget_usd: f64,
    pub monthly_budget_usd: f64,
    pub theme: String,
    pub font_size: u32,
    pub auto_update: bool,
    pub notifications_enabled: bool,
    pub log_level: String,
    // TTS
    pub has_elevenlabs_key: bool,
    pub has_telnyx_key: bool,
    pub tts_enabled: bool,
    pub tts_auto_speak: bool,
    pub tts_provider: String,
    pub tts_speed: f32,
    pub clawdtalk_enabled: bool,
}

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            has_anthropic_key: false,
            has_openai_key: false,
            has_openrouter_key: false,
            has_google_key: false,
            has_groq_key: false,
            has_huggingface_key: false,
            has_litellm_key: false,
            ollama_url: "http://localhost:11434".into(),
            lmstudio_url: "http://localhost:1234".into(),
            local_provider_url: None,
            privacy_mode: false,
            default_model: String::new(),
            auto_routing: true,
            daily_budget_usd: 10.0,
            monthly_budget_usd: 100.0,
            theme: "dark".into(),
            font_size: 14,
            auto_update: true,
            notifications_enabled: true,
            log_level: "info".into(),
            has_elevenlabs_key: false,
            has_telnyx_key: false,
            tts_enabled: false,
            tts_auto_speak: false,
            tts_provider: "qwen3".into(),
            tts_speed: 1.0,
            clawdtalk_enabled: false,
        }
    }
}

impl Global for SettingsData {}

impl SettingsData {
    pub fn from_config(cfg: &hive_core::HiveConfig) -> Self {
        Self {
            has_anthropic_key: cfg.anthropic_api_key.as_ref().map_or(false, |k| !k.is_empty()),
            has_openai_key: cfg.openai_api_key.as_ref().map_or(false, |k| !k.is_empty()),
            has_openrouter_key: cfg
                .openrouter_api_key
                .as_ref()
                .map_or(false, |k| !k.is_empty()),
            has_google_key: cfg.google_api_key.as_ref().map_or(false, |k| !k.is_empty()),
            has_groq_key: cfg.groq_api_key.as_ref().map_or(false, |k| !k.is_empty()),
            has_huggingface_key: cfg
                .huggingface_api_key
                .as_ref()
                .map_or(false, |k| !k.is_empty()),
            has_litellm_key: cfg.litellm_api_key.as_ref().map_or(false, |k| !k.is_empty()),
            ollama_url: cfg.ollama_url.clone(),
            lmstudio_url: cfg.lmstudio_url.clone(),
            local_provider_url: cfg.local_provider_url.clone(),
            privacy_mode: cfg.privacy_mode,
            default_model: cfg.default_model.clone(),
            auto_routing: cfg.auto_routing,
            daily_budget_usd: cfg.daily_budget_usd,
            monthly_budget_usd: cfg.monthly_budget_usd,
            theme: cfg.theme.clone(),
            font_size: cfg.font_size,
            auto_update: cfg.auto_update,
            notifications_enabled: cfg.notifications_enabled,
            log_level: cfg.log_level.clone(),
            has_elevenlabs_key: cfg.elevenlabs_api_key.as_ref().map_or(false, |k| !k.is_empty()),
            has_telnyx_key: cfg.telnyx_api_key.as_ref().map_or(false, |k| !k.is_empty()),
            tts_enabled: cfg.tts_enabled,
            tts_auto_speak: cfg.tts_auto_speak,
            tts_provider: cfg.tts_provider.clone(),
            tts_speed: cfg.tts_speed,
            clawdtalk_enabled: cfg.clawdtalk_enabled,
        }
    }

    pub fn configured_key_count(&self) -> usize {
        [
            self.has_anthropic_key,
            self.has_openai_key,
            self.has_openrouter_key,
            self.has_google_key,
            self.has_groq_key,
            self.has_huggingface_key,
        ]
        .iter()
        .filter(|&&v| v)
        .count()
    }

    pub fn has_any_cloud_key(&self) -> bool {
        self.configured_key_count() > 0
    }
}

impl From<&hive_core::HiveConfig> for SettingsData {
    fn from(cfg: &hive_core::HiveConfig) -> Self {
        Self::from_config(cfg)
    }
}

// ---------------------------------------------------------------------------
// SettingsView -- interactive entity
// ---------------------------------------------------------------------------

/// Interactive settings panel backed by real GPUI input widgets.
/// Auto-saves on every blur (focus-out) from text inputs and on every toggle.
pub struct SettingsView {
    theme: HiveTheme,

    // API key inputs (masked)
    anthropic_key_input: Entity<InputState>,
    openai_key_input: Entity<InputState>,
    openrouter_key_input: Entity<InputState>,
    google_key_input: Entity<InputState>,
    groq_key_input: Entity<InputState>,
    huggingface_key_input: Entity<InputState>,

    // LiteLLM inputs
    litellm_key_input: Entity<InputState>,
    litellm_url_input: Entity<InputState>,

    // URL inputs
    ollama_url_input: Entity<InputState>,
    lmstudio_url_input: Entity<InputState>,
    custom_url_input: Entity<InputState>,

    // Model selector
    model_selector: Entity<ModelSelectorView>,

    // Budget inputs
    daily_budget_input: Entity<InputState>,
    monthly_budget_input: Entity<InputState>,

    // Toggle states
    privacy_mode: bool,
    auto_routing: bool,
    auto_update: bool,
    notifications_enabled: bool,

    // TTS key inputs
    elevenlabs_key_input: Entity<InputState>,
    telnyx_key_input: Entity<InputState>,

    // TTS toggles
    tts_enabled: bool,
    tts_auto_speak: bool,
    clawdtalk_enabled: bool,

    // Track whether keys existed before editing (to preserve on empty save)
    had_anthropic_key: bool,
    had_openai_key: bool,
    had_openrouter_key: bool,
    had_google_key: bool,
    had_groq_key: bool,
    had_huggingface_key: bool,
    had_litellm_key: bool,
    had_elevenlabs_key: bool,
    had_telnyx_key: bool,

    // Discovery status
    discovered_model_count: usize,
}

impl EventEmitter<SettingsSaved> for SettingsView {}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Read current config
        let cfg = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get()
        } else {
            hive_core::HiveConfig::default()
        };

        let had_anthropic = cfg.anthropic_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_openai = cfg.openai_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_openrouter = cfg.openrouter_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_google = cfg.google_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_groq = cfg.groq_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_huggingface = cfg.huggingface_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_litellm = cfg.litellm_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_elevenlabs = cfg.elevenlabs_api_key.as_ref().map_or(false, |k| !k.is_empty());
        let had_telnyx = cfg.telnyx_api_key.as_ref().map_or(false, |k| !k.is_empty());

        // API key inputs — always start empty, placeholder indicates status
        let anthropic_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_anthropic), window, cx);
            state
        });
        let openai_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_openai), window, cx);
            state
        });
        let openrouter_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_openrouter), window, cx);
            state
        });
        let google_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_google), window, cx);
            state
        });

        // Groq + HuggingFace key inputs
        let groq_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_groq), window, cx);
            state
        });
        let huggingface_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_huggingface), window, cx);
            state
        });

        // LiteLLM inputs
        let litellm_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_litellm), window, cx);
            state
        });
        let litellm_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("http://localhost:4000", window, cx);
            if let Some(ref url) = cfg.litellm_url {
                state.set_value(url.clone(), window, cx);
            }
            state
        });

        // TTS key inputs
        let elevenlabs_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_elevenlabs), window, cx);
            state
        });
        let telnyx_key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(key_placeholder(had_telnyx), window, cx);
            state
        });

        // URL inputs — pre-filled with current values
        let ollama_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("http://localhost:11434", window, cx);
            state.set_value(cfg.ollama_url.clone(), window, cx);
            state
        });
        let lmstudio_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("http://localhost:1234", window, cx);
            state.set_value(cfg.lmstudio_url.clone(), window, cx);
            state
        });
        let custom_url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Custom provider URL (optional)", window, cx);
            if let Some(ref url) = cfg.local_provider_url {
                state.set_value(url.clone(), window, cx);
            }
            state
        });

        // Model selector dropdown
        let model_selector = cx.new(|cx| {
            ModelSelectorView::new(cfg.default_model.clone(), window, cx)
        });

        // Budget inputs
        let daily_budget_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("0.00", window, cx);
            state.set_value(format!("{:.2}", cfg.daily_budget_usd), window, cx);
            state
        });
        let monthly_budget_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("0.00", window, cx);
            state.set_value(format!("{:.2}", cfg.monthly_budget_usd), window, cx);
            state
        });

        // Subscribe to blur events on all text inputs → auto-save
        let all_inputs = [
            &anthropic_key_input,
            &openai_key_input,
            &openrouter_key_input,
            &google_key_input,
            &groq_key_input,
            &huggingface_key_input,
            &litellm_key_input,
            &litellm_url_input,
            &elevenlabs_key_input,
            &telnyx_key_input,
            &ollama_url_input,
            &lmstudio_url_input,
            &custom_url_input,
            &daily_budget_input,
            &monthly_budget_input,
        ];
        for input in all_inputs {
            cx.subscribe_in(input, window, Self::on_input_event).detach();
        }

        // Subscribe to model selector → auto-save on pick
        cx.subscribe_in(&model_selector, window, Self::on_model_selected).detach();

        let view = Self {
            theme: HiveTheme::dark(),
            anthropic_key_input,
            openai_key_input,
            openrouter_key_input,
            google_key_input,
            groq_key_input,
            huggingface_key_input,
            litellm_key_input,
            litellm_url_input,
            ollama_url_input,
            lmstudio_url_input,
            custom_url_input,
            model_selector,
            daily_budget_input,
            monthly_budget_input,
            privacy_mode: cfg.privacy_mode,
            auto_routing: cfg.auto_routing,
            auto_update: cfg.auto_update,
            notifications_enabled: cfg.notifications_enabled,
            elevenlabs_key_input,
            telnyx_key_input,
            tts_enabled: cfg.tts_enabled,
            tts_auto_speak: cfg.tts_auto_speak,
            clawdtalk_enabled: cfg.clawdtalk_enabled,
            had_anthropic_key: had_anthropic,
            had_openai_key: had_openai,
            had_openrouter_key: had_openrouter,
            had_google_key: had_google,
            had_groq_key: had_groq,
            had_huggingface_key: had_huggingface,
            had_litellm_key: had_litellm,
            had_elevenlabs_key: had_elevenlabs,
            had_telnyx_key: had_telnyx,
            discovered_model_count: 0,
        };

        // Initialize model selector with current provider availability
        view.sync_enabled_providers(cx);

        view
    }

    /// Called for every InputEvent from any subscribed input.
    /// Auto-saves on blur (when focus leaves the field).
    fn on_input_event(
        &mut self,
        _state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Blur => {
                self.sync_enabled_providers(cx);
                cx.emit(SettingsSaved);
            }
            InputEvent::Change => {
                self.sync_enabled_providers(cx);
                cx.notify();
            }
            _ => {}
        }
    }

    /// Called when the user picks a model from the dropdown.
    fn on_model_selected(
        &mut self,
        _view: &Entity<ModelSelectorView>,
        _event: &ModelSelected,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(SettingsSaved);
    }

    /// Collect all current field values for persistence.
    pub fn collect_values(&self, cx: &App) -> SettingsSnapshot {
        let anthropic_val = self.anthropic_key_input.read(cx).value().to_string();
        let openai_val = self.openai_key_input.read(cx).value().to_string();
        let openrouter_val = self.openrouter_key_input.read(cx).value().to_string();
        let google_val = self.google_key_input.read(cx).value().to_string();
        let groq_val = self.groq_key_input.read(cx).value().to_string();
        let huggingface_val = self.huggingface_key_input.read(cx).value().to_string();
        let litellm_val = self.litellm_key_input.read(cx).value().to_string();

        let elevenlabs_val = self.elevenlabs_key_input.read(cx).value().to_string();
        let telnyx_val = self.telnyx_key_input.read(cx).value().to_string();

        SettingsSnapshot {
            // Only update keys where input is non-empty
            anthropic_key: non_empty_trimmed(&anthropic_val),
            openai_key: non_empty_trimmed(&openai_val),
            openrouter_key: non_empty_trimmed(&openrouter_val),
            google_key: non_empty_trimmed(&google_val),
            groq_key: non_empty_trimmed(&groq_val),
            huggingface_key: non_empty_trimmed(&huggingface_val),
            litellm_key: non_empty_trimmed(&litellm_val),
            elevenlabs_key: non_empty_trimmed(&elevenlabs_val),
            telnyx_key: non_empty_trimmed(&telnyx_val),

            ollama_url: self.ollama_url_input.read(cx).value().to_string(),
            lmstudio_url: self.lmstudio_url_input.read(cx).value().to_string(),
            litellm_url: {
                let v = self.litellm_url_input.read(cx).value().to_string();
                non_empty_trimmed(&v)
            },
            custom_url: {
                let v = self.custom_url_input.read(cx).value().to_string();
                non_empty_trimmed(&v)
            },

            default_model: self.model_selector.read(cx).current_model().to_string(),

            daily_budget: self
                .daily_budget_input
                .read(cx)
                .value()
                .parse::<f64>()
                .unwrap_or(0.0),
            monthly_budget: self
                .monthly_budget_input
                .read(cx)
                .value()
                .parse::<f64>()
                .unwrap_or(0.0),

            privacy_mode: self.privacy_mode,
            auto_routing: self.auto_routing,
            auto_update: self.auto_update,
            notifications_enabled: self.notifications_enabled,
            tts_enabled: self.tts_enabled,
            tts_auto_speak: self.tts_auto_speak,
            clawdtalk_enabled: self.clawdtalk_enabled,
        }
    }

    /// Whether a given API key is configured (either pre-existing or newly entered).
    fn key_is_set(&self, had_key: bool, input: &Entity<InputState>, cx: &Context<Self>) -> bool {
        had_key || !input.read(cx).value().is_empty()
    }

    /// Sync the model selector's enabled-provider set and API keys
    /// based on current input field values.
    fn sync_enabled_providers(&self, cx: &mut Context<Self>) {
        let anthropic_set =
            self.key_is_set(self.had_anthropic_key, &self.anthropic_key_input, cx);
        let openai_set = self.key_is_set(self.had_openai_key, &self.openai_key_input, cx);
        let openrouter_set =
            self.key_is_set(self.had_openrouter_key, &self.openrouter_key_input, cx);
        let google_set = self.key_is_set(self.had_google_key, &self.google_key_input, cx);
        let groq_set = self.key_is_set(self.had_groq_key, &self.groq_key_input, cx);
        let huggingface_set =
            self.key_is_set(self.had_huggingface_key, &self.huggingface_key_input, cx);

        let mut providers = HashSet::new();
        if anthropic_set {
            providers.insert(ProviderType::Anthropic);
        }
        if openai_set {
            providers.insert(ProviderType::OpenAI);
        }
        if openrouter_set {
            providers.insert(ProviderType::OpenRouter);
        }
        if google_set {
            providers.insert(ProviderType::Google);
        }
        if groq_set {
            providers.insert(ProviderType::Groq);
        }
        if huggingface_set {
            providers.insert(ProviderType::HuggingFace);
        }

        // Helper: resolve an API key from input field or saved config
        let resolve_key = |input: &Entity<InputState>, had_key: bool, cx: &Context<Self>,
            config_field: fn(&hive_core::HiveConfig) -> &Option<String>| -> Option<String> {
            let val = input.read(cx).value().to_string();
            if !val.trim().is_empty() {
                Some(val.trim().to_string())
            } else if had_key {
                if cx.has_global::<AppConfig>() {
                    config_field(&cx.global::<AppConfig>().0.get()).clone()
                } else {
                    None
                }
            } else {
                None
            }
        };

        let or_key = resolve_key(
            &self.openrouter_key_input, self.had_openrouter_key, cx,
            |cfg| &cfg.openrouter_api_key,
        );
        let openai_key = resolve_key(
            &self.openai_key_input, self.had_openai_key, cx,
            |cfg| &cfg.openai_api_key,
        );
        let anthropic_key = resolve_key(
            &self.anthropic_key_input, self.had_anthropic_key, cx,
            |cfg| &cfg.anthropic_api_key,
        );
        let google_key = resolve_key(
            &self.google_key_input, self.had_google_key, cx,
            |cfg| &cfg.google_api_key,
        );

        self.model_selector.update(cx, |selector, cx| {
            selector.set_enabled_providers(providers, cx);
            selector.set_openrouter_api_key(or_key, cx);
            selector.set_openai_api_key(openai_key, cx);
            selector.set_anthropic_api_key(anthropic_key, cx);
            selector.set_google_api_key(google_key, cx);
        });
    }

    /// Feed discovered local models into the model selector.
    pub fn refresh_local_models(
        &mut self,
        models: Vec<hive_ai::types::ModelInfo>,
        cx: &mut Context<Self>,
    ) {
        self.discovered_model_count = models.len();
        self.model_selector.update(cx, |selector, cx| {
            selector.set_local_models(models, cx);
        });
        cx.notify();
    }
}

/// Snapshot of settings values collected from the view.
pub struct SettingsSnapshot {
    pub anthropic_key: Option<String>,
    pub openai_key: Option<String>,
    pub openrouter_key: Option<String>,
    pub google_key: Option<String>,
    pub groq_key: Option<String>,
    pub huggingface_key: Option<String>,
    pub litellm_key: Option<String>,
    pub elevenlabs_key: Option<String>,
    pub telnyx_key: Option<String>,
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub litellm_url: Option<String>,
    pub custom_url: Option<String>,
    pub default_model: String,
    pub daily_budget: f64,
    pub monthly_budget: f64,
    pub privacy_mode: bool,
    pub auto_routing: bool,
    pub auto_update: bool,
    pub notifications_enabled: bool,
    pub tts_enabled: bool,
    pub tts_auto_speak: bool,
    pub clawdtalk_enabled: bool,
}

fn key_placeholder(has_key: bool) -> &'static str {
    if has_key {
        "Key configured (enter new to replace)"
    } else {
        "sk-... or enter API key"
    }
}

fn non_empty_trimmed(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;

        // Count configured keys (existing + newly entered)
        let anthropic_set = self.key_is_set(self.had_anthropic_key, &self.anthropic_key_input, cx);
        let openai_set = self.key_is_set(self.had_openai_key, &self.openai_key_input, cx);
        let openrouter_set = self.key_is_set(self.had_openrouter_key, &self.openrouter_key_input, cx);
        let google_set = self.key_is_set(self.had_google_key, &self.google_key_input, cx);
        let groq_set = self.key_is_set(self.had_groq_key, &self.groq_key_input, cx);
        let huggingface_set = self.key_is_set(self.had_huggingface_key, &self.huggingface_key_input, cx);
        let key_count = [anthropic_set, openai_set, openrouter_set, google_set, groq_set, huggingface_set]
            .iter()
            .filter(|&&v| v)
            .count();

        div()
            .id("settings-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .p(theme.space_4)
            .gap(theme.space_4)
            .overflow_y_scroll()
            .on_action(cx.listener(|this: &mut Self, _: &SettingsTogglePrivacy, _, cx| {
                this.privacy_mode = !this.privacy_mode;
                cx.emit(SettingsSaved);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &SettingsToggleAutoRouting, _, cx| {
                this.auto_routing = !this.auto_routing;
                cx.emit(SettingsSaved);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &SettingsToggleAutoUpdate, _, cx| {
                this.auto_update = !this.auto_update;
                cx.emit(SettingsSaved);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &SettingsToggleNotifications, _, cx| {
                this.notifications_enabled = !this.notifications_enabled;
                cx.emit(SettingsSaved);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &SettingsToggleTts, _, cx| {
                this.tts_enabled = !this.tts_enabled;
                cx.emit(SettingsSaved);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &SettingsToggleTtsAutoSpeak, _, cx| {
                this.tts_auto_speak = !this.tts_auto_speak;
                cx.emit(SettingsSaved);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &SettingsToggleClawdTalk, _, cx| {
                this.clawdtalk_enabled = !this.clawdtalk_enabled;
                cx.emit(SettingsSaved);
                cx.notify();
            }))
            // Header
            .child(render_header(key_count, theme))
            // API Keys
            .child(render_api_keys_section(
                key_count,
                anthropic_set, &self.anthropic_key_input,
                openai_set, &self.openai_key_input,
                openrouter_set, &self.openrouter_key_input,
                google_set, &self.google_key_input,
                groq_set, &self.groq_key_input,
                huggingface_set, &self.huggingface_key_input,
                theme,
            ))
            // Local AI
            .child(self.render_local_ai_section(cx))
            // Model Routing
            .child(self.render_model_routing_section(cx))
            // Budget
            .child(self.render_budget_section(cx))
            // Voice & TTS
            .child(self.render_voice_tts_section(cx))
            // General
            .child(self.render_general_section(cx))
    }
}

impl SettingsView {
    fn render_local_ai_section(&self, cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let litellm_set = self.key_is_set(self.had_litellm_key, &self.litellm_key_input, cx);

        let discovery_text = if self.discovered_model_count > 0 {
            format!("{} local model{} discovered", self.discovered_model_count, if self.discovered_model_count == 1 { "" } else { "s" })
        } else {
            "No local models found".to_string()
        };

        card(theme)
            .child(section_title("\u{1F4BB}", "Local AI", theme))
            .child(section_desc(
                "Connect to locally-running models for free, private inference.",
                theme,
            ))
            .child(separator(theme))
            .child(input_row("Ollama URL", &self.ollama_url_input, theme))
            .child(input_row("LM Studio URL", &self.lmstudio_url_input, theme))
            .child(input_row("Custom Local URL", &self.custom_url_input, theme))
            .child(separator(theme))
            .child(input_row("LiteLLM Proxy URL", &self.litellm_url_input, theme))
            .child(api_key_row("LiteLLM API Key", litellm_set, &self.litellm_key_input, theme))
            .child(separator(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(theme.radius_full)
                            .bg(if self.discovered_model_count > 0 {
                                theme.accent_green
                            } else {
                                theme.text_muted
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(discovery_text),
                    ),
            )
            .child(separator(theme))
            .child(switch_row(
                "Privacy Mode",
                "privacy-switch",
                self.privacy_mode,
                SettingsTogglePrivacy,
                theme,
            ))
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(if self.privacy_mode {
                        "Privacy mode ON -- requests are routed to local providers only. No data leaves your machine."
                    } else {
                        "Privacy mode OFF -- requests may be sent to cloud providers when local models are unavailable."
                    }),
            )
            .into_any_element()
    }

    fn render_model_routing_section(&self, _cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;

        card(theme)
            .child(section_title("\u{1F500}", "Model Routing", theme))
            .child(section_desc(
                "Control which model handles your requests.",
                theme,
            ))
            .child(separator(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(theme.space_4)
                    .py(theme.space_1)
                    .child(
                        div()
                            .text_size(theme.font_size_base)
                            .text_color(theme.text_secondary)
                            .child("Default Model"),
                    )
                    .child(
                        div().min_w(px(280.0)).child(self.model_selector.clone()),
                    ),
            )
            .child(switch_row(
                "Auto Routing",
                "auto-routing-switch",
                self.auto_routing,
                SettingsToggleAutoRouting,
                theme,
            ))
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(if self.auto_routing {
                        "Requests are automatically routed to the best model based on task complexity."
                    } else {
                        "All requests will use the default model above."
                    }),
            )
            .into_any_element()
    }

    fn render_budget_section(&self, _cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;

        card(theme)
            .child(section_title("\u{1F4B0}", "Budget", theme))
            .child(section_desc(
                "Set spending limits to prevent unexpected charges.",
                theme,
            ))
            .child(separator(theme))
            .child(budget_row("Daily Budget", &self.daily_budget_input, theme))
            .child(budget_row("Monthly Budget", &self.monthly_budget_input, theme))
            .into_any_element()
    }

    fn render_voice_tts_section(&self, cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let elevenlabs_set = self.key_is_set(self.had_elevenlabs_key, &self.elevenlabs_key_input, cx);
        let telnyx_set = self.key_is_set(self.had_telnyx_key, &self.telnyx_key_input, cx);

        card(theme)
            .child(section_title("\u{1F50A}", "Voice & TTS", theme))
            .child(section_desc(
                "Text-to-speech synthesis. Local providers (Qwen3, F5) work offline; cloud providers require API keys.",
                theme,
            ))
            .child(separator(theme))
            .child(switch_row(
                "Enable TTS",
                "tts-enable-switch",
                self.tts_enabled,
                SettingsToggleTts,
                theme,
            ))
            .child(switch_row(
                "Auto-Speak Responses",
                "tts-auto-speak-switch",
                self.tts_auto_speak,
                SettingsToggleTtsAutoSpeak,
                theme,
            ))
            .child(separator(theme))
            .child(api_key_row("ElevenLabs API Key", elevenlabs_set, &self.elevenlabs_key_input, theme))
            .child(api_key_row("Telnyx API Key", telnyx_set, &self.telnyx_key_input, theme))
            .child(separator(theme))
            .child(switch_row(
                "ClawdTalk Phone Bridge",
                "clawdtalk-switch",
                self.clawdtalk_enabled,
                SettingsToggleClawdTalk,
                theme,
            ))
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(if self.tts_enabled {
                        "TTS enabled -- assistant responses will be spoken aloud."
                    } else {
                        "TTS disabled -- enable to hear assistant responses."
                    }),
            )
            .into_any_element()
    }

    fn render_general_section(&self, _cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;

        card(theme)
            .child(section_title("\u{2699}", "General", theme))
            .child(section_desc(
                "Application preferences and display settings.",
                theme,
            ))
            .child(separator(theme))
            .child(switch_row(
                "Auto Update",
                "auto-update-switch",
                self.auto_update,
                SettingsToggleAutoUpdate,
                theme,
            ))
            .child(switch_row(
                "Notifications",
                "notifications-switch",
                self.notifications_enabled,
                SettingsToggleNotifications,
                theme,
            ))
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Shared card helpers
// ---------------------------------------------------------------------------

fn card(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
}

fn section_title(icon: &str, label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .child(icon.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child(label.to_string()),
        )
        .into_any_element()
}

fn section_desc(text: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_muted)
        .child(text.to_string())
        .into_any_element()
}

fn separator(theme: &HiveTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(theme.border)
        .into_any_element()
}

fn status_dot(present: bool, theme: &HiveTheme) -> AnyElement {
    let color = if present {
        theme.accent_green
    } else {
        theme.accent_red
    };
    div()
        .w(px(8.0))
        .h(px(8.0))
        .rounded(theme.radius_full)
        .bg(color)
        .into_any_element()
}

fn status_badge(connected: bool, theme: &HiveTheme) -> AnyElement {
    let (label, bg, color) = if connected {
        ("Connected", theme.bg_tertiary, theme.accent_green)
    } else {
        ("Not configured", theme.bg_tertiary, theme.accent_red)
    };
    div()
        .px(theme.space_2)
        .py(px(2.0))
        .rounded(theme.radius_sm)
        .bg(bg)
        .text_size(theme.font_size_xs)
        .text_color(color)
        .child(label)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Section: API Keys (free function to avoid borrow issues)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn render_api_keys_section(
    key_count: usize,
    anthropic_set: bool, anthropic_input: &Entity<InputState>,
    openai_set: bool, openai_input: &Entity<InputState>,
    openrouter_set: bool, openrouter_input: &Entity<InputState>,
    google_set: bool, google_input: &Entity<InputState>,
    groq_set: bool, groq_input: &Entity<InputState>,
    huggingface_set: bool, huggingface_input: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    card(theme)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(section_title("\u{1F511}", "API Keys", theme))
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_sm)
                        .bg(theme.bg_tertiary)
                        .text_size(theme.font_size_xs)
                        .text_color(if key_count > 0 {
                            theme.accent_green
                        } else {
                            theme.accent_red
                        })
                        .child(format!("{}/6 configured", key_count)),
                ),
        )
        .child(section_desc(
            "Provider API keys for cloud model access. Keys are stored locally and encrypted. Changes save automatically.",
            theme,
        ))
        .child(separator(theme))
        .child(api_key_row("Anthropic API Key", anthropic_set, anthropic_input, theme))
        .child(api_key_row("OpenAI API Key", openai_set, openai_input, theme))
        .child(api_key_row("OpenRouter API Key", openrouter_set, openrouter_input, theme))
        .child(api_key_row("Google API Key", google_set, google_input, theme))
        .child(api_key_row("Groq API Key", groq_set, groq_input, theme))
        .child(api_key_row("Hugging Face API Key", huggingface_set, huggingface_input, theme))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Row helpers with interactive widgets
// ---------------------------------------------------------------------------

/// An API key row with status dot, masked input, and status badge.
fn api_key_row(
    label: &str,
    has_key: bool,
    input_state: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(theme.space_4)
        .py(theme.space_1)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(status_dot(has_key, theme))
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_secondary)
                        .child(label.to_string()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div().min_w(px(240.0)).child(
                        Input::new(input_state)
                            .appearance(true)
                            .mask_toggle()
                            .cleanable(false),
                    ),
                )
                .child(status_badge(has_key, theme)),
        )
        .into_any_element()
}

/// A standard input row with label on the left and Input on the right.
fn input_row(
    label: &str,
    input_state: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(theme.space_4)
        .py(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(
            div().min_w(px(280.0)).child(
                Input::new(input_state)
                    .appearance(true)
                    .cleanable(false),
            ),
        )
        .into_any_element()
}

/// A budget input row with $ prefix label.
fn budget_row(
    label: &str,
    input_state: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(theme.space_4)
        .py(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("$"),
                )
                .child(
                    div().min_w(px(100.0)).child(
                        Input::new(input_state)
                            .appearance(true)
                            .cleanable(false),
                    ),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("USD"),
                ),
        )
        .into_any_element()
}

/// A toggle row with label on the left and Switch on the right.
fn switch_row<A: Action + Clone>(
    label: &str,
    id: impl Into<ElementId>,
    checked: bool,
    action: A,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(theme.space_4)
        .py(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(
            Switch::new(id)
                .checked(checked)
                .on_click(move |_new_checked, window, cx| {
                    window.dispatch_action(Box::new(action.clone()), cx);
                }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(key_count: usize, theme: &HiveTheme) -> AnyElement {
    let summary = if key_count > 0 {
        format!(
            "{} cloud provider{} connected",
            key_count,
            if key_count == 1 { "" } else { "s" },
        )
    } else {
        "No cloud providers configured -- local-only mode".into()
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
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
                .child(Icon::new(IconName::Settings).size_4()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_2xl)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Settings"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child(summary),
                ),
        )
        .into_any_element()
}
