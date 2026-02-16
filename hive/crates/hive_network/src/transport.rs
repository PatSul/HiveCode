//! WebSocket transport — server and client connections.
//!
//! Provides the low-level WebSocket plumbing for peer-to-peer communication.
//! The server accepts incoming connections and forwards received envelopes
//! into an mpsc channel. The client connects to a remote peer and returns a
//! [`PeerConnection`] handle.

use std::net::SocketAddr;

use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, accept_async, connect_async};
use tracing::{debug, error, info, warn};

use crate::error::NetworkError;
use crate::identity::PeerId;
use crate::message::Envelope;

/// Type alias for the write half of a server-side WebSocket.
type ServerWsSink = SplitSink<WebSocketStream<TcpStream>, Message>;

/// Type alias for the write half of a client-side WebSocket.
type ClientWsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// A handle to an active WebSocket connection with a peer.
///
/// Wraps the write-half of the WebSocket stream, providing a simple
/// `send(envelope)` API. The read-half is consumed by a background task
/// that forwards incoming envelopes to the node's central channel.
pub struct PeerConnection {
    peer_id: PeerId,
    sink: PeerSink,
}

/// The write side can be either a server-accepted or client-initiated socket.
enum PeerSink {
    Server(ServerWsSink),
    Client(ClientWsSink),
}

impl PeerConnection {
    /// Create a connection wrapping a server-accepted WebSocket sink.
    pub fn from_server(peer_id: PeerId, sink: ServerWsSink) -> Self {
        Self {
            peer_id,
            sink: PeerSink::Server(sink),
        }
    }

    /// Create a connection wrapping a client-initiated WebSocket sink.
    pub fn from_client(peer_id: PeerId, sink: ClientWsSink) -> Self {
        Self {
            peer_id,
            sink: PeerSink::Client(sink),
        }
    }

    /// The peer ID this connection is associated with.
    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    /// Send an envelope over the WebSocket connection.
    pub async fn send(&mut self, envelope: &Envelope) -> Result<(), NetworkError> {
        let json = envelope
            .to_json()
            .map_err(|e| NetworkError::Transport(format!("Serialize error: {e}")))?;

        let msg = Message::Text(json.into());
        match &mut self.sink {
            PeerSink::Server(sink) => sink
                .send(msg)
                .await
                .map_err(|e| NetworkError::Transport(format!("Send error: {e}")))?,
            PeerSink::Client(sink) => sink
                .send(msg)
                .await
                .map_err(|e| NetworkError::Transport(format!("Send error: {e}")))?,
        }
        Ok(())
    }

    /// Close the connection gracefully.
    pub async fn close(&mut self) -> Result<(), NetworkError> {
        let close_msg = Message::Close(None);
        match &mut self.sink {
            PeerSink::Server(sink) => {
                let _ = sink.send(close_msg).await;
            }
            PeerSink::Client(sink) => {
                let _ = sink.send(close_msg).await;
            }
        }
        Ok(())
    }
}

/// An incoming event from the transport layer.
#[derive(Debug)]
pub enum TransportEvent {
    /// A new inbound connection was accepted from the given address.
    InboundConnection {
        addr: SocketAddr,
        peer_id: PeerId,
    },
    /// An envelope was received from a peer.
    Message {
        from_addr: SocketAddr,
        envelope: Envelope,
    },
    /// A peer disconnected.
    Disconnected {
        addr: SocketAddr,
    },
}

/// Start the WebSocket server on the given address.
///
/// Accepted connections spawn a read-loop task that forwards received
/// envelopes (and connection/disconnection events) into the provided
/// `event_tx` channel. The returned `JoinHandle` can be used to await
/// or abort the server.
///
/// Server-side `PeerConnection` handles (write sinks) are sent through
/// `conn_tx` so the node can track and write to them.
pub async fn start_server(
    addr: SocketAddr,
    event_tx: mpsc::Sender<TransportEvent>,
    conn_tx: mpsc::Sender<(SocketAddr, PeerConnection)>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), NetworkError> {
    let listener = TcpListener::bind(addr).await.map_err(NetworkError::Io)?;
    info!("WebSocket server listening on {addr}");

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, peer_addr)) => {
                        let event_tx = event_tx.clone();
                        let conn_tx = conn_tx.clone();
                        tokio::spawn(async move {
                            match accept_async(stream).await {
                                Ok(ws_stream) => {
                                    let (sink, mut stream) = ws_stream.split();
                                    let temp_peer_id = PeerId::from_string(format!("pending-{peer_addr}"));

                                    // Send the connection handle to the node.
                                    let conn = PeerConnection::from_server(
                                        temp_peer_id.clone(),
                                        sink,
                                    );
                                    let _ = conn_tx.send((peer_addr, conn)).await;

                                    let _ = event_tx
                                        .send(TransportEvent::InboundConnection {
                                            addr: peer_addr,
                                            peer_id: temp_peer_id,
                                        })
                                        .await;

                                    // Read loop.
                                    while let Some(msg) = stream.next().await {
                                        match msg {
                                            Ok(Message::Text(text)) => {
                                                match Envelope::from_json(&text) {
                                                    Ok(envelope) => {
                                                        let _ = event_tx
                                                            .send(TransportEvent::Message {
                                                                from_addr: peer_addr,
                                                                envelope,
                                                            })
                                                            .await;
                                                    }
                                                    Err(e) => {
                                                        warn!("Bad envelope from {peer_addr}: {e}");
                                                    }
                                                }
                                            }
                                            Ok(Message::Close(_)) => {
                                                debug!("Peer {peer_addr} sent close");
                                                break;
                                            }
                                            Ok(_) => {} // Ignore binary/ping/pong
                                            Err(e) => {
                                                debug!("Read error from {peer_addr}: {e}");
                                                break;
                                            }
                                        }
                                    }

                                    let _ = event_tx
                                        .send(TransportEvent::Disconnected { addr: peer_addr })
                                        .await;
                                }
                                Err(e) => {
                                    error!("WebSocket accept failed for {peer_addr}: {e}");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!("TCP accept failed: {e}");
                    }
                }
            }
            _ = shutdown.recv() => {
                info!("WebSocket server shutting down");
                break;
            }
        }
    }

    Ok(())
}

