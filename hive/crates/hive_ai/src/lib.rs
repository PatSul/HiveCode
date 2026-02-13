pub mod context_engine;
pub mod cost;
pub mod discovery;
pub mod fleet_learning;
pub mod model_registry;
pub mod providers;
pub mod rag;
pub mod routing;
pub mod semantic_search;
pub mod service;
pub mod tts;
pub mod types;

// Re-export core types at crate root for convenience.
pub use context_engine::{
    ContextBudget, ContextEngine, ContextSource, ContextStats, CuratedContext, RelevanceScore,
    SourceType,
};
pub use cost::{BudgetLimits, CostBreakdown, CostTracker};
pub use discovery::{DiscoveredProvider, DiscoveryState, LocalDiscovery};
pub use fleet_learning::{
    FleetInsight, FleetLearningService, InstanceMetrics, LearningPattern, ModelPerformance,
    PatternType,
};
pub use providers::{AiProvider, ProviderError};
pub use rag::{DocumentChunk, IndexStats, RagQuery, RagResult, RagService, ScoredChunk};
pub use semantic_search::{SearchEntry, SearchQuery, SearchResult, SemanticSearchService};
pub use service::{AiService, AiServiceConfig};
pub use tts::service::{TtsService, TtsServiceConfig};
pub use tts::{TtsError, TtsProvider, TtsProviderType};
pub use types::*;
