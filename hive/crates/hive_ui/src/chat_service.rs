//! Chat service bridge between the ChatPanel UI and the AiService backend.
//!
//! `ChatService` is a GPUI Entity that manages the conversation state and
//! drives streaming responses from [`hive_ai::AiService`]. It keeps its own
//! message list, streaming buffer, and error state so the UI can render
//! reactively via `cx.notify()`.

use chrono::{DateTime, Utc};
use gpui::{AsyncApp, Context, EventEmitter, Task, WeakEntity};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use hive_ai::types::{
    ChatMessage as AiChatMessage, MessageRole as AiMessageRole, StreamChunk, TokenUsage,
};
use hive_core::conversations::{
    Conversation, ConversationStore, ConversationSummary, StoredMessage, generate_title,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Role of a message in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Error,
}

impl MessageRole {
    /// Convert to the `hive_ai` wire type used by providers.
    pub fn to_ai_role(self) -> AiMessageRole {
        match self {
            Self::User => AiMessageRole::User,
            Self::Assistant => AiMessageRole::Assistant,
            Self::System => AiMessageRole::System,
            Self::Error => AiMessageRole::Error,
        }
    }

    /// Convert from a string role (as stored in `StoredMessage`).
    pub fn from_stored(role: &str) -> Self {
        match role {
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "system" => Self::System,
            _ => Self::Error,
        }
    }

    /// Convert to the string representation used by `StoredMessage`.
    pub fn to_stored(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Error => "error",
        }
    }
}

/// A single chat message with metadata.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub model: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub cost: Option<f64>,
    pub tokens: Option<(usize, usize)>,
}

impl ChatMessage {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            content: content.into(),
            model: None,
            timestamp: Utc::now(),
            cost: None,
            tokens: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    pub fn assistant_placeholder() -> Self {
        Self::new(MessageRole::Assistant, "")
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Error, content)
    }

    /// Convert this `ChatMessage` into a `StoredMessage` for persistence.
    pub fn to_stored(&self) -> StoredMessage {
        // StoredMessage.tokens is a single u32 (total tokens).
        // ChatMessage.tokens is (input, output). Sum them for storage.
        let total_tokens = self.tokens.map(|(i, o)| (i + o) as u32);

        StoredMessage {
            role: self.role.to_stored().to_string(),
            content: self.content.clone(),
            timestamp: self.timestamp,
            model: self.model.clone(),
            cost: self.cost,
            tokens: total_tokens,
            thinking: None,
        }
    }

    /// Construct a `ChatMessage` from a `StoredMessage`.
    pub fn from_stored(stored: &StoredMessage) -> Self {
        // StoredMessage.tokens is a single u32 total. We cannot recover the
        // input/output split, so we store (0, total) by convention.
        let tokens = stored.tokens.map(|t| (0usize, t as usize));

        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::from_stored(&stored.role),
            content: stored.content.clone(),
            model: stored.model.clone(),
            timestamp: stored.timestamp,
            cost: stored.cost,
            tokens,
        }
    }
}

// ---------------------------------------------------------------------------
// ChatService
// ---------------------------------------------------------------------------

/// GPUI Entity that bridges the chat UI to the AI backend.
///
/// Owns the conversation message list, drives streaming, and exposes
/// read-only accessors for the renderer.
pub struct ChatService {
    pub messages: Vec<ChatMessage>,
    pub streaming_content: String,
    pub is_streaming: bool,
    current_model: String,
    pub error: Option<String>,
    /// Handle to the in-flight streaming task so it is not dropped.
    _stream_task: Option<Task<()>>,
    /// ID of the current conversation for persistence. `None` means the
    /// conversation has not been saved yet (a new UUID will be generated on
    /// first save).
    pub conversation_id: Option<String>,
    /// Last time we notified the UI during streaming. Used to throttle
    /// re-renders to ~15 fps instead of per-token.
    last_stream_notify: std::time::Instant,
}

impl ChatService {
    pub fn new(default_model: String) -> Self {
        Self {
            messages: Vec::new(),
            streaming_content: String::new(),
            is_streaming: false,
            current_model: default_model,
            error: None,
            _stream_task: None,
            conversation_id: None,
            last_stream_notify: std::time::Instant::now(),
        }
    }

