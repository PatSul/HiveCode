#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::borrow::Cow;
use std::sync::mpsc;
use std::time::Duration;

use gpui::*;
use tracing::{error, info, warn};

use hive_ai::service::AiServiceConfig;
use hive_ai::tts::TtsProviderType;
use hive_ai::tts::service::TtsServiceConfig;
use hive_core::config::{ConfigManager, HiveConfig};
use hive_core::logging;
use hive_core::notifications::{AppNotification, NotificationType};
use hive_core::persistence::Database;
use hive_core::security::SecurityGateway;
use hive_core::updater::UpdateService;
use hive_ui::globals::{
    AppAiService, AppAssistant, AppAutomation, AppChannels, AppCli, AppConfig, AppDatabase,
    AppIde, AppLearning, AppMarketplace, AppMcpServer, AppNotifications, AppPersonas, AppRpcConfig,
    AppSecurity, AppShield, AppSkills, AppSpecs, AppTts, AppUpdater, AppWallets,
};
use hive_ui::workspace::{
    ClearChat, HiveWorkspace, NewConversation, SwitchPanel, SwitchToAgents, SwitchToChannels,
    SwitchToChat, SwitchToFiles, SwitchToHistory, SwitchToKanban, SwitchToLogs,
    SwitchToMonitor, SwitchToSpecs, SwitchToWorkflows,
};

const VERSION: &str = env!("HIVE_VERSION");

// ---------------------------------------------------------------------------
// Embedded assets (icons, images)
// ---------------------------------------------------------------------------

#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
struct Assets;

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::get(path).map(|f| f.data))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(p.to_string()))
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Tray global (prevents drop when run callback returns)
// ---------------------------------------------------------------------------

pub struct AppTray(pub Option<tray::TrayService>);
impl gpui::Global for AppTray {}

/// Walk up from `path` looking for a `.git` directory, returning the first
/// ancestor that contains one. Falls back to `path` itself if no git root is
/// found.
fn discover_git_root(path: std::path::PathBuf) -> std::path::PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
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

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(hive, [Quit, TogglePrivacy, OpenSettings]);

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

