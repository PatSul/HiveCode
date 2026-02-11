pub mod background;
pub mod canvas;
pub mod code_review;
pub mod config;
pub mod context;
pub mod conversations;
pub mod enterprise;
pub mod error_handler;
pub mod kanban;
pub mod logging;
pub mod notifications;
pub mod persistence;
pub mod scheduler;
pub mod secure_storage;
pub mod security;
pub mod session;

pub use background::{BackgroundService, BackgroundTask, TaskStatus};
pub use canvas::{CanvasElement, CanvasState, Connection, ElementType, LiveCanvas, Point, Size};
pub use code_review::{
    ChangeType, CodeReview, CodeReviewStore, CommentStatus, FileChange, ReviewComment,
    ReviewStats, ReviewStatus,
};
pub use enterprise::{AuditAction, AuditEntry, EnterpriseService, Team, TeamMember, TeamRole, UsageMetric};
pub use config::HiveConfig;
pub use context::{ContextMessage, ContextSummary, ContextWindow, estimate_tokens, model_context_size};
pub use conversations::{Conversation, ConversationStore, ConversationSummary, StoredMessage};
pub use error_handler::{
    classify_error, ClassifiedCategory, ClassifiedError, ErrorCategory, ErrorSeverity, HiveError,
};
pub use kanban::{BoardMetrics, KanbanBoard, KanbanColumn, KanbanTask, Priority, Subtask, TaskComment};
pub use notifications::{AppNotification, NotificationStore, NotificationType};
pub use persistence::{ConversationRow, Database, MemoryEntry, MessageRow, ModelCostRow};
pub use scheduler::{CronSchedule, ScheduledJob, Scheduler};
pub use secure_storage::SecureStorage;
pub use security::SecurityGateway;
pub use session::SessionState;
