//! Network configuration for a Hive node.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configuration for the Hive networking layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Address to listen on for incoming WebSocket connections.
    #[serde(with = "socket_addr_serde")]
    pub listen_addr: SocketAddr,

    /// Whether LAN discovery (UDP broadcast) is enabled.
    pub discovery_enabled: bool,

    /// UDP port used for LAN discovery announcements.
    pub discovery_port: u16,

    /// Maximum number of simultaneous peer connections.
    pub max_peers: usize,

    /// Interval between heartbeat pings to connected peers.
    #[serde(with = "duration_serde")]
    pub heartbeat_interval: Duration,

    /// Timeout for establishing a new connection.
    #[serde(with = "duration_serde")]
    pub connection_timeout: Duration,

    /// List of bootstrap peer addresses to connect to on startup.
    pub known_peers: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9470".parse().expect("valid default listen address"),
            discovery_enabled: true,
            discovery_port: 9471,
            max_peers: 32,
            heartbeat_interval: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(10),
            known_peers: Vec::new(),
        }
    }
}

impl NetworkConfig {
    /// Save the config to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write config file: {e}"))
    }

    /// Load config from a JSON file, or return defaults if the file is missing.
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(data) => match serde_json::from_str::<NetworkConfig>(&data) {
                    Ok(config) => return config,
                    Err(e) => {
                        tracing::warn!("Corrupt config file, using defaults: {e}");
                    }
                },
                Err(e) => {
                    tracing::warn!("Cannot read config file, using defaults: {e}");
                }
            }
        }
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Serde helpers
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

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(dur: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(dur.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NetworkConfig::default();
        assert_eq!(config.listen_addr.port(), 9470);
        assert!(config.discovery_enabled);
        assert_eq!(config.discovery_port, 9471);
        assert_eq!(config.max_peers, 32);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
        assert!(config.known_peers.is_empty());
    }

    #[test]
    fn test_config_serialize_roundtrip() {
        let config = NetworkConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: NetworkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.listen_addr, config.listen_addr);
        assert_eq!(deserialized.max_peers, config.max_peers);
        assert_eq!(deserialized.discovery_port, config.discovery_port);
    }

    #[test]
    fn test_config_save_load() {
        let dir = std::env::temp_dir().join("hive_network_test_config");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("config.json");
        let mut original = NetworkConfig::default();
        original.max_peers = 64;
        original.known_peers = vec!["192.168.1.100:9470".to_string()];
        original.save_to_file(&path).unwrap();

        let loaded = NetworkConfig::load_or_default(&path);
        assert_eq!(loaded.max_peers, 64);
        assert_eq!(loaded.known_peers.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_config_load_missing_returns_default() {
        let path = std::env::temp_dir().join("hive_network_nonexistent_config.json");
        let _ = std::fs::remove_file(&path);

        let config = NetworkConfig::load_or_default(&path);
        assert_eq!(config.max_peers, 32);
    }
}
