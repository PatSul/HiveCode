//! Telegram messaging provider.
//!
//! Wraps the Telegram Bot API at `https://api.telegram.org/bot{token}/`
//! using `reqwest` for HTTP and bot-token authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest::Client;
use serde::Deserialize;
use tracing::debug;

use super::provider::{
    Attachment, Channel, IncomingMessage, MessagingProvider, Platform, SentMessage,
};

const DEFAULT_BASE_URL: &str = "https://api.telegram.org";

// ── Telegram API response types ──────────────────────────────────

/// Envelope returned by Telegram Bot API methods.
#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    description: Option<String>,
    result: Option<T>,
}

/// A Telegram chat object.
#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
    title: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    chat_type: String,
    username: Option<String>,
}

/// A Telegram user object.
#[derive(Debug, Deserialize)]
struct TelegramUser {
    #[allow(dead_code)]
    id: i64,
    first_name: String,
    username: Option<String>,
}

/// A Telegram message object.
#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    chat: TelegramChat,
    from: Option<TelegramUser>,
    date: i64,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    document: Option<TelegramDocument>,
}

/// A Telegram document attachment.
#[derive(Debug, Deserialize)]
struct TelegramDocument {
    file_id: String,
    file_name: Option<String>,
    mime_type: Option<String>,
    file_size: Option<u64>,
}

/// An update from `getUpdates`.
#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    #[serde(rename = "update_id")]
    _update_id: i64,
    message: Option<TelegramMessage>,
}

// ── Client ─────────────────────────────────────────────────────────

/// Telegram messaging provider using the Telegram Bot API.
pub struct TelegramProvider {
    base_url: String,
    token: String,
    client: Client,
}

impl TelegramProvider {
    /// Create a new Telegram provider with the given bot token.
    pub fn new(bot_token: &str) -> Result<Self> {
        Self::with_base_url(bot_token, DEFAULT_BASE_URL)
    }

    /// Create a new Telegram provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(bot_token: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Telegram")?;

        Ok(Self {
            base_url,
            token: bot_token.to_string(),
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored bot token.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Build the full method URL: `{base_url}/bot{token}/{method}`.
    fn method_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.token, method)
    }

    /// Parse a Unix timestamp into a `DateTime<Utc>`.
    fn parse_unix_ts(ts: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(ts, 0)
            .single()
            .unwrap_or_else(|| Utc::now())
    }

    fn convert_message(&self, msg: &TelegramMessage) -> IncomingMessage {
        let author = msg
            .from
            .as_ref()
            .and_then(|u| u.username.clone())
            .or_else(|| msg.from.as_ref().map(|u| u.first_name.clone()))
            .unwrap_or_else(|| "unknown".into());

        let attachments = msg
            .document
            .as_ref()
            .map(|d| {
                vec![Attachment {
                    name: d.file_name.clone().unwrap_or_else(|| d.file_id.clone()),
                    url: format!(
                        "{}/file/bot{}/{}",
                        self.base_url, self.token, d.file_id
                    ),
                    mime_type: d
                        .mime_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".into()),
                    size: d.file_size.unwrap_or(0),
                }]
            })
            .unwrap_or_default();

        IncomingMessage {
            id: msg.message_id.to_string(),
            channel_id: msg.chat.id.to_string(),
            author,
            content: msg.text.clone().unwrap_or_default(),
            timestamp: Self::parse_unix_ts(msg.date),
            attachments,
            platform: Platform::Telegram,
        }
    }
}

#[async_trait]
impl MessagingProvider for TelegramProvider {
    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let url = self.method_url("sendMessage");
        let payload = serde_json::json!({
            "chat_id": channel,
            "text": text,
        });

