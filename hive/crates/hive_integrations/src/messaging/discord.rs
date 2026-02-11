//! Discord messaging provider.
//!
//! Wraps the Discord REST API v10 at `https://discord.com/api/v10` using
//! `reqwest` for HTTP and bot-token authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::provider::{
    Attachment, Channel, IncomingMessage, MessagingProvider, Platform, SentMessage,
};

const DEFAULT_BASE_URL: &str = "https://discord.com/api/v10";

// ── Discord API response types ─────────────────────────────────────

/// A Discord guild (server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordGuild {
    pub id: String,
    pub name: String,
}

/// A Discord channel.
#[derive(Debug, Clone, Deserialize)]
struct DiscordChannel {
    id: String,
    name: Option<String>,
    #[serde(rename = "type")]
    channel_type: u8,
    #[serde(rename = "guild_id")]
    _guild_id: Option<String>,
}

/// A Discord message.
#[derive(Debug, Clone, Deserialize)]
struct DiscordMessage {
    id: String,
    channel_id: String,
    author: DiscordAuthor,
    content: String,
    timestamp: String,
    #[serde(default)]
    attachments: Vec<DiscordAttachment>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordAuthor {
    #[serde(rename = "id")]
    _id: String,
    username: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordAttachment {
    filename: String,
    url: String,
    content_type: Option<String>,
    size: u64,
}

/// The response from creating a message.
#[derive(Debug, Clone, Deserialize)]
struct DiscordSentMessage {
    id: String,
    channel_id: String,
    timestamp: String,
}

// ── Client ─────────────────────────────────────────────────────────

/// Discord messaging provider using the Discord REST API v10.
pub struct DiscordProvider {
    base_url: String,
    token: String,
    guild_id: String,
    client: Client,
}

impl DiscordProvider {
    /// Create a new Discord provider for the given guild with a bot token.
    pub fn new(bot_token: &str, guild_id: &str) -> Result<Self> {
        Self::with_base_url(bot_token, guild_id, DEFAULT_BASE_URL)
    }

    /// Create a new Discord provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(bot_token: &str, guild_id: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bot {bot_token}"))
            .context("invalid characters in Discord bot token")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("User-Agent", HeaderValue::from_static("Hive/1.0"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Discord")?;

        Ok(Self {
            base_url,
            token: bot_token.to_string(),
            guild_id: guild_id.to_string(),
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

    /// Return the guild ID this provider is scoped to.
    pub fn guild_id(&self) -> &str {
        &self.guild_id
    }

    /// List guilds the bot belongs to.
    pub async fn list_guilds(&self) -> Result<Vec<DiscordGuild>> {
        let url = format!("{}/users/@me/guilds", self.base_url);
        debug!(url = %url, "listing Discord guilds");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Discord list guilds request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error ({}): {}", status, body);
        }

        resp.json()
            .await
            .context("failed to parse Discord guilds response")
    }

    fn convert_message(&self, msg: &DiscordMessage) -> IncomingMessage {
        let timestamp = msg
            .timestamp
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now());

        IncomingMessage {
            id: msg.id.clone(),
            channel_id: msg.channel_id.clone(),
            author: msg.author.username.clone(),
            content: msg.content.clone(),
            timestamp,
            attachments: msg
                .attachments
                .iter()
                .map(|a| Attachment {
                    name: a.filename.clone(),
                    url: a.url.clone(),
                    mime_type: a
                        .content_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".into()),
                    size: a.size,
                })
                .collect(),
            platform: Platform::Discord,
        }
    }
}

#[async_trait]
impl MessagingProvider for DiscordProvider {
    fn platform(&self) -> Platform {
        Platform::Discord
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let url = format!("{}/channels/{}/messages", self.base_url, channel);
        let payload = serde_json::json!({
            "content": text,
        });

        debug!(url = %url, channel = %channel, "sending Discord message");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Discord send message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error ({}): {}", status, body);
        }

        let sent: DiscordSentMessage = resp
            .json()
            .await
            .context("failed to parse Discord send message response")?;

        let timestamp = sent
            .timestamp
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now());

        Ok(SentMessage {
            id: sent.id,
            channel_id: sent.channel_id,
            timestamp,
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/guilds/{}/channels", self.base_url, self.guild_id);

        debug!(url = %url, guild = %self.guild_id, "listing Discord channels");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Discord list channels request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error ({}): {}", status, body);
        }

        let channels: Vec<DiscordChannel> = resp
            .json()
            .await
            .context("failed to parse Discord channels response")?;

        // Only include text channels (type 0) and announcement channels (type 5).
        Ok(channels
            .into_iter()
            .filter(|c| c.channel_type == 0 || c.channel_type == 5)
            .map(|c| Channel {
                id: c.id,
                name: c.name.unwrap_or_else(|| "unnamed".into()),
                platform: Platform::Discord,
            })
            .collect())
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let clamped_limit = limit.min(100); // Discord caps at 100
        let url = format!(
            "{}/channels/{}/messages?limit={}",
            self.base_url, channel, clamped_limit
        );

        debug!(url = %url, channel = %channel, "getting Discord messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Discord get messages request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error ({}): {}", status, body);
        }

        let messages: Vec<DiscordMessage> = resp
            .json()
            .await
            .context("failed to parse Discord messages response")?;

        Ok(messages.iter().map(|m| self.convert_message(m)).collect())
    }

    async fn add_reaction(&self, channel: &str, message_id: &str, emoji: &str) -> Result<()> {
        // Discord reactions use URL-encoded emoji or custom emoji format.
        let encoded_emoji = urlencod(emoji);
        let url = format!(
            "{}/channels/{}/messages/{}/reactions/{}/@me",
            self.base_url, channel, message_id, encoded_emoji
        );

        debug!(url = %url, emoji = %emoji, "adding Discord reaction");

        let resp = self
            .client
            .put(&url)
            .send()
            .await
            .context("Discord add reaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error ({}): {}", status, body);
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Discord guild message search endpoint.
        let encoded_query = urlencod(query);
        let clamped_limit = limit.min(25); // Discord search caps at 25
        let url = format!(
            "{}/guilds/{}/messages/search?content={}&limit={}",
            self.base_url, self.guild_id, encoded_query, clamped_limit
        );

        debug!(url = %url, query = %query, "searching Discord messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Discord search messages request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error ({}): {}", status, body);
        }

        // Discord returns { messages: [[msg, ...], [msg, ...]] } where
        // each inner array is a "context group". We flatten to individual messages.
        let body: serde_json::Value = resp
            .json()
            .await
            .context("failed to parse Discord search response")?;

        let mut results = Vec::new();
        if let Some(groups) = body.get("messages").and_then(|m| m.as_array()) {
            for group in groups {
                if let Some(messages) = group.as_array() {
                    for msg_val in messages {
                        if let Ok(msg) =
                            serde_json::from_value::<DiscordMessage>(msg_val.clone())
                        {
                            results.push(self.convert_message(&msg));
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

/// Minimal percent-encoding for URL path/query segments.
fn urlencod(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    fn make_provider() -> DiscordProvider {
        DiscordProvider::with_base_url("bot-token-123", "guild-456", DEFAULT_BASE_URL).unwrap()
    }

    #[test]
    fn test_discord_provider_default_base_url() {
        let provider = DiscordProvider::new("bot-tok", "guild-1").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_discord_provider_custom_base_url_strips_slash() {
        let provider =
            DiscordProvider::with_base_url("tok", "g1", "https://discord.test/api/v10/").unwrap();
        assert_eq!(provider.base_url(), "https://discord.test/api/v10");
    }

    #[test]
    fn test_discord_provider_token_stored() {
        let provider = make_provider();
        assert_eq!(provider.token(), "bot-token-123");
    }

    #[test]
    fn test_discord_provider_guild_id_stored() {
        let provider = make_provider();
        assert_eq!(provider.guild_id(), "guild-456");
    }

    #[test]
    fn test_discord_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::Discord);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = DiscordProvider::new("tok\nwith\nnewlines", "g1");
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), "/channels/C123/messages");
        assert_eq!(url, "https://discord.com/api/v10/channels/C123/messages");
    }

    #[test]
    fn test_list_channels_url_construction() {
        let provider = make_provider();
        let url = build_url(
            provider.base_url(),
            &format!("/guilds/{}/channels", provider.guild_id()),
        );
        assert_eq!(
            url,
            "https://discord.com/api/v10/guilds/guild-456/channels"
        );
    }

    #[test]
    fn test_get_messages_url_construction() {
        let provider = make_provider();
        let url = build_url(
            provider.base_url(),
            "/channels/C123/messages?limit=50",
        );
        assert!(url.contains("/channels/C123/messages"));
        assert!(url.contains("limit=50"));
    }

    #[test]
    fn test_add_reaction_url_construction() {
        let provider = make_provider();
        let emoji = urlencod("\u{1F44D}");
        let url = build_url(
            provider.base_url(),
            &format!("/channels/C1/messages/M1/reactions/{emoji}/@me"),
        );
        assert!(url.contains("/reactions/"));
        assert!(url.contains("/@me"));
    }

    #[test]
    fn test_search_messages_url_construction() {
        let provider = make_provider();
        let query = urlencod("hello world");
        let url = build_url(
            provider.base_url(),
            &format!(
                "/guilds/{}/messages/search?content={query}&limit=10",
                provider.guild_id()
            ),
        );
        assert!(url.contains("messages/search"));
        assert!(url.contains("hello%20world"));
    }

    #[test]
    fn test_list_guilds_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), "/users/@me/guilds");
        assert_eq!(url, "https://discord.com/api/v10/users/@me/guilds");
    }

    #[test]
    fn test_send_message_payload() {
        let payload = serde_json::json!({ "content": "Hello, Discord!" });
        assert_eq!(payload["content"], "Hello, Discord!");
    }

    #[test]
    fn test_discord_guild_serialization_roundtrip() {
        let guild = DiscordGuild {
            id: "12345".into(),
            name: "My Server".into(),
        };
        let json = serde_json::to_string(&guild).unwrap();
        let back: DiscordGuild = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "12345");
        assert_eq!(back.name, "My Server");
    }

    #[test]
    fn test_discord_channel_deserialization() {
        let json = r#"{"id": "111", "name": "general", "type": 0}"#;
        let ch: DiscordChannel = serde_json::from_str(json).unwrap();
        assert_eq!(ch.id, "111");
        assert_eq!(ch.name.as_deref(), Some("general"));
        assert_eq!(ch.channel_type, 0);
    }

    #[test]
    fn test_discord_message_deserialization() {
        let json = r#"{
            "id": "msg-1",
            "channel_id": "ch-1",
            "author": {"id": "u1", "username": "alice"},
            "content": "Hello!",
            "timestamp": "2025-01-01T00:00:00Z",
            "attachments": []
        }"#;
        let msg: DiscordMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "msg-1");
        assert_eq!(msg.author.username, "alice");
        assert_eq!(msg.content, "Hello!");
    }

    #[test]
    fn test_discord_message_with_attachments() {
        let json = r#"{
            "id": "msg-2",
            "channel_id": "ch-2",
            "author": {"id": "u2", "username": "bob"},
            "content": "See file",
            "timestamp": "2025-01-01T12:00:00Z",
            "attachments": [{
                "filename": "report.pdf",
                "url": "https://cdn.discord.com/report.pdf",
                "content_type": "application/pdf",
                "size": 4096
            }]
        }"#;
        let msg: DiscordMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].filename, "report.pdf");
        assert_eq!(msg.attachments[0].size, 4096);
    }

    #[test]
    fn test_discord_sent_message_deserialization() {
        let json = r#"{"id": "sent-1", "channel_id": "ch-1", "timestamp": "2025-01-01T00:00:00Z"}"#;
        let sent: DiscordSentMessage = serde_json::from_str(json).unwrap();
        assert_eq!(sent.id, "sent-1");
        assert_eq!(sent.channel_id, "ch-1");
    }

    #[test]
    fn test_convert_message() {
        let provider = make_provider();
        let discord_msg = DiscordMessage {
            id: "msg-99".into(),
            channel_id: "ch-5".into(),
            author: DiscordAuthor {
                _id: "u1".into(),
                username: "charlie".into(),
            },
            content: "Test message".into(),
            timestamp: "2025-06-15T10:30:00Z".into(),
            attachments: vec![DiscordAttachment {
                filename: "image.png".into(),
                url: "https://cdn.discord.com/image.png".into(),
                content_type: Some("image/png".into()),
                size: 8192,
            }],
        };

        let msg = provider.convert_message(&discord_msg);
        assert_eq!(msg.id, "msg-99");
        assert_eq!(msg.channel_id, "ch-5");
        assert_eq!(msg.author, "charlie");
        assert_eq!(msg.content, "Test message");
        assert_eq!(msg.platform, Platform::Discord);
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].name, "image.png");
        assert_eq!(msg.attachments[0].mime_type, "image/png");
    }

    #[test]
    fn test_urlencod() {
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencod("safe_name.txt"), "safe_name.txt");
    }

    #[test]
    fn test_discord_channel_filter_text_only() {
        // Type 0 = text, type 2 = voice, type 5 = announcement
        let channels = vec![
            DiscordChannel {
                id: "1".into(),
                name: Some("general".into()),
                channel_type: 0,
                _guild_id: None,
            },
            DiscordChannel {
                id: "2".into(),
                name: Some("voice".into()),
                channel_type: 2,
                _guild_id: None,
            },
            DiscordChannel {
                id: "3".into(),
                name: Some("announcements".into()),
                channel_type: 5,
                _guild_id: None,
            },
        ];

        let filtered: Vec<_> = channels
            .into_iter()
            .filter(|c| c.channel_type == 0 || c.channel_type == 5)
            .collect();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].name.as_deref(), Some("general"));
        assert_eq!(filtered[1].name.as_deref(), Some("announcements"));
    }
}
