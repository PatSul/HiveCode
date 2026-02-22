use hive_ai::MessageRole;
use hive_ui_panels::panels::chat::{
    CachedChatData, ChatPanel, DisplayMessage, MarkdownCache, ToolCallDisplay,
};

// ---------------------------------------------------------------------------
// MarkdownCache
// ---------------------------------------------------------------------------

#[test]
fn markdown_cache_new_is_empty() {
    let cache = MarkdownCache::new();
    // Default just creates an empty cache; no public way to inspect size,
    // but clear() should not panic on empty.
    drop(cache);
}

#[test]
fn markdown_cache_default_matches_new() {
    let a = MarkdownCache::new();
    let b = MarkdownCache::default();
    // Both should be usable without panic.
    drop(a);
    drop(b);
}

#[test]
fn markdown_cache_clear_on_empty() {
    let mut cache = MarkdownCache::new();
    cache.clear(); // must not panic
}

// ---------------------------------------------------------------------------
// CachedChatData
// ---------------------------------------------------------------------------

#[test]
fn cached_chat_data_new_defaults() {
    let data = CachedChatData::new();
    assert!(data.display_messages.is_empty());
    assert_eq!(data.total_cost, 0.0);
    assert_eq!(data.total_tokens, 0);
    assert_eq!(data.generation, u64::MAX);
}

#[test]
fn cached_chat_data_default_matches_new() {
    let data = CachedChatData::default();
    assert_eq!(data.generation, u64::MAX);
}

// ---------------------------------------------------------------------------
// ToolCallDisplay
// ---------------------------------------------------------------------------

#[test]
fn tool_call_display_construction() {
    let tc = ToolCallDisplay {
        name: "read_file".into(),
        args: r#"{"path": "/tmp/test"}"#.into(),
    };
    assert_eq!(tc.name, "read_file");
    assert!(tc.args.contains("path"));
}

// ---------------------------------------------------------------------------
// DisplayMessage
// ---------------------------------------------------------------------------

#[test]
fn display_message_user_role() {
    let msg = DisplayMessage::user("Hello");
    assert_eq!(msg.role, MessageRole::User);
    assert_eq!(msg.content, "Hello");
}

#[test]
fn display_message_assistant_role() {
    let msg = DisplayMessage::assistant("Hi there");
    assert_eq!(msg.role, MessageRole::Assistant);
    assert_eq!(msg.content, "Hi there");
}

#[test]
fn display_message_error_role() {
    let msg = DisplayMessage::error("Something went wrong");
    assert_eq!(msg.role, MessageRole::Error);
    assert_eq!(msg.content, "Something went wrong");
}

#[test]
fn display_message_defaults_are_none() {
    let msg = DisplayMessage::user("test");
    assert!(msg.thinking.is_none());
    assert!(msg.model.is_none());
    assert!(msg.cost.is_none());
    assert!(msg.tokens.is_none());
    assert!(!msg.show_thinking);
    assert!(msg.tool_calls.is_empty());
    assert!(msg.tool_call_id.is_none());
}

// ---------------------------------------------------------------------------
// ChatPanel
// ---------------------------------------------------------------------------

#[test]
fn chat_panel_new_defaults() {
    let panel = ChatPanel::new();
    assert!(panel.messages.is_empty());
    assert!(panel.streaming_content.is_empty());
    assert!(panel.streaming_thinking.is_none());
    assert!(!panel.is_streaming);
    assert_eq!(panel.total_cost, 0.0);
    assert_eq!(panel.total_tokens, 0);
    assert_eq!(panel.current_model, "claude-sonnet-4-5");
}

#[test]
fn chat_panel_default_matches_new() {
    let panel = ChatPanel::default();
    assert_eq!(panel.current_model, "claude-sonnet-4-5");
}

#[test]
fn push_message_accumulates_cost() {
    let mut panel = ChatPanel::new();
    let mut msg = DisplayMessage::assistant("resp");
    msg.cost = Some(0.05);
    panel.push_message(msg);
    assert!((panel.total_cost - 0.05).abs() < f64::EPSILON);
}

