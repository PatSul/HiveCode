//! Peer registry — tracking known peers and their connection state.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::{NodeIdentity, PeerId};

/// Connection state of a peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerState {
    /// Found via discovery but not yet connected.
    Discovered,
    /// WebSocket handshake in progress.
    Connecting,
    /// Active WebSocket connection established.
    Connected,
    /// Was connected but now disconnected.
    Disconnected,
    /// Rejected or misbehaving peer.
    Banned,
}

/// Information about a known peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// The peer's unique identifier.
    pub id: PeerId,
    /// The peer's full identity (name, version, capabilities).
    pub identity: NodeIdentity,
    /// The peer's network address.
    #[serde(with = "socket_addr_serde")]
    pub addr: SocketAddr,
    /// Current connection state.
    pub state: PeerState,
    /// When the peer was first connected (if ever).
    pub connected_at: Option<DateTime<Utc>>,
    /// Last time we heard from this peer.
    pub last_seen: DateTime<Utc>,
    /// Round-trip latency in milliseconds (from heartbeat).
    pub latency_ms: Option<u64>,
}

/// Registry of all known peers.
#[derive(Debug, Serialize, Deserialize)]
pub struct PeerRegistry {
    peers: HashMap<String, PeerInfo>,
}

impl PeerRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    /// Add or update a peer in the registry.
    pub fn add_peer(&mut self, info: PeerInfo) {
        self.peers.insert(info.id.as_str().to_string(), info);
    }

    /// Remove a peer by ID.
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Option<PeerInfo> {
        self.peers.remove(peer_id.as_str())
    }

    /// Get a peer by ID.
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.get(peer_id.as_str())
    }

    /// Get a mutable reference to a peer by ID.
    pub fn get_peer_mut(&mut self, peer_id: &PeerId) -> Option<&mut PeerInfo> {
        self.peers.get_mut(peer_id.as_str())
    }

    /// List all peers that are currently connected.
    pub fn list_connected(&self) -> Vec<&PeerInfo> {
        self.peers
            .values()
            .filter(|p| p.state == PeerState::Connected)
            .collect()
    }

    /// List all known peers regardless of state.
    pub fn list_all(&self) -> Vec<&PeerInfo> {
        self.peers.values().collect()
    }

    /// Return the number of connected peers.
    pub fn connected_count(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.state == PeerState::Connected)
            .count()
    }

    /// Return the total number of known peers.
    pub fn total_count(&self) -> usize {
        self.peers.len()
    }

    /// Update the state of a peer.
    pub fn update_state(&mut self, peer_id: &PeerId, state: PeerState) {
        if let Some(peer) = self.peers.get_mut(peer_id.as_str()) {
            if state == PeerState::Connected && peer.state != PeerState::Connected {
                peer.connected_at = Some(Utc::now());
            }
            peer.state = state;
        }
    }

    /// Update the last-seen timestamp for a peer.
    pub fn update_last_seen(&mut self, peer_id: &PeerId) {
        if let Some(peer) = self.peers.get_mut(peer_id.as_str()) {
            peer.last_seen = Utc::now();
        }
    }

    /// Update the latency measurement for a peer.
    pub fn update_latency(&mut self, peer_id: &PeerId, latency_ms: u64) {
        if let Some(peer) = self.peers.get_mut(peer_id.as_str()) {
            peer.latency_ms = Some(latency_ms);
        }
    }

    /// Save the registry to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize registry: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write registry file: {e}"))
    }

    /// Load the registry from a JSON file, or return an empty one.
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(data) => match serde_json::from_str::<PeerRegistry>(&data) {
                    Ok(mut registry) => {
                        // Mark all loaded peers as disconnected (fresh start).
                        for peer in registry.peers.values_mut() {
                            if peer.state == PeerState::Connected
                                || peer.state == PeerState::Connecting
                            {
                                peer.state = PeerState::Disconnected;
                            }
                        }
                        return registry;
                    }
                    Err(e) => {
                        tracing::warn!("Corrupt peer registry file: {e}");
                    }
                },
                Err(e) => {
                    tracing::warn!("Cannot read peer registry file: {e}");
                }
            }
        }
        Self::new()
    }
}

