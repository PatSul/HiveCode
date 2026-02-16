//! Hive Network — P2P federation and peer communication.
//!
//! This crate provides the networking layer for HiveCode, enabling multiple
//! Hive instances to discover each other, exchange messages, synchronize
//! state, relay agent tasks, and share fleet learning outcomes.
//!
//! # Architecture
//!
//! - **Transport**: WebSocket-based (via `tokio-tungstenite`) bidirectional
//!   connections between peers.
//! - **Discovery**: UDP broadcast on the LAN for automatic peer discovery.
//! - **Protocol**: Envelope-based typed messaging with JSON payloads.
//! - **Routing**: Handler-based dispatch for incoming messages.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use hive_network::{HiveNode, NetworkConfig};
//! use hive_network::identity::NodeIdentity;
//!
//! # async fn example() {
//! let identity = NodeIdentity::generate("my-node");
//! let config = NetworkConfig::default();
//! let mut node = HiveNode::new(identity, config);
//!
//! node.start().await.unwrap();
//! // ... node is running, accepting connections and discovering peers ...
//! node.stop().await;
//! # }
//! ```

pub mod config;
pub mod discovery;
pub mod error;
pub mod identity;
pub mod message;
pub mod node;
pub mod peer;
pub mod router;
pub mod sync;
pub mod transport;

// ── Re-exports for convenience ──────────────────────────────────────────

pub use config::NetworkConfig;
pub use error::NetworkError;
pub use identity::{NodeIdentity, PeerId};
pub use message::{Envelope, MessageKind};
pub use node::HiveNode;
pub use peer::{PeerInfo, PeerRegistry, PeerState};
