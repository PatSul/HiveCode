//! Sync helpers — channel synchronization and fleet learning exchange.
//!
//! Provides helper types that create typed envelopes for specific sync
//! operations. These are thin wrappers making it easy for other crates
//! to build the right message payloads.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::PeerId;
use crate::message::{Envelope, MessageKind};

// ---------------------------------------------------------------------------
// Channel sync
// ---------------------------------------------------------------------------

/// A delta of channel messages to sync between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSyncPayload {
    /// The channel ID being synced.
    pub channel_id: String,
    /// The latest timestamp the sender has for this channel.
    pub last_known_timestamp: DateTime<Utc>,
    /// New messages since that timestamp (if sending updates).
    pub messages: Vec<SyncMessage>,
}

/// A single message in a channel sync payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    pub id: String,
    pub author: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub thread_id: Option<String>,
}

/// Helper for building channel sync envelopes.
pub struct ChannelSyncHelper;

impl ChannelSyncHelper {
    /// Create a sync request envelope asking a peer for channel updates.
    pub fn request_sync(
        from: PeerId,
        to: PeerId,
        channel_id: &str,
        last_known: DateTime<Utc>,
    ) -> Envelope {
        let payload = ChannelSyncPayload {
            channel_id: channel_id.to_string(),
            last_known_timestamp: last_known,
            messages: Vec::new(), // Empty = requesting updates
        };

        Envelope::new(
            from,
            Some(to),
            MessageKind::ChannelSync,
            serde_json::to_value(payload).unwrap_or_default(),
        )
    }

    /// Create a sync response envelope with new messages.
    pub fn send_updates(
        from: PeerId,
        to: PeerId,
        channel_id: &str,
        messages: Vec<SyncMessage>,
    ) -> Envelope {
        let last_ts = messages
            .last()
            .map(|m| m.timestamp)
            .unwrap_or_else(Utc::now);

        let payload = ChannelSyncPayload {
            channel_id: channel_id.to_string(),
            last_known_timestamp: last_ts,
            messages,
        };

        Envelope::new(
            from,
            Some(to),
            MessageKind::ChannelSync,
            serde_json::to_value(payload).unwrap_or_default(),
        )
    }

