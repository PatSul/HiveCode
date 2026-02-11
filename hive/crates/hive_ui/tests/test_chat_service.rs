use chrono::Utc;
use uuid::Uuid;

use hive_ui::chat_service::*;
use hive_ai::types::{
    MessageRole as AiMessageRole, TokenUsage,
};
use hive_core::conversations::{
    ConversationStore, StoredMessage,
};

// -- Unit tests (no GPUI runtime required) ------------------------------

#[test]
fn test_chat_message_user() {
    let msg = ChatMessage::user("Hello!");
    assert_eq!(msg.role, MessageRole::User);
    assert_eq!(msg.content, "Hello!");
    assert!(msg.model.is_none());
    assert!(msg.cost.is_none());
    assert!(msg.tokens.is_none());
    assert!(!msg.id.is_empty());
}

#[test]
fn test_chat_message_unique_ids() {
    let a = ChatMessage::user("a");
    let b = ChatMessage::user("b");
    assert_ne!(a.id, b.id);
}

#[test]
fn test_chat_message_assistant_placeholder() {
    let msg = ChatMessage::assistant_placeholder();
    assert_eq!(msg.role, MessageRole::Assistant);
    assert!(msg.content.is_empty());
}

#[test]
fn test_chat_message_error() {
    let msg = ChatMessage::error("Something went wrong");
    assert_eq!(msg.role, MessageRole::Error);
    assert_eq!(msg.content, "Something went wrong");
}

#[test]
fn test_message_role_to_ai_role() {
    assert_eq!(MessageRole::User.to_ai_role(), AiMessageRole::User);
    assert_eq!(MessageRole::Assistant.to_ai_role(), AiMessageRole::Assistant);
    assert_eq!(MessageRole::System.to_ai_role(), AiMessageRole::System);
    assert_eq!(MessageRole::Error.to_ai_role(), AiMessageRole::Error);
}

#[test]
fn test_chat_service_new() {
    let svc = ChatService::new("claude-sonnet-4-5".into());
    assert_eq!(svc.current_model(), "claude-sonnet-4-5");
    assert!(!svc.is_streaming());
    assert!(svc.messages().is_empty());
    assert!(svc.streaming_content().is_empty());
    assert!(svc.error().is_none());
}

#[test]
fn test_chat_service_set_model() {
    let mut svc = ChatService::new("model-a".into());
    svc.set_model("model-b".into());
    assert_eq!(svc.current_model(), "model-b");
}

#[test]
fn test_chat_service_clear() {
    let mut svc = ChatService::new("model-a".into());
    svc.messages.push(ChatMessage::user("hi"));
    svc.streaming_content = "partial".into();
    svc.is_streaming = true;
    svc.error = Some("oops".into());

    svc.clear();

    assert!(svc.messages().is_empty());
    assert!(svc.streaming_content().is_empty());
    assert!(!svc.is_streaming());
    assert!(svc.error().is_none());
}

#[test]
fn test_build_ai_messages_skips_errors_and_placeholders() {
    let mut svc = ChatService::new("model-a".into());
    svc.messages.push(ChatMessage::user("hello"));
    svc.messages.push(ChatMessage::new(MessageRole::Assistant, "world"));
    svc.messages.push(ChatMessage::error("bad"));
    svc.messages.push(ChatMessage::assistant_placeholder()); // empty assistant

    let ai_msgs = svc.build_ai_messages();
    assert_eq!(ai_msgs.len(), 2);
    assert_eq!(ai_msgs[0].role, AiMessageRole::User);
    assert_eq!(ai_msgs[0].content, "hello");
    assert_eq!(ai_msgs[1].role, AiMessageRole::Assistant);
    assert_eq!(ai_msgs[1].content, "world");
}

#[test]
fn test_finalize_stream_sets_content_and_cost() {
    let mut svc = ChatService::new("model-a".into());
    svc.messages.push(ChatMessage::user("hi"));
    svc.messages.push(ChatMessage::assistant_placeholder());
    svc.is_streaming = true;
    svc.streaming_content = "hello world".into();

    let usage = TokenUsage {
        prompt_tokens: 10,
        completion_tokens: 20,
        total_tokens: 30,
    };

    svc.finalize_stream(1, "hello world", "claude-sonnet-4-5", Some(&usage));

    assert!(!svc.is_streaming());
    assert!(svc.streaming_content().is_empty());

    let assistant = &svc.messages[1];
    assert_eq!(assistant.content, "hello world");
    assert_eq!(assistant.model.as_deref(), Some("claude-sonnet-4-5"));
    assert!(assistant.tokens.is_some());
    let (input, output) = assistant.tokens.unwrap();
    assert_eq!(input, 10);
    assert_eq!(output, 20);
}

