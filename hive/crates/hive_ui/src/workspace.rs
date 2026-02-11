use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Icon;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

use hive_ai::providers::AiProvider;
use hive_ai::types::ChatRequest;
use hive_core::session::SessionState;

use crate::chat_input::{ChatInputView, SubmitMessage};
use crate::chat_service::{ChatService, StreamCompleted};
use crate::globals::{
    AppAiService, AppAssistant, AppConfig, AppLearning, AppMarketplace, AppPersonas, AppShield,
    AppSpecs,
};
use crate::panels::{
    agents::{AgentsPanelData, AgentsPanel},
    assistant::{AssistantPanelData, AssistantPanel},
    chat::ChatPanel,
    costs::{CostData, CostsPanel},
    files::{FilesData, FilesPanel},
    help::HelpPanel,
    history::{HistoryData, HistoryPanel},
    kanban::{KanbanData, KanbanPanel},
    learning::{LearningPanelData, LearningPanel},
    logs::{LogsData, LogsPanel},
    monitor::{MonitorData, MonitorPanel},
    review::{ReviewData, ReviewPanel},
    routing::{RoutingData, RoutingPanel},
    settings::{SettingsSaved, SettingsView},
    shield::{ShieldPanelData, ShieldPanel},
    skills::{SkillsData, SkillsPanel},
    specs::{SpecPanelData, SpecsPanel},
    token_launch::{TokenLaunchData, TokenLaunchPanel},
};
use crate::sidebar::{Panel, Sidebar};
use crate::statusbar::{ConnectivityDisplay, StatusBar};
use crate::theme::HiveTheme;
use crate::titlebar::Titlebar;

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(
    hive_workspace,
    [
        ClearChat,
        NewConversation,
        // Panel switch actions
        SwitchToChat,
        SwitchToHistory,
        SwitchToFiles,
        SwitchToKanban,
        SwitchToMonitor,
        SwitchToLogs,
        SwitchToCosts,
        SwitchToReview,
        SwitchToSkills,
        SwitchToRouting,
        SwitchToTokenLaunch,
        SwitchToSpecs,
        SwitchToAgents,
        SwitchToLearning,
        SwitchToShield,
        SwitchToAssistant,
        SwitchToSettings,
        SwitchToHelp,
        // Files panel
        FilesNavigateBack,
        FilesRefresh,
        FilesNewFile,
        FilesNewFolder,
        // History panel
        HistoryRefresh,
        // Kanban panel
        KanbanAddTask,
        // Logs panel
        LogsClear,
        LogsToggleAutoScroll,
        // Costs panel
        CostsExportCsv,
        CostsResetToday,
        CostsClearHistory,
        // Review panel
        ReviewStageAll,
        ReviewUnstageAll,
        ReviewCommit,
        ReviewDiscardAll,
        // Skills panel
        SkillsRefresh,
        // Routing panel
        RoutingAddRule,
        // Token Launch panel
        TokenLaunchDeploy,
        // Settings panel
        SettingsSave,
        // Monitor panel
        MonitorRefresh,
    ]
);

/// Navigate to a specific directory in the Files panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct FilesNavigateTo {
    pub path: String,
}

/// Open a file by path.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct FilesOpenEntry {
    pub name: String,
    pub is_directory: bool,
}

/// Delete a file entry.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct FilesDeleteEntry {
    pub name: String,
}

/// Load a conversation by ID in the History panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct HistoryLoadConversation {
    pub conversation_id: String,
}

/// Delete a conversation by ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct HistoryDeleteConversation {
    pub conversation_id: String,
}

/// Set log filter level.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct LogsSetFilter {
    pub level: String,
}

/// Token Launch wizard: advance or go back a step.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TokenLaunchSetStep {
    pub step: usize,
}

/// Token Launch: select a chain.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct TokenLaunchSelectChain {
    pub chain: String,
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

/// Root workspace layout: titlebar + sidebar + content + statusbar + chat input.
///
/// Owns the `Entity<ChatService>` and orchestrates the flow between the chat
/// input, AI service, and panel rendering.
pub struct HiveWorkspace {
    theme: HiveTheme,
    sidebar: Sidebar,
    status_bar: StatusBar,
    chat_input: Entity<ChatInputView>,
    chat_service: Entity<ChatService>,
    settings_view: Entity<SettingsView>,
    /// Focus handle for the workspace root div. Ensures that `dispatch_action`
    /// from child panels (Files, History, etc.) can bubble up to the root
    /// div's `.on_action()` handlers even when no input element is focused.
    focus_handle: FocusHandle,
    history_data: HistoryData,
    files_data: FilesData,
    kanban_data: KanbanData,
    monitor_data: MonitorData,
    logs_data: LogsData,
    review_data: ReviewData,
    cost_data: CostData,
    routing_data: RoutingData,
    skills_data: SkillsData,
    token_launch_data: TokenLaunchData,
    specs_data: SpecPanelData,
    agents_data: AgentsPanelData,
    shield_data: ShieldPanelData,
    learning_data: LearningPanelData,
    assistant_data: AssistantPanelData,
    /// In-flight stream spawn task (kept alive to prevent cancellation).
    _stream_task: Option<Task<()>>,
    /// Tracks whether session state needs to be persisted. Avoids writing
    /// `session.json` on every render frame -- only writes when state actually
    /// changed (panel switch, conversation load, stream finalization).
    session_dirty: bool,
    /// The conversation ID at the time of the last session save. Used to
    /// detect when a new conversation was auto-saved by `finalize_stream`.
    last_saved_conversation_id: Option<String>,
}

