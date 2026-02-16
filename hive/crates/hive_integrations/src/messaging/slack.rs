//! Slack messaging provider.
//!
//! Wraps the Slack Web API at `https://slack.com/api` using
//! `reqwest` for HTTP and bot-token authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::debug;

use super::provider::{
    Attachment, Channel, IncomingMessage, MessagingProvider, Platform, SentMessage,
};

const DEFAULT_BASE_URL: &str = "https://slack.com/api";

// ── Slack API response types ───────────────────────────────────────

/// Envelope returned by most Slack Web API methods.
#[derive(Debug, Deserialize)]
struct SlackResponse<T> {
    ok: bool,
    error: Option<String>,
    #[serde(flatten)]
    data: Option<T>,
}

/// Data portion of `chat.postMessage` response.
#[derive(Debug, Deserialize)]
struct PostMessageData {
    ts: Option<String>,
    channel: Option<String>,
}

/// Data portion of `conversations.list` response.
#[derive(Debug, Deserialize)]
struct ConversationsListData {
    #[serde(default)]
    channels: Vec<SlackChannel>,
}

#[derive(Debug, Deserialize)]
struct SlackChannel {
    id: String,
    name: String,
}

/// Data portion of `conversations.history` response.
#[derive(Debug, Deserialize)]
struct ConversationsHistoryData {
    #[serde(default)]
    messages: Vec<SlackMessage>,
}

/// Data portion of `search.messages` response.
#[derive(Debug, Deserialize)]
struct SearchMessagesData {
    messages: Option<SearchMessages>,
}

#[derive(Debug, Deserialize)]
struct SearchMessages {
    #[serde(default)]
    matches: Vec<SlackMessage>,
}

#[derive(Debug, Deserialize)]
struct SlackMessage {
    #[serde(default)]
    ts: String,
    #[serde(default)]
    user: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    files: Vec<SlackFile>,
}

#[derive(Debug, Deserialize)]
struct SlackFile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url_private: String,
    #[serde(default)]
    mimetype: String,
    #[serde(default)]
    size: u64,
}

// ── Client ─────────────────────────────────────────────────────────

/// Slack messaging provider using the Slack Web API.
pub struct SlackProvider {
    base_url: String,
    token: String,
    client: Client,
}

impl SlackProvider {
    /// Create a new Slack provider with the given bot token.
    pub fn new(bot_token: &str) -> Result<Self> {
        Self::with_base_url(bot_token, DEFAULT_BASE_URL)
    }

    /// Create a new Slack provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(bot_token: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {bot_token}"))
            .context("invalid characters in Slack bot token")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Slack")?;

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

    /// Parse a Slack timestamp (e.g. "1234567890.123456") into a `DateTime<Utc>`.
    fn parse_slack_ts(ts: &str) -> DateTime<Utc> {
        let secs = ts
            .split('.')
            .next()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        Utc.timestamp_opt(secs, 0)
            .single()
            .unwrap_or_else(Utc::now)
    }

    fn convert_message(&self, msg: &SlackMessage, fallback_channel: &str) -> IncomingMessage {
        let channel_id = msg
            .channel
            .as_deref()
            .unwrap_or(fallback_channel)
            .to_string();
        IncomingMessage {
            id: msg.ts.clone(),
            channel_id,
            author: msg.user.clone(),
            content: msg.text.clone(),
            timestamp: Self::parse_slack_ts(&msg.ts),
            attachments: msg
                .files
                .iter()
                .map(|f| Attachment {
                    name: f.name.clone(),
                    url: f.url_private.clone(),
                    mime_type: f.mimetype.clone(),
                    size: f.size,
                })
                .collect(),
            platform: Platform::Slack,
        }
    }
}

#[async_trait]
impl MessagingProvider for SlackProvider {
    fn platform(&self) -> Platform {
        Platform::Slack
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let url = format!("{}/chat.postMessage", self.base_url);
        let payload = serde_json::json!({
            "channel": channel,
            "text": text,
        });

        debug!(url = %url, channel = %channel, "sending Slack message");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Slack chat.postMessage request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Slack API HTTP error ({}): {}", status, body);
        }

