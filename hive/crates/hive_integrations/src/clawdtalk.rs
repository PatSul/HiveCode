//! ClawdTalk phone bridge — WebSocket client for voice-over-phone access.
//!
//! Connects to a ClawdTalk server, receives call transcripts (STT handled by
//! ClawdTalk/Telnyx), routes commands to the Hive agents layer, and sends
//! text responses back through the WebSocket for TTS playback on the call.

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the ClawdTalk phone bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClawdTalkConfig {
    /// WebSocket URL for the ClawdTalk server.
    pub server_url: String,
    /// PIN-based access control — callers must enter this PIN.
    pub bot_pin: Option<String>,
    /// Whether the bridge is enabled.
    pub enabled: bool,
}

impl Default for ClawdTalkConfig {
    fn default() -> Self {
        Self {
            server_url: "wss://clawdtalk.example.com/ws".into(),
            bot_pin: None,
            enabled: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Message types (ClawdTalk protocol)
// ---------------------------------------------------------------------------

/// Inbound message from the ClawdTalk server.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InboundMessage {
    /// A new phone call has connected.
    CallConnected { call_id: String, caller: String },
    /// Transcribed speech from the caller.
    Transcript {
        call_id: String,
        text: String,
        is_final: bool,
    },
    /// The call has ended.
    CallEnded { call_id: String },
    /// PIN verification result.
    PinVerified { call_id: String, success: bool },
}

/// Outbound message to the ClawdTalk server.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutboundMessage {
    /// Text response to be spoken to the caller via TTS.
    Speak { call_id: String, text: String },
    /// Request PIN verification from the caller.
    RequestPin { call_id: String },
    /// End the call.
    Hangup { call_id: String },
}

// ---------------------------------------------------------------------------
// Client state
// ---------------------------------------------------------------------------

/// Connection state of the ClawdTalk bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// The ClawdTalk bridge client.
///
/// This is a stateful client that manages the WebSocket connection. Actual
/// WebSocket I/O is driven externally (the client produces/consumes messages).
pub struct ClawdTalkClient {
    config: ClawdTalkConfig,
    state: BridgeState,
    active_calls: Vec<String>,
}

impl ClawdTalkClient {
    pub fn new(config: ClawdTalkConfig) -> Self {
        Self {
            config,
            state: BridgeState::Disconnected,
            active_calls: Vec::new(),
        }
    }