/// Initialize backend services and store them as GPUI globals.
fn init_services(cx: &mut App) -> anyhow::Result<()> {
    let config_manager =
        ConfigManager::new().inspect_err(|e| error!("Config manager init failed: {e}"))?;
    info!(
        "Config loaded (privacy_mode={})",
        config_manager.get().privacy_mode
    );
    cx.set_global(AppConfig(config_manager));

    cx.set_global(AppSecurity(SecurityGateway::new()));
    info!("SecurityGateway initialized");

    cx.set_global(AppNotifications(
        hive_core::notifications::NotificationStore::new(),
    ));

    // Build AI service from config (needed before wiring LearnerTierAdjuster).
    let config = cx.global::<AppConfig>().0.get().clone();
    let ai_config = AiServiceConfig {
        anthropic_api_key: config.anthropic_api_key.clone(),
        openai_api_key: config.openai_api_key.clone(),
        openrouter_api_key: config.openrouter_api_key.clone(),
        google_api_key: config.google_api_key.clone(),
        groq_api_key: config.groq_api_key.clone(),
        huggingface_api_key: config.huggingface_api_key.clone(),
        litellm_url: config.litellm_url.clone(),
        litellm_api_key: config.litellm_api_key.clone(),
        ollama_url: config.ollama_url.clone(),
        lmstudio_url: config.lmstudio_url.clone(),
        local_provider_url: config.local_provider_url.clone(),
        privacy_mode: config.privacy_mode,
        default_model: config.default_model.clone(),
        auto_routing: config.auto_routing,
    };
    cx.set_global(AppAiService(hive_ai::AiService::new(ai_config)));
    cx.global_mut::<AppAiService>().0.start_discovery();
    info!("AiService initialized");

    // Compute DB paths before the parallel section (HiveConfig::base_dir is cheap).
    let learning_db_str = HiveConfig::base_dir()
        .map(|d| d.join("learning.db"))
        .unwrap_or_else(|_| std::path::PathBuf::from("learning.db"))
        .to_string_lossy()
        .to_string();
    let assistant_db_str = HiveConfig::base_dir()
        .map(|d| d.join("assistant.db"))
        .unwrap_or_else(|_| std::path::PathBuf::from("assistant.db"))
        .to_string_lossy()
        .to_string();

    // Open all three databases in parallel — they are independent and each opens
    // its own SQLite connection.  `std::thread::scope` ensures the borrows of the
    // path strings are valid for the lifetime of the spawned threads.
    let (db_result, learning_result, assistant_result) = std::thread::scope(|s| {
        let db_handle = s.spawn(Database::open);
        let learn_handle = s.spawn(|| hive_learn::LearningService::open(&learning_db_str));
        let assist_handle = s.spawn(|| hive_assistant::AssistantService::open(&assistant_db_str));

        (
            db_handle.join().expect("Database::open thread panicked"),
            learn_handle
                .join()
                .expect("LearningService::open thread panicked"),
            assist_handle
                .join()
                .expect("AssistantService::open thread panicked"),
        )
    });

    // --- Register results with cx sequentially (cx is !Send) ---

    let db = db_result.inspect_err(|e| error!("Database open failed: {e}"))?;

    // Backfill: import any JSON conversations that aren't yet in SQLite,
    // including building their FTS5 search index.
    if let Ok(conv_dir) = HiveConfig::conversations_dir()
        && let Err(e) = db.backfill_from_json(&conv_dir)
    {
        warn!("JSON→SQLite backfill failed: {e}");
    }

    cx.set_global(AppDatabase(db));
    info!("Database opened");

    // Learning service
    match learning_result {
        Ok(learning) => {
            let learning = std::sync::Arc::new(learning);
            info!("LearningService initialized at {}", learning_db_str);

            // Wire the tier adjuster into the AI router so routing decisions
            // benefit from learned outcome data.
            let adjuster = hive_learn::LearnerTierAdjuster::new(std::sync::Arc::clone(&learning));
            cx.global_mut::<AppAiService>()
                .0
                .router_mut()
                .set_tier_adjuster(std::sync::Arc::new(adjuster));
            info!("LearnerTierAdjuster wired into ModelRouter");

            cx.set_global(AppLearning(learning));
        }
        Err(e) => {
            error!("LearningService init failed: {e}");
        }
    }

    // Privacy shield — default config.
    let shield = std::sync::Arc::new(hive_shield::HiveShield::new(
        hive_shield::ShieldConfig::default(),
    ));
    cx.set_global(AppShield(shield));
    info!("HiveShield initialized");

    // TTS service — build from config keys.
    let tts_config = TtsServiceConfig {
        default_provider: TtsProviderType::from_str_loose(&config.tts_provider)
            .unwrap_or(TtsProviderType::Qwen3),
        default_voice_id: config.tts_voice_id.clone(),
        speed: config.tts_speed,
        enabled: config.tts_enabled,
        auto_speak: config.tts_auto_speak,
        openai_api_key: config.openai_api_key.clone(),
        huggingface_api_key: config.huggingface_api_key.clone(),
        elevenlabs_api_key: config.elevenlabs_api_key.clone(),
        telnyx_api_key: config.telnyx_api_key.clone(),
    };
    let tts = std::sync::Arc::new(hive_ai::TtsService::new(tts_config));
    cx.set_global(AppTts(tts));
    info!("TTS service initialized");

    // Skills registry — built-in /commands.
    cx.set_global(AppSkills(hive_agents::skills::SkillsRegistry::new()));
    info!("SkillsRegistry initialized (built-in commands)");

    // Skill marketplace — install/remove community skills with security scanning.
    cx.set_global(AppMarketplace(hive_agents::SkillMarketplace::new()));
    info!("SkillMarketplace initialized");

    // Persona registry — built-in agent roles.
    cx.set_global(AppPersonas(hive_agents::personas::PersonaRegistry::new()));
    info!("PersonaRegistry initialized (6 built-in personas)");

    // Automation service — workflow engine.
    let workspace_root = discover_git_root(std::env::current_dir().unwrap_or_default());
    let mut automation = hive_agents::AutomationService::new();
    let workflow_report = automation.initialize_workflows(&workspace_root);
    if workflow_report.failed > 0 {
        for load_error in &workflow_report.errors {
            warn!("Workflow load error: {load_error}");
        }
    }
    cx.set_global(AppAutomation(automation));
    info!(
        "AutomationService initialized (loaded={}, failed={}, skipped={})",
        workflow_report.loaded, workflow_report.failed, workflow_report.skipped
    );

    // Built-in MCP tool server — file I/O, command exec, search, git.
    cx.set_global(AppMcpServer(hive_agents::mcp_server::McpServer::new(
        workspace_root,
    )));
    info!("McpServer initialized (6 built-in tools)");

    // Spec manager — project specifications.
    cx.set_global(AppSpecs(hive_agents::SpecManager::new()));
    info!("SpecManager initialized");

    // CLI service — built-in commands, doctor checks.
    cx.set_global(AppCli(hive_terminal::CliService::new()));
    info!("CliService initialized");

    // Assistant service
    match assistant_result {
        Ok(assistant) => {
            cx.set_global(AppAssistant(assistant));
            info!("AssistantService initialized");
        }
        Err(e) => {
            error!("AssistantService init failed: {e}");
        }
    }

    // Wallet store — load existing wallets or start empty.
    let wallet_path = HiveConfig::base_dir()
        .map(|d| d.join("wallets.enc"))
        .unwrap_or_else(|_| std::path::PathBuf::from("wallets.enc"));
    let wallets = if wallet_path.exists() {
        hive_blockchain::wallet_store::WalletStore::load_from_file(&wallet_path).unwrap_or_else(
            |e| {
                error!("WalletStore load failed: {e}");
                hive_blockchain::wallet_store::WalletStore::new()
            },
        )
    } else {
        hive_blockchain::wallet_store::WalletStore::new()
    };
    cx.set_global(AppWallets(wallets));
    info!("WalletStore initialized");

    // RPC config — default endpoints for EVM and Solana chains.
    cx.set_global(AppRpcConfig(
        hive_blockchain::rpc_config::RpcConfigStore::with_defaults(),
    ));
    info!("RpcConfigStore initialized");

    // IDE integration — workspace and file tracking.
    cx.set_global(AppIde(hive_integrations::ide::IdeIntegrationService::new()));
    info!("IdeIntegrationService initialized");

    // Channel store — AI agent messaging channels.
    let mut channel_store = hive_core::channels::ChannelStore::new();
    channel_store.ensure_default_channels();
    cx.set_global(AppChannels(channel_store));
    info!("ChannelStore initialized with default channels");

    // Auto-update service — checks GitHub releases for newer versions.
    let updater = UpdateService::new(VERSION);
    cx.set_global(AppUpdater(updater));
    info!("UpdateService initialized (current: v{VERSION})");

    Ok(())
}