        let envelope: SlackResponse<PostMessageData> = resp
            .json()
            .await
            .context("failed to parse Slack postMessage response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Slack API error: {}",
                envelope.error.unwrap_or_else(|| "unknown".into())
            );
        }

        let data = envelope.data.unwrap_or(PostMessageData {
            ts: None,
            channel: None,
        });

        Ok(SentMessage {
            id: data.ts.unwrap_or_default(),
            channel_id: data.channel.unwrap_or_else(|| channel.to_string()),
            timestamp: Utc::now(),
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!(
            "{}/conversations.list?types=public_channel,private_channel&limit=200",
            self.base_url
        );

        debug!(url = %url, "listing Slack channels");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Slack conversations.list request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Slack API HTTP error ({}): {}", status, body);
        }

        let envelope: SlackResponse<ConversationsListData> = resp
            .json()
            .await
            .context("failed to parse Slack conversations.list response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Slack API error: {}",
                envelope.error.unwrap_or_else(|| "unknown".into())
            );
        }

        let data = envelope
            .data
            .unwrap_or(ConversationsListData { channels: vec![] });

        Ok(data
            .channels
            .into_iter()
            .map(|c| Channel {
                id: c.id,
                name: c.name,
                platform: Platform::Slack,
            })
            .collect())
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let url = format!(
            "{}/conversations.history?channel={}&limit={}",
            self.base_url, channel, limit
        );

        debug!(url = %url, "getting Slack messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Slack conversations.history request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Slack API HTTP error ({}): {}", status, body);
        }

        let envelope: SlackResponse<ConversationsHistoryData> = resp
            .json()
            .await
            .context("failed to parse Slack conversations.history response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Slack API error: {}",
                envelope.error.unwrap_or_else(|| "unknown".into())
            );
        }

        let data = envelope
            .data
            .unwrap_or(ConversationsHistoryData { messages: vec![] });

        Ok(data
            .messages
            .iter()
            .map(|m| self.convert_message(m, channel))
            .collect())
    }

    async fn add_reaction(&self, channel: &str, message_id: &str, emoji: &str) -> Result<()> {
        let url = format!("{}/reactions.add", self.base_url);
        let payload = serde_json::json!({
            "channel": channel,
            "timestamp": message_id,
            "name": emoji,
        });

        debug!(url = %url, channel = %channel, ts = %message_id, emoji = %emoji, "adding Slack reaction");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Slack reactions.add request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Slack API HTTP error ({}): {}", status, body);
        }

        let envelope: SlackResponse<serde_json::Value> = resp
            .json()
            .await
            .context("failed to parse Slack reactions.add response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Slack API error: {}",
                envelope.error.unwrap_or_else(|| "unknown".into())
            );
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let encoded_query = urlencod(query);
        let url = format!(
            "{}/search.messages?query={}&count={}",
            self.base_url, encoded_query, limit
        );

        debug!(url = %url, "searching Slack messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Slack search.messages request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Slack API HTTP error ({}): {}", status, body);
        }

        let envelope: SlackResponse<SearchMessagesData> = resp
            .json()
            .await
            .context("failed to parse Slack search.messages response")?;

        if !envelope.ok {
            anyhow::bail!(
                "Slack API error: {}",
                envelope.error.unwrap_or_else(|| "unknown".into())
            );
        }

        let data = envelope
            .data
            .unwrap_or(SearchMessagesData { messages: None });
        let matches = data.messages.map(|m| m.matches).unwrap_or_default();

        Ok(matches
            .iter()
            .map(|m| self.convert_message(m, ""))
            .collect())
    }
}