    pub fn state(&self) -> BridgeState {
        self.state
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn active_calls(&self) -> &[String] {
        &self.active_calls
    }

    /// Process an inbound message and return any outbound response(s).
    pub fn handle_message(&mut self, msg: InboundMessage) -> Vec<OutboundMessage> {
        let mut responses = Vec::new();

        match msg {
            InboundMessage::CallConnected { call_id, caller } => {
                info!(call_id, caller, "ClawdTalk call connected");
                self.active_calls.push(call_id.clone());

                // If PIN is configured, request verification.
                if self.config.bot_pin.is_some() {
                    responses.push(OutboundMessage::RequestPin {
                        call_id: call_id.clone(),
                    });
                    responses.push(OutboundMessage::Speak {
                        call_id,
                        text: "Welcome to Hive. Please enter your access PIN.".into(),
                    });
                } else {
                    responses.push(OutboundMessage::Speak {
                        call_id,
                        text: "Welcome to Hive. How can I help you?".into(),
                    });
                }
            }

            InboundMessage::Transcript {
                call_id,
                text,
                is_final,
            } => {
                if is_final {
                    debug!(call_id, text, "Final transcript from caller");
                    // The actual intent classification + AI response will be
                    // handled by the caller of this method (connecting to
                    // VoiceAssistant / agents). We return an empty vec here;
                    // the orchestrator adds the Speak response after processing.
                }
            }

            InboundMessage::CallEnded { call_id } => {
                info!(call_id, "ClawdTalk call ended");
                self.active_calls.retain(|id| id != &call_id);
            }

            InboundMessage::PinVerified { call_id, success } => {
                if success {
                    responses.push(OutboundMessage::Speak {
                        call_id,
                        text: "PIN accepted. How can I help you?".into(),
                    });
                } else {
                    responses.push(OutboundMessage::Speak {
                        call_id: call_id.clone(),
                        text: "Invalid PIN. Goodbye.".into(),
                    });
                    responses.push(OutboundMessage::Hangup { call_id });
                }
            }
        }

        responses
    }

    /// Mark the client as connected.
    pub fn set_connected(&mut self) {
        self.state = BridgeState::Connected;
        info!("ClawdTalk bridge connected");
    }

    /// Mark the client as disconnected.
    pub fn set_disconnected(&mut self) {
        self.state = BridgeState::Disconnected;
        self.active_calls.clear();
        info!("ClawdTalk bridge disconnected");
    }

    /// Mark the client as errored.
    pub fn set_error(&mut self) {
        self.state = BridgeState::Error;
        warn!("ClawdTalk bridge error");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> ClawdTalkClient {
        ClawdTalkClient::new(ClawdTalkConfig {
            enabled: true,
            bot_pin: Some("1234".into()),
            ..Default::default()
        })
    }

    fn test_client_no_pin() -> ClawdTalkClient {
        ClawdTalkClient::new(ClawdTalkConfig {
            enabled: true,
            bot_pin: None,
            ..Default::default()
        })
    }

    #[test]
    fn initial_state_is_disconnected() {
        let client = test_client();
        assert_eq!(client.state(), BridgeState::Disconnected);
        assert!(client.active_calls().is_empty());
    }

    #[test]
    fn call_connected_with_pin() {
        let mut client = test_client();
        let responses = client.handle_message(InboundMessage::CallConnected {
            call_id: "c1".into(),
            caller: "+1234567890".into(),
        });
        assert_eq!(client.active_calls().len(), 1);
        assert_eq!(responses.len(), 2); // RequestPin + Speak
    }

    #[test]
    fn call_connected_without_pin() {
        let mut client = test_client_no_pin();
        let responses = client.handle_message(InboundMessage::CallConnected {
            call_id: "c1".into(),
            caller: "+1234567890".into(),
        });
        assert_eq!(responses.len(), 1); // Just Speak
    }

    #[test]
    fn call_ended_removes_from_active() {
        let mut client = test_client();
        client.handle_message(InboundMessage::CallConnected {
            call_id: "c1".into(),
            caller: "+1".into(),
        });
        assert_eq!(client.active_calls().len(), 1);

        client.handle_message(InboundMessage::CallEnded {
            call_id: "c1".into(),
        });
        assert!(client.active_calls().is_empty());
    }

    #[test]
    fn pin_verified_success() {
        let mut client = test_client();
        let responses = client.handle_message(InboundMessage::PinVerified {
            call_id: "c1".into(),
            success: true,
        });
        assert_eq!(responses.len(), 1);
    }

    #[test]
    fn pin_verified_failure_hangs_up() {
        let mut client = test_client();
        let responses = client.handle_message(InboundMessage::PinVerified {
            call_id: "c1".into(),
            success: false,
        });
        assert_eq!(responses.len(), 2); // Speak + Hangup
    }

    #[test]
    fn transcript_final_returns_empty() {
        let mut client = test_client();
        let responses = client.handle_message(InboundMessage::Transcript {
            call_id: "c1".into(),
            text: "hello hive".into(),
            is_final: true,
        });
        assert!(responses.is_empty());
    }

    #[test]
    fn state_transitions() {
        let mut client = test_client();
        client.set_connected();
        assert_eq!(client.state(), BridgeState::Connected);

        client.set_error();
        assert_eq!(client.state(), BridgeState::Error);

        client.set_disconnected();
        assert_eq!(client.state(), BridgeState::Disconnected);
    }

    #[test]
    fn config_serde_round_trip() {
        let cfg = ClawdTalkConfig {
            server_url: "wss://test.example.com/ws".into(),
            bot_pin: Some("9999".into()),
            enabled: true,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: ClawdTalkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.server_url, "wss://test.example.com/ws");
        assert_eq!(parsed.bot_pin.as_deref(), Some("9999"));
        assert!(parsed.enabled);
    }

    #[test]
    fn inbound_message_deserialize() {
        let json = r#"{"type":"call_connected","call_id":"abc","caller":"+1555"}"#;
        let msg: InboundMessage = serde_json::from_str(json).unwrap();
        matches!(msg, InboundMessage::CallConnected { .. });
    }

    #[test]
    fn outbound_message_serialize() {
        let msg = OutboundMessage::Speak {
            call_id: "c1".into(),
            text: "hello".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"speak\""));
        assert!(json.contains("\"call_id\":\"c1\""));
    }
}