/// Register global keyboard shortcuts and action handlers.
fn register_actions(cx: &mut App) {
    // macOS uses Cmd for shortcuts; all other platforms use Ctrl.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        // App-level actions
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-p", TogglePrivacy, None),
        // Chat actions
        KeyBinding::new("cmd-n", NewConversation, None),
        KeyBinding::new("cmd-l", ClearChat, None),
        // Panel switching: cmd-1..cmd-0 map to first 10 sidebar panels
        KeyBinding::new("cmd-1", SwitchToChat, None),
        KeyBinding::new("cmd-2", SwitchToHistory, None),
        KeyBinding::new("cmd-3", SwitchToFiles, None),
        KeyBinding::new("cmd-4", SwitchToSpecs, None),
        KeyBinding::new("cmd-5", SwitchToAgents, None),
        KeyBinding::new("cmd-6", SwitchToWorkflows, None),
        KeyBinding::new("cmd-7", SwitchToChannels, None),
        KeyBinding::new("cmd-8", SwitchToKanban, None),
        KeyBinding::new("cmd-9", SwitchToMonitor, None),
        KeyBinding::new("cmd-0", SwitchToLogs, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        // App-level actions
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("ctrl-p", TogglePrivacy, None),
        // Chat actions
        KeyBinding::new("ctrl-n", NewConversation, None),
        KeyBinding::new("ctrl-l", ClearChat, None),
        // Panel switching: ctrl-1..ctrl-0 map to first 10 sidebar panels
        KeyBinding::new("ctrl-1", SwitchToChat, None),
        KeyBinding::new("ctrl-2", SwitchToHistory, None),
        KeyBinding::new("ctrl-3", SwitchToFiles, None),
        KeyBinding::new("ctrl-4", SwitchToSpecs, None),
        KeyBinding::new("ctrl-5", SwitchToAgents, None),
        KeyBinding::new("ctrl-6", SwitchToWorkflows, None),
        KeyBinding::new("ctrl-7", SwitchToChannels, None),
        KeyBinding::new("ctrl-8", SwitchToKanban, None),
        KeyBinding::new("ctrl-9", SwitchToMonitor, None),
        KeyBinding::new("ctrl-0", SwitchToLogs, None),
    ]);

    cx.on_action(|_: &Quit, cx: &mut App| {
        info!("Quit action triggered");
        cx.quit();
    });

    cx.on_action(|_: &OpenSettings, _cx| {
        info!("OpenSettings action triggered");
    });

    cx.on_action(|_: &TogglePrivacy, cx: &mut App| {
        info!("TogglePrivacy action triggered");
        if cx.has_global::<AppConfig>() {
            let current = cx.global::<AppConfig>().0.get().privacy_mode;
            let _ = cx
                .global_mut::<AppConfig>()
                .0
                .update(|c| c.privacy_mode = !current);
            info!("Privacy mode toggled to {}", !current);
        }
    });
}