    /// Parse a channel sync payload from an envelope.
    pub fn parse_payload(envelope: &Envelope) -> Result<ChannelSyncPayload, String> {
        serde_json::from_value(envelope.payload.clone())
            .map_err(|e| format!("Invalid ChannelSync payload: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Fleet learning sync
// ---------------------------------------------------------------------------

/// A learning outcome to share across the fleet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetLearnPayload {
    /// The type of learning outcome (e.g. "model_preference", "routing_rule").
    pub outcome_type: String,
    /// Free-form context describing what was learned.
    pub context: String,
    /// The outcome data (model-specific or task-specific).
    pub data: serde_json::Value,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,
    /// When the learning was recorded.
    pub learned_at: DateTime<Utc>,
}

/// Helper for building fleet learning envelopes.
pub struct FleetSyncHelper;

impl FleetSyncHelper {
    /// Create a fleet learning broadcast envelope.
    pub fn share_learning(
        from: PeerId,
        outcome_type: &str,
        context: &str,
        data: serde_json::Value,
        confidence: f64,
    ) -> Envelope {
        let payload = FleetLearnPayload {
            outcome_type: outcome_type.to_string(),
            context: context.to_string(),
            data,
            confidence,
            learned_at: Utc::now(),
        };

        Envelope::broadcast(
            from,
            MessageKind::FleetLearn,
            serde_json::to_value(payload).unwrap_or_default(),
        )
    }

    /// Create a targeted fleet learning envelope for a specific peer.
    pub fn share_with_peer(
        from: PeerId,
        to: PeerId,
        outcome_type: &str,
        context: &str,
        data: serde_json::Value,
        confidence: f64,
    ) -> Envelope {
        let payload = FleetLearnPayload {
            outcome_type: outcome_type.to_string(),
            context: context.to_string(),
            data,
            confidence,
            learned_at: Utc::now(),
        };

        Envelope::new(
            from,
            Some(to),
            MessageKind::FleetLearn,
            serde_json::to_value(payload).unwrap_or_default(),
        )
    }

    /// Parse a fleet learning payload from an envelope.
    pub fn parse_payload(envelope: &Envelope) -> Result<FleetLearnPayload, String> {
        serde_json::from_value(envelope.payload.clone())
            .map_err(|e| format!("Invalid FleetLearn payload: {e}"))
    }
}

// ---------------------------------------------------------------------------
// State sync (generic)
// ---------------------------------------------------------------------------

/// A generic state sync payload for custom data exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSyncPayload {
    /// The type of state being synced (e.g. "skills", "config", "reminders").
    pub state_type: String,
    /// The version/revision of the state.
    pub revision: u64,
    /// The actual state data.
    pub data: serde_json::Value,
}

/// Helper for building generic state sync envelopes.
pub struct StateSyncHelper;

impl StateSyncHelper {
    /// Create a state sync envelope.
    pub fn sync_state(
        from: PeerId,
        to: Option<PeerId>,
        state_type: &str,
        revision: u64,
        data: serde_json::Value,
    ) -> Envelope {
        let payload = StateSyncPayload {
            state_type: state_type.to_string(),
            revision,
            data,
        };

        Envelope::new(
            from,
            to,
            MessageKind::StateSync,
            serde_json::to_value(payload).unwrap_or_default(),
        )
    }

    /// Parse a state sync payload from an envelope.
    pub fn parse_payload(envelope: &Envelope) -> Result<StateSyncPayload, String> {
        serde_json::from_value(envelope.payload.clone())
            .map_err(|e| format!("Invalid StateSync payload: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_sync_request() {
        let from = PeerId::from_string("peer-a");
        let to = PeerId::from_string("peer-b");
        let ts = Utc::now();

        let envelope = ChannelSyncHelper::request_sync(from, to, "general", ts);
        assert_eq!(envelope.kind, MessageKind::ChannelSync);

        let payload = ChannelSyncHelper::parse_payload(&envelope).unwrap();
        assert_eq!(payload.channel_id, "general");
        assert!(payload.messages.is_empty());
    }

    #[test]
    fn test_channel_sync_updates() {
        let messages = vec![
            SyncMessage {
                id: "msg-1".to_string(),
                author: "user".to_string(),
                content: "Hello".to_string(),
                timestamp: Utc::now(),
                thread_id: None,
            },
            SyncMessage {
                id: "msg-2".to_string(),
                author: "agent".to_string(),
                content: "Hi there".to_string(),
                timestamp: Utc::now(),
                thread_id: None,
            },
        ];

        let envelope = ChannelSyncHelper::send_updates(
            PeerId::from_string("peer-a"),
            PeerId::from_string("peer-b"),
            "debug",
            messages,
        );

        let payload = ChannelSyncHelper::parse_payload(&envelope).unwrap();
        assert_eq!(payload.channel_id, "debug");
        assert_eq!(payload.messages.len(), 2);
    }

    #[test]
    fn test_fleet_learn_broadcast() {
        let envelope = FleetSyncHelper::share_learning(
            PeerId::from_string("learner"),
            "model_preference",
            "Claude performs better on code tasks",
            serde_json::json!({"model": "claude-3", "task_type": "code", "score": 0.95}),
            0.9,
        );

        assert_eq!(envelope.kind, MessageKind::FleetLearn);
        assert!(envelope.to.is_none()); // Broadcast

        let payload = FleetSyncHelper::parse_payload(&envelope).unwrap();
        assert_eq!(payload.outcome_type, "model_preference");
        assert!((payload.confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fleet_learn_targeted() {
        let envelope = FleetSyncHelper::share_with_peer(
            PeerId::from_string("peer-a"),
            PeerId::from_string("peer-b"),
            "routing_rule",
            "Use GPT-4 for summarization",
            serde_json::json!({}),
            0.75,
        );

        assert!(envelope.to.is_some());
        assert_eq!(envelope.to.unwrap(), PeerId::from_string("peer-b"));
    }

    #[test]
    fn test_state_sync() {
        let envelope = StateSyncHelper::sync_state(
            PeerId::from_string("peer-a"),
            Some(PeerId::from_string("peer-b")),
            "skills",
            42,
            serde_json::json!({"installed": ["code-gen", "test-gen"]}),
        );

        assert_eq!(envelope.kind, MessageKind::StateSync);

        let payload = StateSyncHelper::parse_payload(&envelope).unwrap();
        assert_eq!(payload.state_type, "skills");
        assert_eq!(payload.revision, 42);
    }

    #[test]
    fn test_payload_serialize_roundtrip() {
        let payload = FleetLearnPayload {
            outcome_type: "test".to_string(),
            context: "Testing roundtrip".to_string(),
            data: serde_json::json!({"key": "value"}),
            confidence: 0.85,
            learned_at: Utc::now(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: FleetLearnPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.outcome_type, "test");
        assert!((deserialized.confidence - 0.85).abs() < f64::EPSILON);
    }
}
