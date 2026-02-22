use hive_ui_panels::panels::channels::{ChannelCreated, ChannelMessageSent};

// ---------------------------------------------------------------------------
// ChannelMessageSent
// ---------------------------------------------------------------------------

#[test]
fn channel_message_sent_construction() {
    let msg = ChannelMessageSent {
        channel_id: "ch-1".into(),
        content: "Hello team".into(),
        assigned_agents: vec!["agent-a".into(), "agent-b".into()],
    };
    assert_eq!(msg.channel_id, "ch-1");
    assert_eq!(msg.content, "Hello team");
    assert_eq!(msg.assigned_agents.len(), 2);
}

#[test]
fn channel_message_sent_empty_agents() {
    let msg = ChannelMessageSent {
        channel_id: "ch-2".into(),
        content: "Solo message".into(),
        assigned_agents: Vec::new(),
    };
    assert!(msg.assigned_agents.is_empty());
}

#[test]
fn channel_message_sent_field_access() {
    let msg = ChannelMessageSent {
        channel_id: "test".into(),
        content: "content".into(),
        assigned_agents: vec!["x".into()],
    };
    assert_eq!(msg.assigned_agents[0], "x");
}

// ---------------------------------------------------------------------------
// ChannelCreated
// ---------------------------------------------------------------------------

#[test]
fn channel_created_construction() {
    let created = ChannelCreated {
        name: "general".into(),
        agents: vec!["coder".into(), "reviewer".into()],
    };
    assert_eq!(created.name, "general");
    assert_eq!(created.agents.len(), 2);
}

#[test]
fn channel_created_empty_agents() {
    let created = ChannelCreated {
        name: "empty-channel".into(),
        agents: Vec::new(),
    };
    assert!(created.agents.is_empty());
}

#[test]
fn channel_created_multi_agents() {
    let created = ChannelCreated {
        name: "team".into(),
        agents: vec!["a".into(), "b".into(), "c".into()],
    };
    assert_eq!(created.agents.len(), 3);
    assert_eq!(created.agents[2], "c");
}