/// Build the main window options, restoring the saved window size if available.
fn window_options(cx: &App) -> WindowOptions {
    let session = hive_core::session::SessionState::load().unwrap_or_default();
    let (w, h) = match session.window_size {
        Some([w, h]) if w >= 400 && h >= 300 => (w as f32, h as f32),
        _ => (1280.0, 800.0),
    };

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(w), px(h)),
            cx,
        ))),
        titlebar: Some(gpui_component::TitleBar::title_bar_options()),
        ..Default::default()
    }
}

/// Update tray menu toggle text for current window visibility.
fn set_tray_window_visible(cx: &App, visible: bool) {
    if let Some(tray) = cx.global::<AppTray>().0.as_ref() {
        tray.set_visible(visible);
    }
}

/// Close all open windows while keeping the app/tray process alive.
fn hide_all_windows(cx: &mut App) {
    let windows = cx.windows();
    for handle in windows {
        let _ = handle.update(cx, |_, window, _| {
            window.remove_window();
        });
    }
    set_tray_window_visible(cx, false);
}

/// Platform-specific wording for where the background icon lives.
#[cfg(target_os = "macos")]
fn close_to_tray_target() -> &'static str {
    "menu bar"
}

#[cfg(target_os = "windows")]
fn close_to_tray_target() -> &'static str {
    "system tray"
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn close_to_tray_target() -> &'static str {
    "tray area"
}

fn handle_main_window_close(window: &mut Window, cx: &mut App) -> bool {
    // If there is no tray icon, just quit directly.
    if cx.global::<AppTray>().0.is_none() {
        return true;
    }

    // Always prompt the user: Quit, Minimize to tray, or Cancel.
    let detail = format!(
        "Would you like to quit Hive or minimize it to the {}?",
        close_to_tray_target()
    );
    let response = window.prompt(
        PromptLevel::Info,
        "Close Hive",
        Some(&detail),
        &["Quit Hive", "Minimize to Tray", "Cancel"],
        cx,
    );

    cx.spawn(async move |app: &mut AsyncApp| {
        if let Ok(choice) = response.await {
            let _ = app.update(|cx| match choice {
                0 => cx.quit(),
                1 => hide_all_windows(cx),
                _ => {} // Cancel — do nothing
            });
        }
    })
    .detach();

    // Return false to veto the platform close; the prompt handles the outcome.
    false
}

/// Open the main application window and wire close-to-tray behavior.
fn open_main_window(cx: &mut App) -> anyhow::Result<()> {
    cx.open_window(window_options(cx), |window, cx| {
        // Keep the app alive for background tasks when the user closes the
        // window (Alt+F4 / titlebar close / platform close request).
        // Returning `false` vetoes the platform close while we remove the
        // window ourselves so the taskbar button disappears.
        window.on_window_should_close(cx, handle_main_window_close);

        let workspace = cx.new(|cx| HiveWorkspace::new(window, cx));

        // Push the git-based version (from build.rs) into the status bar.
        workspace.update(cx, |ws, _cx| {
            ws.set_version(VERSION.to_string());
        });

        cx.subscribe(&workspace, |workspace, event: &SwitchPanel, cx| {
            workspace.update(cx, |ws, cx| {
                ws.set_active_panel(event.0);
                cx.notify();
            });
        })
        .detach();

        cx.new(|cx| gpui_component::Root::new(workspace.clone(), window, cx))
    })?;

    set_tray_window_visible(cx, true);
    info!("Hive v{VERSION} window opened");
    Ok(())
}

