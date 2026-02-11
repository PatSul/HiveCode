pub mod context_engine;
pub mod cost;
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
pub use context_engine::{ContextEngine, ContextSource, SourceType, ContextBudget, CuratedContext, RelevanceScore, ContextStats};
pub use cost::{CostBreakdown, CostTracker, BudgetLimits};
pub use fleet_learning::{FleetInsight, FleetLearningService, InstanceMetrics, LearningPattern, ModelPerformance, PatternType};
pub use providers::{AiProvider, ProviderError};
pub use rag::{RagService, RagQuery, RagResult, DocumentChunk, ScoredChunk, IndexStats};
pub use semantic_search::{SemanticSearchService, SearchEntry, SearchResult, SearchQuery};
pub use service::{AiService, AiServiceConfig};
pub use tts::{TtsError, TtsProvider, TtsProviderType};
pub use tts::service::{TtsService, TtsServiceConfig};
pub use types::*;
