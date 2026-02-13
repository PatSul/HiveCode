//! WebChat messaging provider.
//!
//! A simple HTTP webhook-based chat provider. Sends outgoing messages
//! via POST to a configurable webhook URL and stores received messages
//! in-memory for retrieval.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::provider::{
    Attachment, Channel, IncomingMessage, MessagingProvider, Platform, SentMessage,
};

const DEFAULT_BASE_URL: &str = "https://webhooks.example.com";

// ── WebChat types ────────────────────────────────────────────────

/// Payload sent to the webhook endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebChatOutgoing {
    pub channel: String,
    pub text: String,
    pub timestamp: String,
}

/// Payload received from external sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebChatIncoming {
    pub id: String,
    pub channel: String,
    pub author: String,
    pub text: String,
    pub timestamp: String,
    #[serde(default)]
    pub attachments: Vec<WebChatAttachment>,
}

/// An attachment on a WebChat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebChatAttachment {
    pub name: String,
    pub url: String,
    pub mime_type: String,
    pub size: u64,
}

/// Response from the webhook endpoint after sending a message.
#[derive(Debug, Deserialize)]
struct WebChatSendResponse {
    id: String,
    timestamp: Option<String>,
}

// ── Client ─────────────────────────────────────────────────────────

/// WebChat messaging provider using HTTP webhooks.
///
/// Outgoing messages are sent via POST to `{base_url}/send`. Incoming
/// messages are stored in-memory and can be pushed via [`receive_message`].
pub struct WebChatProvider {
    base_url: String,
    token: String,
    client: Client,
    /// In-memory store for received messages.
    inbox: Arc<Mutex<Vec<IncomingMessage>>>,
}

impl WebChatProvider {
    /// Create a new WebChat provider with the given API token.
    pub fn new(token: &str) -> Result<Self> {
        Self::with_base_url(token, DEFAULT_BASE_URL)
    }

    /// Create a new WebChat provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(token: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("invalid characters in WebChat token")?;
        headers.insert("X-Api-Key", auth_value);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for WebChat")?;