#[test]
fn test_finalize_stream_no_usage() {
    let mut svc = ChatService::new("model-a".into());
    svc.messages.push(ChatMessage::user("hi"));
    svc.messages.push(ChatMessage::assistant_placeholder());
    svc.is_streaming = true;

    svc.finalize_stream(1, "response text", "local-model", None);

    let assistant = &svc.messages[1];
    assert_eq!(assistant.content, "response text");
    assert!(assistant.cost.is_none());
    assert!(assistant.tokens.is_none());
}

#[test]
fn test_finalize_stream_out_of_bounds_is_safe() {
    let mut svc = ChatService::new("model-a".into());
    // No messages at all -- finalize should not panic.
    svc.finalize_stream(99, "content", "model", None);
    assert!(!svc.is_streaming());
}

// -- Persistence tests --------------------------------------------------

/// Helper: create a temp-dir-backed ConversationStore.
fn temp_store() -> (ConversationStore, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir");
    let store = ConversationStore::new_at(tmp.path().to_path_buf())
        .expect("Failed to create store");
    (store, tmp)
}

#[test]
fn test_new_conversation_assigns_id_and_clears() {
    let mut svc = ChatService::new("model-a".into());
    svc.messages.push(ChatMessage::user("old message"));
    svc.error = Some("old error".into());

    svc.new_conversation();

    assert!(svc.messages().is_empty());
    assert!(svc.error().is_none());
    assert!(svc.conversation_id().is_some());
    let id = svc.conversation_id().unwrap().to_string();
    // Should be a valid UUID (36 chars with hyphens).
    assert_eq!(id.len(), 36);
}

#[test]
fn test_new_conversation_generates_unique_ids() {
    let mut svc = ChatService::new("model-a".into());
    svc.new_conversation();
    let id1 = svc.conversation_id().unwrap().to_string();
    svc.new_conversation();
    let id2 = svc.conversation_id().unwrap().to_string();
    assert_ne!(id1, id2);
}

#[test]
fn test_conversation_id_none_by_default() {
    let svc = ChatService::new("model-a".into());
    assert!(svc.conversation_id().is_none());
}

#[test]
fn test_save_and_load_round_trip() {
    let (store, _tmp) = temp_store();

    // Build a ChatService with some messages.
    let mut svc = ChatService::new("claude-sonnet-4-5".into());
    svc.conversation_id = Some("test-conv-001".to_string());

    svc.messages.push(ChatMessage::user("Hello, how are you?"));
    let mut assistant = ChatMessage::new(MessageRole::Assistant, "I'm doing great!");
    assistant.model = Some("claude-sonnet-4-5".to_string());
    assistant.cost = Some(0.002);
    assistant.tokens = Some((10, 20));
    svc.messages.push(assistant);

    // Save using the test store.
    svc.save_to_store(&store, "test-conv-001").unwrap();

    // Load into a fresh ChatService.
    let mut svc2 = ChatService::new("default-model".into());
    svc2.load_from_store(&store, "test-conv-001").unwrap();

    assert_eq!(svc2.conversation_id(), Some("test-conv-001"));
    assert_eq!(svc2.current_model(), "claude-sonnet-4-5");
    assert_eq!(svc2.messages().len(), 2);

    let user_msg = &svc2.messages()[0];
    assert_eq!(user_msg.role, MessageRole::User);
    assert_eq!(user_msg.content, "Hello, how are you?");

    let assistant_msg = &svc2.messages()[1];
    assert_eq!(assistant_msg.role, MessageRole::Assistant);
    assert_eq!(assistant_msg.content, "I'm doing great!");
    assert_eq!(assistant_msg.model.as_deref(), Some("claude-sonnet-4-5"));
    assert!(assistant_msg.cost.is_some());
    assert!((assistant_msg.cost.unwrap() - 0.002).abs() < f64::EPSILON);
}

