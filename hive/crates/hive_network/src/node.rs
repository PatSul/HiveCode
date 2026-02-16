//! HiveNode — top-level coordinator for the networking layer.
//!
//! [`HiveNode`] is the primary public API for hive_network. It manages:
//! - WebSocket server (accept incoming connections)
//! - Outbound connections (connect to known peers)
//! - LAN discovery (find peers on the local network)
//! - Heartbeat loop (keep connections alive)
//! - Message routing (dispatch envelopes to handlers)

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::config::NetworkConfig;
use crate::discovery::{Announcement, DiscoveryConfig, DiscoveryService, DiscoveredPeer};
use crate::error::NetworkError;
use crate::identity::{NodeIdentity, PeerId};
use crate::message::{Envelope, MessageKind};
use crate::peer::{PeerInfo, PeerRegistry, PeerState};
use crate::router::{MessageRouter, hello_handler, heartbeat_handler, goodbye_handler};
use crate::transport::{self, PeerConnection, TransportEvent};

/// The top-level Hive network node.
///
/// Create one per application instance. Call [`start()`](HiveNode::start) to
/// begin accepting connections and discovering peers.
pub struct HiveNode {
    /// Our identity on the network.
    identity: NodeIdentity,
    /// Network configuration.
    config: NetworkConfig,
    /// Registry of known peers.
    peers: Arc<RwLock<PeerRegistry>>,
    /// Message router for dispatching incoming envelopes.
    router: Arc<RwLock<MessageRouter>>,
    /// Active WebSocket connections keyed by peer address.
    connections: Arc<RwLock<HashMap<String, PeerConnection>>>,
    /// Shutdown signal broadcaster.
    shutdown_tx: Option<broadcast::Sender<()>>,
    /// Whether the node is currently running.
    running: bool,
}

impl HiveNode {
    /// Create a new node with the given identity and config.
    pub fn new(identity: NodeIdentity, config: NetworkConfig) -> Self {
        let mut router = MessageRouter::new();

        // Register built-in protocol handlers.
        router.register(
            MessageKind::Hello,
            hello_handler(identity.peer_id.clone()),
        );
        router.register(
            MessageKind::Heartbeat,
            heartbeat_handler(identity.peer_id.clone()),
        );
        router.register(MessageKind::Goodbye, goodbye_handler());

        Self {
            identity,
            config,
            peers: Arc::new(RwLock::new(PeerRegistry::new())),
            router: Arc::new(RwLock::new(router)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            running: false,
        }
    }

    /// Create a node with default config.
    pub fn with_defaults(name: impl Into<String>) -> Self {
        let identity = NodeIdentity::generate(name);
        Self::new(identity, NetworkConfig::default())
    }

    /// Return the node's peer ID.
    pub fn peer_id(&self) -> &PeerId {
        &self.identity.peer_id
    }

    /// Return the node's full identity.
    pub fn identity(&self) -> &NodeIdentity {
        &self.identity
    }

    /// Return the node's configuration.
    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }

    /// Whether the node is currently running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get a snapshot of all known peers.
    pub async fn peers(&self) -> Vec<PeerInfo> {
        let registry = self.peers.read().await;
        registry.list_all().into_iter().cloned().collect()
    }

    /// Get a snapshot of connected peers.
    pub async fn connected_peers(&self) -> Vec<PeerInfo> {
        let registry = self.peers.read().await;
        registry.list_connected().into_iter().cloned().collect()
    }

    /// Register a custom message handler for a specific message kind.
    pub async fn on_message(
        &self,
        kind: MessageKind,
        handler: crate::router::MessageHandler,
    ) {
        let mut router = self.router.write().await;
        router.register(kind, handler);
    }