        Ok(Self {
            base_url,
            token: token.to_string(),
            client,
            inbox: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored token.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Push an externally received message into the in-memory inbox.
    pub fn receive_message(&self, msg: IncomingMessage) {
        let mut inbox = self.inbox.lock().unwrap();
        inbox.push(msg);
    }

    /// Push an incoming payload (from a webhook callback) into the inbox.
    pub fn receive_webhook(&self, incoming: &WebChatIncoming) {
        let timestamp = incoming.timestamp.parse().unwrap_or_else(|_| Utc::now());

        let msg = IncomingMessage {
            id: incoming.id.clone(),
            channel_id: incoming.channel.clone(),
            author: incoming.author.clone(),
            content: incoming.text.clone(),
            timestamp,
            attachments: incoming
                .attachments
                .iter()
                .map(|a| Attachment {
                    name: a.name.clone(),
                    url: a.url.clone(),
                    mime_type: a.mime_type.clone(),
                    size: a.size,
                })
                .collect(),
            platform: Platform::WebChat,
        };

        self.receive_message(msg);
    }

    /// Return the current count of messages in the inbox.
    pub fn inbox_count(&self) -> usize {
        self.inbox.lock().unwrap().len()
    }

    /// Clear all messages from the inbox.
    pub fn clear_inbox(&self) {
        self.inbox.lock().unwrap().clear();
    }
}

#[async_trait]
impl MessagingProvider for WebChatProvider {
    fn platform(&self) -> Platform {
        Platform::WebChat
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let url = format!("{}/send", self.base_url);
        let payload = WebChatOutgoing {
            channel: channel.to_string(),
            text: text.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        };

        debug!(url = %url, channel = %channel, "sending WebChat message");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("WebChat send message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("WebChat API HTTP error ({}): {}", status, body);
        }

        let send_resp: WebChatSendResponse = resp
            .json()
            .await
            .context("failed to parse WebChat send response")?;

        let timestamp = send_resp
            .timestamp
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(Utc::now);

        Ok(SentMessage {
            id: send_resp.id,
            channel_id: channel.to_string(),
            timestamp,
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/channels", self.base_url);

        debug!(url = %url, "listing WebChat channels");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("WebChat list channels request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("WebChat API HTTP error ({}): {}", status, body);
        }

        #[derive(Deserialize)]
        struct ChannelEntry {
            id: String,
            name: String,
        }

        let entries: Vec<ChannelEntry> = resp
            .json()
            .await
            .context("failed to parse WebChat channels response")?;

        Ok(entries
            .into_iter()
            .map(|c| Channel {
                id: c.id,
                name: c.name,
                platform: Platform::WebChat,
            })
            .collect())
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Return messages from the in-memory inbox filtered by channel.
        let inbox = self.inbox.lock().unwrap();
        let messages: Vec<IncomingMessage> = inbox
            .iter()
            .filter(|m| m.channel_id == channel)
            .rev()
            .take(limit as usize)
            .cloned()
            .collect();

        Ok(messages)
    }

    async fn add_reaction(&self, channel: &str, message_id: &str, emoji: &str) -> Result<()> {
        let url = format!("{}/reactions", self.base_url);
        let payload = serde_json::json!({
            "channel": channel,
            "message_id": message_id,
            "emoji": emoji,
        });

        debug!(url = %url, message_id = %message_id, emoji = %emoji, "adding WebChat reaction");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("WebChat add reaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("WebChat API HTTP error ({}): {}", status, body);
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Search the in-memory inbox client-side.
        let inbox = self.inbox.lock().unwrap();
        let query_lower = query.to_lowercase();

        let messages: Vec<IncomingMessage> = inbox
            .iter()
            .filter(|m| m.content.to_lowercase().contains(&query_lower))
            .rev()
            .take(limit as usize)
            .cloned()
            .collect();

        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    fn make_provider() -> WebChatProvider {
        WebChatProvider::with_base_url("wc-token-123", DEFAULT_BASE_URL).unwrap()
    }

    #[test]
    fn test_webchat_provider_default_base_url() {
        let provider = WebChatProvider::new("wc-tok").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_webchat_provider_custom_base_url_strips_slash() {
        let provider = WebChatProvider::with_base_url("tok", "https://webhooks.test/api/").unwrap();
        assert_eq!(provider.base_url(), "https://webhooks.test/api");
    }

    #[test]
    fn test_webchat_provider_token_stored() {
        let provider = make_provider();
        assert_eq!(provider.token(), "wc-token-123");
    }

    #[test]
    fn test_webchat_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::WebChat);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = WebChatProvider::new("tok\nwith\nnewlines");
        assert!(result.is_err());
    }

    #[test]
    fn test_send_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), "/send");
        assert_eq!(url, "https://webhooks.example.com/send");
    }

    #[test]
    fn test_channels_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), "/channels");
        assert_eq!(url, "https://webhooks.example.com/channels");
    }

    #[test]
    fn test_reactions_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), "/reactions");
        assert_eq!(url, "https://webhooks.example.com/reactions");
    }

    #[test]
    fn test_outgoing_serialization() {
        let msg = WebChatOutgoing {
            channel: "lobby".into(),
            text: "Hello!".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"channel\":\"lobby\""));
        assert!(json.contains("\"text\":\"Hello!\""));
    }

    #[test]
    fn test_outgoing_deserialization() {
        let json = r#"{"channel":"lobby","text":"Hi","timestamp":"2025-01-01T00:00:00Z"}"#;
        let msg: WebChatOutgoing = serde_json::from_str(json).unwrap();
        assert_eq!(msg.channel, "lobby");
        assert_eq!(msg.text, "Hi");
    }

    #[test]
    fn test_incoming_deserialization() {
        let json = r#"{
            "id": "msg-1",
            "channel": "lobby",
            "author": "alice",
            "text": "Hello!",
            "timestamp": "2025-01-01T00:00:00Z",
            "attachments": []
        }"#;
        let msg: WebChatIncoming = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "msg-1");
        assert_eq!(msg.channel, "lobby");
        assert_eq!(msg.author, "alice");
        assert_eq!(msg.text, "Hello!");
    }

    #[test]
    fn test_incoming_with_attachments() {
        let json = r#"{
            "id": "msg-2",
            "channel": "files",
            "author": "bob",
            "text": "See file",
            "timestamp": "2025-01-01T00:00:00Z",
            "attachments": [{
                "name": "doc.pdf",
                "url": "https://files.example.com/doc.pdf",
                "mimeType": "application/pdf",
                "size": 4096
            }]
        }"#;
        let msg: WebChatIncoming = serde_json::from_str(json).unwrap();
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].name, "doc.pdf");
        assert_eq!(msg.attachments[0].size, 4096);
    }

    #[test]
    fn test_send_response_deserialization() {
        let json = r#"{"id": "sent-1", "timestamp": "2025-01-01T00:00:00Z"}"#;
        let resp: WebChatSendResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "sent-1");
        assert_eq!(resp.timestamp.as_deref(), Some("2025-01-01T00:00:00Z"));
    }

    #[test]
    fn test_inbox_empty_initially() {
        let provider = make_provider();
        assert_eq!(provider.inbox_count(), 0);
    }

    #[test]
    fn test_receive_message_adds_to_inbox() {
        let provider = make_provider();
        let msg = IncomingMessage {
            id: "msg-1".into(),
            channel_id: "lobby".into(),
            author: "alice".into(),
            content: "Hello!".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::WebChat,
        };
        provider.receive_message(msg);
        assert_eq!(provider.inbox_count(), 1);
    }

    #[test]
    fn test_receive_webhook_adds_to_inbox() {
        let provider = make_provider();
        let incoming = WebChatIncoming {
            id: "msg-2".into(),
            channel: "lobby".into(),
            author: "bob".into(),
            text: "Hey!".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
            attachments: vec![],
        };
        provider.receive_webhook(&incoming);
        assert_eq!(provider.inbox_count(), 1);
    }

    #[test]
    fn test_clear_inbox() {
        let provider = make_provider();
        let msg = IncomingMessage {
            id: "msg-1".into(),
            channel_id: "lobby".into(),
            author: "alice".into(),
            content: "Hello!".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::WebChat,
        };
        provider.receive_message(msg);
        assert_eq!(provider.inbox_count(), 1);
        provider.clear_inbox();
        assert_eq!(provider.inbox_count(), 0);
    }

    #[tokio::test]
    async fn test_get_messages_returns_inbox_filtered() {
        let provider = make_provider();

        provider.receive_message(IncomingMessage {
            id: "msg-1".into(),
            channel_id: "lobby".into(),
            author: "alice".into(),
            content: "Hello!".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::WebChat,
        });
        provider.receive_message(IncomingMessage {
            id: "msg-2".into(),
            channel_id: "other".into(),
            author: "bob".into(),
            content: "In other channel".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::WebChat,
        });
        provider.receive_message(IncomingMessage {
            id: "msg-3".into(),
            channel_id: "lobby".into(),
            author: "charlie".into(),
            content: "Also in lobby".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::WebChat,
        });

        let msgs = provider.get_messages("lobby", 10).await.unwrap();
        assert_eq!(msgs.len(), 2);
        // Most recent first (reversed).
        assert_eq!(msgs[0].id, "msg-3");
        assert_eq!(msgs[1].id, "msg-1");
    }

    #[tokio::test]
    async fn test_search_messages_filters_inbox() {
        let provider = make_provider();

        provider.receive_message(IncomingMessage {
            id: "msg-1".into(),
            channel_id: "lobby".into(),
            author: "alice".into(),
            content: "Hello world!".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::WebChat,
        });
        provider.receive_message(IncomingMessage {
            id: "msg-2".into(),
            channel_id: "lobby".into(),
            author: "bob".into(),
            content: "Goodbye!".into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform: Platform::WebChat,
        });

        let results = provider.search_messages("hello", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "msg-1");
    }

    #[test]
    fn test_send_message_payload() {
        let payload = WebChatOutgoing {
            channel: "lobby".into(),
            text: "Hello, WebChat!".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["channel"], "lobby");
        assert_eq!(json["text"], "Hello, WebChat!");
    }

    #[test]
    fn test_reaction_payload() {
        let payload = serde_json::json!({
            "channel": "lobby",
            "message_id": "msg-1",
            "emoji": "thumbsup",
        });
        assert_eq!(payload["channel"], "lobby");
        assert_eq!(payload["message_id"], "msg-1");
        assert_eq!(payload["emoji"], "thumbsup");
    }
}
