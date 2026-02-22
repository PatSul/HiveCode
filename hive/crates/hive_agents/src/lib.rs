pub mod auto_commit;
pub mod automation;
pub mod collective_memory;
pub mod competence_detection;
pub mod coordinator;
pub mod guardian;
pub mod heartbeat;
pub mod hiveloop;
pub mod hivemind;
pub mod integration_tools;
pub mod knowledge_acquisition;
pub mod mcp_client;
pub mod mcp_server;
pub mod persistence;
pub mod personas;
pub mod queen;
pub mod skill_authoring;
pub mod skill_marketplace;
pub mod skills;
pub mod specs;
pub mod standup;
pub mod swarm;
pub mod tool_use;
pub mod ui_automation;
pub mod voice;
pub mod worktree;

pub use auto_commit::{AutoCommitConfig, AutoCommitService, CommitResult};
pub use automation::{
    ActionType, AutomationService, Condition, ConditionOp, TriggerType, Workflow,
    WorkflowLoadReport, WorkflowRunResult, WorkflowStatus, WorkflowStep, BUILTIN_DOGFOOD_WORKFLOW_ID,
    USER_WORKFLOW_DIR,
};
pub use collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry, MemoryStats};
pub use competence_detection::{
    CompetenceAssessment, CompetenceConfig, CompetenceDetector, CompetenceGap, GapSeverity,
    GapType, SuggestedAction,
};
pub use coordinator::{
    Coordinator, CoordinatorConfig, CoordinatorResult, PlannedTask, TaskPlan, TaskResult,
};
pub use heartbeat::{AgentHeartbeat, HeartbeatService};
pub use persistence::{AgentPersistenceService, AgentSnapshot, CompletedTask};
pub use personas::{Persona, PersonaKind, PersonaRegistry, PromptOverride, execute_with_persona};
pub use queen::Queen;
pub use knowledge_acquisition::{
    AcquisitionResult, CodeBlock, KnowledgeAcquisitionAgent, KnowledgeConfig, KnowledgePage,
    KnowledgeSummary,
};
pub use skill_authoring::{
    DraftSkill, SkillAuthoringConfig, SkillAuthoringPipeline, SkillAuthoringRequest,
    SkillAuthoringResult, SkillResultSource, SkillSearchResult,
};
pub use skill_marketplace::{
    AvailableSkill, InstalledSkill, SecurityIssue, SecurityIssueType, Severity, SkillCategory,
    SkillDirectory, SkillMarketplace, SkillOrg, SkillSource,
};
pub use specs::{Spec, SpecEntry, SpecManager, SpecSection, SpecStatus};
pub use standup::{AgentReport, DailyStandup, StandupService};
pub use swarm::{
    InnerResult, MergeResult, OrchestrationMode, SwarmConfig, SwarmPlan, SwarmResult, SwarmStatus,
    SwarmStatusCallback, TeamObjective, TeamResult, TeamStatus,
};
pub use voice::{VoiceAssistant, VoiceCommand, VoiceIntent, VoiceState, WakeWordConfig};
pub use worktree::{MergeBranchResult, TeamWorktree, WorktreeManager};