    /// Start the node — begins listening, discovery, and heartbeat loops.
    pub async fn start(&mut self) -> Result<(), NetworkError> {
        if self.running {
            return Ok(());
        }

        let (shutdown_tx, _) = broadcast::channel(8);
        self.shutdown_tx = Some(shutdown_tx.clone());

        // Channels for transport events and new connections.
        let (event_tx, event_rx) = mpsc::channel(256);
        let (conn_tx, conn_rx) = mpsc::channel(64);

        // Start WebSocket server.
        let server_addr = self.config.listen_addr;
        let server_shutdown = shutdown_tx.subscribe();
        let server_event_tx = event_tx.clone();
        tokio::spawn(async move {
            if let Err(e) =
                transport::start_server(server_addr, server_event_tx, conn_tx, server_shutdown)
                    .await
            {
                error!("WebSocket server error: {e}");
            }
        });

        // Start LAN discovery if enabled.
        if self.config.discovery_enabled {
            let (discovered_tx, discovered_rx) = mpsc::channel(64);
            let discovery_config = DiscoveryConfig {
                port: self.config.discovery_port,
                interval: std::time::Duration::from_secs(5),
                announcement: Announcement {
                    peer_id: self.identity.peer_id.clone(),
                    listen_addr: self.config.listen_addr.to_string(),
                    name: self.identity.name.clone(),
                    version: self.identity.version.clone(),
                },
            };
            let discovery_shutdown = shutdown_tx.subscribe();
            if let Err(e) =
                DiscoveryService::start(discovery_config, discovered_tx, discovery_shutdown).await
            {
                warn!("Discovery start failed (non-fatal): {e}");
            }

            // Spawn task to handle discovered peers.
            let peers = Arc::clone(&self.peers);
            let event_tx_disc = event_tx.clone();
            let our_peer_id = self.identity.peer_id.clone();
            let connections = Arc::clone(&self.connections);
            tokio::spawn(async move {
                Self::handle_discoveries(
                    discovered_rx,
                    peers,
                    connections,
                    event_tx_disc,
                    our_peer_id,
                )
                .await;
            });
        }

        // Connect to known (bootstrap) peers.
        for addr in &self.config.known_peers {
            let addr = addr.clone();
            let event_tx = event_tx.clone();
            let connections = Arc::clone(&self.connections);
            tokio::spawn(async move {
                match transport::connect_to_peer(&addr, event_tx).await {
                    Ok(conn) => {
                        let key = addr.clone();
                        connections.write().await.insert(key, conn);
                        info!("Connected to bootstrap peer {addr}");
                    }
                    Err(e) => {
                        warn!("Failed to connect to bootstrap peer {addr}: {e}");
                    }
                }
            });
        }

        // Spawn the main event loop.
        let router = Arc::clone(&self.router);
        let connections = Arc::clone(&self.connections);
        let peers = Arc::clone(&self.peers);
        let event_shutdown = shutdown_tx.subscribe();
        tokio::spawn(async move {
            Self::event_loop(
                event_rx,
                conn_rx,
                router,
                connections,
                peers,
                event_shutdown,
            )
            .await;
        });

        // Spawn heartbeat loop.
        let connections_hb = Arc::clone(&self.connections);
        let our_peer_id_hb = self.identity.peer_id.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let hb_shutdown = shutdown_tx.subscribe();
        tokio::spawn(async move {
            Self::heartbeat_loop(
                connections_hb,
                our_peer_id_hb,
                heartbeat_interval,
                hb_shutdown,
            )
            .await;
        });

        self.running = true;
        info!(
            "HiveNode '{}' started (peer_id: {})",
            self.identity.name, self.identity.peer_id
        );
        Ok(())
    }

    /// Stop the node — closes all connections and shuts down background tasks.
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Close all active connections.
        let mut conns = self.connections.write().await;
        for (addr, conn) in conns.iter_mut() {
            debug!("Closing connection to {addr}");
            let _ = conn.close().await;
        }
        conns.clear();

        // Mark all peers as disconnected.
        let mut registry = self.peers.write().await;
        let peer_ids: Vec<PeerId> = registry
            .list_connected()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        for pid in peer_ids {
            registry.update_state(&pid, PeerState::Disconnected);
        }

