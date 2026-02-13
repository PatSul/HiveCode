//! Matrix messaging provider.
//!
//! Wraps the Matrix Client-Server API v3 at
//! `https://matrix.example.com/_matrix/client/v3` using `reqwest` for HTTP
//! and access-token authentication.

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

const DEFAULT_BASE_URL: &str = "https://matrix.org/_matrix/client/v3";

// ── Matrix API response types ────────────────────────────────────

/// Response from sending an event.
#[derive(Debug, Deserialize)]
struct MatrixSendResponse {
    event_id: String,
}

/// A joined room entry from the sync response.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MatrixRoom {
    room_id: Option<String>,
    name: Option<String>,
    canonical_alias: Option<String>,
    num_joined_members: Option<u32>,
}

/// Response from `publicRooms` or `joined_rooms`.
#[derive(Debug, Deserialize)]
struct MatrixJoinedRoomsResponse {
    joined_rooms: Vec<String>,
}

/// Response from room name state event.
#[derive(Debug, Deserialize)]
struct MatrixRoomNameResponse {
    name: Option<String>,
}

/// Response from `messages` endpoint.
#[derive(Debug, Deserialize)]
struct MatrixMessagesResponse {
    #[serde(default)]
    chunk: Vec<MatrixEvent>,
    #[serde(rename = "end")]
    _end: Option<String>,
}

/// A Matrix event.
#[derive(Debug, Deserialize)]
struct MatrixEvent {
    event_id: String,
    sender: String,
    origin_server_ts: i64,
    #[serde(rename = "type")]
    event_type: String,
    content: Option<MatrixEventContent>,
    room_id: Option<String>,
}

/// Content of a Matrix event.
#[derive(Debug, Deserialize)]
struct MatrixEventContent {
    body: Option<String>,
    msgtype: Option<String>,
    url: Option<String>,
    #[serde(rename = "m.relates_to")]
    _relates_to: Option<serde_json::Value>,
    info: Option<MatrixFileInfo>,
}

/// File info in a Matrix event.
#[derive(Debug, Deserialize)]
struct MatrixFileInfo {
    mimetype: Option<String>,
    size: Option<u64>,
}

/// Error response from the Matrix API.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MatrixError {
    errcode: String,
    error: String,
}

// ── Client ─────────────────────────────────────────────────────────

/// Matrix messaging provider using the Matrix Client-Server API v3.
pub struct MatrixProvider {
    base_url: String,
    token: String,
    client: Client,
}