        debug!(url = %url, channel = %channel, "sending Telegram message");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Telegram sendMessage request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API HTTP error ({}): {}", status, body);
        }

        let envelope: TelegramResponse<TelegramMessage> = resp
            .json()
            .await
            .context("failed to parse Telegram sendMessage response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Telegram API error: {}",
                envelope.description.unwrap_or_else(|| "unknown".into())
            );
        }

        let msg = envelope
            .result
            .context("Telegram sendMessage returned no result")?;

        Ok(SentMessage {
            id: msg.message_id.to_string(),
            channel_id: msg.chat.id.to_string(),
            timestamp: Self::parse_unix_ts(msg.date),
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        // Telegram bots don't have a "list all chats" API. We use getUpdates
        // to discover chats the bot has interacted with.
        let url = self.method_url("getUpdates");

        debug!(url = %url, "listing Telegram channels via getUpdates");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Telegram getUpdates request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API HTTP error ({}): {}", status, body);
        }

        let envelope: TelegramResponse<Vec<TelegramUpdate>> = resp
            .json()
            .await
            .context("failed to parse Telegram getUpdates response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Telegram API error: {}",
                envelope.description.unwrap_or_else(|| "unknown".into())
            );
        }

        let updates = envelope.result.unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut channels = Vec::new();

        for update in &updates {
            if let Some(msg) = &update.message {
                let chat_id = msg.chat.id.to_string();
                if seen.insert(chat_id.clone()) {
                    let name = msg
                        .chat
                        .title
                        .clone()
                        .or_else(|| msg.chat.username.clone())
                        .unwrap_or_else(|| chat_id.clone());
                    channels.push(Channel {
                        id: chat_id,
                        name,
                        platform: Platform::Telegram,
                    });
                }
            }
        }

        Ok(channels)
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Telegram getUpdates returns updates across all chats; we filter
        // by the requested channel (chat_id).
        let url = format!("{}?limit={}", self.method_url("getUpdates"), limit);

        debug!(url = %url, channel = %channel, "getting Telegram messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Telegram getUpdates request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API HTTP error ({}): {}", status, body);
        }

        let envelope: TelegramResponse<Vec<TelegramUpdate>> = resp
            .json()
            .await
            .context("failed to parse Telegram getUpdates response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Telegram API error: {}",
                envelope.description.unwrap_or_else(|| "unknown".into())
            );
        }

        let updates = envelope.result.unwrap_or_default();

        Ok(updates
            .iter()
            .filter_map(|u| u.message.as_ref())
            .filter(|m| m.chat.id.to_string() == channel)
            .take(limit as usize)
            .map(|m| self.convert_message(m))
            .collect())
    }

    async fn add_reaction(&self, _channel: &str, message_id: &str, emoji: &str) -> Result<()> {
        // Telegram Bot API supports setMessageReaction (API 7.0+).
        let url = self.method_url("setMessageReaction");
        let payload = serde_json::json!({
            "chat_id": _channel,
            "message_id": message_id.parse::<i64>().unwrap_or(0),
            "reaction": [{"type": "emoji", "emoji": emoji}],
        });

        debug!(url = %url, message_id = %message_id, emoji = %emoji, "adding Telegram reaction");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Telegram setMessageReaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API HTTP error ({}): {}", status, body);
        }

        let envelope: TelegramResponse<bool> = resp
            .json()
            .await
            .context("failed to parse Telegram setMessageReaction response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Telegram API error: {}",
                envelope.description.unwrap_or_else(|| "unknown".into())
            );
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Telegram Bot API has no search endpoint. We fetch recent updates
        // and filter client-side.
        let url = format!("{}?limit=100", self.method_url("getUpdates"));

        debug!(url = %url, query = %query, "searching Telegram messages (client-side filter)");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Telegram getUpdates request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API HTTP error ({}): {}", status, body);
        }

        let envelope: TelegramResponse<Vec<TelegramUpdate>> = resp
            .json()
            .await
            .context("failed to parse Telegram getUpdates response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Telegram API error: {}",
                envelope.description.unwrap_or_else(|| "unknown".into())
            );
        }

        let updates = envelope.result.unwrap_or_default();
        let query_lower = query.to_lowercase();

        Ok(updates
            .iter()
            .filter_map(|u| u.message.as_ref())
            .filter(|m| {
                m.text
                    .as_deref()
                    .map(|t| t.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
            })
            .take(limit as usize)
            .map(|m| self.convert_message(m))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API method.
    fn build_url(base: &str, token: &str, method: &str) -> String {
        format!("{base}/bot{token}/{method}")
    }

    fn make_provider() -> TelegramProvider {
        TelegramProvider::with_base_url("123456:ABC-DEF", DEFAULT_BASE_URL).unwrap()
    }

    #[test]
    fn test_telegram_provider_default_base_url() {
        let provider = TelegramProvider::new("123456:ABC-DEF").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_telegram_provider_custom_base_url_strips_slash() {
        let provider =
            TelegramProvider::with_base_url("tok", "https://telegram.test/").unwrap();
        assert_eq!(provider.base_url(), "https://telegram.test");
    }

    #[test]
    fn test_telegram_provider_token_stored() {
        let provider = make_provider();
        assert_eq!(provider.token(), "123456:ABC-DEF");
    }

    #[test]
    fn test_telegram_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::Telegram);
    }

    #[test]
    fn test_method_url_construction() {
        let provider = make_provider();
        let url = provider.method_url("sendMessage");
        assert_eq!(
            url,
            "https://api.telegram.org/bot123456:ABC-DEF/sendMessage"
        );
    }

    #[test]
    fn test_send_message_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), provider.token(), "sendMessage");
        assert_eq!(
            url,
            "https://api.telegram.org/bot123456:ABC-DEF/sendMessage"
        );
    }

    #[test]
    fn test_get_updates_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), provider.token(), "getUpdates");
        assert!(url.contains("bot123456:ABC-DEF"));
        assert!(url.contains("getUpdates"));
    }

    #[test]
    fn test_get_chat_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), provider.token(), "getChat");
        assert!(url.contains("getChat"));
    }

    #[test]
    fn test_send_message_payload() {
        let payload = serde_json::json!({
            "chat_id": "12345",
            "text": "Hello, Telegram!",
        });
        assert_eq!(payload["chat_id"], "12345");
        assert_eq!(payload["text"], "Hello, Telegram!");
    }

    #[test]
    fn test_set_message_reaction_payload() {
        let payload = serde_json::json!({
            "chat_id": "12345",
            "message_id": 42,
            "reaction": [{"type": "emoji", "emoji": "\u{1F44D}"}],
        });
        assert_eq!(payload["chat_id"], "12345");
        assert_eq!(payload["message_id"], 42);
    }

    #[test]
    fn test_parse_unix_ts() {
        let dt = TelegramProvider::parse_unix_ts(1609459200);
        assert_eq!(dt.timestamp(), 1609459200);
    }

    #[test]
    fn test_parse_unix_ts_zero() {
        let dt = TelegramProvider::parse_unix_ts(0);
        assert_eq!(dt.timestamp(), 0);
    }

    #[test]
    fn test_telegram_response_ok_deserialization() {
        let json = r#"{"ok": true, "result": {"message_id": 1, "chat": {"id": 100, "type": "private"}, "date": 1609459200}}"#;
        let resp: TelegramResponse<TelegramMessage> = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert!(resp.description.is_none());
        let msg = resp.result.unwrap();
        assert_eq!(msg.message_id, 1);
        assert_eq!(msg.chat.id, 100);
    }

    #[test]
    fn test_telegram_response_error_deserialization() {
        let json = r#"{"ok": false, "description": "Bad Request: chat not found"}"#;
        let resp: TelegramResponse<TelegramMessage> = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(
            resp.description.as_deref(),
            Some("Bad Request: chat not found")
        );
    }

    #[test]
    fn test_telegram_chat_deserialization() {
        let json = r#"{"id": 100, "title": "My Group", "type": "group"}"#;
        let chat: TelegramChat = serde_json::from_str(json).unwrap();
        assert_eq!(chat.id, 100);
        assert_eq!(chat.title.as_deref(), Some("My Group"));
        assert_eq!(chat.chat_type, "group");
    }

    #[test]
    fn test_telegram_message_deserialization() {
        let json = r#"{
            "message_id": 42,
            "chat": {"id": 100, "type": "private"},
            "from": {"id": 1, "first_name": "Alice", "username": "alice"},
            "date": 1609459200,
            "text": "Hello!"
        }"#;
        let msg: TelegramMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_id, 42);
        assert_eq!(msg.chat.id, 100);
        assert_eq!(msg.from.as_ref().unwrap().first_name, "Alice");
        assert_eq!(msg.text.as_deref(), Some("Hello!"));
    }

    #[test]
    fn test_telegram_message_with_document() {
        let json = r#"{
            "message_id": 43,
            "chat": {"id": 100, "type": "private"},
            "date": 1609459200,
            "document": {
                "file_id": "BQACAgIAA",
                "file_name": "report.pdf",
                "mime_type": "application/pdf",
                "file_size": 4096
            }
        }"#;
        let msg: TelegramMessage = serde_json::from_str(json).unwrap();
        let doc = msg.document.as_ref().unwrap();
        assert_eq!(doc.file_id, "BQACAgIAA");
        assert_eq!(doc.file_name.as_deref(), Some("report.pdf"));
        assert_eq!(doc.file_size, Some(4096));
    }

    #[test]
    fn test_telegram_update_deserialization() {
        let json = r#"{
            "update_id": 999,
            "message": {
                "message_id": 42,
                "chat": {"id": 100, "type": "private"},
                "from": {"id": 1, "first_name": "Alice"},
                "date": 1609459200,
                "text": "Hi"
            }
        }"#;
        let update: TelegramUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(update._update_id, 999);
        assert!(update.message.is_some());
        assert_eq!(update.message.unwrap().message_id, 42);
    }

    #[test]
    fn test_convert_message() {
        let provider = make_provider();
        let tg_msg = TelegramMessage {
            message_id: 42,
            chat: TelegramChat {
                id: 100,
                title: Some("Test Group".into()),
                chat_type: "group".into(),
                username: None,
            },
            from: Some(TelegramUser {
                id: 1,
                first_name: "Alice".into(),
                username: Some("alice".into()),
            }),
            date: 1609459200,
            text: Some("Hello there".into()),
            document: None,
        };

        let msg = provider.convert_message(&tg_msg);
        assert_eq!(msg.id, "42");
        assert_eq!(msg.channel_id, "100");
        assert_eq!(msg.author, "alice");
        assert_eq!(msg.content, "Hello there");
        assert_eq!(msg.platform, Platform::Telegram);
        assert!(msg.attachments.is_empty());
    }

    #[test]
    fn test_convert_message_with_document() {
        let provider = make_provider();
        let tg_msg = TelegramMessage {
            message_id: 43,
            chat: TelegramChat {
                id: 200,
                title: None,
                chat_type: "private".into(),
                username: Some("bob".into()),
            },
            from: Some(TelegramUser {
                id: 2,
                first_name: "Bob".into(),
                username: None,
            }),
            date: 1609459200,
            text: Some("See attached".into()),
            document: Some(TelegramDocument {
                file_id: "BQACAgIAA".into(),
                file_name: Some("doc.pdf".into()),
                mime_type: Some("application/pdf".into()),
                file_size: Some(2048),
            }),
        };

        let msg = provider.convert_message(&tg_msg);
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].name, "doc.pdf");
        assert_eq!(msg.attachments[0].mime_type, "application/pdf");
        assert_eq!(msg.attachments[0].size, 2048);
    }
}