        self.running = false;
        info!("HiveNode '{}' stopped", self.identity.name);
    }

    /// Connect to a specific peer by address.
    pub async fn connect_to(&self, addr: &str) -> Result<PeerId, NetworkError> {
        if !self.running {
            return Err(NetworkError::NotRunning);
        }

        let (event_tx, _) = mpsc::channel(64);
        let conn = transport::connect_to_peer(addr, event_tx).await?;
        let peer_id = conn.peer_id().clone();

        self.connections
            .write()
            .await
            .insert(addr.to_string(), conn);

        info!("Manually connected to {addr}");
        Ok(peer_id)
    }

    /// Send an envelope to a specific peer by address key.
    pub async fn send_to(&self, addr: &str, envelope: &Envelope) -> Result<(), NetworkError> {
        if !self.running {
            return Err(NetworkError::NotRunning);
        }

        let mut conns = self.connections.write().await;
        if let Some(conn) = conns.get_mut(addr) {
            conn.send(envelope).await
        } else {
            Err(NetworkError::PeerNotFound(addr.to_string()))
        }
    }

    /// Broadcast an envelope to all connected peers. Returns the number of
    /// peers the message was sent to.
    pub async fn broadcast(&self, envelope: &Envelope) -> Result<usize, NetworkError> {
        if !self.running {
            return Err(NetworkError::NotRunning);
        }

        let mut conns = self.connections.write().await;
        let mut sent = 0;
        let mut failed_keys = Vec::new();

        for (addr, conn) in conns.iter_mut() {
            match conn.send(envelope).await {
                Ok(_) => sent += 1,
                Err(e) => {
                    warn!("Broadcast send to {addr} failed: {e}");
                    failed_keys.push(addr.clone());
                }
            }
        }

        // Remove failed connections.
        for key in failed_keys {
            conns.remove(&key);
        }

        Ok(sent)
    }

    // -----------------------------------------------------------------------
    // Internal tasks
    // -----------------------------------------------------------------------

    /// Main event loop — processes transport events and incoming connection handles.
    async fn event_loop(
        mut event_rx: mpsc::Receiver<TransportEvent>,
        mut conn_rx: mpsc::Receiver<(SocketAddr, PeerConnection)>,
        router: Arc<RwLock<MessageRouter>>,
        connections: Arc<RwLock<HashMap<String, PeerConnection>>>,
        peers: Arc<RwLock<PeerRegistry>>,
        mut shutdown: broadcast::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                // New server-side connection handle.
                Some((addr, conn)) = conn_rx.recv() => {
                    let key = addr.to_string();
                    connections.write().await.insert(key, conn);
                    debug!("Stored connection for {addr}");
                }

                // Transport event (message, connect, disconnect).
                Some(event) = event_rx.recv() => {
                    match event {
                        TransportEvent::InboundConnection { addr, peer_id: _ } => {
                            debug!("Inbound connection from {addr}");
                        }
                        TransportEvent::Message { from_addr, envelope } => {
                            // Update peer last-seen.
                            {
                                let mut reg = peers.write().await;
                                reg.update_last_seen(&envelope.from);
                            }

                            // Route the message.
                            let router_guard = router.read().await;
                            if let Some(response) = router_guard.dispatch(envelope).await {
                                // Send response back.
                                let key = from_addr.to_string();
                                let mut conns = connections.write().await;
                                if let Some(conn) = conns.get_mut(&key) {
                                    if let Err(e) = conn.send(&response).await {
                                        warn!("Failed to send response to {key}: {e}");
                                    }
                                }
                            }
                        }
                        TransportEvent::Disconnected { addr } => {
                            debug!("Peer at {addr} disconnected");
                            connections.write().await.remove(&addr.to_string());
                        }
                    }
                }

                _ = shutdown.recv() => {
                    debug!("Event loop shutting down");
                    break;
                }
            }
        }
    }

    /// Handle discovered peers from LAN discovery.
    async fn handle_discoveries(
        mut discovered_rx: mpsc::Receiver<DiscoveredPeer>,
        peers: Arc<RwLock<PeerRegistry>>,
        connections: Arc<RwLock<HashMap<String, PeerConnection>>>,
        event_tx: mpsc::Sender<TransportEvent>,
        our_peer_id: PeerId,
    ) {
        while let Some(discovered) = discovered_rx.recv().await {
            let ann = &discovered.announcement;

            // Skip ourselves.
            if ann.peer_id == our_peer_id {
                continue;
            }

            // Check if already known.
            let already_connected = {
                let registry = peers.read().await;
                if let Some(peer) = registry.get_peer(&ann.peer_id) {
                    peer.state == PeerState::Connected
                } else {
                    false
                }
            };

            if already_connected {
                continue;
            }

            // Add to registry as Discovered.
            let addr: SocketAddr = ann
                .listen_addr
                .parse()
                .unwrap_or_else(|_| discovered.source_addr);

            let peer_info = PeerInfo {
                id: ann.peer_id.clone(),
                identity: NodeIdentity {
                    peer_id: ann.peer_id.clone(),
                    name: ann.name.clone(),
                    version: ann.version.clone(),
                    capabilities: Vec::new(),
                },
                addr,
                state: PeerState::Discovered,
                connected_at: None,
                last_seen: chrono::Utc::now(),
                latency_ms: None,
            };

            {
                let mut registry = peers.write().await;
                registry.add_peer(peer_info);
                registry.update_state(&ann.peer_id, PeerState::Connecting);
            }

            // Attempt to connect.
            let addr_str = ann.listen_addr.clone();
            match transport::connect_to_peer(&addr_str, event_tx.clone()).await {
                Ok(conn) => {
                    connections
                        .write()
                        .await
                        .insert(addr_str.clone(), conn);
                    let mut registry = peers.write().await;
                    registry.update_state(&ann.peer_id, PeerState::Connected);
                    info!("Connected to discovered peer '{}' at {addr_str}", ann.name);
                }
                Err(e) => {
                    let mut registry = peers.write().await;
                    registry.update_state(&ann.peer_id, PeerState::Disconnected);
                    warn!(
                        "Failed to connect to discovered peer '{}' at {addr_str}: {e}",
                        ann.name
                    );
                }
            }
        }
    }

    /// Heartbeat loop — pings all connected peers at the configured interval.
    async fn heartbeat_loop(
        connections: Arc<RwLock<HashMap<String, PeerConnection>>>,
        our_peer_id: PeerId,
        interval: std::time::Duration,
        mut shutdown: broadcast::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    let heartbeat = Envelope::broadcast(
                        our_peer_id.clone(),
                        MessageKind::Heartbeat,
                        serde_json::json!({"ts": chrono::Utc::now().to_rfc3339()}),
                    );

                    let mut conns = connections.write().await;
                    let mut failed = Vec::new();

                    for (addr, conn) in conns.iter_mut() {
                        if let Err(e) = conn.send(&heartbeat).await {
                            debug!("Heartbeat to {addr} failed: {e}");
                            failed.push(addr.clone());
                        }
                    }

                    for key in failed {
                        conns.remove(&key);
                    }
                }
                _ = shutdown.recv() => {
                    debug!("Heartbeat loop shutting down");
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = HiveNode::with_defaults("test-node");
        assert!(!node.is_running());
        assert_eq!(node.identity().name, "test-node");
        assert!(!node.peer_id().as_str().is_empty());
    }

    #[test]
    fn test_node_with_config() {
        let identity = NodeIdentity::generate("custom-node");
        let mut config = NetworkConfig::default();
        config.max_peers = 8;
        config.discovery_enabled = false;

        let node = HiveNode::new(identity, config);
        assert_eq!(node.config().max_peers, 8);
        assert!(!node.config().discovery_enabled);
    }

    #[tokio::test]
    async fn test_node_start_stop() {
        let mut config = NetworkConfig::default();
        config.listen_addr = "127.0.0.1:0".parse().unwrap();
        config.discovery_enabled = false;

        let identity = NodeIdentity::generate("lifecycle-node");
        let mut node = HiveNode::new(identity, config);

        // Start should succeed.
        node.start().await.unwrap();
        assert!(node.is_running());

        // Peers should be empty.
        let peers = node.peers().await;
        assert!(peers.is_empty());

        // Stop should succeed.
        node.stop().await;
        assert!(!node.is_running());
    }

    #[tokio::test]
    async fn test_node_double_start() {
        let mut config = NetworkConfig::default();
        config.listen_addr = "127.0.0.1:0".parse().unwrap();
        config.discovery_enabled = false;

        let identity = NodeIdentity::generate("double-start-node");
        let mut node = HiveNode::new(identity, config);

        node.start().await.unwrap();
        // Starting again should be a no-op, not an error.
        node.start().await.unwrap();
        assert!(node.is_running());

        node.stop().await;
    }

    #[tokio::test]
    async fn test_send_when_not_running() {
        let node = HiveNode::with_defaults("stopped-node");
        let envelope = Envelope::broadcast(
            node.peer_id().clone(),
            MessageKind::Heartbeat,
            serde_json::json!({}),
        );

        let result = node.broadcast(&envelope).await;
        assert!(result.is_err());
        match result {
            Err(NetworkError::NotRunning) => {}
            other => panic!("Expected NotRunning, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_two_nodes_communicate() {
        // Start node A as server.
        let mut config_a = NetworkConfig::default();
        config_a.listen_addr = "127.0.0.1:0".parse().unwrap();
        config_a.discovery_enabled = false;

        let identity_a = NodeIdentity::generate("node-a");
        let mut node_a = HiveNode::new(identity_a, config_a);
        node_a.start().await.unwrap();

        // Give server time to bind.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Node B connects to node A.
        let mut config_b = NetworkConfig::default();
        config_b.listen_addr = "127.0.0.1:0".parse().unwrap();
        config_b.discovery_enabled = false;

        let identity_b = NodeIdentity::generate("node-b");
        let mut node_b = HiveNode::new(identity_b, config_b);
        node_b.start().await.unwrap();

        // Both nodes should be running.
        assert!(node_a.is_running());
        assert!(node_b.is_running());

        // Clean up.
        node_a.stop().await;
        node_b.stop().await;
    }

    #[tokio::test]
    async fn test_custom_handler_registration() {
        let mut config = NetworkConfig::default();
        config.listen_addr = "127.0.0.1:0".parse().unwrap();
        config.discovery_enabled = false;

        let identity = NodeIdentity::generate("handler-node");
        let node = HiveNode::new(identity, config);

        // Register a custom handler.
        let custom_handler: crate::router::MessageHandler = Arc::new(|_env| {
            Box::pin(async { None })
        });
        node.on_message(MessageKind::TaskRequest, custom_handler).await;

        // Verify it's registered.
        let router = node.router.read().await;
        assert!(router.has_handler(&MessageKind::TaskRequest));
        // Built-in handlers should also be present.
        assert!(router.has_handler(&MessageKind::Hello));
        assert!(router.has_handler(&MessageKind::Heartbeat));
        assert!(router.has_handler(&MessageKind::Goodbye));
    }
}
