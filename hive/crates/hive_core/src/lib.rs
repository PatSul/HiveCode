//! Core data types and services for the Hive platform.
//!
//! This crate provides the foundational types, configuration management,
//! persistence layer, and shared services used across all other Hive crates.

/// Background task scheduling and lifecycle management.
pub mod background;
/// Interactive whiteboard canvas with element and connection management.
pub mod canvas;
/// AI agent messaging channels with threaded conversations.
pub mod channels;
/// Code review management with comments, file changes, and review workflows.
pub mod code_review;
/// Application configuration, API key management, and hot-reload support.
pub mod config;
/// Context window management for token-aware conversation pruning.
pub mod context;
/// Conversation persistence and search using JSON files.
pub mod conversations;
/// Enterprise team management, audit logging, and usage tracking.
pub mod enterprise;
/// Error classification, severity levels, and user-friendly error messages.
pub mod error_handler;
/// Kanban board with WIP limits, subtasks, dependencies, and metrics.
pub mod kanban;
/// Logging initialization with daily file rotation and console output.
pub mod logging;
/// In-app notification management with read/unread tracking.
pub mod notifications;
/// SQLite-backed persistence for conversations, memory, and cost tracking.
pub mod persistence;
/// Cron-based task scheduler with job lifecycle management.
pub mod scheduler;
/// AES-256-GCM encrypted storage for API keys and sensitive data.
pub mod secure_storage;
/// Security gateway for command, URL, path, and injection validation.
pub mod security;
/// Session state persistence for crash recovery and workspace restoration.
pub mod session;

pub use background::{BackgroundService, BackgroundTask, TaskStatus};
pub use canvas::{CanvasElement, CanvasState, Connection, ElementType, LiveCanvas, Point, Size};
pub use code_review::{
    ChangeType, CodeReview, CodeReviewStore, CommentStatus, FileChange, ReviewComment, ReviewStats,
    ReviewStatus,
};
pub use config::HiveConfig;
pub use context::{
    ContextMessage, ContextSummary, ContextWindow, estimate_tokens, model_context_size,
};
pub use conversations::{Conversation, ConversationStore, ConversationSummary, StoredMessage};
pub use enterprise::{
    AuditAction, AuditEntry, EnterpriseService, Team, TeamMember, TeamRole, UsageMetric,
};
pub use error_handler::{
    ClassifiedCategory, ClassifiedError, ErrorCategory, ErrorSeverity, HiveError, classify_error,
};
pub use kanban::{
    BoardMetrics, KanbanBoard, KanbanColumn, KanbanTask, Priority, Subtask, TaskComment,
};
pub use notifications::{AppNotification, NotificationStore, NotificationType};
pub use persistence::{ConversationRow, Database, MemoryEntry, MessageRow, ModelCostRow};
pub use scheduler::{CronSchedule, ScheduledJob, Scheduler};
pub use secure_storage::SecureStorage;
pub use security::SecurityGateway;
pub use session::SessionState;
pub use channels::{AgentChannel, ChannelMessage, ChannelStore, ChannelThread, MessageAuthor};
