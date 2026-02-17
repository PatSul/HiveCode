//! GPUI Global wrappers for backend services.
//!
//! These are defined in `hive_ui` (not `hive_app`) so that both the workspace
//! (which reads them) and the bootstrap code (which sets them) share the same
//! types.  Each wrapper is a newtype around the service it wraps.

use std::sync::Arc;

use gpui::Global;

use hive_agents::automation::AutomationService;
use hive_agents::mcp_server::McpServer;
use hive_agents::personas::PersonaRegistry;
use hive_agents::skill_marketplace::SkillMarketplace;
use hive_agents::skills::SkillsRegistry;
use hive_agents::specs::SpecManager;
use hive_ai::service::AiService;
use hive_ai::tts::service::TtsService;
use hive_assistant::AssistantService;
use hive_blockchain::rpc_config::RpcConfigStore;
use hive_blockchain::wallet_store::WalletStore;
use hive_core::channels::ChannelStore;
use hive_network::HiveNode;
use hive_core::config::ConfigManager;
use hive_core::notifications::NotificationStore;
use hive_core::persistence::Database;
use hive_core::security::SecurityGateway;
use hive_core::updater::UpdateService;
use hive_integrations::ide::IdeIntegrationService;
use hive_learn::LearningService;
use hive_shield::HiveShield;
use hive_terminal::CliService;

/// Global wrapper for the AI service (providers, routing, cost tracking).
pub struct AppAiService(pub AiService);
impl Global for AppAiService {}

/// Global wrapper for the configuration manager (hot-reload, read/write).
pub struct AppConfig(pub ConfigManager);
impl Global for AppConfig {}

/// Global wrapper for the SQLite database (conversations, memory, costs).
pub struct AppDatabase(pub Database);
impl Global for AppDatabase {}

/// Global wrapper for in-app notification storage.
pub struct AppNotifications(pub NotificationStore);
impl Global for AppNotifications {}

/// Global wrapper for the security gateway (command/URL/path validation).
pub struct AppSecurity(pub SecurityGateway);
impl Global for AppSecurity {}

/// Global wrapper for the learning service (outcome tracking, routing adjustments).
pub struct AppLearning(pub Arc<LearningService>);
impl Global for AppLearning {}

/// Global wrapper for the privacy/security shield (PII, secrets, threats).
pub struct AppShield(pub Arc<HiveShield>);
impl Global for AppShield {}

/// Global wrapper for the TTS service (voice synthesis, provider routing).
pub struct AppTts(pub Arc<TtsService>);
impl Global for AppTts {}

/// Global wrapper for the skills registry (/command dispatch, built-in skills).
pub struct AppSkills(pub SkillsRegistry);
impl Global for AppSkills {}

/// Global wrapper for the skill marketplace (install/remove, security scanning).
pub struct AppMarketplace(pub SkillMarketplace);
impl Global for AppMarketplace {}

/// Global wrapper for the built-in MCP tool server.
pub struct AppMcpServer(pub McpServer);
impl Global for AppMcpServer {}

/// Global wrapper for the persona registry (agent roles + custom personas).
pub struct AppPersonas(pub PersonaRegistry);
impl Global for AppPersonas {}

/// Global wrapper for the automation service (workflow engine).
pub struct AppAutomation(pub AutomationService);
impl Global for AppAutomation {}

/// Global wrapper for the spec manager (project specifications).
pub struct AppSpecs(pub SpecManager);
impl Global for AppSpecs {}

/// Global wrapper for the CLI service (built-in commands, doctor checks).
pub struct AppCli(pub CliService);
impl Global for AppCli {}

/// Global wrapper for the assistant service (email, calendar, reminders).
pub struct AppAssistant(pub AssistantService);
impl Global for AppAssistant {}

/// Global wrapper for the wallet store (blockchain accounts).
pub struct AppWallets(pub WalletStore);
impl Global for AppWallets {}

/// Global wrapper for blockchain RPC endpoint configuration.
pub struct AppRpcConfig(pub RpcConfigStore);
impl Global for AppRpcConfig {}

/// Global wrapper for IDE integration (diagnostics, symbols, workspace info).
pub struct AppIde(pub IdeIntegrationService);
impl Global for AppIde {}

/// Global wrapper for the AI agent channel store (persistent messaging channels).
pub struct AppChannels(pub ChannelStore);
impl Global for AppChannels {}

/// Global wrapper for the P2P network node (federation, peer discovery).
pub struct AppNetwork(pub Arc<HiveNode>);
impl Global for AppNetwork {}

/// Global wrapper for the auto-update service (version check, binary replacement).
pub struct AppUpdater(pub UpdateService);
impl Global for AppUpdater {}
