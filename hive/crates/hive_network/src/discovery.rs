//! LAN peer discovery via UDP broadcast.
//!
//! The [`DiscoveryService`] periodically broadcasts an announcement packet
//! on the local network and listens for announcements from other peers.
//! Discovered peers are reported through an mpsc channel.

use std::net::SocketAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

use crate::identity::PeerId;

/// An announcement broadcast by a peer on the LAN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    /// The peer's unique ID.
    pub peer_id: PeerId,
    /// The address the peer is listening on for WebSocket connections.
    pub listen_addr: String,
    /// Human-readable node name.
    pub name: String,
    /// Software version.
    pub version: String,
}

/// Event emitted when a peer is discovered on the LAN.
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    /// The discovered peer's announcement.
    pub announcement: Announcement,
    /// The source address of the UDP packet.
    pub source_addr: SocketAddr,
}

/// Configuration for the discovery service.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// UDP port to broadcast on and listen on.
    pub port: u16,
    /// How often to broadcast an announcement.
    pub interval: Duration,
    /// Our own announcement to broadcast.
    pub announcement: Announcement,
}

/// LAN discovery service using UDP broadcast.
pub struct DiscoveryService;

impl DiscoveryService {
    /// Start the discovery service in the background.
    ///
    /// Spawns two tasks:
    /// 1. A broadcaster that sends our announcement at the configured interval.
    /// 2. A listener that receives announcements from other peers and forwards
    ///    them through `discovered_tx`.
    ///
    /// Both tasks will exit when the shutdown signal is received.
    pub async fn start(
        config: DiscoveryConfig,
        discovered_tx: mpsc::Sender<DiscoveredPeer>,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), crate::error::NetworkError> {
        let bind_addr: SocketAddr = format!("0.0.0.0:{}", config.port)
            .parse()
            .expect("valid bind address from port number");

        // Bind the listener socket.
        let listener_socket = UdpSocket::bind(bind_addr)
            .await
            .map_err(|e| crate::error::NetworkError::Discovery(format!("Bind failed: {e}")))?;

        listener_socket
            .set_broadcast(true)
            .map_err(|e| crate::error::NetworkError::Discovery(format!("Set broadcast: {e}")))?;

        info!("Discovery service listening on {bind_addr}");

        let our_peer_id = config.announcement.peer_id.clone();
        let announcement_json = serde_json::to_string(&config.announcement).unwrap_or_default();
        let broadcast_addr: SocketAddr = format!("255.255.255.255:{}", config.port)
            .parse()
            .expect("valid broadcast address from port number");
        let interval = config.interval;

        // Bind a separate socket for sending broadcasts.
        let sender_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| crate::error::NetworkError::Discovery(format!("Sender bind: {e}")))?;

        sender_socket
            .set_broadcast(true)
            .map_err(|e| crate::error::NetworkError::Discovery(format!("Set broadcast: {e}")))?;

        // Spawn broadcaster.
        let announcement_bytes = announcement_json.into_bytes();
        let mut shutdown_bcast = shutdown.resubscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        match sender_socket.send_to(&announcement_bytes, broadcast_addr).await {
                            Ok(_) => trace!("Broadcast announcement sent"),
                            Err(e) => debug!("Broadcast send failed: {e}"),
                        }
                    }
                    _ = shutdown_bcast.recv() => {
                        debug!("Discovery broadcaster shutting down");
                        break;
                    }
                }
            }
        });

        // Spawn listener.
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            loop {
                tokio::select! {
                    result = listener_socket.recv_from(&mut buf) => {
                        match result {
                            Ok((len, src_addr)) => {
                                if let Ok(announcement) = serde_json::from_slice::<Announcement>(&buf[..len]) {
                                    // Skip our own announcements.
                                    if announcement.peer_id == our_peer_id {
                                        continue;
                                    }

                                    debug!("Discovered peer '{}' at {src_addr}", announcement.name);
                                    let _ = discovered_tx
                                        .send(DiscoveredPeer {
                                            announcement,
                                            source_addr: src_addr,
                                        })
                                        .await;
                                }
                            }
                            Err(e) => {
                                warn!("Discovery recv error: {e}");
                            }
                        }
                    }
                    _ = shutdown.recv() => {
                        debug!("Discovery listener shutting down");
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_announcement_serialize_roundtrip() {
        let announcement = Announcement {
            peer_id: PeerId::from_string("test-peer"),
            listen_addr: "127.0.0.1:9470".to_string(),
            name: "test-node".to_string(),
            version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&announcement).unwrap();
        let deserialized: Announcement = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.peer_id, announcement.peer_id);
        assert_eq!(deserialized.name, "test-node");
        assert_eq!(deserialized.listen_addr, "127.0.0.1:9470");
    }

    #[test]
    fn test_discovery_config_creation() {
        let config = DiscoveryConfig {
            port: 9471,
            interval: Duration::from_secs(5),
            announcement: Announcement {
                peer_id: PeerId::from_string("my-peer"),
                listen_addr: "0.0.0.0:9470".to_string(),
                name: "my-node".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        assert_eq!(config.port, 9471);
        assert_eq!(config.interval, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_discovery_udp_loopback() {
        // Test that we can send and receive on loopback.
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();

        let announcement = Announcement {
            peer_id: PeerId::from_string("loopback-peer"),
            listen_addr: "127.0.0.1:9470".to_string(),
            name: "loopback-node".to_string(),
            version: "0.1.0".to_string(),
        };
        let data = serde_json::to_vec(&announcement).unwrap();

        // Send to ourselves.
        socket.send_to(&data, addr).await.unwrap();

        let mut buf = vec![0u8; 4096];
        let (len, _) = socket.recv_from(&mut buf).await.unwrap();
        let received: Announcement = serde_json::from_slice(&buf[..len]).unwrap();
        assert_eq!(received.peer_id, announcement.peer_id);
        assert_eq!(received.name, "loopback-node");
    }
}
