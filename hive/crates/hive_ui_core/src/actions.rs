use gpui::*;

// ---------------------------------------------------------------------------
// Zero-sized actions
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
        SwitchToModels,
        SwitchToTokenLaunch,
        SwitchToSpecs,
        SwitchToAgents,
        SwitchToLearning,
        SwitchToShield,
        SwitchToAssistant,
        SwitchToSettings,
        SwitchToHelp,
        OpenWorkspaceDirectory,
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
        // Agents panel
        AgentsReloadWorkflows,
        // Panel switch — new panels
        SwitchToWorkflows,
        SwitchToChannels,
        // Workflow builder
        WorkflowBuilderSave,
        WorkflowBuilderRun,
        WorkflowBuilderDeleteNode,
        // Connected accounts
        AccountConnect,
        AccountDisconnect,
        AccountRefresh,
    ]
);

// ---------------------------------------------------------------------------
// Data-carrying actions
// ---------------------------------------------------------------------------

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

/// Load a specific workflow into the visual builder canvas.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct WorkflowBuilderLoadWorkflow {
    pub workflow_id: String,
}

/// Select a channel in the Channels panel.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct ChannelSelect {
    pub channel_id: String,
}

/// Initiate an OAuth connection for a specific platform.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AccountConnectPlatform {
    pub platform: String,
}

/// Disconnect a connected account.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AccountDisconnectPlatform {
    pub platform: String,
}

/// Run a specific automation workflow by ID from the Agents panel.
///
/// `instruction` is optional free-form text describing the task for this run.
/// When provided, the workflow runtime will be planned against that instruction
/// before execution.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct AgentsRunWorkflow {
    pub workflow_id: String,
    pub instruction: String,
    pub source: String,
    pub source_id: String,
}
