use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Icon;
use gpui_component::scroll::ScrollableElement;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

use hive_ai::providers::AiProvider;
use hive_ai::types::{ChatRequest, ToolDefinition as AiToolDefinition};
use hive_core::config::HiveConfig;
use hive_core::notifications::{AppNotification, NotificationType};
use hive_core::session::SessionState;
use hive_assistant::ReminderTrigger;

use crate::chat_input::{ChatInputView, SubmitMessage};
use crate::chat_service::{ChatService, StreamCompleted};
use chrono::Utc;
use hive_ui_core::{
    // Globals
    AppAiService, AppAssistant, AppAutomation, AppConfig, AppLearning, AppMarketplace, AppNotifications,
    AppPersonas, AppSecurity, AppShield, AppSpecs,
    // Types
    HiveTheme, Panel, Sidebar,
};
// Re-export actions so hive_app can import from hive_ui::workspace::*
pub use hive_ui_core::{
    ClearChat, NewConversation,
    SwitchToChat, SwitchToHistory, SwitchToFiles, SwitchToKanban, SwitchToMonitor,
    SwitchToLogs, SwitchToCosts, SwitchToReview, SwitchToSkills, SwitchToRouting,
    SwitchToTokenLaunch, SwitchToSpecs, SwitchToAgents, SwitchToLearning, SwitchToShield,
    SwitchToAssistant, SwitchToSettings, SwitchToHelp,
    FilesNavigateBack, FilesRefresh, FilesNewFile, FilesNewFolder,
    FilesNavigateTo, FilesOpenEntry, FilesDeleteEntry,
    HistoryRefresh, HistoryLoadConversation, HistoryDeleteConversation,
    KanbanAddTask, LogsClear, LogsToggleAutoScroll, LogsSetFilter,
    CostsExportCsv, CostsResetToday, CostsClearHistory,
    ReviewStageAll, ReviewUnstageAll, ReviewCommit, ReviewDiscardAll,
    SkillsRefresh, RoutingAddRule, TokenLaunchDeploy, TokenLaunchSetStep, TokenLaunchSelectChain,
    SettingsSave, MonitorRefresh, AgentsReloadWorkflows, AgentsRunWorkflow,
};
use hive_ui_panels::panels::chat::{DisplayMessage, ToolCallDisplay};
use hive_ui_panels::panels::{
    agents::{AgentsPanel, AgentsPanelData},
    assistant::{AssistantPanel, AssistantPanelData},
    chat::{CachedChatData, ChatPanel},
    costs::{CostData, CostsPanel},
    files::{FilesData, FilesPanel},
    help::HelpPanel,
    history::{HistoryData, HistoryPanel},
    kanban::{KanbanData, KanbanPanel},
    learning::{LearningPanel, LearningPanelData},
    logs::{LogsData, LogsPanel},
    monitor::{MonitorData, MonitorPanel},
    review::{ReviewData, ReviewPanel},
    routing::{RoutingData, RoutingPanel},
    settings::{SettingsSaved, SettingsView},
    shield::{ShieldPanel, ShieldPanelData},
    skills::{SkillsData, SkillsPanel},
    specs::{SpecPanelData, SpecsPanel},
    token_launch::{TokenLaunchData, TokenLaunchPanel},
};
use crate::statusbar::{ConnectivityDisplay, StatusBar};
use crate::titlebar::Titlebar;

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
    current_project_root: PathBuf,
    current_project_name: String,
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
    /// Cached display data for the chat panel. Rebuilt only when the
    /// `ChatService` generation counter changes, avoiding per-frame string
    /// cloning and enabling markdown parse caching.
    cached_chat_data: CachedChatData,
    /// Timestamp of the last discovery scan (for 30s cadence).
    last_discovery_scan: Option<std::time::Instant>,
    /// Whether a discovery scan is currently in-flight.
    discovery_scan_pending: bool,
    /// Set to `true` by the background scan thread when done.
    discovery_done_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
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
            let load_result = chat_service.update(cx, |svc, _cx| svc.load_conversation(conv_id));
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

        let project_root = Self::resolve_project_root_from_session(&session);
        let project_name = Self::project_name_from_path(&project_root);
        let files_data = FilesData::from_path(&project_root);
        status_bar.active_project = format!(
            "{} [{}]",
            project_name,
            project_root.display()
        );

        // Create the interactive chat input entity.
        let chat_input = cx.new(|cx| ChatInputView::new(window, cx));

        // When the user submits a message, feed it into the send flow.
        cx.subscribe_in(
            &chat_input,
            window,
            |this, _view, event: &SubmitMessage, window, cx| {
                this.handle_send_text(event.0.clone(), window, cx);
            },
        )
        .detach();

        // Create the interactive settings view entity.
        let settings_view = cx.new(|cx| SettingsView::new(window, cx));

        // When settings are saved, persist to AppConfig.
        cx.subscribe_in(
            &settings_view,
            window,
            |this, _view, _event: &SettingsSaved, _window, cx| {
                this.handle_settings_save_from_view(cx);
            },
        )
        .detach();

        // Focus handle for the workspace root — ensures dispatch_action works
        // from child panel click handlers even when no input is focused.
        let focus_handle = cx.focus_handle();

        let history_data = HistoryData::empty();
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
            current_project_root: project_root,
            current_project_name: project_name,
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
            cached_chat_data: CachedChatData::new(),
            last_discovery_scan: None,
            discovery_scan_pending: false,
            discovery_done_flag: None,
        }
    }

    fn resolve_project_root_from_session(session: &SessionState) -> PathBuf {
        let fallback = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let requested = session
            .working_directory
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| fallback.clone());

        let requested = if requested.exists() { requested } else { fallback };
        Self::discover_project_root(&requested)
    }

    fn discover_project_root(path: &Path) -> PathBuf {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut current = canonical.as_path();

        while let Some(parent) = current.parent() {
            if current.join(".git").exists() {
                return current.to_path_buf();
            }
            current = parent;
        }

        if canonical.join(".git").exists() {
            return canonical;
        }

        canonical
    }

    fn project_name_from_path(path: &Path) -> String {
        path.file_name()
            .unwrap_or_else(|| path.as_os_str())
            .to_string_lossy()
            .to_string()
    }

    fn project_label(&self) -> String {
        format!("{} [{}]", self.current_project_name, self.current_project_root.display())
    }

    fn apply_project_context(&mut self, cwd: &Path, cx: &mut Context<Self>) {
        let project_root = Self::discover_project_root(cwd);
        if project_root != self.current_project_root {
            self.current_project_root = project_root;
            self.current_project_name = Self::project_name_from_path(&self.current_project_root);
            self.status_bar.active_project = self.project_label();
            self.session_dirty = true;
            self.save_session(cx);
            cx.notify();
        } else if self.current_project_name.is_empty() {
            self.current_project_name = Self::project_name_from_path(&self.current_project_root);
            self.status_bar.active_project = self.project_label();
            cx.notify();
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
        use hive_ui_panels::panels::learning::*;

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
                trend: eval
                    .as_ref()
                    .map_or("Stable".into(), |e| format!("{:?}", e.trend)),
                total_interactions: learning.interaction_count(),
                correction_rate: eval.as_ref().map_or(0.0, |e| e.correction_rate),
                regeneration_rate: eval.as_ref().map_or(0.0, |e| e.regeneration_rate),
                cost_efficiency: eval.as_ref().map_or(0.0, |e| e.cost_per_quality_point),
            },
            log_entries,
            preferences,
            prompt_suggestions: Vec::new(),
            routing_insights,
            weak_areas: eval.as_ref().map_or(Vec::new(), |e| e.weak_areas.clone()),
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
            self.routing_data = RoutingData::from_router(cx.global::<AppAiService>().0.router());
        }
    }

    fn refresh_skills_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::skills::InstalledSkill as UiSkill;

        let mut installed = Vec::new();

        // Built-in skills from the registry.
        if cx.has_global::<hive_ui_core::AppSkills>() {
            for skill in cx.global::<hive_ui_core::AppSkills>().0.list() {
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
        use hive_ui_panels::panels::agents::{PersonaDisplay, RunDisplay, WorkflowDisplay};

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

        if cx.has_global::<AppAutomation>() {
            let automation = &cx.global::<AppAutomation>().0;

            self.agents_data.workflows = automation
                .list_workflows()
                .iter()
                .map(|wf| WorkflowDisplay {
                    id: wf.id.clone(),
                    name: wf.name.clone(),
                    description: wf.description.clone(),
                    commands: Self::workflow_command_preview(wf),
                    source: if wf.id.starts_with("builtin:") {
                        "Built-in".into()
                    } else if wf.id.starts_with("file:") {
                        "User file".into()
                    } else {
                        "Runtime".into()
                    },
                    status: format!("{:?}", wf.status),
                    trigger: Self::trigger_label(&wf.trigger),
                    steps: wf.steps.len(),
                    run_count: wf.run_count as usize,
                    last_run: wf
                        .last_run
                        .as_ref()
                        .map(|ts: &chrono::DateTime<chrono::Utc>| {
                            ts.format("%Y-%m-%d %H:%M").to_string()
                        }),
                })
                .collect();

            self.agents_data.active_runs = automation
                .list_workflows()
                .iter()
                .filter(|wf| {
                    matches!(
                        wf.status,
                        hive_agents::automation::WorkflowStatus::Active
                            | hive_agents::automation::WorkflowStatus::Draft
                    )
                })
                .map(|wf| RunDisplay {
                    id: wf.id.clone(),
                    spec_title: wf.name.clone(),
                    status: format!("{:?}", wf.status),
                    progress: if wf.steps.is_empty() { 0.0 } else { 1.0 },
                    tasks_done: wf.steps.len(),
                    tasks_total: wf.steps.len(),
                    cost: 0.0,
                    elapsed: wf
                        .last_run
                        .as_ref()
                        .map(|_| "recent".to_string())
                        .unwrap_or_else(|| "-".to_string()),
                })
                .collect();

            self.agents_data.run_history = automation
                .list_run_history()
                .iter()
                .rev()
                .take(8)
                .filter_map(|run| {
                    let workflow = automation.get_workflow(&run.workflow_id)?;
                    Some(RunDisplay {
                        id: run.workflow_id.clone(),
                        spec_title: workflow.name.clone(),
                        status: if run.success {
                            "Complete".into()
                        } else {
                            "Failed".into()
                        },
                        progress: if run.success { 1.0 } else { 0.0 },
                        tasks_done: run.steps_completed,
                        tasks_total: workflow.steps.len(),
                        cost: 0.0,
                        elapsed: format!(
                            "{}s",
                            (run.completed_at - run.started_at).num_seconds().max(0)
                        ),
                    })
                })
                .collect();

            self.agents_data.workflow_source_dir = hive_agents::USER_WORKFLOW_DIR.to_string();
            self.agents_data.workflow_hint = Some(format!(
                "{} workflows loaded ({} active)",
                automation.workflow_count(),
                automation.active_count()
            ));
        }
    }

    fn workflow_command_preview(
        workflow: &hive_agents::automation::Workflow,
    ) -> Vec<String> {
        workflow
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                hive_agents::automation::ActionType::RunCommand { command } => {
                    Some(command.to_string())
                }
                _ => None,
            })
            .collect()
    }

    fn trigger_label(trigger: &hive_agents::automation::TriggerType) -> String {
        match trigger {
            hive_agents::automation::TriggerType::ManualTrigger => "Manual".into(),
            hive_agents::automation::TriggerType::Schedule { cron } => {
                format!("Schedule ({cron})")
            }
            hive_agents::automation::TriggerType::FileChange { path } => {
                format!("File Change ({path})")
            }
            hive_agents::automation::TriggerType::WebhookReceived { event } => {
                format!("Webhook ({event})")
            }
            hive_agents::automation::TriggerType::OnMessage { pattern } => {
                format!("Message ({pattern})")
            }
            hive_agents::automation::TriggerType::OnError { source } => {
                format!("Error ({source})")
            }
        }
    }

    fn refresh_specs_data(&mut self, cx: &App) {
        use hive_ui_panels::panels::specs::SpecSummary;

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
        use hive_ui_panels::panels::assistant::{ActiveReminder, BriefingSummary};

        if cx.has_global::<AppAssistant>() {
            let svc = &cx.global::<AppAssistant>().0;
            let briefing = svc.daily_briefing_for_project(Some(&self.current_project_root));

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
                    due: match &r.trigger {
                        ReminderTrigger::At(at) => at.format("%Y-%m-%d %H:%M").to_string(),
                        ReminderTrigger::Recurring(expr) => {
                            format!("Recurring: {expr}")
                        }
                        ReminderTrigger::OnEvent(event) => {
                            format!("On event: {event}")
                        }
                    },
                    is_overdue: matches!(&r.trigger, ReminderTrigger::At(at) if *at <= Utc::now()),
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
            working_directory: Some(self.current_project_root.to_string_lossy().to_string()),
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
    fn handle_send_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
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

        // 3. Build tool definitions from the built-in tool registry.
        let agent_defs = hive_agents::tool_use::builtin_tool_definitions();
        let tool_defs: Vec<AiToolDefinition> = agent_defs
            .into_iter()
            .map(|d| AiToolDefinition {
                name: d.name,
                description: d.description,
                input_schema: d.input_schema,
            })
            .collect();

        // 4. Extract provider + request from the global (sync — no await).
        let stream_setup: Option<(Arc<dyn AiProvider>, ChatRequest)> = if cx
            .has_global::<AppAiService>()
        {
            cx.global::<AppAiService>()
                .0
                .prepare_stream(ai_messages, &model, None, Some(tool_defs))
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

        // 5. Spawn async: call provider.stream_chat, then attach with tool loop.
        let chat_svc = self.chat_service.downgrade();
        let model_for_attach = model.clone();
        let provider_for_loop = provider.clone();
        let request_for_loop = request.clone();

        let task = cx.spawn(async move |_this, app: &mut AsyncApp| {
            match provider.stream_chat(&request).await {
                Ok(rx) => {
                    let _ = chat_svc.update(app, |svc, cx| {
                        svc.attach_tool_stream(
                            rx,
                            model_for_attach,
                            provider_for_loop,
                            request_for_loop,
                            cx,
                        );
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

        self.status_bar.active_project = self.project_label();

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

        // -- Discovery: periodic scan + connectivity update --
        self.maybe_trigger_discovery_scan(cx);
        self.sync_connectivity(cx);
    }

    /// Trigger a discovery scan every 30 seconds (non-blocking).
    ///
    /// Runs the actual HTTP probing on a background OS thread with its own Tokio
    /// runtime (reqwest requires Tokio, but GPUI uses a smol-based executor).
    /// On the next `sync_status_bar()` tick the completion flag is checked and
    /// the UI is updated with any newly discovered models.
    fn maybe_trigger_discovery_scan(&mut self, cx: &mut Context<Self>) {
        // Check if a previous scan just finished.
        if self.discovery_scan_pending {
            if let Some(flag) = &self.discovery_done_flag {
                if flag.load(std::sync::atomic::Ordering::Acquire) {
                    self.discovery_scan_pending = false;
                    self.discovery_done_flag = None;
                    // Refresh UI with discovered models.
                    if cx.has_global::<AppAiService>() {
                        if let Some(d) = cx.global::<AppAiService>().0.discovery() {
                            let models = d.snapshot().all_models();
                            self.settings_view.update(cx, |settings, cx| {
                                settings.refresh_local_models(models, cx);
                            });
                        }
                    }
                    cx.notify();
                }
            }
            return;
        }

        let should_scan = match self.last_discovery_scan {
            None => true,
            Some(t) => t.elapsed() >= std::time::Duration::from_secs(30),
        };
        if !should_scan {
            return;
        }

        let discovery = if cx.has_global::<AppAiService>() {
            cx.global::<AppAiService>().0.discovery().cloned()
        } else {
            None
        };

        let Some(discovery) = discovery else { return };

        self.discovery_scan_pending = true;
        self.last_discovery_scan = Some(std::time::Instant::now());

        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.discovery_done_flag = Some(Arc::clone(&done));

        std::thread::spawn(move || {
            discovery.scan_all_blocking();
            done.store(true, std::sync::atomic::Ordering::Release);
        });
    }

    /// Update status bar connectivity based on registered + discovered providers.
    fn sync_connectivity(&mut self, cx: &App) {
        if !cx.has_global::<AppAiService>() {
            return;
        }
        let ai = &cx.global::<AppAiService>().0;
        let has_cloud = ai.available_providers().iter().any(|p| {
            matches!(
                p,
                hive_ai::types::ProviderType::Anthropic
                    | hive_ai::types::ProviderType::OpenAI
                    | hive_ai::types::ProviderType::OpenRouter
                    | hive_ai::types::ProviderType::Google
                    | hive_ai::types::ProviderType::Groq
                    | hive_ai::types::ProviderType::HuggingFace
            )
        });
        let has_local = ai
            .discovery()
            .map(|d| d.snapshot().any_online())
            .unwrap_or(false);

        self.status_bar.connectivity = match (has_cloud, has_local) {
            (true, _) => ConnectivityDisplay::Online,
            (false, true) => ConnectivityDisplay::LocalOnly,
            (false, false) => ConnectivityDisplay::Offline,
        };
    }

    // -- Rendering -----------------------------------------------------------

    fn render_active_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.sidebar.active_panel == Panel::Chat {
            return self.render_chat_cached(cx);
        }
        let theme = &self.theme;
        match self.sidebar.active_panel {
            Panel::Chat => unreachable!(),
            Panel::History => HistoryPanel::render(&self.history_data, theme).into_any_element(),
            Panel::Files => FilesPanel::render(&self.files_data, theme).into_any_element(),
            Panel::Kanban => KanbanPanel::render(&self.kanban_data, theme).into_any_element(),
            Panel::Monitor => MonitorPanel::render(&self.monitor_data, theme).into_any_element(),
            Panel::Logs => LogsPanel::render(&self.logs_data, theme).into_any_element(),
            Panel::Costs => CostsPanel::render(&self.cost_data, theme).into_any_element(),
            Panel::Review => ReviewPanel::render(&self.review_data, theme).into_any_element(),
            Panel::Skills => SkillsPanel::render(&self.skills_data, theme).into_any_element(),
            Panel::Routing => RoutingPanel::render(&self.routing_data, theme).into_any_element(),
            Panel::TokenLaunch => {
                TokenLaunchPanel::render(&self.token_launch_data, theme).into_any_element()
            }
            Panel::Specs => SpecsPanel::render(&self.specs_data, theme).into_any_element(),
            Panel::Agents => AgentsPanel::render(&self.agents_data, theme).into_any_element(),
            Panel::Shield => ShieldPanel::render(&self.shield_data, theme).into_any_element(),
            Panel::Learning => LearningPanel::render(&self.learning_data, theme).into_any_element(),
            Panel::Assistant => {
                AssistantPanel::render(&self.assistant_data, theme).into_any_element()
            }
            Panel::Settings => self.settings_view.clone().into_any_element(),
            Panel::Help => HelpPanel::render(theme).into_any_element(),
        }
    }

    /// Render the chat panel using cached display data.
    ///
    /// Syncs `CachedChatData` from `ChatService` only when the generation
    /// counter has changed, then renders from the cached `DisplayMessage`
    /// vec and pre-parsed markdown IR.
    fn render_chat_cached(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let svc = self.chat_service.read(cx);

        // Rebuild display messages only when the service has mutated.
        sync_chat_cache(&mut self.cached_chat_data, svc);

        let streaming_content = svc.streaming_content().to_string();
        let is_streaming = svc.is_streaming();
        let current_model = svc.current_model().to_string();

        ChatPanel::render_cached(
            &mut self.cached_chat_data,
            &streaming_content,
            is_streaming,
            &current_model,
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
        self.cached_chat_data.markdown_cache.clear();
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
        self.cached_chat_data.markdown_cache.clear();
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

    // -- Agents panel handlers -----------------------------------------------

    fn handle_agents_reload_workflows(
        &mut self,
        _action: &AgentsReloadWorkflows,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppAutomation>() {
            return;
        }

        let workspace_root = std::env::current_dir().unwrap_or_default();
        let report = {
            let automation = &mut cx.global_mut::<AppAutomation>().0;
            automation.ensure_builtin_workflows();
            automation.reload_user_workflows(&workspace_root)
        };

        info!(
            "Agents: reloaded workflows (loaded={}, failed={}, skipped={})",
            report.loaded, report.failed, report.skipped
        );

        if cx.has_global::<AppNotifications>() {
            let msg = format!(
                "Reloaded workflows: {} loaded, {} failed, {} skipped",
                report.loaded, report.failed, report.skipped
            );
            let notif_type = if report.failed > 0 {
                NotificationType::Warning
            } else {
                NotificationType::Success
            };
            cx.global_mut::<AppNotifications>()
                .0
                .push(AppNotification::new(notif_type, msg).with_title("Workflow Reload"));
        }

        for error in report.errors {
            warn!("Workflow load error: {error}");
        }

        self.refresh_agents_data(cx);
        cx.notify();
    }

    fn handle_agents_run_workflow(
        &mut self,
        action: &AgentsRunWorkflow,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<AppAutomation>() {
            return;
        }

        let Some(workflow) = self.make_workflow_for_run(action, cx) else {
            return;
        };

        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>()
                .0
                .push(AppNotification::new(
                    NotificationType::Info,
                    format!(
                        "Running workflow '{}' ({} step(s)) from {} in {}",
                        workflow.id,
                        workflow.steps.len(),
                        if action.source.is_empty() {
                            "manual trigger"
                        } else {
                            action.source.as_str()
                        },
                        self.current_project_root.display()
                    ),
                ));
        }

        let working_dir = self
            .current_project_root
            .clone()
            .canonicalize()
            .unwrap_or_else(|_| self.current_project_root.clone());
        let workflow_for_thread = workflow.clone();
        let run_result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let run_result_for_thread = std::sync::Arc::clone(&run_result);

        // Execute on a background OS thread so tokio process execution works
        // regardless of the UI executor.
        std::thread::spawn(move || {
            let result =
                hive_agents::automation::AutomationService::execute_run_commands_blocking(
                    &workflow_for_thread,
                    working_dir,
                );
            *run_result_for_thread.lock().unwrap() = Some(result);
        });

        let run_result_for_ui = std::sync::Arc::clone(&run_result);
        let workflow_id_for_ui = workflow.id.clone();

        cx.spawn(async move |this, app: &mut AsyncApp| {
            // Poll until the thread writes the result.
            loop {
                if let Some(result) = run_result_for_ui.lock().unwrap().take() {
                    let _ = this.update(app, |this, cx| {
                        match result {
                            Ok(run) => {
                                let _ = cx.global_mut::<AppAutomation>().0.record_run(
                                    &run.workflow_id,
                                    run.success,
                                    run.steps_completed,
                                    run.error.clone(),
                                );

                                if cx.has_global::<AppNotifications>() {
                                    let notif_type = if run.success {
                                        NotificationType::Success
                                    } else {
                                        NotificationType::Error
                                    };
                                    let msg = if run.success {
                                        format!(
                                            "Workflow '{}' completed ({} steps)",
                                            run.workflow_id, run.steps_completed
                                        )
                                    } else {
                                        format!(
                                            "Workflow '{}' failed after {} step(s)",
                                            run.workflow_id, run.steps_completed
                                        )
                                    };
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(notif_type, msg).with_title(
                                            if run.success {
                                                "Workflow Complete"
                                            } else {
                                                "Workflow Failed"
                                            },
                                        ),
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("Agents: workflow run error ({workflow_id_for_ui}): {e}");
                                if cx.has_global::<AppNotifications>() {
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(
                                            NotificationType::Error,
                                            format!("Workflow '{workflow_id_for_ui}' failed: {e}"),
                                        )
                                        .with_title("Workflow Run Failed"),
                                    );
                                }
                            }
                        }

                        this.refresh_agents_data(cx);
                        cx.notify();
                    });
                    break;
                }

                app.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
            }
        })
        .detach();
    }

    fn make_workflow_for_run(
        &self,
        action: &AgentsRunWorkflow,
        cx: &App,
    ) -> Option<hive_agents::automation::Workflow> {
        if !cx.has_global::<AppAutomation>() {
            return None;
        }

        let requested_id = if action.workflow_id.trim().is_empty() {
            hive_agents::automation::BUILTIN_DOGFOOD_WORKFLOW_ID.to_string()
        } else {
            action.workflow_id.clone()
        };

        let automation = &cx.global::<AppAutomation>().0;
        let workflow = automation
            .clone_workflow(&requested_id)
            .or_else(|| automation.clone_workflow(hive_agents::automation::BUILTIN_DOGFOOD_WORKFLOW_ID))
            .or_else(|| Some(Self::fallback_workflow(&requested_id)));

        let Some(mut workflow) = workflow else {
            warn!(
                "Agents: unable to resolve workflow '{requested_id}' for planned execution"
            );
            return None;
        };

        let instruction = action.instruction.trim();
        if !instruction.is_empty() {
            let planned_steps =
                self.workflow_steps_from_instruction(instruction, &action.source, &action.source_id, cx);
            if !planned_steps.is_empty() {
                workflow.steps = planned_steps;
                workflow.name = if action.source.is_empty() {
                    "Planned Workflow".to_string()
                } else if action.source_id.is_empty() {
                    format!("Planned Workflow ({})", action.source)
                } else {
                    format!("Planned Workflow ({}:{})", action.source, action.source_id)
                };
                workflow.description = format!(
                    "Planned execution for {} {}",
                    if action.source.is_empty() {
                        "manual action"
                    } else {
                        action.source.as_str()
                    },
                    if action.source_id.is_empty() {
                        "request"
                    } else {
                        action.source_id.as_str()
                    }
                );
            }
        }

        if workflow.steps.is_empty() {
            workflow.steps = self.fallback_workflow_steps();
        }

        Some(workflow)
    }

    fn workflow_steps_from_instruction(
        &self,
        instruction: &str,
        source: &str,
        source_id: &str,
        cx: &App,
    ) -> Vec<hive_agents::automation::WorkflowStep> {
        let explicit = Self::extract_explicit_commands(instruction);
        let mut commands = if explicit.is_empty() {
            self.extract_keyword_commands(instruction)
                .into_iter()
                .chain(self.extract_source_commands(source, source_id, cx))
                .collect::<Vec<_>>()
        } else {
            explicit
        };

        commands = Self::dedupe_preserve_order(commands);
        if commands.is_empty() {
            commands = self.fallback_workflow_commands();
        }

        commands
            .into_iter()
            .enumerate()
            .map(|(idx, command)| hive_agents::automation::WorkflowStep {
                id: format!("runtime:{idx}"),
                name: format!("Run command {idx}"),
                action: hive_agents::automation::ActionType::RunCommand { command },
                conditions: Vec::new(),
                timeout_secs: Some(900),
                retry_count: 0,
            })
            .collect()
    }

    fn extract_explicit_commands(instruction: &str) -> Vec<String> {
        let mut commands = Vec::new();
        let mut in_fence = false;

        for line in instruction.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }

            if in_fence {
                Self::add_command_if_valid(line, &mut commands);
                continue;
            }

            let mut remaining = line;
            while let Some(start) = remaining.find('`') {
                let after = &remaining[start + 1..];
                let Some(end) = after.find('`') else {
                    break;
                };
                let candidate = &after[..end];
                Self::add_command_if_valid(candidate, &mut commands);
                remaining = &after[end + 1..];
            }

            if let Some((prefix, rest)) = line.split_once(':') {
                let normalized = prefix.trim().to_ascii_lowercase();
                if matches!(
                    normalized.as_str(),
                    "run" | "command" | "run command" | "execute"
                ) {
                    Self::add_command_if_valid(rest, &mut commands);
                    continue;
                }
            }

            Self::add_command_if_valid(line, &mut commands);
        }

        commands
    }

    fn extract_keyword_commands(&self, instruction: &str) -> Vec<String> {
        let lower = instruction.to_lowercase();
        let mut commands = Vec::new();

        if lower.contains("build") {
            commands.push("cargo check --quiet".to_string());
        }

        if lower.contains("test") {
            commands.push("cargo test --quiet -p hive_app".to_string());
        }

        if lower.contains("lint") || lower.contains("format") {
            commands.push("cargo fmt --check".to_string());
            commands.push("cargo clippy --all-targets -- -D warnings".to_string());
        }

        if lower.contains("release") {
            commands.push("cargo build --release".to_string());
        }

        if lower.contains("docs") {
            commands.push("cargo doc --no-deps --all-features".to_string());
        }

        if lower.contains("status") {
            commands.push("git status --short".to_string());
        }

        if lower.contains("diff") {
            commands.push("git diff --stat".to_string());
        }

        Self::dedupe_preserve_order(commands)
    }

    fn extract_source_commands(&self, source: &str, source_id: &str, cx: &App) -> Vec<String> {
        let source = source.to_lowercase();
        let mut commands = Vec::new();

        if source == "spec" && !source_id.is_empty() && cx.has_global::<AppSpecs>() {
            if let Some(spec) = cx.global::<AppSpecs>().0.specs.get(source_id) {
                if spec.entry_count() == 0 || spec.checked_count() < spec.entry_count() {
                    commands.push("cargo check --quiet".to_string());
                }
                commands.push("cargo test --quiet -p hive_app".to_string());
            }
        }

        if source == "kanban-task" && !source_id.is_empty() {
            let task_id: u64 = source_id.parse().unwrap_or(0);
            if task_id > 0 {
                for col in &self.kanban_data.columns {
                    if let Some(task) = col.tasks.iter().find(|task| task.id == task_id) {
                        let title = task.title.to_lowercase();
                        let desc = task.description.to_lowercase();
                        if title.contains("build") || desc.contains("build") {
                            commands.push("cargo check --quiet".to_string());
                        }
                        if title.contains("test") || desc.contains("test") {
                            commands.push("cargo test --quiet -p hive_app".to_string());
                        }
                        if title.contains("lint") || desc.contains("lint") {
                            commands.push("cargo fmt --check".to_string());
                            commands.push("cargo clippy --all-targets -- -D warnings".to_string());
                        }
                        break;
                    }
                }
            }
        }

        Self::dedupe_preserve_order(commands)
    }

    fn add_command_if_valid(raw: &str, out: &mut Vec<String>) {
        let Some(command) = Self::normalize_command(raw) else {
            return;
        };
        out.push(command);
    }

    fn normalize_command(raw: &str) -> Option<String> {
        let command = raw
            .trim()
            .trim_matches(['"', '\'', '`'])
            .trim_end_matches(';')
            .trim();
        if command.is_empty() || !Self::is_command_like(command) {
            return None;
        }
        Some(command.to_string())
    }

    fn is_command_like(text: &str) -> bool {
        let lower = text.to_lowercase();
        const PREFIXES: [&str; 11] = [
            "cargo ",
            "git ",
            "npm ",
            "pnpm ",
            "yarn ",
            "make ",
            "python ",
            "pytest ",
            "cargo.exe ",
            "./",
            "bash ",
        ];
        PREFIXES.iter().any(|prefix| lower.starts_with(prefix))
            || lower == "cargo"
            || lower == "git"
    }

    fn dedupe_preserve_order(commands: Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        commands
            .into_iter()
            .filter(|command| seen.insert(command.clone()))
            .collect()
    }

    fn fallback_workflow(workflow_id: &str) -> hive_agents::automation::Workflow {
        Self::fallback_workflow_with_id(workflow_id)
    }

    fn fallback_workflow_with_id(workflow_id: &str) -> hive_agents::automation::Workflow {
        let now = chrono::Utc::now();
        hive_agents::automation::Workflow {
            id: workflow_id.to_string(),
            name: "Local Build Check".to_string(),
            description: "Fallback local validation loop.".to_string(),
            trigger: hive_agents::automation::TriggerType::ManualTrigger,
            steps: Self::fallback_workflow_steps_static(),
            status: hive_agents::automation::WorkflowStatus::Active,
            created_at: now,
            updated_at: now,
            last_run: None,
            run_count: 0,
        }
    }

    fn fallback_workflow_steps(&self) -> Vec<hive_agents::automation::WorkflowStep> {
        Self::fallback_workflow_steps_static()
    }

    fn fallback_workflow_steps_static() -> Vec<hive_agents::automation::WorkflowStep> {
        vec![
            hive_agents::automation::WorkflowStep {
                id: "fallback:check".to_string(),
                name: "Cargo check".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "cargo check --quiet".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(900),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:test".to_string(),
                name: "Cargo test".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "cargo test --quiet -p hive_app".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(1200),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:status".to_string(),
                name: "Git status".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "git status --short".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(120),
                retry_count: 0,
            },
            hive_agents::automation::WorkflowStep {
                id: "fallback:diff".to_string(),
                name: "Git diff".to_string(),
                action: hive_agents::automation::ActionType::RunCommand {
                    command: "git diff --stat".to_string(),
                },
                conditions: Vec::new(),
                timeout_secs: Some(120),
                retry_count: 0,
            },
        ]
    }

    fn fallback_workflow_commands(&self) -> Vec<String> {
        Self::fallback_workflow_steps_static()
            .into_iter()
            .filter_map(|step| match step.action {
                hive_agents::automation::ActionType::RunCommand { command } => Some(command),
                _ => None,
            })
            .collect()
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
            self.apply_project_context(&parent, cx);
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
        self.apply_project_context(&path, cx);
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
            self.apply_project_context(&new_path, cx);
            self.files_data = FilesData::from_path(&new_path);
        } else {
            let file_path = self.files_data.current_path.join(&action.name);
            // Security: canonicalize and validate path stays within current_path
            // to prevent path traversal before passing to OS shell commands.
            let file_path = match file_path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    error!("Files: cannot resolve path: {e}");
                    return;
                }
            };
            let base = match self.files_data.current_path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    error!("Files: cannot resolve base path: {e}");
                    return;
                }
            };
            if !file_path.starts_with(&base) {
                error!("Files: path traversal blocked: {}", file_path.display());
                return;
            }
            info!("Files: open file {}", file_path.display());
            self.files_data.selected_file = Some(action.name.clone());
            // Open in default system editor, validating the launch command.
            let command_string = if cfg!(target_os = "windows") {
                format!("cmd /C start \"\" \"{}\"", file_path.to_string_lossy())
            } else if cfg!(target_os = "macos") {
                format!("open \"{}\"", file_path.to_string_lossy())
            } else {
                format!("xdg-open \"{}\"", file_path.to_string_lossy())
            };
            if cx.has_global::<AppSecurity>() {
                if let Err(e) = cx.global::<AppSecurity>().0.check_command(&command_string) {
                    error!("Files: blocked open command: {e}");
                    self.push_notification(
                        cx,
                        NotificationType::Error,
                        "Files",
                        format!("Blocked file open command: {e}"),
                    );
                    return;
                }
            }

            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", &file_path.to_string_lossy()])
                .spawn();
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open").arg(&file_path).spawn();
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open")
                .arg(&file_path)
                .spawn();
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
        // Security: canonicalize and validate target stays within current_path
        // to prevent path traversal attacks (e.g. action.name = "../../etc").
        let target = match target.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                error!("Files: cannot resolve path: {e}");
                return;
            }
        };
        let base = match self.files_data.current_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                error!("Files: cannot resolve base path: {e}");
                return;
            }
        };
        if !target.starts_with(&base) {
            error!("Files: path traversal blocked: {}", target.display());
            return;
        }
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
                self.cached_chat_data.markdown_cache.clear();
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
        use hive_ui_panels::panels::kanban::{KanbanTask, Priority};
        info!("Kanban: add task");
        let task = KanbanTask {
            id: self
                .kanban_data
                .columns
                .iter()
                .map(|c| c.tasks.len() as u64)
                .sum::<u64>()
                + 1,
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

    fn push_notification(
        &self,
        cx: &mut Context<Self>,
        kind: NotificationType,
        title: &str,
        message: impl Into<String>,
    ) {
        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>()
                .0
                .push(AppNotification::new(kind, message).with_title(title));
        }
    }

    fn run_checked_git_command(
        &self,
        cx: &Context<Self>,
        args: &[&str],
        security_check: &str,
    ) -> Result<std::process::Output, String> {
        if cx.has_global::<AppSecurity>() {
            cx.global::<AppSecurity>().0.check_command(security_check)?;
        }

        std::process::Command::new("git")
            .args(args)
            .output()
            .map_err(|e| format!("Failed to run git {}: {e}", args.join(" ")))
    }

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
        use hive_ui_panels::panels::logs::LogLevel;
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
        cx: &mut Context<Self>,
    ) {
        info!("Costs: export CSV");
        let Some(csv) = cx
            .has_global::<AppAiService>()
            .then(|| cx.global::<AppAiService>().0.cost_tracker().export_csv())
        else {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Cost Export",
                "No cost tracker available.",
            );
            return;
        };

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let export_dir = HiveConfig::base_dir()
            .map(|d| d.join("exports"))
            .unwrap_or_else(|_| PathBuf::from(".hive/exports"));
        let export_path = export_dir.join(format!("costs-{timestamp}.csv"));

        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(&export_dir)?;
            std::fs::write(&export_path, csv)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Cost Export",
                    format!("Exported CSV to {}", export_path.display()),
                );
            }
            Err(e) => {
                error!("Costs: failed to export CSV: {e}");
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Cost Export",
                    format!("Failed to export CSV: {e}"),
                );
            }
        }
    }

    fn handle_costs_reset_today(
        &mut self,
        _action: &CostsResetToday,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Costs: reset today");
        if cx.has_global::<AppAiService>() {
            cx.global_mut::<AppAiService>()
                .0
                .cost_tracker_mut()
                .reset_today();
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
        match self.run_checked_git_command(cx, &["add", "-A"], "git add -A") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_cwd();
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git add -A failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    fn handle_review_unstage_all(
        &mut self,
        _action: &ReviewUnstageAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: unstage all");
        match self.run_checked_git_command(cx, &["reset", "HEAD"], "git reset HEAD") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_cwd();
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git reset HEAD failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    fn handle_review_commit(
        &mut self,
        _action: &ReviewCommit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: commit");
        let staged = self.review_data.staged_count;
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
        let message = if staged > 0 {
            format!("chore(review): apply {staged} staged change(s) ({timestamp})")
        } else {
            format!("chore(review): snapshot commit ({timestamp})")
        };

        match self.run_checked_git_command(cx, &["commit", "-m", &message], "git commit -m") {
            Ok(output) if output.status.success() => {
                let commit_hash = self
                    .run_checked_git_command(
                        cx,
                        &["rev-parse", "--short", "HEAD"],
                        "git rev-parse HEAD",
                    )
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "unknown".to_string());

                self.review_data = ReviewData::from_cwd();
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Review",
                    format!("Created commit {commit_hash}"),
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let msg = if !stderr.trim().is_empty() {
                    stderr.trim().to_string()
                } else if !stdout.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    "git commit failed".to_string()
                };
                self.push_notification(cx, NotificationType::Warning, "Review", msg);
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    fn handle_review_discard_all(
        &mut self,
        _action: &ReviewDiscardAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: discard all");
        match self.run_checked_git_command(cx, &["checkout", "--", "."], "git checkout -- .") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_cwd();
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git checkout -- . failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
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
        use hive_ui_panels::panels::routing::RoutingRule;
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
        use hive_ui_panels::panels::token_launch::WizardStep;
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
        use hive_ui_panels::panels::token_launch::ChainOption;
        info!("TokenLaunch: select chain {}", action.chain);
        self.token_launch_data.selected_chain = match action.chain.as_str() {
            "solana" => Some(ChainOption::Solana),
            "ethereum" => Some(ChainOption::Ethereum),
            "base" => Some(ChainOption::Base),
            _ => None,
        };

        if let Some(chain) = self.token_launch_data.selected_chain {
            self.token_launch_data.decimals = chain.default_decimals();
            self.token_launch_data.estimated_cost = Some(match chain {
                ChainOption::Solana => 0.05,
                ChainOption::Ethereum => 0.015,
                ChainOption::Base => 0.0001,
            });
        } else {
            self.token_launch_data.estimated_cost = None;
        }

        cx.notify();
    }

    fn handle_token_launch_deploy(
        &mut self,
        _action: &TokenLaunchDeploy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("TokenLaunch: deploy");
        use hive_ui_panels::panels::token_launch::DeployStatus;

        if self.token_launch_data.selected_chain.is_none() {
            self.token_launch_data.deploy_status =
                DeployStatus::Failed("Select a target chain before deploying.".to_string());
            cx.notify();
            return;
        }

        if self.token_launch_data.token_name.trim().is_empty()
            || self.token_launch_data.token_symbol.trim().is_empty()
            || self.token_launch_data.total_supply.trim().is_empty()
        {
            self.token_launch_data.deploy_status = DeployStatus::Failed(
                "Token name, symbol, and total supply are required.".to_string(),
            );
            cx.notify();
            return;
        }

        if self.token_launch_data.wallet_address.is_none() {
            self.token_launch_data.deploy_status =
                DeployStatus::Failed("Connect a wallet before deploying.".to_string());
            cx.notify();
            return;
        }

        self.token_launch_data.deploy_status = DeployStatus::Deploying;
        self.token_launch_data.deploy_status = DeployStatus::Failed(
            "On-chain deployment is not enabled in this build yet. Use the wizard to validate configuration, then deploy via backend blockchain APIs once enabled."
                .to_string(),
        );
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
                cfg.litellm_url = snapshot.litellm_url.clone();
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
            let key_pairs: &[(&str, &Option<String>)] = &[
                ("anthropic", &snapshot.anthropic_key),
                ("openai", &snapshot.openai_key),
                ("openrouter", &snapshot.openrouter_key),
                ("google", &snapshot.google_key),
                ("groq", &snapshot.groq_key),
                ("huggingface", &snapshot.huggingface_key),
                ("litellm", &snapshot.litellm_key),
                ("elevenlabs", &snapshot.elevenlabs_key),
                ("telnyx", &snapshot.telnyx_key),
            ];
            for (provider, key) in key_pairs {
                if let Some(k) = key {
                    if let Err(e) = config_mgr.set_api_key(provider, Some(k.clone())) {
                        warn!("Settings: failed to save {provider} API key: {e}");
                    }
                }
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

        // Render the active panel first (may require &mut self for cache updates).
        let active_panel_el = self.render_active_panel(cx);

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
            // Agents
            .on_action(cx.listener(Self::handle_agents_reload_workflows))
            .on_action(cx.listener(Self::handle_agents_run_workflow))
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
                            .child(active_panel_el)
                            // Chat input (only shown on Chat panel)
                            .when(active_panel == Panel::Chat, |el: Div| el.child(chat_input)),
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
            .w(px(196.0))
            .h_full()
            .bg(theme.bg_secondary)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px(theme.space_3)
                    .pt(theme.space_3)
                    .pb(theme.space_2)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("NAVIGATION"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .px(theme.space_2)
                    .pb(theme.space_2)
                    .gap(theme.space_3)
                    .child(render_sidebar_section(
                        "Core",
                        &[Panel::Chat, Panel::History, Panel::Files, Panel::Specs],
                        active,
                        theme,
                        cx,
                    ))
                    .child(render_sidebar_section(
                        "Build",
                        &[
                            Panel::Agents,
                            Panel::Kanban,
                            Panel::Review,
                            Panel::Skills,
                            Panel::Routing,
                            Panel::Learning,
                        ],
                        active,
                        theme,
                        cx,
                    ))
                    .child(render_sidebar_section(
                        "Observe",
                        &[Panel::Monitor, Panel::Logs, Panel::Costs, Panel::Shield],
                        active,
                        theme,
                        cx,
                    ))
                    .child(render_sidebar_section(
                        "Platform",
                        &[Panel::Assistant, Panel::TokenLaunch],
                        active,
                        theme,
                        cx,
                    )),
            )
            .child(
                div()
                    .px(theme.space_2)
                    .py(theme.space_2)
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(theme.space_1)
                            .child(render_sidebar_item(Panel::Settings, active, theme, cx))
                            .child(render_sidebar_item(Panel::Help, active, theme, cx)),
                    ),
            )
    }
}

fn render_sidebar_section(
    title: &'static str,
    panels: &[Panel],
    active: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .px(theme.space_2)
                .pb(px(2.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .children(
            panels
                .iter()
                .copied()
                .map(|panel| render_sidebar_item(panel, active, theme, cx)),
        )
        .into_any_element()
}

fn render_sidebar_item(
    panel: Panel,
    active: Panel,
    theme: &HiveTheme,
    cx: &mut Context<HiveWorkspace>,
) -> AnyElement {
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
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .w_full()
        .h(px(34.0))
        .px(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(bg)
        .border_l_2()
        .border_color(left_border)
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                info!("Sidebar click: {:?}", panel);
                this.switch_to_panel(panel, cx);
            }),
        )
        .child(Icon::new(panel.icon()).size_3p5().text_color(text_color))
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(text_color)
                .font_weight(if is_active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(panel.label()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Event emitted when clicking a sidebar panel.
#[derive(Debug, Clone)]
pub struct SwitchPanel(pub Panel);

impl EventEmitter<SwitchPanel> for HiveWorkspace {}

// ---------------------------------------------------------------------------
// Chat cache sync (bridges ChatService → CachedChatData across crate boundary)
// ---------------------------------------------------------------------------

fn sync_chat_cache(cache: &mut CachedChatData, svc: &ChatService) {
    let svc_gen = svc.generation();
    if svc_gen == cache.generation {
        return;
    }

    cache.display_messages.clear();
    cache.total_cost = 0.0;
    cache.total_tokens = 0;

    for msg in svc.messages() {
        if msg.role == crate::chat_service::MessageRole::Assistant && msg.content.is_empty() {
            continue;
        }
        let role = match msg.role {
            crate::chat_service::MessageRole::User => hive_ai::MessageRole::User,
            crate::chat_service::MessageRole::Assistant => hive_ai::MessageRole::Assistant,
            crate::chat_service::MessageRole::System => hive_ai::MessageRole::System,
            crate::chat_service::MessageRole::Error => hive_ai::MessageRole::Error,
            crate::chat_service::MessageRole::Tool => hive_ai::MessageRole::Tool,
        };
        let tool_calls = msg
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCallDisplay {
                        name: tc.name.clone(),
                        args: serde_json::to_string_pretty(&tc.input)
                            .unwrap_or_else(|_| tc.input.to_string()),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let display_msg = DisplayMessage {
            role,
            content: msg.content.clone(),
            thinking: None,
            model: msg.model.clone(),
            cost: msg.cost,
            tokens: msg.tokens.map(|(i, o)| (i + o) as u32),
            timestamp: msg.timestamp,
            show_thinking: false,
            tool_calls,
            tool_call_id: msg.tool_call_id.clone(),
        };
        if let Some(c) = display_msg.cost {
            cache.total_cost += c;
        }
        if let Some(t) = display_msg.tokens {
            cache.total_tokens += t;
        }
        cache.display_messages.push(display_msg);
    }

    cache.generation = svc_gen;
}
