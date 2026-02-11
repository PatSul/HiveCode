//! Messaging provider trait and shared types.
//!
//! Defines the [`MessagingProvider`] trait that all platform-specific
//! implementations (Slack, Discord, etc.) must satisfy, along with the
//! common data types exchanged across providers.

use async_trait::async_trait;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

// ── Platform enum ──────────────────────────────────────────────────

/// Supported messaging platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Slack,
    Discord,
    Telegram,
    WhatsApp,
    Teams,
    Signal,
    Matrix,
    GoogleChat,
    WebChat,
    IMessage,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Slack => write!(f, "slack"),
            Platform::Discord => write!(f, "discord"),
            Platform::Telegram => write!(f, "telegram"),
            Platform::WhatsApp => write!(f, "whatsapp"),
            Platform::Teams => write!(f, "teams"),
            Platform::Signal => write!(f, "signal"),
            Platform::Matrix => write!(f, "matrix"),
            Platform::GoogleChat => write!(f, "google_chat"),
            Platform::WebChat => write!(f, "web_chat"),
            Platform::IMessage => write!(f, "imessage"),
        }
    }
}

// ── Shared data types ──────────────────────────────────────────────

/// A channel or conversation on a messaging platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub platform: Platform,
}

/// An attachment on a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    pub url: String,
    pub mime_type: String,
    pub size: u64,
}

/// A message received from a messaging platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingMessage {
    pub id: String,
    pub channel_id: String,
    pub author: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub platform: Platform,
}

/// Confirmation of a successfully sent message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentMessage {
    pub id: String,
    pub channel_id: String,
    pub timestamp: DateTime<Utc>,
}

// ── Provider trait ─────────────────────────────────────────────────

/// Trait that every messaging platform integration must implement.
#[async_trait]
pub trait MessagingProvider: Send + Sync {
    /// Return the platform this provider handles.
    fn platform(&self) -> Platform;

    /// Send a text message to the given channel.
    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage>;

    /// List the channels visible to the bot.
    async fn list_channels(&self) -> Result<Vec<Channel>>;

    /// Retrieve recent messages from a channel.
    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>>;

    /// Add a reaction (emoji) to a message.
    async fn add_reaction(&self, channel: &str, message_id: &str, emoji: &str) -> Result<()>;

    /// Search messages across channels.
    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Slack.to_string(), "slack");
        assert_eq!(Platform::Discord.to_string(), "discord");
        assert_eq!(Platform::Telegram.to_string(), "telegram");
        assert_eq!(Platform::WhatsApp.to_string(), "whatsapp");
        assert_eq!(Platform::Teams.to_string(), "teams");
        assert_eq!(Platform::Signal.to_string(), "signal");
        assert_eq!(Platform::Matrix.to_string(), "matrix");
        assert_eq!(Platform::GoogleChat.to_string(), "google_chat");
        assert_eq!(Platform::WebChat.to_string(), "web_chat");
        assert_eq!(Platform::IMessage.to_string(), "imessage");
    }

    #[test]
    fn test_platform_serialize() {
        let json = serde_json::to_string(&Platform::Slack).unwrap();
        assert_eq!(json, r#""slack""#);
    }

    #[test]
    fn test_platform_deserialize() {
        let p: Platform = serde_json::from_str(r#""discord""#).unwrap();
        assert_eq!(p, Platform::Discord);
    }

    #[test]
    fn test_platform_roundtrip() {
        for platform in [
            Platform::Slack,
            Platform::Discord,
            Platform::Telegram,
            Platform::WhatsApp,
            Platform::Teams,
            Platform::Signal,
            Platform::Matrix,
            Platform::GoogleChat,
            Platform::WebChat,
            Platform::IMessage,
        ] {
            let json = serde_json::to_string(&platform).unwrap();
            let back: Platform = serde_json::from_str(&json).unwrap();
            assert_eq!(back, platform);
        }
    }

    #[test]
    fn test_channel_serialization_roundtrip() {
        let channel = Channel {
            id: "C123".into(),
            name: "general".into(),
            platform: Platform::Slack,
        };
        let json = serde_json::to_string(&channel).unwrap();
        let back: Channel = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "C123");
        assert_eq!(back.name, "general");
        assert_eq!(back.platform, Platform::Slack);
    }

    #[test]
    fn test_attachment_serialization() {
        let att = Attachment {
            name: "image.png".into(),
            url: "https://files.example.com/image.png".into(),
            mime_type: "image/png".into(),
            size: 4096,
        };
        let json = serde_json::to_string(&att).unwrap();
        assert!(json.contains("mimeType"));
        let back: Attachment = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "image.png");
        assert_eq!(back.size, 4096);
    }

    #[test]
    fn test_incoming_message_serialization() {
        let msg = IncomingMessage {
            id: "msg-1".into(),
            channel_id: "C123".into(),
            author: "alice".into(),
            content: "Hello, world!".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::Slack,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: IncomingMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "msg-1");
        assert_eq!(back.author, "alice");
        assert_eq!(back.content, "Hello, world!");
        assert_eq!(back.platform, Platform::Slack);
    }

    #[test]
    fn test_incoming_message_with_attachments() {
        let msg = IncomingMessage {
            id: "msg-2".into(),
            channel_id: "C456".into(),
            author: "bob".into(),
            content: "See attached".into(),
            timestamp: Utc::now(),
            attachments: vec![
                Attachment {
                    name: "doc.pdf".into(),
                    url: "https://files.example.com/doc.pdf".into(),
                    mime_type: "application/pdf".into(),
                    size: 10240,
                },
            ],
            platform: Platform::Discord,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: IncomingMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.attachments.len(), 1);
        assert_eq!(back.attachments[0].name, "doc.pdf");
    }

    #[test]
    fn test_sent_message_serialization() {
        let sent = SentMessage {
            id: "sent-1".into(),
            channel_id: "C789".into(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&sent).unwrap();
        let back: SentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "sent-1");
        assert_eq!(back.channel_id, "C789");
    }

    #[test]
    fn test_platform_equality() {
        assert_eq!(Platform::Slack, Platform::Slack);
        assert_ne!(Platform::Slack, Platform::Discord);
    }

    #[test]
    fn test_platform_hash_used_as_key() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(Platform::Slack, "slack-token");
        map.insert(Platform::Discord, "discord-token");
        assert_eq!(map.get(&Platform::Slack), Some(&"slack-token"));
        assert_eq!(map.get(&Platform::Discord), Some(&"discord-token"));
        assert_eq!(map.get(&Platform::Telegram), None);
    }
}