impl HiveWorkspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Read default model from config if available.
        let default_model = if cx.has_global::<AppConfig>() {
            cx.global::<AppConfig>().0.get().default_model.clone()
        } else {
            String::new()
        };

        let chat_service = cx.new(|_| ChatService::new(default_model.clone()));

        // Observe chat service — re-render whenever streaming state changes.
        cx.observe(&chat_service, |_this, _svc, cx| {
            cx.notify();
        })
        .detach();

        // Subscribe to stream completion events for learning instrumentation.
        cx.subscribe(&chat_service, |_this, _svc, event: &StreamCompleted, cx| {
            if cx.has_global::<AppLearning>() {
                let learning = &cx.global::<AppLearning>().0;
                let record = hive_learn::OutcomeRecord {
                    conversation_id: String::new(),
                    message_id: uuid::Uuid::new_v4().to_string(),
                    model_id: event.model.clone(),
                    task_type: "chat".into(),
                    tier: "standard".into(),
                    persona: None,
                    outcome: hive_learn::Outcome::Accepted,
                    edit_distance: None,
                    follow_up_count: 0,
                    quality_score: 0.8, // default; refined by future edits/regeneration
                    cost: event.cost.unwrap_or(0.0),
                    latency_ms: 0,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                if let Err(e) = learning.on_outcome(&record) {
                    tracing::warn!("Learning: failed to record outcome: {e}");
                }
            }
        })
        .detach();

        // Build initial status bar from config + providers.
        let mut status_bar = StatusBar::new();
        if cx.has_global::<AppConfig>() {
            let config = cx.global::<AppConfig>().0.get();
            status_bar.current_model = if config.default_model.is_empty() {
                "(no model)".to_string()
            } else {
                config.default_model.clone()
            };
            status_bar.privacy_mode = config.privacy_mode;
        }
        if cx.has_global::<AppAiService>() {
            let providers = cx.global::<AppAiService>().0.available_providers();
            status_bar.connectivity = if providers.is_empty() {
                ConnectivityDisplay::Offline
            } else {
                ConnectivityDisplay::Online
            };
        }

        // -- Session recovery: restore last conversation + panel ----------------
        let session = SessionState::load().unwrap_or_default();
        let mut restored_panel = Panel::Chat;

        if let Some(ref conv_id) = session.active_conversation_id {
            let load_result =
                chat_service.update(cx, |svc, _cx| svc.load_conversation(conv_id));
            match load_result {
                Ok(()) => {
                    info!("Session recovery: loaded conversation {conv_id}");
                    restored_panel = Panel::from_stored(&session.active_panel);
                }
                Err(e) => {
                    warn!("Session recovery: failed to load conversation {conv_id}: {e}");
                    // Start fresh -- don't propagate the error.
                }
            }
        } else if !session.active_panel.is_empty() {
            // No conversation to restore, but the user may have been on a
            // non-Chat panel (e.g. Settings, Files).
            restored_panel = Panel::from_stored(&session.active_panel);
        }

        let mut sidebar = Sidebar::new();
        sidebar.active_panel = restored_panel;

        // Create the interactive chat input entity.
        let chat_input = cx.new(|cx| ChatInputView::new(window, cx));

        // When the user submits a message, feed it into the send flow.
        cx.subscribe_in(&chat_input, window, |this, _view, event: &SubmitMessage, window, cx| {
            this.handle_send_text(event.0.clone(), window, cx);
        })
        .detach();

        // Create the interactive settings view entity.
        let settings_view = cx.new(|cx| SettingsView::new(window, cx));

        // When settings are saved, persist to AppConfig.
        cx.subscribe_in(&settings_view, window, |this, _view, _event: &SettingsSaved, _window, cx| {
            this.handle_settings_save_from_view(cx);
        })
        .detach();

        // Focus handle for the workspace root — ensures dispatch_action works
        // from child panel click handlers even when no input is focused.
        let focus_handle = cx.focus_handle();

        let history_data = HistoryData::empty();
        // Defer directory listing — will load when Files panel is first opened.
        let files_data = FilesData {
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            entries: Vec::new(),
            search_query: String::new(),
            selected_file: None,
            breadcrumbs: Vec::new(),
        };
        let kanban_data = KanbanData::default();
        let monitor_data = MonitorData::empty();
        let logs_data = LogsData::empty();
        let review_data = ReviewData::empty();
        let cost_data = CostData::empty();
        let routing_data = RoutingData::empty();
        let skills_data = SkillsData::empty();
        let token_launch_data = TokenLaunchData::new();
        let specs_data = SpecPanelData::empty();
        let agents_data = AgentsPanelData::empty();
        let shield_data = ShieldPanelData::empty();
        let learning_data = LearningPanelData::empty();
        let assistant_data = AssistantPanelData::empty();

        Self {
            theme: HiveTheme::dark(),
            sidebar,
            status_bar,
            chat_input,
            chat_service,
            settings_view,
            focus_handle,
            history_data,
            files_data,
            kanban_data,
            monitor_data,
            logs_data,
            review_data,
            cost_data,
            routing_data,
            skills_data,
            token_launch_data,
            specs_data,
            agents_data,
            shield_data,
            learning_data,
            assistant_data,
            _stream_task: None,
            session_dirty: false,
            last_saved_conversation_id: session.active_conversation_id.clone(),
        }
    }

    pub fn set_active_panel(&mut self, panel: Panel) {
        self.sidebar.active_panel = panel;
        self.session_dirty = true;
    }

    // -- History data --------------------------------------------------------

    pub fn refresh_history(&mut self) {
        self.history_data = Self::load_history_data();
    }

    fn refresh_learning_data(&mut self, cx: &App) {
        use crate::panels::learning::*;

        if !cx.has_global::<AppLearning>() {
            return;
        }
        let learning = &cx.global::<AppLearning>().0;

        let log_entries = learning
            .learning_log(20)
            .unwrap_or_default()
            .into_iter()
            .map(|e| LogEntryDisplay {
                event_type: e.event_type,
                description: e.description,
                timestamp: e.timestamp,
            })
            .collect();

        let preferences = learning
            .all_preferences()
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value, confidence)| PreferenceDisplay {
                key,
                value,
                confidence,
            })
            .collect();

        let routing_insights = learning
            .routing_learner
            .current_adjustments()
            .into_iter()
            .map(|adj| RoutingInsightDisplay {
                task_type: adj.task_type,
                from_tier: adj.from_tier,
                to_tier: adj.to_tier,
                confidence: adj.confidence,
            })
            .collect();

        let eval = learning.self_evaluator.evaluate().ok();

        self.learning_data = LearningPanelData {
            metrics: QualityMetrics {
                overall_quality: eval.as_ref().map_or(0.0, |e| e.overall_quality),
                trend: eval.as_ref().map_or("Stable".into(), |e| {
                    format!("{:?}", e.trend)
                }),
                total_interactions: learning.interaction_count(),
                correction_rate: eval.as_ref().map_or(0.0, |e| e.correction_rate),
                regeneration_rate: eval.as_ref().map_or(0.0, |e| e.regeneration_rate),
                cost_efficiency: eval.as_ref().map_or(0.0, |e| e.cost_per_quality_point),
            },
            log_entries,
            preferences,
            prompt_suggestions: Vec::new(),
            routing_insights,
            weak_areas: eval
                .as_ref()
                .map_or(Vec::new(), |e| e.weak_areas.clone()),
            best_model: eval.as_ref().and_then(|e| e.best_model.clone()),
            worst_model: eval.as_ref().and_then(|e| e.worst_model.clone()),
        };
    }

    fn refresh_shield_data(&mut self, cx: &App) {
        if cx.has_global::<AppShield>() {
            let shield = &cx.global::<AppShield>().0;
            self.shield_data.enabled = true;
            self.shield_data.pii_detections = shield.pii_detection_count();
            self.shield_data.secrets_blocked = shield.secrets_blocked_count();
            self.shield_data.threats_caught = shield.threats_caught_count();
        }
    }

    fn refresh_routing_data(&mut self, cx: &App) {
        if cx.has_global::<AppAiService>() {
            self.routing_data =
                RoutingData::from_router(cx.global::<AppAiService>().0.router());
        }
    }

    fn refresh_skills_data(&mut self, cx: &App) {
        use crate::panels::skills::InstalledSkill as UiSkill;

        let mut installed = Vec::new();

        // Built-in skills from the registry.
        if cx.has_global::<crate::globals::AppSkills>() {
            for skill in cx.global::<crate::globals::AppSkills>().0.list() {
                installed.push(UiSkill {
                    id: format!("builtin:{}", skill.name),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    version: "built-in".into(),
                    enabled: skill.enabled,
                    integrity_hash: skill.integrity_hash.clone(),
                });
            }
        }

        // Marketplace-installed skills.
        if cx.has_global::<AppMarketplace>() {
            for skill in cx.global::<AppMarketplace>().0.list_installed() {
                installed.push(UiSkill {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    version: skill.installed_at.format("%Y-%m-%d").to_string(),
                    enabled: skill.enabled,
                    integrity_hash: skill.integrity_hash.clone(),
                });
            }
        }

        self.skills_data.installed = installed;
    }

    fn refresh_agents_data(&mut self, cx: &App) {
        use crate::panels::agents::PersonaDisplay;

        if cx.has_global::<AppPersonas>() {
            let registry = &cx.global::<AppPersonas>().0;
            self.agents_data.personas = registry
                .all()
                .into_iter()
                .map(|p| PersonaDisplay {
                    name: p.name.clone(),
                    kind: format!("{:?}", p.kind),
                    description: p.description.clone(),
                    model_tier: format!("{:?}", p.model_tier),
                    active: false,
                })
                .collect();
        }
    }

    fn refresh_specs_data(&mut self, cx: &App) {
        use crate::panels::specs::SpecSummary;

        if cx.has_global::<AppSpecs>() {
            let manager = &cx.global::<AppSpecs>().0;
            self.specs_data.specs = manager
                .specs
                .values()
                .map(|s| SpecSummary {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    status: format!("{:?}", s.status),
                    entries_total: s.entry_count(),
                    entries_checked: s.checked_count(),
                    updated_at: s.updated_at.format("%Y-%m-%d %H:%M").to_string(),
                })
                .collect();
        }
    }

    fn refresh_assistant_data(&mut self, cx: &App) {
        use crate::panels::assistant::{ActiveReminder, BriefingSummary};

        if cx.has_global::<AppAssistant>() {
            let svc = &cx.global::<AppAssistant>().0;
            let briefing = svc.daily_briefing();

            self.assistant_data.briefing = Some(BriefingSummary {
                greeting: "Good morning!".into(),
                date: briefing.date.clone(),
                event_count: briefing.events.len(),
                unread_emails: briefing.email_summary.as_ref().map_or(0, |d| d.email_count),
                active_reminders: briefing.active_reminders.len(),
                top_priority: briefing.action_items.first().cloned(),
            });

            self.assistant_data.reminders = briefing
                .active_reminders
                .iter()
                .map(|r| ActiveReminder {
                    title: r.title.clone(),
                    due: r.updated_at.clone(),
                    is_overdue: false,
                })
                .collect();
        }
    }

    fn refresh_cost_data(&mut self, cx: &App) {
        self.cost_data = if cx.has_global::<AppAiService>() {
            CostData::from_tracker(cx.global::<AppAiService>().0.cost_tracker())
        } else {
            CostData::empty()
        };
    }

    pub fn load_history_data() -> HistoryData {
        match hive_core::ConversationStore::new() {
            Ok(store) => {
                let summaries = store.list_summaries().unwrap_or_default();
                HistoryData::from_summaries(summaries)
            }
            Err(_) => HistoryData::empty(),
        }
    }

    // -- Session persistence -------------------------------------------------

    /// Persist the current session state (conversation ID, active panel) to
    /// `~/.hive/session.json`. This is lightweight -- just a small JSON write.
    /// Errors are logged but never propagated.
    pub fn save_session(&mut self, cx: &App) {
        let svc = self.chat_service.read(cx);
        let conv_id = svc.conversation_id().map(String::from);

        let state = SessionState {
            active_conversation_id: conv_id.clone(),
            active_panel: self.sidebar.active_panel.to_stored().to_string(),
            window_size: None, // TODO: read from window when GPUI exposes it
            working_directory: None,
            open_files: Vec::new(),
            chat_draft: None,
        };

        if let Err(e) = state.save() {
            warn!("Failed to save session: {e}");
        }

        self.last_saved_conversation_id = conv_id;
        self.session_dirty = false;
    }

    // -- Send flow -----------------------------------------------------------

    /// Initiate sending a user message and streaming the AI response.
    ///
    /// Called when `ChatInputView` emits `SubmitMessage`. The input has
    /// already been cleared by the view before this is invoked.
    ///
    /// 1. Records the text in `ChatService`.
    /// 2. Extracts the provider + request from the `AppAiService` global.
    /// 3. Spawns an async task that calls `provider.stream_chat()` and feeds
    ///    the resulting receiver back into `ChatService::attach_stream`.
    fn handle_send_text(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if text.trim().is_empty() {
            return;
        }

        let model = self.chat_service.read(cx).current_model().to_string();

        // Shield: scan outgoing text before sending to AI.
        let send_text = if cx.has_global::<AppShield>() {
            let shield = &cx.global::<AppShield>().0;
            let result = shield.process_outgoing(&text, &model);
            match result.action {
                hive_shield::ShieldAction::Allow => text,
                hive_shield::ShieldAction::CloakAndAllow(ref cloaked) => {
                    info!("Shield: PII cloaked in outgoing message");
                    cloaked.text.clone()
                }
                hive_shield::ShieldAction::Block(ref reason) => {
                    warn!("Shield: blocked outgoing message: {reason}");
                    self.chat_service.update(cx, |svc, cx| {
                        svc.set_error(format!("Message blocked by privacy shield: {reason}"), cx);
                    });
                    return;
                }
                hive_shield::ShieldAction::Warn(ref warning) => {
                    warn!("Shield: warning on outgoing message: {warning}");
                    text
                }
            }
        } else {
            text
        };

        // 1. Record user message + create placeholder assistant message.
        self.chat_service.update(cx, |svc, cx| {
            svc.send_message(send_text, &model, cx);
        });

        // 2. Build the AI wire-format messages.
        let ai_messages = self.chat_service.read(cx).build_ai_messages();

        // 3. Extract provider + request from the global (sync — no await).
        let stream_setup: Option<(Arc<dyn AiProvider>, ChatRequest)> =
            if cx.has_global::<AppAiService>() {
                cx.global::<AppAiService>()
                    .0
                    .prepare_stream(ai_messages, &model, None)
            } else {
                None
            };

        let Some((provider, request)) = stream_setup else {
            self.chat_service.update(cx, |svc, cx| {
                svc.set_error(
                    "No AI providers configured. Check Settings \u{2192} API Keys.",
                    cx,
                );
            });
            return;
        };

        // 4. Spawn async: call provider.stream_chat, then attach to service.
        let chat_svc = self.chat_service.downgrade();
        let model_for_attach = model.clone();

        let task = cx.spawn(async move |_this, app: &mut AsyncApp| {
            match provider.stream_chat(&request).await {
                Ok(rx) => {
                    let _ = chat_svc.update(app, |svc, cx| {
                        svc.attach_stream(rx, model_for_attach, cx);
                    });
                }
                Err(e) => {
                    error!("Stream error: {e}");
                    let _ = chat_svc.update(app, |svc, cx| {
                        svc.set_error(format!("AI request failed: {e}"), cx);
                    });
                }
            }
        });

        self._stream_task = Some(task);
        self.chat_input.update(cx, |input, cx| {
            input.set_sending(true, window, cx);
        });

        info!("Send initiated (model={})", model);
        cx.notify();
    }

    /// Sync status bar with current chat service state.
    /// NOTE: This runs on every render frame — must be cheap. No file I/O here.
    fn sync_status_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Read all state from the chat service first, then release the borrow.
        let (model, is_streaming, total, current_conv_id) = {
            let svc = self.chat_service.read(cx);
            let model = svc.current_model().to_string();
            let streaming = svc.is_streaming();
            let total: f64 = svc.messages().iter().filter_map(|m| m.cost).sum();
            let conv_id = svc.conversation_id().map(String::from);
            (model, streaming, total, conv_id)
        };

        self.status_bar.current_model = if model.is_empty() {
            "(no model)".to_string()
        } else {
            model
        };
        self.status_bar.total_cost = total;

        // Sync the chat input disabled state with streaming status.
        self.chat_input.update(cx, |input, cx| {
            input.set_sending(is_streaming, window, cx);
        });

        // Detect conversation ID changes (e.g. after stream finalization
        // auto-saves a conversation and assigns an ID for the first time).
        if current_conv_id != self.last_saved_conversation_id {
            self.session_dirty = true;
            // Save session on actual state change — not every frame.
            self.save_session(cx);
        }
    }

    // -- Rendering -----------------------------------------------------------

    fn render_active_panel(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        match self.sidebar.active_panel {
            Panel::Chat => self.render_chat_from_service(cx).into_any_element(),
            Panel::History => HistoryPanel::render(&self.history_data, theme).into_any_element(),
            Panel::Files => FilesPanel::render(&self.files_data, theme).into_any_element(),
            Panel::Kanban => KanbanPanel::render(&self.kanban_data, theme).into_any_element(),
            Panel::Monitor => MonitorPanel::render(&self.monitor_data, theme).into_any_element(),
            Panel::Logs => LogsPanel::render(&self.logs_data, theme).into_any_element(),
            Panel::Costs => CostsPanel::render(&self.cost_data, theme).into_any_element(),
            Panel::Review => ReviewPanel::render(&self.review_data, theme).into_any_element(),
            Panel::Skills => SkillsPanel::render(&self.skills_data, theme).into_any_element(),
            Panel::Routing => RoutingPanel::render(&self.routing_data, theme).into_any_element(),
            Panel::TokenLaunch => TokenLaunchPanel::render(&self.token_launch_data, theme).into_any_element(),
            Panel::Specs => SpecsPanel::render(&self.specs_data, theme).into_any_element(),
            Panel::Agents => AgentsPanel::render(&self.agents_data, theme).into_any_element(),
            Panel::Shield => ShieldPanel::render(&self.shield_data, theme).into_any_element(),
            Panel::Learning => LearningPanel::render(&self.learning_data, theme).into_any_element(),
            Panel::Assistant => AssistantPanel::render(&self.assistant_data, theme).into_any_element(),
            Panel::Settings => self.settings_view.clone().into_any_element(),
            Panel::Help => HelpPanel::render(theme).into_any_element(),
        }
    }

    /// Render the chat panel by reading data from ChatService entity.
    fn render_chat_from_service(&self, cx: &Context<Self>) -> AnyElement {
        let svc = self.chat_service.read(cx);
        ChatPanel::render_from_service(
            svc.messages(),
            svc.streaming_content(),
            svc.is_streaming(),
            svc.current_model(),
            &self.theme,
        )
    }

    // -- Keyboard action handlers --------------------------------------------

    fn handle_new_conversation(
        &mut self,
        _action: &NewConversation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("NewConversation action triggered");
        self.chat_service.update(cx, |svc, _cx| {
            svc.new_conversation();
        });
        self.refresh_history();
        self.sidebar.active_panel = Panel::Chat;
        self.session_dirty = true;
        cx.notify();
    }

    fn handle_clear_chat(
        &mut self,
        _action: &ClearChat,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClearChat action triggered");
        self.chat_service.update(cx, |svc, _cx| {
            svc.clear();
        });
        cx.notify();
    }

    fn switch_to_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        info!("SwitchToPanel action: {:?}", panel);
        self.sidebar.active_panel = panel;

        // Lazy-load data for panels that need it on first visit.
        match panel {
            Panel::History if self.history_data.conversations.is_empty() => {
                self.history_data = Self::load_history_data();
            }
            Panel::Files if self.files_data.entries.is_empty() => {
                self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
            }
            Panel::Review => {
                self.review_data = ReviewData::from_cwd();
            }
            Panel::Costs => {
                self.refresh_cost_data(cx);
            }
            Panel::Learning => {
                self.refresh_learning_data(cx);
            }
            Panel::Shield => {
                self.refresh_shield_data(cx);
            }
            Panel::Routing => {
                self.refresh_routing_data(cx);
            }
            Panel::Skills => {
                self.refresh_skills_data(cx);
            }
            Panel::Agents => {
                self.refresh_agents_data(cx);
            }
            Panel::Specs => {
                self.refresh_specs_data(cx);
            }
            Panel::Assistant => {
                self.refresh_assistant_data(cx);
            }
            _ => {}
        }

        // Save session immediately (this is an action handler, not render path).
        self.save_session(cx);
        cx.notify();
    }

    fn handle_switch_to_chat(
        &mut self,
        _action: &SwitchToChat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Chat, cx);
        // Focus the chat text input so the user can start typing immediately.
        let fh = self.chat_input.read(cx).input_focus_handle();
        window.focus(&fh);
    }

    fn handle_switch_to_history(
        &mut self,
        _action: &SwitchToHistory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::History, cx);
    }

    fn handle_switch_to_files(
        &mut self,
        _action: &SwitchToFiles,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Files, cx);
    }

    fn handle_switch_to_kanban(
        &mut self,
        _action: &SwitchToKanban,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Kanban, cx);
    }

    fn handle_switch_to_monitor(
        &mut self,
        _action: &SwitchToMonitor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Monitor, cx);
    }

    fn handle_switch_to_logs(
        &mut self,
        _action: &SwitchToLogs,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Logs, cx);
    }

    fn handle_switch_to_costs(
        &mut self,
        _action: &SwitchToCosts,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Costs, cx);
    }

    fn handle_switch_to_review(
        &mut self,
        _action: &SwitchToReview,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Review, cx);
    }

    fn handle_switch_to_skills(
        &mut self,
        _action: &SwitchToSkills,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Skills, cx);
    }

    fn handle_switch_to_routing(
        &mut self,
        _action: &SwitchToRouting,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Routing, cx);
    }

    fn handle_switch_to_token_launch(
        &mut self,
        _action: &SwitchToTokenLaunch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::TokenLaunch, cx);
    }

    fn handle_switch_to_specs(
        &mut self,
        _action: &SwitchToSpecs,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Specs, cx);
    }

    fn handle_switch_to_agents(
        &mut self,
        _action: &SwitchToAgents,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Agents, cx);
    }

    fn handle_switch_to_learning(
        &mut self,
        _action: &SwitchToLearning,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Learning, cx);
    }

    fn handle_switch_to_shield(
        &mut self,
        _action: &SwitchToShield,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Shield, cx);
    }

    fn handle_switch_to_assistant(
        &mut self,
        _action: &SwitchToAssistant,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Assistant, cx);
    }

    fn handle_switch_to_settings(
        &mut self,
        _action: &SwitchToSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Settings, cx);
    }

    fn handle_switch_to_help(
        &mut self,
        _action: &SwitchToHelp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to_panel(Panel::Help, cx);
    }

    // -- Files panel handlers ------------------------------------------------

    fn handle_files_navigate_back(
        &mut self,
        _action: &FilesNavigateBack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(parent) = self.files_data.current_path.parent() {
            let parent = parent.to_path_buf();
            info!("Files: navigate back to {}", parent.display());
            self.files_data = FilesData::from_path(&parent);
            cx.notify();
        }
    }

    fn handle_files_navigate_to(
        &mut self,
        action: &FilesNavigateTo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&action.path);
        info!("Files: navigate to {}", path.display());
        self.files_data = FilesData::from_path(&path);
        cx.notify();
    }

    fn handle_files_open_entry(
        &mut self,
        action: &FilesOpenEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.is_directory {
            let new_path = self.files_data.current_path.join(&action.name);
            info!("Files: open directory {}", new_path.display());
            self.files_data = FilesData::from_path(&new_path);
        } else {
            let file_path = self.files_data.current_path.join(&action.name);
            info!("Files: open file {}", file_path.display());
            self.files_data.selected_file = Some(action.name.clone());
            // Open in default system editor
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("cmd").args(["/C", "start", "", &file_path.to_string_lossy()]).spawn();
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open").arg(&file_path).spawn();
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open").arg(&file_path).spawn();
        }
        cx.notify();
    }

    fn handle_files_delete_entry(
        &mut self,
        action: &FilesDeleteEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.files_data.current_path.join(&action.name);
        info!("Files: delete {}", target.display());
        let result = if target.is_dir() {
            std::fs::remove_dir_all(&target)
        } else {
            std::fs::remove_file(&target)
        };
        if let Err(e) = result {
            warn!("Files: failed to delete {}: {e}", target.display());
        }
        // Refresh the listing
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    fn handle_files_refresh(
        &mut self,
        _action: &FilesRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Files: refresh");
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    fn handle_files_new_file(
        &mut self,
        _action: &FilesNewFile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self.files_data.current_path.join("untitled.txt");
        info!("Files: create new file {}", path.display());
        if let Err(e) = std::fs::write(&path, "") {
            warn!("Files: failed to create file: {e}");
        }
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    fn handle_files_new_folder(
        &mut self,
        _action: &FilesNewFolder,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self.files_data.current_path.join("new_folder");
        info!("Files: create new folder {}", path.display());
        if let Err(e) = std::fs::create_dir(&path) {
            warn!("Files: failed to create folder: {e}");
        }
        self.files_data = FilesData::from_path(&self.files_data.current_path.clone());
        cx.notify();
    }

    // -- History panel handlers ----------------------------------------------

    fn handle_history_load(
        &mut self,
        action: &HistoryLoadConversation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("History: load conversation {}", action.conversation_id);
        let result = self.chat_service.update(cx, |svc, _cx| {
            svc.load_conversation(&action.conversation_id)
        });
        match result {
            Ok(()) => {
                self.sidebar.active_panel = Panel::Chat;
                self.session_dirty = true;
            }
            Err(e) => warn!("History: failed to load conversation: {e}"),
        }
        cx.notify();
    }

    fn handle_history_delete(
        &mut self,
        action: &HistoryDeleteConversation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("History: delete conversation {}", action.conversation_id);
        if let Ok(store) = hive_core::ConversationStore::new() {
            if let Err(e) = store.delete(&action.conversation_id) {
                warn!("History: failed to delete conversation: {e}");
            }
        }
        self.refresh_history();
        cx.notify();
    }

    fn handle_history_refresh(
        &mut self,
        _action: &HistoryRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_history();
        cx.notify();
    }

    // -- Kanban panel handlers -----------------------------------------------

    fn handle_kanban_add_task(
        &mut self,
        _action: &KanbanAddTask,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::panels::kanban::{KanbanTask, Priority};
        info!("Kanban: add task");
        let task = KanbanTask {
            id: self.kanban_data.columns.iter().map(|c| c.tasks.len() as u64).sum::<u64>() + 1,
            title: "New Task".to_string(),
            description: String::new(),
            priority: Priority::Medium,
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            assigned_model: None,
        };
        self.kanban_data.columns[0].tasks.push(task);
        cx.notify();
    }

    // -- Logs panel handlers -------------------------------------------------

    fn handle_logs_clear(
        &mut self,
        _action: &LogsClear,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Logs: clear");
        self.logs_data.entries.clear();
        cx.notify();
    }

    fn handle_logs_set_filter(
        &mut self,
        action: &LogsSetFilter,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::panels::logs::LogLevel;
        info!("Logs: set filter to {}", action.level);
        self.logs_data.filter = match action.level.as_str() {
            "error" => LogLevel::Error,
            "warning" => LogLevel::Warning,
            "info" => LogLevel::Info,
            _ => LogLevel::Debug,
        };
        cx.notify();
    }

    fn handle_logs_toggle_auto_scroll(
        &mut self,
        _action: &LogsToggleAutoScroll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.logs_data.auto_scroll = !self.logs_data.auto_scroll;
        cx.notify();
    }

    // -- Costs panel handlers ------------------------------------------------

    fn handle_costs_export_csv(
        &mut self,
        _action: &CostsExportCsv,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        info!("Costs: export CSV");
        // TODO: implement CSV export
    }

    fn handle_costs_reset_today(
        &mut self,
        _action: &CostsResetToday,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Costs: reset today");
        if cx.has_global::<AppAiService>() {
            cx.global_mut::<AppAiService>().0.cost_tracker_mut().reset_today();
        }
        cx.notify();
    }

    fn handle_costs_clear_history(
        &mut self,
        _action: &CostsClearHistory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Costs: clear all history");
        if cx.has_global::<AppAiService>() {
            cx.global_mut::<AppAiService>().0.cost_tracker_mut().clear();
        }
        cx.notify();
    }

    // -- Review panel handlers -----------------------------------------------

    fn handle_review_stage_all(
        &mut self,
        _action: &ReviewStageAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: stage all");
        let _ = std::process::Command::new("git").args(["add", "-A"]).output();
        self.review_data = ReviewData::from_cwd();
        cx.notify();
    }

    fn handle_review_unstage_all(
        &mut self,
        _action: &ReviewUnstageAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: unstage all");
        let _ = std::process::Command::new("git").args(["reset", "HEAD"]).output();
        self.review_data = ReviewData::from_cwd();
        cx.notify();
    }

    fn handle_review_commit(
        &mut self,
        _action: &ReviewCommit,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        info!("Review: commit");
        // TODO: prompt for commit message
    }

    fn handle_review_discard_all(
        &mut self,
        _action: &ReviewDiscardAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: discard all");
        let _ = std::process::Command::new("git").args(["checkout", "--", "."]).output();
        self.review_data = ReviewData::from_cwd();
        cx.notify();
    }

    // -- Skills panel handlers -----------------------------------------------

    fn handle_skills_refresh(
        &mut self,
        _action: &SkillsRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Skills: refresh");
        self.refresh_skills_data(cx);
        cx.notify();
    }

    // -- Routing panel handlers ----------------------------------------------

    fn handle_routing_add_rule(
        &mut self,
        _action: &RoutingAddRule,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::panels::routing::RoutingRule;
        info!("Routing: add rule");
        self.routing_data.custom_rules.push(RoutingRule {
            name: "New Rule".to_string(),
            condition: "task_type == \"code\"".to_string(),
            target_model: "auto".to_string(),
            enabled: true,
        });
        cx.notify();
    }

    // -- Token Launch panel handlers -----------------------------------------

    fn handle_token_launch_set_step(
        &mut self,
        action: &TokenLaunchSetStep,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::panels::token_launch::WizardStep;
        info!("TokenLaunch: set step {}", action.step);
        self.token_launch_data.current_step = match action.step {
            0 => WizardStep::SelectChain,
            1 => WizardStep::TokenDetails,
            2 => WizardStep::WalletSetup,
            _ => WizardStep::Deploy,
        };
        cx.notify();
    }

    fn handle_token_launch_select_chain(
        &mut self,
        action: &TokenLaunchSelectChain,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::panels::token_launch::ChainOption;
        info!("TokenLaunch: select chain {}", action.chain);
        self.token_launch_data.selected_chain = match action.chain.as_str() {
            "solana" => Some(ChainOption::Solana),
            "ethereum" => Some(ChainOption::Ethereum),
            "base" => Some(ChainOption::Base),
            _ => None,
        };
        cx.notify();
    }

    fn handle_token_launch_deploy(
        &mut self,
        _action: &TokenLaunchDeploy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("TokenLaunch: deploy");
        // TODO: wire to actual blockchain deployment
        cx.notify();
    }

    // -- Settings panel handlers ---------------------------------------------

    fn handle_settings_save(
        &mut self,
        _action: &SettingsSave,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Save is now handled via SettingsSaved event from SettingsView.
        // The action still dispatches to the view which emits the event.
    }

    /// Called when `SettingsView` emits `SettingsSaved`. Reads all values from
    /// the view and persists them to `AppConfig`.
    fn handle_settings_save_from_view(&mut self, cx: &mut Context<Self>) {
        info!("Settings: persisting from SettingsView");

        let snapshot = self.settings_view.read(cx).collect_values(cx);

        if cx.has_global::<AppConfig>() {
            let config_mgr = &cx.global::<AppConfig>().0;

            // Persist non-key fields via update()
            if let Err(e) = config_mgr.update(|cfg| {
                cfg.ollama_url = snapshot.ollama_url.clone();
                cfg.lmstudio_url = snapshot.lmstudio_url.clone();
                cfg.local_provider_url = snapshot.custom_url.clone();
                cfg.default_model = snapshot.default_model.clone();
                cfg.daily_budget_usd = snapshot.daily_budget;
                cfg.monthly_budget_usd = snapshot.monthly_budget;
                cfg.privacy_mode = snapshot.privacy_mode;
                cfg.auto_routing = snapshot.auto_routing;
                cfg.auto_update = snapshot.auto_update;
                cfg.notifications_enabled = snapshot.notifications_enabled;
                cfg.tts_enabled = snapshot.tts_enabled;
                cfg.tts_auto_speak = snapshot.tts_auto_speak;
                cfg.clawdtalk_enabled = snapshot.clawdtalk_enabled;
            }) {
                warn!("Settings: failed to save config: {e}");
            }

            // Persist API keys only when user entered a new value
            if let Some(ref key) = snapshot.anthropic_key {
                let _ = config_mgr.set_api_key("anthropic", Some(key.clone()));
            }
            if let Some(ref key) = snapshot.openai_key {
                let _ = config_mgr.set_api_key("openai", Some(key.clone()));
            }
            if let Some(ref key) = snapshot.openrouter_key {
                let _ = config_mgr.set_api_key("openrouter", Some(key.clone()));
            }
            if let Some(ref key) = snapshot.google_key {
                let _ = config_mgr.set_api_key("google", Some(key.clone()));
            }
            if let Some(ref key) = snapshot.elevenlabs_key {
                let _ = config_mgr.set_api_key("elevenlabs", Some(key.clone()));
            }
            if let Some(ref key) = snapshot.telnyx_key {
                let _ = config_mgr.set_api_key("telnyx", Some(key.clone()));
            }

            // Sync status bar with potentially changed model/privacy
            self.status_bar.current_model = if snapshot.default_model.is_empty() {
                "(no model)".to_string()
            } else {
                snapshot.default_model
            };
            self.status_bar.privacy_mode = snapshot.privacy_mode;
        }

        cx.notify();
    }

    // -- Monitor panel handlers ----------------------------------------------

    fn handle_monitor_refresh(
        &mut self,
        _action: &MonitorRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Monitor: refresh");
        cx.notify();
    }
}

impl Render for HiveWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_status_bar(window, cx);

        // Auto-focus: when nothing is focused, give focus to the chat input on
        // the Chat panel or the workspace root on other panels. This ensures
        // typing goes straight into the input and dispatch_action() still works.
        if window.focused(cx).is_none() {
            if self.sidebar.active_panel == Panel::Chat {
                let fh = self.chat_input.read(cx).input_focus_handle();
                window.focus(&fh);
            } else {
                window.focus(&self.focus_handle);
            }
        }

        let theme = &self.theme;
        let active_panel = self.sidebar.active_panel;
        let chat_input = self.chat_input.clone();

        div()
            .id("workspace-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_primary)
            .text_color(theme.text_primary)
            // -- Action handlers for keyboard shortcuts -----------------------
            .on_action(cx.listener(Self::handle_new_conversation))
            .on_action(cx.listener(Self::handle_clear_chat))
            .on_action(cx.listener(Self::handle_switch_to_chat))
            .on_action(cx.listener(Self::handle_switch_to_history))
            .on_action(cx.listener(Self::handle_switch_to_files))
            .on_action(cx.listener(Self::handle_switch_to_kanban))
            .on_action(cx.listener(Self::handle_switch_to_monitor))
            .on_action(cx.listener(Self::handle_switch_to_logs))
            .on_action(cx.listener(Self::handle_switch_to_costs))
            .on_action(cx.listener(Self::handle_switch_to_review))
            .on_action(cx.listener(Self::handle_switch_to_skills))
            .on_action(cx.listener(Self::handle_switch_to_routing))
            .on_action(cx.listener(Self::handle_switch_to_token_launch))
            .on_action(cx.listener(Self::handle_switch_to_specs))
            .on_action(cx.listener(Self::handle_switch_to_agents))
            .on_action(cx.listener(Self::handle_switch_to_learning))
            .on_action(cx.listener(Self::handle_switch_to_shield))
            .on_action(cx.listener(Self::handle_switch_to_assistant))
            .on_action(cx.listener(Self::handle_switch_to_settings))
            .on_action(cx.listener(Self::handle_switch_to_help))
            // -- Panel action handlers -----------------------------------
            // Files
            .on_action(cx.listener(Self::handle_files_navigate_back))
            .on_action(cx.listener(Self::handle_files_navigate_to))
            .on_action(cx.listener(Self::handle_files_open_entry))
            .on_action(cx.listener(Self::handle_files_delete_entry))
            .on_action(cx.listener(Self::handle_files_refresh))
            .on_action(cx.listener(Self::handle_files_new_file))
            .on_action(cx.listener(Self::handle_files_new_folder))
            // History
            .on_action(cx.listener(Self::handle_history_load))
            .on_action(cx.listener(Self::handle_history_delete))
            .on_action(cx.listener(Self::handle_history_refresh))
            // Kanban
            .on_action(cx.listener(Self::handle_kanban_add_task))
            // Logs
            .on_action(cx.listener(Self::handle_logs_clear))
            .on_action(cx.listener(Self::handle_logs_set_filter))
            .on_action(cx.listener(Self::handle_logs_toggle_auto_scroll))
            // Costs
            .on_action(cx.listener(Self::handle_costs_export_csv))
            .on_action(cx.listener(Self::handle_costs_reset_today))
            .on_action(cx.listener(Self::handle_costs_clear_history))
            // Review
            .on_action(cx.listener(Self::handle_review_stage_all))
            .on_action(cx.listener(Self::handle_review_unstage_all))
            .on_action(cx.listener(Self::handle_review_commit))
            .on_action(cx.listener(Self::handle_review_discard_all))
            // Skills
            .on_action(cx.listener(Self::handle_skills_refresh))
            // Routing
            .on_action(cx.listener(Self::handle_routing_add_rule))
            // Token Launch
            .on_action(cx.listener(Self::handle_token_launch_set_step))
            .on_action(cx.listener(Self::handle_token_launch_select_chain))
            .on_action(cx.listener(Self::handle_token_launch_deploy))
            // Settings
            .on_action(cx.listener(Self::handle_settings_save))
            // Monitor
            .on_action(cx.listener(Self::handle_monitor_refresh))
            // Titlebar
            .child(Titlebar::render(theme, window))
            // Main content area: sidebar + panel
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    // Sidebar
                    .child(self.render_sidebar(cx))
                    // Active panel content
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_hidden()
                            .child(self.render_active_panel(cx))
                            // Chat input (only shown on Chat panel)
                            .when(active_panel == Panel::Chat, |el: Div| {
                                el.child(chat_input)
                            }),
                    ),
            )
            // Status bar
            .child(self.status_bar.render(theme))
    }
}