/// Post an error notification into the global store.
fn notify_error(cx: &mut App, message: impl Into<String>) {
    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>().0.push(
            AppNotification::new(NotificationType::Error, message).with_title("Startup Error"),
        );
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let _log_guard = logging::init_logging().expect("Failed to initialize logging");
    info!("Starting Hive v{VERSION}");

    HiveConfig::ensure_dirs().expect("Failed to create config directories");

    Application::new().with_assets(Assets).run(|cx| {
        gpui_component::init(cx);

        if let Err(e) = init_services(cx) {
            error!("Service initialization failed: {e:#}");
            notify_error(cx, format!("Failed to initialize services: {e}"));
        }

        register_actions(cx);

        // Keep tray label synchronized even when windows are closed by means
        // other than the tray event loop.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                set_tray_window_visible(cx, false);
            }
        })
        .detach();

        let (tray_tx, tray_rx) = mpsc::channel::<tray::TrayEvent>();

        // System tray — stored as a GPUI global to prevent drop.
        let tray = tray::try_create_tray(move |event| {
            let _ = tray_tx.send(event);
        });
        cx.set_global(AppTray(tray));

        // Poll tray events on the main thread and mutate GPUI state there.
        cx.spawn(async move |app: &mut AsyncApp| {
            loop {
                loop {
                    match tray_rx.try_recv() {
                        Ok(event) => {
                            let _ = app.update(|cx| {
                                info!("Tray event: {event:?}");
                                match event {
                                    tray::TrayEvent::ToggleVisibility => {
                                        if cx.windows().is_empty() {
                                            if let Err(e) = open_main_window(cx) {
                                                error!("Failed to open window from tray: {e:#}");
                                                notify_error(
                                                    cx,
                                                    format!(
                                                        "Failed to open window from tray: {e}"
                                                    ),
                                                );
                                            } else {
                                                cx.activate(true);
                                            }
                                        } else {
                                            hide_all_windows(cx);
                                        }
                                    }
                                    tray::TrayEvent::Quit => cx.quit(),
                                }
                            });
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }

                app.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
            }
        })
        .detach();

        open_main_window(cx).expect("Failed to open window");

        // Bring the app to the foreground and ensure macOS shows its dock icon.
        // Without this, running the binary directly (e.g. `cargo run`) may not
        // display the app in the dock.
        cx.activate(true);

        // Background update check — runs 5s after startup and every 4 hours.
        // The blocking HTTP call runs on an OS thread; results are polled on the
        // main thread to update the status bar.
        if cx.has_global::<AppConfig>() && cx.global::<AppConfig>().0.get().auto_update {
            let updater = cx.global::<AppUpdater>().0.clone();
            cx.spawn(async move |app: &mut AsyncApp| {
                // Wait 5 seconds before first check to avoid slowing startup.
                app.background_executor()
                    .timer(Duration::from_secs(5))
                    .await;

                loop {
                    // Run the blocking HTTP check on a background OS thread.
                    let updater_clone = updater.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let result = updater_clone.check_for_updates();
                        let _ = tx.send(result);
                    });

                    // Poll for the result.
                    let check_result = loop {
                        match rx.try_recv() {
                            Ok(result) => break result,
                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                app.background_executor()
                                    .timer(Duration::from_millis(500))
                                    .await;
                            }
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                break Err(anyhow::anyhow!("Update check thread died"));
                            }
                        }
                    };

                    match check_result {
                        Ok(Some(update_info)) => {
                            info!(
                                "Update available: v{} (release: {})",
                                update_info.version, update_info.release_url
                            );
                            let version = update_info.version.clone();
                            let _ = app.update(|cx| {
                                if cx.has_global::<AppNotifications>() {
                                    cx.global_mut::<AppNotifications>().0.push(
                                        AppNotification::new(
                                            NotificationType::Info,
                                            format!(
                                                "Hive v{version} is available. Click the update badge in the status bar to install."
                                            ),
                                        )
                                        .with_title("Update Available"),
                                    );
                                }
                            });
                        }
                        Ok(None) => {
                            info!("No updates available");
                        }
                        Err(e) => {
                            warn!("Update check failed: {e}");
                        }
                    }

                    // Re-check every 4 hours.
                    app.background_executor()
                        .timer(Duration::from_secs(4 * 60 * 60))
                        .await;
                }
            })
            .detach();
        }
    });
}
