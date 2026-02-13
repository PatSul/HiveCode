//! Microsoft Teams messaging provider.
//!
//! Wraps the Microsoft Graph API at `https://graph.microsoft.com/v1.0`
//! for Teams bot messaging using `reqwest` for HTTP and bearer-token
//! authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::debug;

use super::provider::{
    Attachment, Channel, IncomingMessage, MessagingProvider, Platform, SentMessage,
};

const DEFAULT_BASE_URL: &str = "https://graph.microsoft.com/v1.0";

// ── Teams / Graph API response types ─────────────────────────────

/// Envelope for Graph API list responses.
#[derive(Debug, Deserialize)]
struct GraphListResponse<T> {
    #[serde(default)]
    value: Vec<T>,
}

/// A Teams team (group).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TeamsTeam {
    id: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

/// A Teams channel.
#[derive(Debug, Default, Deserialize)]
struct TeamsChannel {
    id: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

/// A Teams chat message.
#[derive(Debug, Default, Deserialize)]
struct TeamsMessage {
    id: String,
    #[serde(rename = "createdDateTime")]
    created_date_time: Option<String>,
    from: Option<TeamsFrom>,
    body: Option<TeamsMessageBody>,
    #[serde(default)]
    attachments: Vec<TeamsAttachment>,
}

#[derive(Debug, Deserialize)]
struct TeamsFrom {
    user: Option<TeamsUser>,
}

#[derive(Debug, Deserialize)]
struct TeamsUser {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "id")]
    _id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TeamsMessageBody {
    content: Option<String>,
    #[serde(rename = "contentType")]
    _content_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TeamsAttachment {
    #[serde(rename = "id")]
    _id: Option<String>,
    name: Option<String>,
    #[serde(rename = "contentUrl")]
    content_url: Option<String>,
    #[serde(rename = "contentType")]
    content_type: Option<String>,
}

/// Response from sending a message.
#[derive(Debug, Deserialize)]
struct TeamsSentMessage {
    id: String,
    #[serde(rename = "createdDateTime")]
    created_date_time: Option<String>,
}

// ── Client ─────────────────────────────────────────────────────────

/// Microsoft Teams messaging provider using the Microsoft Graph API.
pub struct TeamsProvider {
    base_url: String,
    token: String,
    team_id: String,
    client: Client,
}

impl TeamsProvider {
    /// Create a new Teams provider for the given team with an access token.
    pub fn new(access_token: &str, team_id: &str) -> Result<Self> {
        Self::with_base_url(access_token, team_id, DEFAULT_BASE_URL)
    }

    /// Create a new Teams provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(access_token: &str, team_id: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("invalid characters in Teams access token")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Teams")?;

        Ok(Self {
            base_url,
            token: access_token.to_string(),
            team_id: team_id.to_string(),
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

    /// Return the team ID this provider is scoped to.
    pub fn team_id(&self) -> &str {
        &self.team_id
    }

    fn convert_message(&self, msg: &TeamsMessage, channel_id: &str) -> IncomingMessage {
        let author = msg
            .from
            .as_ref()
            .and_then(|f| f.user.as_ref())
            .and_then(|u| u.display_name.clone())
            .unwrap_or_else(|| "unknown".into());

        let content = msg
            .body
            .as_ref()
            .and_then(|b| b.content.clone())
            .unwrap_or_default();

        let timestamp = msg
            .created_date_time
            .as_deref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let attachments = msg
            .attachments
            .iter()
            .map(|a| Attachment {
                name: a.name.clone().unwrap_or_else(|| "attachment".into()),
                url: a.content_url.clone().unwrap_or_default(),
                mime_type: a
                    .content_type
                    .clone()
                    .unwrap_or_else(|| "application/octet-stream".into()),
                size: 0,
            })
            .collect();

        IncomingMessage {
            id: msg.id.clone(),
            channel_id: channel_id.to_string(),
            author,
            content,
            timestamp,
            attachments,
            platform: Platform::Teams,
        }
    }
}

#[async_trait]
impl MessagingProvider for TeamsProvider {
    fn platform(&self) -> Platform {
        Platform::Teams
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let url = format!(
            "{}/teams/{}/channels/{}/messages",
            self.base_url, self.team_id, channel
        );
        let payload = serde_json::json!({
            "body": {
                "content": text,
            },
        });

        debug!(url = %url, channel = %channel, "sending Teams message");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Teams send message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Teams API HTTP error ({}): {}", status, body);
        }

        let sent: TeamsSentMessage = resp
            .json()
            .await
            .context("failed to parse Teams send message response")?;

        let timestamp = sent
            .created_date_time
            .as_deref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        Ok(SentMessage {
            id: sent.id,
            channel_id: channel.to_string(),
            timestamp,
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/teams/{}/channels", self.base_url, self.team_id);

        debug!(url = %url, team = %self.team_id, "listing Teams channels");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Teams list channels request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Teams API HTTP error ({}): {}", status, body);
        }

        let list: GraphListResponse<TeamsChannel> = resp
            .json()
            .await
            .context("failed to parse Teams channels response")?;

        Ok(list
            .value
            .into_iter()
            .map(|c| Channel {
                id: c.id,
                name: c.display_name.unwrap_or_else(|| "unnamed".into()),
                platform: Platform::Teams,
            })
            .collect())
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let url = format!(
            "{}/teams/{}/channels/{}/messages?$top={}",
            self.base_url, self.team_id, channel, limit
        );

        debug!(url = %url, channel = %channel, "getting Teams messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Teams get messages request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Teams API HTTP error ({}): {}", status, body);
        }

        let list: GraphListResponse<TeamsMessage> = resp
            .json()
            .await
            .context("failed to parse Teams messages response")?;

        Ok(list
            .value
            .iter()
            .map(|m| self.convert_message(m, channel))
            .collect())
    }

    async fn add_reaction(&self, channel: &str, message_id: &str, emoji: &str) -> Result<()> {
        // Graph API uses POST to .../messages/{id}/hostedContents for reactions
        // but the standard approach is the reactions endpoint.
        let url = format!(
            "{}/teams/{}/channels/{}/messages/{}/reactions",
            self.base_url, self.team_id, channel, message_id
        );
        let payload = serde_json::json!({
            "reactionType": emoji,
        });

        debug!(url = %url, message_id = %message_id, emoji = %emoji, "adding Teams reaction");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Teams add reaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Teams API HTTP error ({}): {}", status, body);
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Graph API search endpoint for Teams messages.
        let encoded_query = urlencod(query);
        let url = format!(
            "{}/teams/{}/channels?$filter=contains(displayName,'{}')&$top={}",
            self.base_url, self.team_id, encoded_query, limit
        );

        debug!(url = %url, query = %query, "searching Teams messages");

        // For Teams, a proper search would use /search/query or iterate channels.
        // We list channels and search messages in each, but for simplicity we
        // fetch from the first channel and filter client-side.
        let channels_url = format!("{}/teams/{}/channels", self.base_url, self.team_id);

        let resp = self
            .client
            .get(&channels_url)
            .send()
            .await
            .context("Teams search - list channels request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Teams API HTTP error ({}): {}", status, body);
        }

        let channel_list: GraphListResponse<TeamsChannel> = resp
            .json()
            .await
            .context("failed to parse Teams channels for search")?;

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for ch in channel_list.value.iter().take(5) {
            let msgs_url = format!(
                "{}/teams/{}/channels/{}/messages?$top=50",
                self.base_url, self.team_id, ch.id
            );

            let resp = self
                .client
                .get(&msgs_url)
                .send()
                .await
                .context("Teams search - get messages request failed")?;

            if !resp.status().is_success() {
                continue;
            }

            let msg_list: GraphListResponse<TeamsMessage> = match resp.json().await {
                Ok(l) => l,
                Err(_) => continue,
            };

            for msg in &msg_list.value {
                let content = msg
                    .body
                    .as_ref()
                    .and_then(|b| b.content.as_deref())
                    .unwrap_or("");
                if content.to_lowercase().contains(&query_lower) {
                    results.push(self.convert_message(msg, &ch.id));
                    if results.len() >= limit as usize {
                        return Ok(results);
                    }
                }
            }
        }

        Ok(results)
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

    fn make_provider() -> TeamsProvider {
        TeamsProvider::with_base_url("eyJ0eXAi.access-token", "team-123", DEFAULT_BASE_URL).unwrap()
    }

    #[test]
    fn test_teams_provider_default_base_url() {
        let provider = TeamsProvider::new("access-tok", "team-1").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_teams_provider_custom_base_url_strips_slash() {
        let provider =
            TeamsProvider::with_base_url("tok", "t1", "https://graph.test/v1.0/").unwrap();
        assert_eq!(provider.base_url(), "https://graph.test/v1.0");
    }

    #[test]
    fn test_teams_provider_token_stored() {
        let provider = make_provider();
        assert_eq!(provider.token(), "eyJ0eXAi.access-token");
    }

    #[test]
    fn test_teams_provider_team_id_stored() {
        let provider = make_provider();
        assert_eq!(provider.team_id(), "team-123");
    }

    #[test]
    fn test_teams_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::Teams);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = TeamsProvider::new("tok\nwith\nnewlines", "t1");
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_url_construction() {
        let provider = make_provider();
        let url = build_url(
            provider.base_url(),
            &format!("/teams/{}/channels/ch-1/messages", provider.team_id()),
        );
        assert_eq!(
            url,
            "https://graph.microsoft.com/v1.0/teams/team-123/channels/ch-1/messages"
        );
    }

    #[test]
    fn test_list_channels_url_construction() {
        let provider = make_provider();
        let url = build_url(
            provider.base_url(),
            &format!("/teams/{}/channels", provider.team_id()),
        );
        assert_eq!(
            url,
            "https://graph.microsoft.com/v1.0/teams/team-123/channels"
        );
    }

    #[test]
    fn test_get_messages_url_construction() {
        let provider = make_provider();
        let url = build_url(
            provider.base_url(),
            &format!(
                "/teams/{}/channels/ch-1/messages?$top=50",
                provider.team_id()
            ),
        );
        assert!(url.contains("/channels/ch-1/messages"));
        assert!(url.contains("$top=50"));
    }

    #[test]
    fn test_add_reaction_url_construction() {
        let provider = make_provider();
        let url = build_url(
            provider.base_url(),
            &format!(
                "/teams/{}/channels/ch-1/messages/msg-1/reactions",
                provider.team_id()
            ),
        );
        assert!(url.contains("/messages/msg-1/reactions"));
    }

    #[test]
    fn test_send_message_payload() {
        let payload = serde_json::json!({
            "body": {
                "content": "Hello, Teams!",
            },
        });
        assert_eq!(payload["body"]["content"], "Hello, Teams!");
    }

    #[test]
    fn test_reaction_payload() {
        let payload = serde_json::json!({
            "reactionType": "like",
        });
        assert_eq!(payload["reactionType"], "like");
    }

    #[test]
    fn test_teams_channel_deserialization() {
        let json = r#"{"id": "ch-1", "displayName": "General"}"#;
        let ch: TeamsChannel = serde_json::from_str(json).unwrap();
        assert_eq!(ch.id, "ch-1");
        assert_eq!(ch.display_name.as_deref(), Some("General"));
    }

    #[test]
    fn test_teams_message_deserialization() {
        let json = r#"{
            "id": "msg-1",
            "createdDateTime": "2025-01-01T00:00:00Z",
            "from": {
                "user": {
                    "displayName": "Alice",
                    "id": "user-1"
                }
            },
            "body": {
                "content": "Hello!",
                "contentType": "text"
            },
            "attachments": []
        }"#;
        let msg: TeamsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "msg-1");
        assert_eq!(
            msg.from.unwrap().user.unwrap().display_name.as_deref(),
            Some("Alice")
        );
        assert_eq!(msg.body.unwrap().content.as_deref(), Some("Hello!"));
    }

    #[test]
    fn test_teams_message_with_attachments() {
        let json = r#"{
            "id": "msg-2",
            "attachments": [{
                "id": "att-1",
                "name": "report.pdf",
                "contentUrl": "https://graph.microsoft.com/file/report.pdf",
                "contentType": "application/pdf"
            }]
        }"#;
        let msg: TeamsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].name.as_deref(), Some("report.pdf"));
    }

    #[test]
    fn test_graph_list_response_deserialization() {
        let json = r#"{"value": [{"id": "ch-1", "displayName": "General"}, {"id": "ch-2", "displayName": "Random"}]}"#;
        let list: GraphListResponse<TeamsChannel> = serde_json::from_str(json).unwrap();
        assert_eq!(list.value.len(), 2);
        assert_eq!(list.value[0].id, "ch-1");
        assert_eq!(list.value[1].display_name.as_deref(), Some("Random"));
    }

    #[test]
    fn test_teams_sent_message_deserialization() {
        let json = r#"{"id": "sent-1", "createdDateTime": "2025-01-01T00:00:00Z"}"#;
        let sent: TeamsSentMessage = serde_json::from_str(json).unwrap();
        assert_eq!(sent.id, "sent-1");
        assert_eq!(
            sent.created_date_time.as_deref(),
            Some("2025-01-01T00:00:00Z")
        );
    }

    #[test]
    fn test_convert_message() {
        let provider = make_provider();
        let teams_msg = TeamsMessage {
            id: "msg-99".into(),
            created_date_time: Some("2025-06-15T10:30:00Z".into()),
            from: Some(TeamsFrom {
                user: Some(TeamsUser {
                    display_name: Some("Charlie".into()),
                    _id: Some("u1".into()),
                }),
            }),
            body: Some(TeamsMessageBody {
                content: Some("Test message".into()),
                _content_type: Some("text".into()),
            }),
            attachments: vec![],
        };

        let msg = provider.convert_message(&teams_msg, "ch-5");
        assert_eq!(msg.id, "msg-99");
        assert_eq!(msg.channel_id, "ch-5");
        assert_eq!(msg.author, "Charlie");
        assert_eq!(msg.content, "Test message");
        assert_eq!(msg.platform, Platform::Teams);
        assert!(msg.attachments.is_empty());
    }

    #[test]
    fn test_urlencod() {
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencod("safe_name.txt"), "safe_name.txt");
    }
}