    // -- Accessors ----------------------------------------------------------

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    pub fn streaming_content(&self) -> &str {
        &self.streaming_content
    }

    pub fn current_model(&self) -> &str {
        &self.current_model
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Returns the current conversation ID, if one has been assigned.
    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    // -- Mutators -----------------------------------------------------------

    pub fn set_model(&mut self, model: String) {
        self.current_model = model;
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.streaming_content.clear();
        self.is_streaming = false;
        self.error = None;
        self._stream_task = None;
    }

    // -- Persistence --------------------------------------------------------

    /// Start a fresh conversation, clearing all messages and assigning a new
    /// UUID. The previous conversation (if any) is not automatically saved;
    /// call [`save_conversation`] first if you need to persist it.
    pub fn new_conversation(&mut self) {
        self.clear();
        self.conversation_id = Some(Uuid::new_v4().to_string());
    }

    /// Save the current conversation to disk via [`ConversationStore`].
    ///
    /// If no `conversation_id` has been set yet, a new UUID is generated.
    /// The title is auto-generated from the first user message (up to 50
    /// chars). Error messages are excluded from the persisted data.
    pub fn save_conversation(&mut self) -> anyhow::Result<()> {
        // Lazily assign an ID on first save.
        let id = match &self.conversation_id {
            Some(id) => id.clone(),
            None => {
                let id = Uuid::new_v4().to_string();
                self.conversation_id = Some(id.clone());
                id
            }
        };

        let store = ConversationStore::new()?;
        self.save_to_store(&store, &id)
    }

    /// Save the current conversation to an arbitrary [`ConversationStore`].
    /// Useful for tests that provide a temp-dir-backed store.
    pub fn save_to_store(&self, store: &ConversationStore, id: &str) -> anyhow::Result<()> {
        // Convert ChatMessages -> StoredMessages, skipping errors and empty
        // placeholders (same filter as build_ai_messages).
        let stored_messages: Vec<StoredMessage> = self
            .messages
            .iter()
            .filter(|m| {
                m.role != MessageRole::Error
                    && !(m.role == MessageRole::Assistant && m.content.is_empty())
            })
            .map(|m| m.to_stored())
            .collect();

        let title = generate_title(&stored_messages);

        let total_cost: f64 = stored_messages
            .iter()
            .filter_map(|m| m.cost)
            .sum();

        let total_tokens: u32 = stored_messages
            .iter()
            .filter_map(|m| m.tokens)
            .sum();

        let now = Utc::now();

        // Try to load existing conversation to preserve created_at.
        let created_at = store
            .load(id)
            .map(|existing| existing.created_at)
            .unwrap_or(now);

        let conversation = Conversation {
            id: id.to_string(),
            title,
            messages: stored_messages,
            model: self.current_model.clone(),
            total_cost,
            total_tokens,
            created_at,
            updated_at: now,
        };

        store.save(&conversation)
    }

    /// Load a conversation from disk by ID, replacing the current message
    /// list and state.
    ///
    /// On success the `conversation_id` is set to the loaded conversation's
    /// ID, and the `current_model` is updated to match the persisted model.
    pub fn load_conversation(&mut self, id: &str) -> anyhow::Result<()> {
        let store = ConversationStore::new()?;
        self.load_from_store(&store, id)
    }

    /// Load a conversation from an arbitrary [`ConversationStore`].
    /// Useful for tests that provide a temp-dir-backed store.
    pub fn load_from_store(&mut self, store: &ConversationStore, id: &str) -> anyhow::Result<()> {
        let conversation = store.load(id)?;

        // Convert StoredMessage -> ChatMessage.
        let messages: Vec<ChatMessage> = conversation
            .messages
            .iter()
            .map(ChatMessage::from_stored)
            .collect();

        self.messages = messages;
        self.conversation_id = Some(conversation.id);
        self.current_model = conversation.model;
        self.streaming_content.clear();
        self.is_streaming = false;
        self.error = None;
        self._stream_task = None;

        info!(
            "ChatService: loaded conversation {} ({} messages)",
            id,
            self.messages.len()
        );

        Ok(())
    }

    /// List conversation summaries from disk, sorted newest-first.
    pub fn list_conversations() -> anyhow::Result<Vec<ConversationSummary>> {
        let store = ConversationStore::new()?;
        store.list_summaries()
    }

    /// Delete a conversation from disk by ID.
    pub fn delete_conversation(id: &str) -> anyhow::Result<()> {
        let store = ConversationStore::new()?;
        store.delete(id)
    }

    // -- Sending ------------------------------------------------------------

    /// Send a user message and begin streaming the assistant response.
    ///
    /// This is the primary entry point called by the UI when the user presses
    /// Send. It:
    /// 1. Appends the user message to the conversation.
    /// 2. Creates a placeholder assistant message.
    /// 3. Spawns an async task that receives a `tokio::sync::mpsc::Receiver`
    ///    of `StreamChunk`s and feeds them back to `self` through
    ///    `WeakEntity::update`.
    ///
    /// The actual provider call (`AiService::stream_chat`) is expected to be
    /// initiated *outside* this entity because `AiService` lives as a GPUI
    /// Global and cannot be accessed from within `Context<ChatService>`.
    /// Instead, we use a channel: the caller is responsible for calling
    /// [`ChatService::attach_stream`] with the receiver.
    pub fn send_message(&mut self, content: String, model: &str, cx: &mut Context<Self>) {
        // Clear previous error.
        self.error = None;

        // 1. Record the user message.
        let user_msg = ChatMessage::user(&content);
        self.messages.push(user_msg);

        // 2. Prepare streaming state.
        self.is_streaming = true;
        self.streaming_content.clear();
        self.current_model = model.to_string();

        // 3. Add a placeholder assistant message that will be finalized later.
        let placeholder = ChatMessage::assistant_placeholder();
        self.messages.push(placeholder);

        info!(
            "ChatService: user message queued, awaiting stream attachment (model={})",
            model
        );

        // Notify the UI so the user message renders immediately.
        cx.notify();
    }

    /// Attach a stream receiver from `AiService::stream_chat` and begin
    /// consuming chunks.
    ///
    /// This must be called immediately after `send_message` while the
    /// placeholder assistant message is still the last entry. Typically the
    /// orchestrating layer (workspace or app) does:
    ///
    /// ```ignore
    /// chat_service.update(cx, |svc, cx| svc.send_message(text, model, cx));
    /// let rx = ai_service.stream_chat(messages, model, None).await?;
    /// chat_service.update(cx, |svc, cx| svc.attach_stream(rx, model, cx));
    /// ```
    pub fn attach_stream(
        &mut self,
        mut rx: mpsc::Receiver<StreamChunk>,
        model: String,
        cx: &mut Context<Self>,
    ) {
        let assistant_idx = self.messages.len().saturating_sub(1);
        let model_clone = model.clone();

        let task = cx.spawn(async move |this: WeakEntity<ChatService>, app: &mut AsyncApp| {
            let mut accumulated = String::new();
            let mut final_usage: Option<TokenUsage> = None;

            loop {
                // Receive the next chunk. We poll via a small async block
                // because `rx.recv()` is cancel-safe.
                let chunk = rx.recv().await;

                match chunk {
                    Some(chunk) => {
                        accumulated.push_str(&chunk.content);

                        if let Some(usage) = &chunk.usage {
                            final_usage = Some(usage.clone());
                        }

                        let is_done = chunk.done;

                        // Throttle UI updates to ~15 fps (67ms) during streaming.
                        // Always notify on the final chunk.
                        let content_snapshot = accumulated.clone();
                        let update_result =
                            this.update(app, |this: &mut ChatService, cx| {
                                this.streaming_content = content_snapshot;
                                let elapsed = this.last_stream_notify.elapsed();
                                if is_done || elapsed.as_millis() >= 67 {
                                    this.last_stream_notify = std::time::Instant::now();
                                    cx.notify();
                                }
                            });

                        if update_result.is_err() {
                            // Entity was dropped.
                            break;
                        }

                        if is_done {
                            break;
                        }
                    }
                    None => {
                        // Channel closed (stream ended without a done flag).
                        break;
                    }
                }
            }

            // Finalize: move accumulated content into the placeholder message.
            let usage = final_usage;
            let _ = this.update(app, |this: &mut ChatService, cx| {
                this.finalize_stream(assistant_idx, &accumulated, &model_clone, usage.as_ref());
                this.emit_stream_completed(&model_clone, cx);
                cx.notify();
            });
        });

        self._stream_task = Some(task);
    }

    /// Convenience method that combines `send_message` and `attach_stream`.
    ///
    /// Use this when the stream receiver is already available (e.g. in tests
    /// or when the caller has pre-started the stream).
    pub fn send_message_with_stream(
        &mut self,
        content: String,
        model: &str,
        rx: mpsc::Receiver<StreamChunk>,
        cx: &mut Context<Self>,
    ) {
        self.send_message(content, model, cx);
        self.attach_stream(rx, model.to_string(), cx);
    }

    /// Build the AI wire-format message history for the current conversation.
    ///
    /// Skips placeholder (empty assistant) and error messages. Useful for
    /// the caller to construct the `AiService::stream_chat` request.
    pub fn build_ai_messages(&self) -> Vec<AiChatMessage> {
        self.messages
            .iter()
            .filter(|m| {
                m.role != MessageRole::Error
                    && !(m.role == MessageRole::Assistant && m.content.is_empty())
            })
            .map(|m| AiChatMessage {
                role: m.role.to_ai_role(),
                content: m.content.clone(),
                timestamp: m.timestamp,
            })
            .collect()
    }

    // -- Internal -----------------------------------------------------------

    /// Replace the placeholder assistant message with the final content and
    /// update streaming state.
    pub fn finalize_stream(
        &mut self,
        assistant_idx: usize,
        content: &str,
        model: &str,
        usage: Option<&TokenUsage>,
    ) {
        if let Some(msg) = self.messages.get_mut(assistant_idx) {
            msg.content = content.to_string();
            msg.model = Some(model.to_string());

            if let Some(usage) = usage {
                let cost = hive_ai::cost::calculate_cost(
                    model,
                    usage.prompt_tokens as usize,
                    usage.completion_tokens as usize,
                );
                msg.cost = Some(cost.total_cost);
                msg.tokens = Some((
                    usage.prompt_tokens as usize,
                    usage.completion_tokens as usize,
                ));
            }
        }

        self.streaming_content.clear();
        self.is_streaming = false;
        self._stream_task = None;

        info!(
            "ChatService: stream finalized ({} messages, model={})",
            self.messages.len(),
            model
        );

        // Auto-save after finalization. Fire-and-forget: log on error but
        // don't propagate since streaming itself succeeded.
        if let Err(e) = self.save_conversation() {
            warn!("ChatService: auto-save failed: {e}");
        }
    }

    /// Emit a stream-completed event. Called from the attach_stream closure
    /// after finalize_stream completes.
    fn emit_stream_completed(&self, model: &str, cx: &mut Context<Self>) {
        let last_msg = self.messages.last();
        let cost = last_msg.and_then(|m| m.cost);
        let tokens = last_msg.and_then(|m| m.tokens);

        cx.emit(StreamCompleted {
            model: model.to_string(),
            message_count: self.messages.len(),
            cost,
            tokens,
        });
    }

    /// Record an error from the streaming task.
    pub fn set_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        let msg = message.into();
        error!("ChatService error: {}", msg);
        self.error = Some(msg.clone());
        self.is_streaming = false;
        self.streaming_content.clear();
        self._stream_task = None;

        // Remove the placeholder assistant message (last entry) if it is empty.
        if let Some(last) = self.messages.last() {
            if last.role == MessageRole::Assistant && last.content.is_empty() {
                self.messages.pop();
            }
        }

        // Push an error message so the user sees what happened.
        self.messages.push(ChatMessage::error(msg));
        cx.notify();
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when a streaming response is fully finalized.
///
/// The workspace subscribes to this to record learning outcomes.
#[derive(Debug, Clone)]
pub struct StreamCompleted {
    pub model: String,
    pub message_count: usize,
    pub cost: Option<f64>,
    pub tokens: Option<(usize, usize)>,
}

impl EventEmitter<StreamCompleted> for ChatService {}
