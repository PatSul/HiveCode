//! Peer identity — unique node identification and persistence.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A unique identifier for a peer node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub String);

impl PeerId {
    /// Generate a new random peer ID (UUID v4).
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Create a PeerId from an existing string.
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Return the inner string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The full identity of a Hive node on the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    /// Unique peer identifier.
    pub peer_id: PeerId,
    /// Human-readable name for the node (e.g. hostname).
    pub name: String,
    /// Software version string.
    pub version: String,
    /// Capabilities advertised by this node.
    pub capabilities: Vec<String>,
}

impl NodeIdentity {
    /// Create a new identity with a fresh PeerId.
    pub fn generate(name: impl Into<String>) -> Self {
        Self {
            peer_id: PeerId::generate(),
            name: name.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![
                "agent_relay".to_string(),
                "channel_sync".to_string(),
                "fleet_learn".to_string(),
            ],
        }
    }

    /// Save the identity to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize identity: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write identity file: {e}"))
    }

    /// Load an identity from a JSON file, or generate a new one if the file
    /// does not exist.
    pub fn load_or_generate(path: &Path, name: impl Into<String>) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(data) => match serde_json::from_str::<NodeIdentity>(&data) {
                    Ok(identity) => return identity,
                    Err(e) => {
                        tracing::warn!("Corrupt identity file, generating new: {e}");
                    }
                },
                Err(e) => {
                    tracing::warn!("Cannot read identity file, generating new: {e}");
                }
            }
        }

        let identity = Self::generate(name);
        if let Err(e) = identity.save_to_file(path) {
            tracing::warn!("Failed to persist new identity: {e}");
        }
        identity
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_id_generation() {
        let a = PeerId::generate();
        let b = PeerId::generate();
        assert_ne!(a, b);
        assert!(!a.as_str().is_empty());
    }

    #[test]
    fn test_peer_id_from_string() {
        let id = PeerId::from_string("test-peer-123");
        assert_eq!(id.as_str(), "test-peer-123");
        assert_eq!(format!("{id}"), "test-peer-123");
    }

    #[test]
    fn test_identity_generate() {
        let identity = NodeIdentity::generate("test-node");
        assert_eq!(identity.name, "test-node");
        assert!(!identity.peer_id.as_str().is_empty());
        assert!(!identity.capabilities.is_empty());
    }

    #[test]
    fn test_identity_serialize_roundtrip() {
        let identity = NodeIdentity::generate("roundtrip-node");
        let json = serde_json::to_string(&identity).unwrap();
        let deserialized: NodeIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.peer_id, identity.peer_id);
        assert_eq!(deserialized.name, identity.name);
        assert_eq!(deserialized.capabilities, identity.capabilities);
    }

    #[test]
    fn test_identity_save_load() {
        let dir = std::env::temp_dir().join("hive_network_test_identity");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("identity.json");
        let original = NodeIdentity::generate("persist-test");
        original.save_to_file(&path).unwrap();

        let loaded = NodeIdentity::load_or_generate(&path, "fallback-name");
        assert_eq!(loaded.peer_id, original.peer_id);
        assert_eq!(loaded.name, "persist-test");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_identity_load_missing_generates_new() {
        let path = std::env::temp_dir().join("hive_network_nonexistent_identity.json");
        let _ = std::fs::remove_file(&path);

        let identity = NodeIdentity::load_or_generate(&path, "new-node");
        assert_eq!(identity.name, "new-node");
        assert!(!identity.peer_id.as_str().is_empty());

        let _ = std::fs::remove_file(&path);
    }
}