#[test]
fn test_save_skips_error_and_placeholder_messages() {
    let (store, _tmp) = temp_store();

    let mut svc = ChatService::new("model-a".into());
    svc.conversation_id = Some("skip-test".to_string());

    svc.messages.push(ChatMessage::user("hi"));
    svc.messages.push(ChatMessage::new(MessageRole::Assistant, "hello"));
    svc.messages.push(ChatMessage::error("bad thing"));
    svc.messages.push(ChatMessage::assistant_placeholder()); // empty

    svc.save_to_store(&store, "skip-test").unwrap();

    let loaded = store.load("skip-test").unwrap();
    // Only the user and non-empty assistant should be persisted.
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].role, "user");
    assert_eq!(loaded.messages[1].role, "assistant");
}

#[test]
fn test_save_auto_generates_title_from_first_user_message() {
    let (store, _tmp) = temp_store();

    let mut svc = ChatService::new("model-a".into());
    svc.conversation_id = Some("title-test".to_string());
    svc.messages
        .push(ChatMessage::user("Tell me about quantum computing"));
    svc.messages.push(ChatMessage::new(
        MessageRole::Assistant,
        "Quantum computing is...",
    ));

    svc.save_to_store(&store, "title-test").unwrap();

    let loaded = store.load("title-test").unwrap();
    assert_eq!(loaded.title, "Tell me about quantum computing");
}

#[test]
fn test_save_truncates_long_title() {
    let (store, _tmp) = temp_store();

    let long_msg = "a".repeat(80);
    let mut svc = ChatService::new("model-a".into());
    svc.conversation_id = Some("long-title".to_string());
    svc.messages.push(ChatMessage::user(&long_msg));

    svc.save_to_store(&store, "long-title").unwrap();

    let loaded = store.load("long-title").unwrap();
    assert!(loaded.title.ends_with("..."));
    let prefix = loaded.title.trim_end_matches("...");
    assert!(prefix.len() <= 50);
}

#[test]
fn test_save_assigns_id_when_none() {
    let mut svc = ChatService::new("model-a".into());
    assert!(svc.conversation_id().is_none());

    // We can't call save_conversation() in tests because it uses
    // ConversationStore::new() which needs ~/.hive. But we can verify
    // the ID assignment logic via save_to_store.
    let (store, _tmp) = temp_store();
    svc.messages.push(ChatMessage::user("test"));

    // Manually do what save_conversation does for ID assignment.
    let id = Uuid::new_v4().to_string();
    svc.conversation_id = Some(id.clone());
    svc.save_to_store(&store, &id).unwrap();

    assert!(svc.conversation_id().is_some());
    assert!(store.load(&id).is_ok());
}

#[test]
fn test_load_nonexistent_conversation_returns_error() {
    let (store, _tmp) = temp_store();
    let mut svc = ChatService::new("model-a".into());
    assert!(svc.load_from_store(&store, "nonexistent-id").is_err());
}

#[test]
fn test_load_replaces_existing_state() {
    let (store, _tmp) = temp_store();

    // Save a conversation.
    let mut svc = ChatService::new("model-a".into());
    svc.conversation_id = Some("replace-test".to_string());
    svc.messages.push(ChatMessage::user("original message"));
    svc.save_to_store(&store, "replace-test").unwrap();

    // Set up a service with different state.
    let mut svc2 = ChatService::new("model-b".into());
    svc2.messages.push(ChatMessage::user("different message"));
    svc2.error = Some("some error".into());
    svc2.is_streaming = true;
    svc2.streaming_content = "partial".into();

    // Load should overwrite everything.
    svc2.load_from_store(&store, "replace-test").unwrap();

    assert_eq!(svc2.conversation_id(), Some("replace-test"));
    assert_eq!(svc2.current_model(), "model-a");
    assert_eq!(svc2.messages().len(), 1);
    assert_eq!(svc2.messages()[0].content, "original message");
    assert!(svc2.error().is_none());
    assert!(!svc2.is_streaming());
    assert!(svc2.streaming_content().is_empty());
}