impl Default for PeerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Serde helper for SocketAddr
// ---------------------------------------------------------------------------

mod socket_addr_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::net::SocketAddr;

    pub fn serialize<S: Serializer>(addr: &SocketAddr, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&addr.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SocketAddr, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::NodeIdentity;

    fn make_peer(name: &str, port: u16) -> PeerInfo {
        let identity = NodeIdentity::generate(name);
        PeerInfo {
            id: identity.peer_id.clone(),
            identity,
            addr: format!("127.0.0.1:{port}").parse().unwrap(),
            state: PeerState::Discovered,
            connected_at: None,
            last_seen: Utc::now(),
            latency_ms: None,
        }
    }

    #[test]
    fn test_registry_add_and_get() {
        let mut registry = PeerRegistry::new();
        let peer = make_peer("alpha", 9470);
        let peer_id = peer.id.clone();

        registry.add_peer(peer);
        assert_eq!(registry.total_count(), 1);
        assert!(registry.get_peer(&peer_id).is_some());
    }

    #[test]
    fn test_registry_remove() {
        let mut registry = PeerRegistry::new();
        let peer = make_peer("beta", 9471);
        let peer_id = peer.id.clone();

        registry.add_peer(peer);
        let removed = registry.remove_peer(&peer_id);
        assert!(removed.is_some());
        assert_eq!(registry.total_count(), 0);
    }

    #[test]
    fn test_registry_state_transitions() {
        let mut registry = PeerRegistry::new();
        let peer = make_peer("gamma", 9472);
        let peer_id = peer.id.clone();

        registry.add_peer(peer);
        assert_eq!(
            registry.get_peer(&peer_id).unwrap().state,
            PeerState::Discovered
        );

        registry.update_state(&peer_id, PeerState::Connecting);
        assert_eq!(
            registry.get_peer(&peer_id).unwrap().state,
            PeerState::Connecting
        );

        registry.update_state(&peer_id, PeerState::Connected);
        assert_eq!(
            registry.get_peer(&peer_id).unwrap().state,
            PeerState::Connected
        );
        assert!(registry.get_peer(&peer_id).unwrap().connected_at.is_some());
    }

    #[test]
    fn test_registry_list_connected() {
        let mut registry = PeerRegistry::new();

        let mut p1 = make_peer("delta", 9473);
        p1.state = PeerState::Connected;
        let mut p2 = make_peer("epsilon", 9474);
        p2.state = PeerState::Disconnected;
        let mut p3 = make_peer("zeta", 9475);
        p3.state = PeerState::Connected;

        registry.add_peer(p1);
        registry.add_peer(p2);
        registry.add_peer(p3);

        assert_eq!(registry.connected_count(), 2);
        assert_eq!(registry.list_connected().len(), 2);
        assert_eq!(registry.list_all().len(), 3);
    }

    #[test]
    fn test_registry_latency_update() {
        let mut registry = PeerRegistry::new();
        let peer = make_peer("eta", 9476);
        let peer_id = peer.id.clone();

        registry.add_peer(peer);
        assert!(registry.get_peer(&peer_id).unwrap().latency_ms.is_none());

        registry.update_latency(&peer_id, 42);
        assert_eq!(registry.get_peer(&peer_id).unwrap().latency_ms, Some(42));
    }

    #[test]
    fn test_registry_save_load() {
        let dir = std::env::temp_dir().join("hive_network_test_registry");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("peers.json");

        let mut registry = PeerRegistry::new();
        let mut peer = make_peer("theta", 9477);
        peer.state = PeerState::Connected;
        registry.add_peer(peer);
        registry.save_to_file(&path).unwrap();

        // Load and verify connected peers are reset to Disconnected.
        let loaded = PeerRegistry::load_or_default(&path);
        assert_eq!(loaded.total_count(), 1);
        let loaded_peer = loaded.list_all()[0];
        assert_eq!(loaded_peer.state, PeerState::Disconnected);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
