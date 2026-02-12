#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::borrow::Cow;

use gpui::*;
use tracing::{error, info};

use hive_ai::service::AiServiceConfig;
use hive_ai::tts::service::TtsServiceConfig;
use hive_ai::tts::TtsProviderType;
use hive_core::config::{ConfigManager, HiveConfig};
use hive_core::logging;
use hive_core::notifications::{AppNotification, NotificationType};
use hive_core::persistence::Database;
use hive_core::security::SecurityGateway;
use hive_ui::globals::{
    AppAiService, AppAssistant, AppAutomation, AppCli, AppConfig, AppDatabase, AppIde,
    AppLearning, AppMarketplace, AppMcpServer, AppNotifications, AppPersonas, AppRpcConfig,
    AppSecurity, AppShield, AppSkills, AppSpecs, AppTts, AppWallets,
};
use hive_ui::workspace::{
    ClearChat, HiveWorkspace, NewConversation, SwitchPanel, SwitchToAgents, SwitchToChat,
    SwitchToCosts, SwitchToFiles, SwitchToHistory, SwitchToKanban, SwitchToLogs,
    SwitchToMonitor, SwitchToReview, SwitchToSpecs,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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

    cx.set_global(AppNotifications(hive_core::notifications::NotificationStore::new()));

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
        let db_handle = s.spawn(|| Database::open());
        let learn_handle = s.spawn(|| hive_learn::LearningService::open(&learning_db_str));
        let assist_handle =
            s.spawn(|| hive_assistant::AssistantService::open(&assistant_db_str));

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

    // Built-in MCP tool server — file I/O, command exec, search, git.
    let workspace_root = std::env::current_dir().unwrap_or_default();
    cx.set_global(AppMcpServer(hive_agents::mcp_server::McpServer::new(
        workspace_root,
    )));
    info!("McpServer initialized (6 built-in tools)");

    // Persona registry — built-in agent roles.
    cx.set_global(AppPersonas(hive_agents::personas::PersonaRegistry::new()));
    info!("PersonaRegistry initialized (6 built-in personas)");

    // Automation service — workflow engine.
    cx.set_global(AppAutomation(hive_agents::AutomationService::new()));
    info!("AutomationService initialized");

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
        hive_blockchain::wallet_store::WalletStore::load_from_file(&wallet_path)
            .unwrap_or_else(|e| {
                error!("WalletStore load failed: {e}");
                hive_blockchain::wallet_store::WalletStore::new()
            })
    } else {
        hive_blockchain::wallet_store::WalletStore::new()
    };
    cx.set_global(AppWallets(wallets));
    info!("WalletStore initialized");

    // RPC config — default endpoints for EVM and Solana chains.
    cx.set_global(AppRpcConfig(hive_blockchain::rpc_config::RpcConfigStore::with_defaults()));
    info!("RpcConfigStore initialized");

    // IDE integration — workspace and file tracking.
    cx.set_global(AppIde(hive_integrations::ide::IdeIntegrationService::new()));
    info!("IdeIntegrationService initialized");

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
        KeyBinding::new("cmd-6", SwitchToKanban, None),
        KeyBinding::new("cmd-7", SwitchToMonitor, None),
        KeyBinding::new("cmd-8", SwitchToLogs, None),
        KeyBinding::new("cmd-9", SwitchToCosts, None),
        KeyBinding::new("cmd-0", SwitchToReview, None),
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
        KeyBinding::new("ctrl-6", SwitchToKanban, None),
        KeyBinding::new("ctrl-7", SwitchToMonitor, None),
        KeyBinding::new("ctrl-8", SwitchToLogs, None),
        KeyBinding::new("ctrl-9", SwitchToCosts, None),
        KeyBinding::new("ctrl-0", SwitchToReview, None),
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
            let _ = cx.global_mut::<AppConfig>().0.update(|c| c.privacy_mode = !current);
            info!("Privacy mode toggled to {}", !current);
        }
    });
}

/// Build the main window options with a centered 1280x800 frame.
fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1280.0), px(800.0)),
            cx,
        ))),
        titlebar: Some(gpui_component::TitleBar::title_bar_options()),
        ..Default::default()
    }
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

        // System tray — stored as a GPUI global to prevent drop.
        let tray = tray::try_create_tray(|event| {
            info!("Tray event: {event:?}");
            if event == tray::TrayEvent::Quit {
                std::process::exit(0);
            }
        });
        cx.set_global(AppTray(tray));

        cx.open_window(window_options(cx), |window, cx| {
            let workspace = cx.new(|cx| HiveWorkspace::new(window, cx));

            cx.subscribe(&workspace, |workspace, event: &SwitchPanel, cx| {
                workspace.update(cx, |ws, cx| {
                    ws.set_active_panel(event.0);
                    cx.notify();
                });
            })
            .detach();

            cx.new(|cx| gpui_component::Root::new(workspace.clone(), window, cx))
        })
        .expect("Failed to open window");

        info!("Hive v{VERSION} window opened");
    });
}