/// Connect to a remote peer as a client.
///
/// Returns a `PeerConnection` (write handle) and spawns a read-loop task
/// that forwards incoming envelopes to `event_tx`.
pub async fn connect_to_peer(
    addr: &str,
    event_tx: mpsc::Sender<TransportEvent>,
) -> Result<PeerConnection, NetworkError> {
    let url = if addr.starts_with("ws://") || addr.starts_with("wss://") {
        addr.to_string()
    } else {
        format!("ws://{addr}")
    };

    let (ws_stream, _) = connect_async(&url)
        .await
        .map_err(|e| NetworkError::Transport(format!("Connect to {addr} failed: {e}")))?;

    let peer_addr: SocketAddr = addr
        .trim_start_matches("ws://")
        .trim_start_matches("wss://")
        .parse()
        .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

    let (sink, mut stream) = ws_stream.split();
    let temp_peer_id = PeerId::from_string(format!("outbound-{addr}"));

    let conn = PeerConnection::from_client(temp_peer_id, sink);

    // Spawn read loop.
    tokio::spawn(async move {
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match Envelope::from_json(&text) {
                        Ok(envelope) => {
                            let _ = event_tx
                                .send(TransportEvent::Message {
                                    from_addr: peer_addr,
                                    envelope,
                                })
                                .await;
                        }
                        Err(e) => {
                            warn!("Bad envelope from {peer_addr}: {e}");
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    debug!("Remote {peer_addr} sent close");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    debug!("Read error from {peer_addr}: {e}");
                    break;
                }
            }
        }

        let _ = event_tx
            .send(TransportEvent::Disconnected { addr: peer_addr })
            .await;
    });

    Ok(conn)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_connection_creation() {
        // Just verify the types compile correctly.
        let _peer_id = PeerId::from_string("test-peer");
    }

    #[tokio::test]
    async fn test_server_bind_and_shutdown() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(addr).await.unwrap();
        let bound_addr = listener.local_addr().unwrap();

        // Verify we can bind and get a port.
        assert_ne!(bound_addr.port(), 0);
        drop(listener);
    }

    #[tokio::test]
    async fn test_connect_to_server() {
        let (event_tx, mut event_rx) = mpsc::channel(32);
        let (conn_tx, mut conn_rx) = mpsc::channel(32);
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        // Bind server to a random port.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();
        drop(listener);

        // Start server.
        let event_tx_clone = event_tx.clone();
        let server_handle = tokio::spawn(async move {
            let _ = start_server(server_addr, event_tx_clone, conn_tx, shutdown_rx).await;
        });

        // Give server time to start.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect as client.
        let client_result = connect_to_peer(&server_addr.to_string(), event_tx).await;
        assert!(client_result.is_ok());

        let mut client_conn = client_result.unwrap();

        // Send a message from client.
        let envelope = Envelope::new(
            PeerId::from_string("client-peer"),
            None,
            crate::message::MessageKind::Hello,
            serde_json::json!({"name": "test-client"}),
        );
        client_conn.send(&envelope).await.unwrap();

        // Server should receive the connection and message.
        // Wait for inbound connection event.
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            event_rx.recv(),
        )
        .await;
        assert!(event.is_ok());

        // Wait for server-side connection handle.
        let server_conn = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            conn_rx.recv(),
        )
        .await;
        assert!(server_conn.is_ok());

        // Cleanup.
        client_conn.close().await.unwrap();
        let _ = shutdown_tx.send(());
        let _ = server_handle.await;
    }
}