#[test]
fn push_message_accumulates_tokens() {
    let mut panel = ChatPanel::new();
    let mut msg = DisplayMessage::assistant("resp");
    msg.tokens = Some(100);
    panel.push_message(msg);
    assert_eq!(panel.total_tokens, 100);
}

#[test]
fn push_message_no_cost_no_accumulation() {
    let mut panel = ChatPanel::new();
    let msg = DisplayMessage::user("hello");
    panel.push_message(msg);
    assert_eq!(panel.total_cost, 0.0);
    assert_eq!(panel.total_tokens, 0);
}

#[test]
fn push_message_multiple_accumulates() {
    let mut panel = ChatPanel::new();
    let mut m1 = DisplayMessage::assistant("a");
    m1.cost = Some(0.03);
    m1.tokens = Some(50);
    let mut m2 = DisplayMessage::assistant("b");
    m2.cost = Some(0.07);
    m2.tokens = Some(150);
    panel.push_message(m1);
    panel.push_message(m2);
    assert!((panel.total_cost - 0.10).abs() < f64::EPSILON);
    assert_eq!(panel.total_tokens, 200);
}

#[test]
fn start_streaming_sets_flag() {
    let mut panel = ChatPanel::new();
    panel.start_streaming();
    assert!(panel.is_streaming);
    assert!(panel.streaming_content.is_empty());
    assert!(panel.streaming_thinking.is_none());
}

#[test]
fn append_streaming_content() {
    let mut panel = ChatPanel::new();
    panel.start_streaming();
    panel.append_streaming("Hello ", None);
    panel.append_streaming("World", None);
    assert_eq!(panel.streaming_content, "Hello World");
    assert!(panel.streaming_thinking.is_none());
}

#[test]
fn append_streaming_thinking() {
    let mut panel = ChatPanel::new();
    panel.start_streaming();
    panel.append_streaming("", Some("step 1"));
    panel.append_streaming("", Some(" step 2"));
    assert_eq!(
        panel.streaming_thinking.as_deref(),
        Some("step 1 step 2")
    );
}

#[test]
fn finish_streaming_creates_message() {
    let mut panel = ChatPanel::new();
    panel.start_streaming();
    panel.append_streaming("response text", Some("thinking"));
    panel.finish_streaming(
        Some("claude-sonnet-4-5".into()),
        Some(0.02),
        Some(500),
    );
    assert!(!panel.is_streaming);
    assert!(panel.streaming_content.is_empty());
    assert_eq!(panel.messages.len(), 1);
    let msg = &panel.messages[0];
    assert_eq!(msg.role, MessageRole::Assistant);
    assert_eq!(msg.content, "response text");
    assert_eq!(msg.thinking.as_deref(), Some("thinking"));
    assert_eq!(msg.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(msg.cost, Some(0.02));
    assert_eq!(msg.tokens, Some(500));
}

#[test]
fn finish_streaming_accumulates_cost_tokens() {
    let mut panel = ChatPanel::new();
    panel.start_streaming();
    panel.append_streaming("text", None);
    panel.finish_streaming(None, Some(0.10), Some(1000));
    assert!((panel.total_cost - 0.10).abs() < f64::EPSILON);
    assert_eq!(panel.total_tokens, 1000);
}

#[test]
fn toggle_thinking_in_bounds() {
    let mut panel = ChatPanel::new();
    let mut msg = DisplayMessage::assistant("test");
    msg.thinking = Some("thought".into());
    panel.push_message(msg);
    assert!(!panel.messages[0].show_thinking);
    panel.toggle_thinking(0);
    assert!(panel.messages[0].show_thinking);
    panel.toggle_thinking(0);
    assert!(!panel.messages[0].show_thinking);
}

#[test]
fn toggle_thinking_out_of_bounds_no_panic() {
    let mut panel = ChatPanel::new();
    panel.toggle_thinking(99); // should not panic
}
