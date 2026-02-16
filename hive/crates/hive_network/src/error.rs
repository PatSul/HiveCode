//! Network error types.

use std::time::Duration;

/// Errors that can occur in the hive_network crate.
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    /// A transport-level error (WebSocket connect/send/receive).
    #[error("Transport error: {0}")]
    Transport(String),

    /// The requested peer was not found in the registry.
    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    /// JSON serialization / deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Discovery subsystem error.
    #[error("Discovery error: {0}")]
    Discovery(String),

    /// A peer explicitly refused the connection.
    #[error("Connection refused by {0}")]
    ConnectionRefused(String),

    /// An operation timed out.
    #[error("Timeout after {0:?}")]
    Timeout(Duration),

    /// The node is not running.
    #[error("Node not running")]
    NotRunning,

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
