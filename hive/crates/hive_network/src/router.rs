//! Message router — dispatches incoming envelopes to registered handlers.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::message::{Envelope, MessageKind};

/// A handler function that processes an envelope and optionally returns a
/// response envelope.
pub type MessageHandler = Arc<
    dyn Fn(Envelope) -> Pin<Box<dyn Future<Output = Option<Envelope>> + Send>> + Send + Sync,
>;

/// Routes incoming envelopes to the appropriate handler based on their
/// [`MessageKind`].
pub struct MessageRouter {
    handlers: HashMap<String, MessageHandler>,
    default_handler: Option<MessageHandler>,
}

impl MessageRouter {
    /// Create a new router with no handlers registered.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            default_handler: None,
        }
    }

    /// Register a handler for a specific message kind.
    pub fn register(&mut self, kind: MessageKind, handler: MessageHandler) {
        let key = kind.dispatch_key();
        debug!("Registering handler for message kind: {key}");
        self.handlers.insert(key, handler);
    }

    /// Register a default handler for unmatched message kinds.
    pub fn set_default_handler(&mut self, handler: MessageHandler) {
        self.default_handler = Some(handler);
    }

    /// Check if a handler is registered for a specific message kind.
    pub fn has_handler(&self, kind: &MessageKind) -> bool {
        self.handlers.contains_key(&kind.dispatch_key())
    }

    /// Return the number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Dispatch an envelope to its handler. Returns the response envelope
    /// if the handler produced one.
    pub async fn dispatch(&self, envelope: Envelope) -> Option<Envelope> {
        let key = envelope.kind.dispatch_key();

        if let Some(handler) = self.handlers.get(&key) {
            debug!("Dispatching {key} envelope {}", envelope.id);
            handler(envelope).await
        } else if let Some(default) = &self.default_handler {
            debug!("Using default handler for {key} envelope {}", envelope.id);
            default(envelope).await
        } else {
            warn!("No handler for message kind: {key}");
            None
        }
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in handler factories
// ---------------------------------------------------------------------------

/// Create a handler that responds to Hello messages with a Welcome.
pub fn hello_handler(our_peer_id: crate::identity::PeerId) -> MessageHandler {
    Arc::new(move |envelope: Envelope| {
        let peer_id = our_peer_id.clone();
        Box::pin(async move {
            Some(Envelope::new(
                peer_id,
                Some(envelope.from),
                MessageKind::Welcome,
                serde_json::json!({"status": "accepted"}),
            ))
        })
    })
}

/// Create a handler that responds to Heartbeat with HeartbeatAck.
pub fn heartbeat_handler(our_peer_id: crate::identity::PeerId) -> MessageHandler {
    Arc::new(move |envelope: Envelope| {
        let peer_id = our_peer_id.clone();
        Box::pin(async move {
            Some(Envelope::new(
                peer_id,
                Some(envelope.from),
                MessageKind::HeartbeatAck,
                serde_json::json!({}),
            ))
        })
    })
}

/// Create a handler that logs Goodbye messages (no response needed).
pub fn goodbye_handler() -> MessageHandler {
    Arc::new(|envelope: Envelope| {
        Box::pin(async move {
            debug!("Peer {} said goodbye", envelope.from);
            None
        })
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PeerId;

    fn make_envelope(kind: MessageKind) -> Envelope {
        Envelope::new(
            PeerId::from_string("test-sender"),
            Some(PeerId::from_string("test-receiver")),
            kind,
            serde_json::json!({}),
        )
    }

    #[tokio::test]
    async fn test_register_and_dispatch() {
        let mut router = MessageRouter::new();

        let handler: MessageHandler = Arc::new(|_env| {
            Box::pin(async {
                Some(Envelope::new(
                    PeerId::from_string("responder"),
                    None,
                    MessageKind::Welcome,
                    serde_json::json!({"handled": true}),
                ))
            })
        });

        router.register(MessageKind::Hello, handler);
        assert!(router.has_handler(&MessageKind::Hello));
        assert_eq!(router.handler_count(), 1);

        let envelope = make_envelope(MessageKind::Hello);
        let response = router.dispatch(envelope).await;
        assert!(response.is_some());
        assert_eq!(response.unwrap().kind, MessageKind::Welcome);
    }

    #[tokio::test]
    async fn test_unhandled_message() {
        let router = MessageRouter::new();
        let envelope = make_envelope(MessageKind::TaskRequest);
        let response = router.dispatch(envelope).await;
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn test_default_handler() {
        let mut router = MessageRouter::new();

        let default: MessageHandler = Arc::new(|env| {
            Box::pin(async move {
                Some(Envelope::new(
                    PeerId::from_string("default"),
                    Some(env.from),
                    MessageKind::Custom("default_response".to_string()),
                    serde_json::json!({"fallback": true}),
                ))
            })
        });

        router.set_default_handler(default);

        // No specific handler registered for TaskRequest — should use default.
        let envelope = make_envelope(MessageKind::TaskRequest);
        let response = router.dispatch(envelope).await;
        assert!(response.is_some());
        assert_eq!(
            response.unwrap().kind,
            MessageKind::Custom("default_response".to_string())
        );
    }

    #[tokio::test]
    async fn test_hello_handler() {
        let our_id = PeerId::from_string("our-node");
        let handler = hello_handler(our_id);

        let envelope = make_envelope(MessageKind::Hello);
        let response = handler(envelope).await;
        assert!(response.is_some());

        let resp = response.unwrap();
        assert_eq!(resp.kind, MessageKind::Welcome);
        assert_eq!(resp.from, PeerId::from_string("our-node"));
    }

    #[tokio::test]
    async fn test_heartbeat_handler() {
        let our_id = PeerId::from_string("our-node");
        let handler = heartbeat_handler(our_id);

        let envelope = make_envelope(MessageKind::Heartbeat);
        let response = handler(envelope).await;
        assert!(response.is_some());
        assert_eq!(response.unwrap().kind, MessageKind::HeartbeatAck);
    }

    #[tokio::test]
    async fn test_goodbye_handler() {
        let handler = goodbye_handler();
        let envelope = make_envelope(MessageKind::Goodbye);
        let response = handler(envelope).await;
        assert!(response.is_none()); // Goodbye produces no response.
    }
}