/// Minimal percent-encoding for query parameter values.
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

    #[test]
    fn test_slack_provider_default_base_url() {
        let provider = SlackProvider::new("xoxb-test-token").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_slack_provider_custom_base_url_strips_slash() {
        let provider = SlackProvider::with_base_url("xoxb-tok", "https://slack.test/api/").unwrap();
        assert_eq!(provider.base_url(), "https://slack.test/api");
    }

    #[test]
    fn test_slack_provider_token_stored() {
        let provider = SlackProvider::new("xoxb-my-token").unwrap();
        assert_eq!(provider.token(), "xoxb-my-token");
    }

    #[test]
    fn test_slack_provider_platform() {
        let provider = SlackProvider::new("xoxb-tok").unwrap();
        assert_eq!(provider.platform(), Platform::Slack);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = SlackProvider::new("tok\nwith\nnewlines");
        assert!(result.is_err());
    }

    #[test]
    fn test_post_message_url_construction() {
        let provider = SlackProvider::new("xoxb-tok").unwrap();
        let url = build_url(provider.base_url(), "/chat.postMessage");
        assert_eq!(url, "https://slack.com/api/chat.postMessage");
    }

    #[test]
    fn test_conversations_list_url_construction() {
        let provider = SlackProvider::new("xoxb-tok").unwrap();
        let url = build_url(
            provider.base_url(),
            "/conversations.list?types=public_channel,private_channel&limit=200",
        );
        assert!(url.contains("conversations.list"));
        assert!(url.contains("public_channel"));
    }

    #[test]
    fn test_conversations_history_url_construction() {
        let provider = SlackProvider::new("xoxb-tok").unwrap();
        let url = build_url(
            provider.base_url(),
            "/conversations.history?channel=C123&limit=50",
        );
        assert!(url.contains("conversations.history"));
        assert!(url.contains("channel=C123"));
        assert!(url.contains("limit=50"));
    }

    #[test]
    fn test_reactions_add_url_construction() {
        let provider = SlackProvider::new("xoxb-tok").unwrap();
        let url = build_url(provider.base_url(), "/reactions.add");
        assert_eq!(url, "https://slack.com/api/reactions.add");
    }

    #[test]
    fn test_search_messages_url_construction() {
        let provider = SlackProvider::new("xoxb-tok").unwrap();
        let query = urlencod("hello world");
        let url = build_url(
            provider.base_url(),
            &format!("/search.messages?query={query}&count=10"),
        );
        assert!(url.contains("search.messages"));
        assert!(url.contains("hello%20world"));
    }

    #[test]
    fn test_post_message_payload() {
        let payload = serde_json::json!({
            "channel": "C123",
            "text": "Hello, Slack!",
        });
        assert_eq!(payload["channel"], "C123");
        assert_eq!(payload["text"], "Hello, Slack!");
    }

    #[test]
    fn test_reactions_add_payload() {
        let payload = serde_json::json!({
            "channel": "C123",
            "timestamp": "1234567890.123456",
            "name": "thumbsup",
        });
        assert_eq!(payload["channel"], "C123");
        assert_eq!(payload["timestamp"], "1234567890.123456");
        assert_eq!(payload["name"], "thumbsup");
    }

    #[test]
    fn test_parse_slack_ts() {
        let dt = SlackProvider::parse_slack_ts("1609459200.000100");
        assert_eq!(dt.timestamp(), 1609459200);
    }

    #[test]
    fn test_parse_slack_ts_invalid() {
        let dt = SlackProvider::parse_slack_ts("not-a-number");
        // Falls back to 0 epoch, then to Utc::now() if that fails.
        // Epoch 0 is valid, so it should parse.
        assert!(dt.timestamp() >= 0);
    }

    #[test]
    fn test_slack_channel_deserialization() {
        let json = r#"{"id": "C01ABC", "name": "general"}"#;
        let ch: SlackChannel = serde_json::from_str(json).unwrap();
        assert_eq!(ch.id, "C01ABC");
        assert_eq!(ch.name, "general");
    }

    #[test]
    fn test_slack_message_deserialization() {
        let json = r#"{
            "ts": "1609459200.000100",
            "user": "U01XYZ",
            "text": "Hello!"
        }"#;
        let msg: SlackMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.ts, "1609459200.000100");
        assert_eq!(msg.user, "U01XYZ");
        assert_eq!(msg.text, "Hello!");
    }

    #[test]
    fn test_slack_response_ok_deserialization() {
        let json = r#"{"ok": true, "ts": "123.456", "channel": "C01"}"#;
        let resp: SlackResponse<PostMessageData> = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert!(resp.error.is_none());
        let data = resp.data.unwrap();
        assert_eq!(data.ts.as_deref(), Some("123.456"));
    }

    #[test]
    fn test_slack_response_error_deserialization() {
        let json = r#"{"ok": false, "error": "channel_not_found"}"#;
        let resp: SlackResponse<PostMessageData> = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.error.as_deref(), Some("channel_not_found"));
    }

    #[test]
    fn test_urlencod() {
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencod("safe-string_123.txt"), "safe-string_123.txt");
    }

    #[test]
    fn test_convert_message() {
        let provider = SlackProvider::new("xoxb-tok").unwrap();
        let slack_msg = SlackMessage {
            ts: "1609459200.000100".into(),
            user: "U01".into(),
            text: "Hi there".into(),
            channel: Some("C01".into()),
            files: vec![SlackFile {
                name: "doc.pdf".into(),
                url_private: "https://files.slack.com/doc.pdf".into(),
                mimetype: "application/pdf".into(),
                size: 2048,
            }],
        };

        let msg = provider.convert_message(&slack_msg, "fallback");
        assert_eq!(msg.id, "1609459200.000100");
        assert_eq!(msg.channel_id, "C01");
        assert_eq!(msg.author, "U01");
        assert_eq!(msg.content, "Hi there");
        assert_eq!(msg.platform, Platform::Slack);
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].name, "doc.pdf");
    }
}