impl MatrixProvider {
    /// Create a new Matrix provider with the given access token.
    pub fn new(access_token: &str) -> Result<Self> {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new Matrix provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("invalid characters in Matrix access token")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Matrix")?;

        Ok(Self {
            base_url,
            token: access_token.to_string(),
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored access token.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Parse a Matrix origin_server_ts (milliseconds since epoch) into `DateTime<Utc>`.
    fn parse_matrix_ts(ts: i64) -> DateTime<Utc> {
        let secs = ts / 1000;
        let nsecs = ((ts % 1000) * 1_000_000) as u32;
        Utc.timestamp_opt(secs, nsecs)
            .single()
            .unwrap_or_else(|| Utc::now())
    }

    fn convert_event(&self, event: &MatrixEvent, fallback_room: &str) -> IncomingMessage {
        let room_id = event
            .room_id
            .as_deref()
            .unwrap_or(fallback_room)
            .to_string();

        let content_text = event
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .unwrap_or_default();

        let attachments = event
            .content
            .as_ref()
            .and_then(|c| {
                let msgtype = c.msgtype.as_deref().unwrap_or("");
                if matches!(msgtype, "m.file" | "m.image" | "m.audio" | "m.video") {
                    let url = c.url.clone().unwrap_or_default();
                    let name = c.body.clone().unwrap_or_else(|| "attachment".into());
                    let (mime_type, size) = c
                        .info
                        .as_ref()
                        .map(|i| {
                            (
                                i.mimetype
                                    .clone()
                                    .unwrap_or_else(|| "application/octet-stream".into()),
                                i.size.unwrap_or(0),
                            )
                        })
                        .unwrap_or_else(|| ("application/octet-stream".into(), 0));
                    Some(vec![Attachment {
                        name,
                        url,
                        mime_type,
                        size,
                    }])
                } else {
                    None
                }
            })
            .unwrap_or_default();

        IncomingMessage {
            id: event.event_id.clone(),
            channel_id: room_id,
            author: event.sender.clone(),
            content: content_text,
            timestamp: Self::parse_matrix_ts(event.origin_server_ts),
            attachments,
            platform: Platform::Matrix,
        }
    }
}

#[async_trait]
impl MessagingProvider for MatrixProvider {
    fn platform(&self) -> Platform {
        Platform::Matrix
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let txn_id = format!("hive-{}", Utc::now().timestamp_millis());
        let encoded_room = urlencod(channel);
        let url = format!(
            "{}/rooms/{}/send/m.room.message/{}",
            self.base_url, encoded_room, txn_id
        );
        let payload = serde_json::json!({
            "msgtype": "m.text",
            "body": text,
        });

        debug!(url = %url, channel = %channel, "sending Matrix message");

        let resp = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .await
            .context("Matrix send message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Matrix API HTTP error ({}): {}", status, body);
        }

        let send_resp: MatrixSendResponse = resp
            .json()
            .await
            .context("failed to parse Matrix send response")?;

        Ok(SentMessage {
            id: send_resp.event_id,
            channel_id: channel.to_string(),
            timestamp: Utc::now(),
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/joined_rooms", self.base_url);

        debug!(url = %url, "listing Matrix rooms");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Matrix joined_rooms request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Matrix API HTTP error ({}): {}", status, body);
        }

        let joined: MatrixJoinedRoomsResponse = resp
            .json()
            .await
            .context("failed to parse Matrix joined_rooms response")?;

        let mut channels = Vec::new();
        for room_id in &joined.joined_rooms {
            // Try to fetch the room name.
            let encoded_room = urlencod(room_id);
            let name_url = format!(
                "{}/rooms/{}/state/m.room.name/",
                self.base_url, encoded_room
            );

            let name = match self.client.get(&name_url).send().await {
                Ok(resp) if resp.status().is_success() => resp
                    .json::<MatrixRoomNameResponse>()
                    .await
                    .ok()
                    .and_then(|r| r.name)
                    .unwrap_or_else(|| room_id.clone()),
                _ => room_id.clone(),
            };

            channels.push(Channel {
                id: room_id.clone(),
                name,
                platform: Platform::Matrix,
            });
        }

        Ok(channels)
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let encoded_room = urlencod(channel);
        let url = format!(
            "{}/rooms/{}/messages?dir=b&limit={}",
            self.base_url, encoded_room, limit
        );

        debug!(url = %url, channel = %channel, "getting Matrix messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Matrix messages request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Matrix API HTTP error ({}): {}", status, body);
        }

        let messages: MatrixMessagesResponse = resp
            .json()
            .await
            .context("failed to parse Matrix messages response")?;

        Ok(messages
            .chunk
            .iter()
            .filter(|e| e.event_type == "m.room.message")
            .map(|e| self.convert_event(e, channel))
            .collect())
    }

    async fn add_reaction(&self, channel: &str, message_id: &str, emoji: &str) -> Result<()> {
        let txn_id = format!("hive-react-{}", Utc::now().timestamp_millis());
        let encoded_room = urlencod(channel);
        let url = format!(
            "{}/rooms/{}/send/m.reaction/{}",
            self.base_url, encoded_room, txn_id
        );
        let payload = serde_json::json!({
            "m.relates_to": {
                "rel_type": "m.annotation",
                "event_id": message_id,
                "key": emoji,
            },
        });

        debug!(url = %url, message_id = %message_id, emoji = %emoji, "adding Matrix reaction");

        let resp = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .await
            .context("Matrix reaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Matrix API HTTP error ({}): {}", status, body);
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let url = format!("{}/search", self.base_url);
        let payload = serde_json::json!({
            "search_categories": {
                "room_events": {
                    "search_term": query,
                    "order_by": "recent",
                    "keys": ["content.body"],
                },
            },
        });

        debug!(url = %url, query = %query, "searching Matrix messages");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Matrix search request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Matrix API HTTP error ({}): {}", status, body);
        }

        // The search response is complex; extract results from the nested structure.
        let body: serde_json::Value = resp
            .json()
            .await
            .context("failed to parse Matrix search response")?;

        let mut results = Vec::new();
        if let Some(room_events) = body
            .get("search_categories")
            .and_then(|sc| sc.get("room_events"))
            .and_then(|re| re.get("results"))
            .and_then(|r| r.as_array())
        {
            for result in room_events.iter().take(limit as usize) {
                if let Some(event_val) = result.get("result") {
                    if let Ok(event) = serde_json::from_value::<MatrixEvent>(event_val.clone()) {
                        results.push(self.convert_event(&event, ""));
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

    fn make_provider() -> MatrixProvider {
        MatrixProvider::with_base_url("syt_matrix_token", DEFAULT_BASE_URL).unwrap()
    }

    #[test]
    fn test_matrix_provider_default_base_url() {
        let provider = MatrixProvider::new("syt_tok").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_matrix_provider_custom_base_url_strips_slash() {
        let provider =
            MatrixProvider::with_base_url("tok", "https://matrix.test/_matrix/client/v3/").unwrap();
        assert_eq!(provider.base_url(), "https://matrix.test/_matrix/client/v3");
    }

    #[test]
    fn test_matrix_provider_token_stored() {
        let provider = make_provider();
        assert_eq!(provider.token(), "syt_matrix_token");
    }

    #[test]
    fn test_matrix_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::Matrix);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = MatrixProvider::new("tok\nwith\nnewlines");
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_url_construction() {
        let provider = make_provider();
        let room = urlencod("!room123:matrix.org");
        let url = build_url(
            provider.base_url(),
            &format!("/rooms/{room}/send/m.room.message/txn-1"),
        );
        assert!(url.contains("/rooms/"));
        assert!(url.contains("/send/m.room.message/"));
    }

    #[test]
    fn test_joined_rooms_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), "/joined_rooms");
        assert_eq!(url, "https://matrix.org/_matrix/client/v3/joined_rooms");
    }

    #[test]
    fn test_get_messages_url_construction() {
        let provider = make_provider();
        let room = urlencod("!room123:matrix.org");
        let url = build_url(
            provider.base_url(),
            &format!("/rooms/{room}/messages?dir=b&limit=50"),
        );
        assert!(url.contains("/messages"));
        assert!(url.contains("dir=b"));
        assert!(url.contains("limit=50"));
    }

    #[test]
    fn test_reaction_url_construction() {
        let provider = make_provider();
        let room = urlencod("!room123:matrix.org");
        let url = build_url(
            provider.base_url(),
            &format!("/rooms/{room}/send/m.reaction/txn-react-1"),
        );
        assert!(url.contains("/send/m.reaction/"));
    }

    #[test]
    fn test_search_url_construction() {
        let provider = make_provider();
        let url = build_url(provider.base_url(), "/search");
        assert_eq!(url, "https://matrix.org/_matrix/client/v3/search");
    }

    #[test]
    fn test_send_message_payload() {
        let payload = serde_json::json!({
            "msgtype": "m.text",
            "body": "Hello, Matrix!",
        });
        assert_eq!(payload["msgtype"], "m.text");
        assert_eq!(payload["body"], "Hello, Matrix!");
    }

    #[test]
    fn test_reaction_payload() {
        let payload = serde_json::json!({
            "m.relates_to": {
                "rel_type": "m.annotation",
                "event_id": "$event123",
                "key": "\u{1F44D}",
            },
        });
        let relates = &payload["m.relates_to"];
        assert_eq!(relates["rel_type"], "m.annotation");
        assert_eq!(relates["event_id"], "$event123");
    }

    #[test]
    fn test_search_payload() {
        let payload = serde_json::json!({
            "search_categories": {
                "room_events": {
                    "search_term": "hello",
                    "order_by": "recent",
                    "keys": ["content.body"],
                },
            },
        });
        assert_eq!(
            payload["search_categories"]["room_events"]["search_term"],
            "hello"
        );
    }

    #[test]
    fn test_parse_matrix_ts() {
        let dt = MatrixProvider::parse_matrix_ts(1609459200000);
        assert_eq!(dt.timestamp(), 1609459200);
    }

    #[test]
    fn test_parse_matrix_ts_zero() {
        let dt = MatrixProvider::parse_matrix_ts(0);
        assert_eq!(dt.timestamp(), 0);
    }

    #[test]
    fn test_matrix_send_response_deserialization() {
        let json = r#"{"event_id": "$event123"}"#;
        let resp: MatrixSendResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.event_id, "$event123");
    }

    #[test]
    fn test_matrix_joined_rooms_deserialization() {
        let json = r#"{"joined_rooms": ["!room1:matrix.org", "!room2:matrix.org"]}"#;
        let resp: MatrixJoinedRoomsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.joined_rooms.len(), 2);
        assert_eq!(resp.joined_rooms[0], "!room1:matrix.org");
    }

    #[test]
    fn test_matrix_event_deserialization() {
        let json = r#"{
            "event_id": "$evt1",
            "sender": "@alice:matrix.org",
            "origin_server_ts": 1609459200000,
            "type": "m.room.message",
            "content": {
                "body": "Hello!",
                "msgtype": "m.text"
            }
        }"#;
        let event: MatrixEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_id, "$evt1");
        assert_eq!(event.sender, "@alice:matrix.org");
        assert_eq!(event.event_type, "m.room.message");
        assert_eq!(event.content.unwrap().body.as_deref(), Some("Hello!"));
    }

    #[test]
    fn test_matrix_event_with_file() {
        let json = r#"{
            "event_id": "$evt2",
            "sender": "@bob:matrix.org",
            "origin_server_ts": 1609459200000,
            "type": "m.room.message",
            "content": {
                "body": "report.pdf",
                "msgtype": "m.file",
                "url": "mxc://matrix.org/abc123",
                "info": {
                    "mimetype": "application/pdf",
                    "size": 4096
                }
            }
        }"#;
        let event: MatrixEvent = serde_json::from_str(json).unwrap();
        let content = event.content.unwrap();
        assert_eq!(content.msgtype.as_deref(), Some("m.file"));
        assert_eq!(content.url.as_deref(), Some("mxc://matrix.org/abc123"));
        let info = content.info.unwrap();
        assert_eq!(info.mimetype.as_deref(), Some("application/pdf"));
        assert_eq!(info.size, Some(4096));
    }

    #[test]
    fn test_matrix_messages_response_deserialization() {
        let json = r#"{
            "chunk": [{
                "event_id": "$evt1",
                "sender": "@alice:matrix.org",
                "origin_server_ts": 1609459200000,
                "type": "m.room.message",
                "content": {"body": "Hello!", "msgtype": "m.text"}
            }],
            "end": "t12345"
        }"#;
        let resp: MatrixMessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.chunk.len(), 1);
        assert_eq!(resp._end.as_deref(), Some("t12345"));
    }

    #[test]
    fn test_matrix_error_deserialization() {
        let json = r#"{"errcode": "M_FORBIDDEN", "error": "You are not allowed"}"#;
        let err: MatrixError = serde_json::from_str(json).unwrap();
        assert_eq!(err.errcode, "M_FORBIDDEN");
        assert_eq!(err.error, "You are not allowed");
    }

    #[test]
    fn test_convert_event() {
        let provider = make_provider();
        let event = MatrixEvent {
            event_id: "$evt99".into(),
            sender: "@charlie:matrix.org".into(),
            origin_server_ts: 1609459200000,
            event_type: "m.room.message".into(),
            content: Some(MatrixEventContent {
                body: Some("Test message".into()),
                msgtype: Some("m.text".into()),
                url: None,
                _relates_to: None,
                info: None,
            }),
            room_id: Some("!room5:matrix.org".into()),
        };

        let msg = provider.convert_event(&event, "fallback");
        assert_eq!(msg.id, "$evt99");
        assert_eq!(msg.channel_id, "!room5:matrix.org");
        assert_eq!(msg.author, "@charlie:matrix.org");
        assert_eq!(msg.content, "Test message");
        assert_eq!(msg.platform, Platform::Matrix);
        assert!(msg.attachments.is_empty());
    }

    #[test]
    fn test_convert_event_with_file_attachment() {
        let provider = make_provider();
        let event = MatrixEvent {
            event_id: "$evt100".into(),
            sender: "@dave:matrix.org".into(),
            origin_server_ts: 1609459200000,
            event_type: "m.room.message".into(),
            content: Some(MatrixEventContent {
                body: Some("photo.jpg".into()),
                msgtype: Some("m.image".into()),
                url: Some("mxc://matrix.org/xyz789".into()),
                _relates_to: None,
                info: Some(MatrixFileInfo {
                    mimetype: Some("image/jpeg".into()),
                    size: Some(8192),
                }),
            }),
            room_id: None,
        };

        let msg = provider.convert_event(&event, "!fallback:matrix.org");
        assert_eq!(msg.channel_id, "!fallback:matrix.org");
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].name, "photo.jpg");
        assert_eq!(msg.attachments[0].url, "mxc://matrix.org/xyz789");
        assert_eq!(msg.attachments[0].mime_type, "image/jpeg");
        assert_eq!(msg.attachments[0].size, 8192);
    }

    #[test]
    fn test_urlencod() {
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("!room:matrix.org"), "%21room%3Amatrix.org");
        assert_eq!(urlencod("safe_name.txt"), "safe_name.txt");
    }
}
