use hive_ui::chat_input::*;

#[test]
fn test_submit_message_event_contains_text() {
    let event = SubmitMessage("hello world".to_string());
    assert_eq!(event.0, "hello world");
}

#[test]
fn test_submit_message_clone() {
    let event = SubmitMessage("test".to_string());
    let cloned = event.clone();
    assert_eq!(cloned.0, "test");
}

#[test]
fn test_submit_message_debug() {
    let event = SubmitMessage("debug me".to_string());
    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("debug me"));
}