impl HiveWorkspace {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let active = self.sidebar.active_panel;

        div()
            .flex()
            .flex_col()
            .w(px(52.0))
            .h_full()
            .bg(theme.bg_secondary)
            .border_r_1()
            .border_color(theme.border)
            .pt(theme.space_2)
            .gap(theme.space_1)
            .children(Panel::ALL.into_iter().map(|panel| {
                let is_active = panel == active;
                let bg = if is_active {
                    theme.bg_tertiary
                } else {
                    Hsla::transparent_black()
                };
                let text_color = if is_active {
                    theme.accent_aqua
                } else {
                    theme.text_secondary
                };
                let left_border = if is_active {
                    theme.accent_aqua
                } else {
                    Hsla::transparent_black()
                };

                div()
                    .id(ElementId::Name(panel.label().into()))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .h(px(44.0))
                    .bg(bg)
                    .border_l_2()
                    .border_color(left_border)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            info!("Sidebar click: {:?}", panel);
                            this.set_active_panel(panel);
                            cx.notify();
                        }),
                    )
                    .child(
                        Icon::new(panel.icon())
                            .size_4()
                            .text_color(text_color),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(text_color)
                            .child(panel.label()),
                    )
            }))
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Event emitted when clicking a sidebar panel.
#[derive(Debug, Clone)]
pub struct SwitchPanel(pub Panel);

impl EventEmitter<SwitchPanel> for HiveWorkspace {}
