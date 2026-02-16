//! Network message protocol — envelope-based typed messaging.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::PeerId;

/// The kind of message carried in an [`Envelope`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    // ── Discovery & handshake ───────────────────────────────────────
    /// Initial peer introduction (includes NodeIdentity).
    Hello,
    /// Response to Hello, acknowledging the connection.
    Welcome,
    /// Clean disconnect notification.
    Goodbye,
    /// Keep-alive ping.
    Heartbeat,
    /// Keep-alive pong (response to Heartbeat).
    HeartbeatAck,

    // ── Agent relay ─────────────────────────────────────────────────
    /// Forward a task to a remote swarm for execution.
    TaskRequest,
    /// Return the result of a remotely-executed task.
    TaskResult,
    /// Relay an agent message to a remote peer.
    AgentRelay,

    // ── Sync ────────────────────────────────────────────────────────
    /// Synchronize channel messages between peers.
    ChannelSync,
    /// Share fleet learning outcomes across peers.
    FleetLearn,
    /// Generic state synchronization payload.
    StateSync,

    // ── Extensible ──────────────────────────────────────────────────
    /// User-defined message type for extensions.
    Custom(String),
}

impl MessageKind {
    /// Return a string key used for handler dispatch.
    pub fn dispatch_key(&self) -> String {
        match self {
            Self::Hello => "hello".to_string(),
            Self::Welcome => "welcome".to_string(),
            Self::Goodbye => "goodbye".to_string(),
            Self::Heartbeat => "heartbeat".to_string(),
            Self::HeartbeatAck => "heartbeat_ack".to_string(),
            Self::TaskRequest => "task_request".to_string(),
            Self::TaskResult => "task_result".to_string(),
            Self::AgentRelay => "agent_relay".to_string(),
            Self::ChannelSync => "channel_sync".to_string(),
            Self::FleetLearn => "fleet_learn".to_string(),
            Self::StateSync => "state_sync".to_string(),
            Self::Custom(name) => format!("custom:{name}"),
        }
    }
}

/// A network message envelope carrying a typed payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// Unique message identifier (UUID v4).
    pub id: String,
    /// PeerId of the sender.
    pub from: PeerId,
    /// Target peer. `None` means broadcast to all connected peers.
    pub to: Option<PeerId>,
    /// The kind/type of message.
    pub kind: MessageKind,
    /// The payload data (JSON value, interpreted based on `kind`).
    pub payload: serde_json::Value,
    /// When the message was created.
    pub timestamp: DateTime<Utc>,
}

impl Envelope {
    /// Create a new envelope from this node to a specific peer.
    pub fn new(from: PeerId, to: Option<PeerId>, kind: MessageKind, payload: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from,
            to,
            kind,
            payload,
            timestamp: Utc::now(),
        }
    }

    /// Create a broadcast envelope (to = None).
    pub fn broadcast(from: PeerId, kind: MessageKind, payload: serde_json::Value) -> Self {
        Self::new(from, None, kind, payload)
    }

    /// Serialize the envelope to a JSON string for transmission.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize an envelope from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_creation() {
        let from = PeerId::from_string("peer-a");
        let to = PeerId::from_string("peer-b");
        let env = Envelope::new(
            from.clone(),
            Some(to.clone()),
            MessageKind::Hello,
            serde_json::json!({"name": "test-node"}),
        );

        assert_eq!(env.from, from);
        assert_eq!(env.to, Some(to));
        assert_eq!(env.kind, MessageKind::Hello);
        assert!(!env.id.is_empty());
    }

    #[test]
    fn test_envelope_broadcast() {
        let from = PeerId::from_string("peer-a");
        let env = Envelope::broadcast(from, MessageKind::Heartbeat, serde_json::json!({}));
        assert!(env.to.is_none());
        assert_eq!(env.kind, MessageKind::Heartbeat);
    }

    #[test]
    fn test_envelope_serialize_roundtrip() {
        let env = Envelope::new(
            PeerId::from_string("peer-a"),
            Some(PeerId::from_string("peer-b")),
            MessageKind::TaskRequest,
            serde_json::json!({"task": "build project", "budget": 1.0}),
        );

        let json = env.to_json().unwrap();
        let deserialized = Envelope::from_json(&json).unwrap();
        assert_eq!(deserialized.id, env.id);
        assert_eq!(deserialized.from, env.from);
        assert_eq!(deserialized.to, env.to);
        assert_eq!(deserialized.kind, env.kind);
    }

    #[test]
    fn test_all_message_kinds_serialize() {
        let kinds = vec![
            MessageKind::Hello,
            MessageKind::Welcome,
            MessageKind::Goodbye,
            MessageKind::Heartbeat,
            MessageKind::HeartbeatAck,
            MessageKind::TaskRequest,
            MessageKind::TaskResult,
            MessageKind::AgentRelay,
            MessageKind::ChannelSync,
            MessageKind::FleetLearn,
            MessageKind::StateSync,
            MessageKind::Custom("my_extension".to_string()),
        ];

        for kind in &kinds {
            let json = serde_json::to_string(kind).unwrap();
            let deserialized: MessageKind = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, kind);
        }
    }

    #[test]
    fn test_dispatch_keys() {
        assert_eq!(MessageKind::Hello.dispatch_key(), "hello");
        assert_eq!(MessageKind::TaskRequest.dispatch_key(), "task_request");
        assert_eq!(
            MessageKind::Custom("foo".to_string()).dispatch_key(),
            "custom:foo"
        );
    }
}