#[test]
fn test_save_preserves_created_at_on_update() {
    let (store, _tmp) = temp_store();

    // First save.
    let mut svc = ChatService::new("model-a".into());
    svc.conversation_id = Some("preserve-ts".to_string());
    svc.messages.push(ChatMessage::user("first"));
    svc.save_to_store(&store, "preserve-ts").unwrap();

    let first_load = store.load("preserve-ts").unwrap();
    let original_created = first_load.created_at;

    // Add another message and save again.
    svc.messages.push(ChatMessage::new(
        MessageRole::Assistant,
        "second",
    ));
    svc.save_to_store(&store, "preserve-ts").unwrap();

    let second_load = store.load("preserve-ts").unwrap();
    assert_eq!(second_load.created_at, original_created);
    assert!(second_load.updated_at >= second_load.created_at);
    assert_eq!(second_load.messages.len(), 2);
}

#[test]
fn test_save_accumulates_cost_and_tokens() {
    let (store, _tmp) = temp_store();

    let mut svc = ChatService::new("model-a".into());
    svc.conversation_id = Some("cost-test".to_string());

    let mut msg1 = ChatMessage::user("q1");
    msg1.cost = Some(0.01);
    msg1.tokens = Some((10, 20));
    svc.messages.push(msg1);

    let mut msg2 = ChatMessage::new(MessageRole::Assistant, "a1");
    msg2.cost = Some(0.02);
    msg2.tokens = Some((5, 15));
    svc.messages.push(msg2);

    svc.save_to_store(&store, "cost-test").unwrap();

    let loaded = store.load("cost-test").unwrap();
    assert!((loaded.total_cost - 0.03).abs() < f64::EPSILON);
    // Tokens are stored as single total per message: (10+20) + (5+15) = 50
    assert_eq!(loaded.total_tokens, 50);
}

#[test]
fn test_chat_message_to_stored_conversion() {
    let mut msg = ChatMessage::user("hello");
    msg.model = Some("test-model".into());
    msg.cost = Some(0.005);
    msg.tokens = Some((100, 200));

    let stored = msg.to_stored();
    assert_eq!(stored.role, "user");
    assert_eq!(stored.content, "hello");
    assert_eq!(stored.model.as_deref(), Some("test-model"));
    assert!((stored.cost.unwrap() - 0.005).abs() < f64::EPSILON);
    assert_eq!(stored.tokens, Some(300)); // 100 + 200
    assert!(stored.thinking.is_none());
}

#[test]
fn test_chat_message_from_stored_conversion() {
    let stored = StoredMessage {
        role: "assistant".into(),
        content: "world".into(),
        timestamp: Utc::now(),
        model: Some("claude".into()),
        cost: Some(0.01),
        tokens: Some(50),
        thinking: None,
    };

    let msg = ChatMessage::from_stored(&stored);
    assert_eq!(msg.role, MessageRole::Assistant);
    assert_eq!(msg.content, "world");
    assert_eq!(msg.model.as_deref(), Some("claude"));
    assert!((msg.cost.unwrap() - 0.01).abs() < f64::EPSILON);
    // Total tokens stored as (0, total) since we cannot recover the split.
    assert_eq!(msg.tokens, Some((0, 50)));
}

#[test]
fn test_message_role_from_stored() {
    assert_eq!(MessageRole::from_stored("user"), MessageRole::User);
    assert_eq!(MessageRole::from_stored("assistant"), MessageRole::Assistant);
    assert_eq!(MessageRole::from_stored("system"), MessageRole::System);
    assert_eq!(MessageRole::from_stored("error"), MessageRole::Error);
    // Unknown roles map to Error.
    assert_eq!(MessageRole::from_stored("unknown"), MessageRole::Error);
}

#[test]
fn test_message_role_to_stored() {
    assert_eq!(MessageRole::User.to_stored(), "user");
    assert_eq!(MessageRole::Assistant.to_stored(), "assistant");
    assert_eq!(MessageRole::System.to_stored(), "system");
    assert_eq!(MessageRole::Error.to_stored(), "error");
}

#[test]
fn test_save_empty_conversation() {
    let (store, _tmp) = temp_store();

    let mut svc = ChatService::new("model-a".into());
    svc.conversation_id = Some("empty-conv".to_string());

    svc.save_to_store(&store, "empty-conv").unwrap();

    let loaded = store.load("empty-conv").unwrap();
    assert!(loaded.messages.is_empty());
    assert_eq!(loaded.title, "New Conversation");
    assert!((loaded.total_cost - 0.0).abs() < f64::EPSILON);
    assert_eq!(loaded.total_tokens, 0);
}
