//! Google Gmail API v1 client.
//!
//! Wraps the REST API at `https://gmail.googleapis.com/gmail/v1/users/me`
//! using `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

// ── Types ─────────────────────────────────────────────────────────

/// A single email message from the Gmail API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub snippet: String,
}

/// A paginated list of email messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailList {
    #[serde(default)]
    pub messages: Vec<EmailListEntry>,
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub result_size_estimate: u32,
}

/// A minimal entry returned by the messages.list endpoint (only id and threadId).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailListEntry {
    pub id: String,
    #[serde(default)]
    pub thread_id: String,
}

/// Request body for sending an email.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendEmailRequest {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Request body for creating a draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRequest {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// A Gmail label.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabel {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub r#type: String,
}

/// Request to modify labels on a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyLabelsRequest {
    #[serde(default)]
    pub add_label_ids: Vec<String>,
    #[serde(default)]
    pub remove_label_ids: Vec<String>,
}

/// A Gmail draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailDraft {
    #[serde(default)]
    pub id: String,
    pub message: Option<EmailListEntry>,
}

/// Raw Gmail message returned from the API (before header extraction).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGmailMessage {
    #[serde(default)]
    id: String,
    #[serde(default)]
    thread_id: String,
    #[serde(default)]
    label_ids: Vec<String>,
    #[serde(default)]
    snippet: String,
    payload: Option<RawPayload>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPayload {
    #[serde(default)]
    headers: Vec<RawHeader>,
    body: Option<RawBody>,
    #[serde(default)]
    parts: Vec<RawPart>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHeader {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBody {
    #[serde(default)]
    data: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPart {
    #[serde(default)]
    mime_type: String,
    body: Option<RawBody>,
}

/// Wrapper for the labels list response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelsListResponse {
    #[serde(default)]
    labels: Vec<GmailLabel>,
}

// ── Client ────────────────────────────────────────────────────────

/// Client for the Google Gmail v1 REST API.
pub struct GmailClient {
    base_url: String,
    client: Client,
}

impl GmailClient {
    /// Create a new client using the given OAuth access token.
    pub fn new(access_token: &str) -> Self {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom base URL (useful for testing).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        // access_token is assumed to be a valid bearer token
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {access_token}")) {
            headers.insert(AUTHORIZATION, val);
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { base_url, client }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// List email messages, optionally filtered by a Gmail search query.
    ///
    /// Returns minimal entries (id + threadId). Call `get_message` for full details.
    pub async fn list_messages(
        &self,
        query: Option<&str>,
        max_results: u32,
    ) -> Result<EmailList> {
        let mut url = format!(
            "{}/messages?maxResults={}",
            self.base_url, max_results
        );

        if let Some(q) = query {
            url.push_str(&format!("&q={}", urlencod(q)));
        }

        debug!(url = %url, "listing Gmail messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Gmail list_messages request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Gmail message list")
    }

    /// Get a full email message by ID, including parsed headers and body.
    pub async fn get_message(&self, message_id: &str) -> Result<EmailMessage> {
        let url = format!(
            "{}/messages/{}?format=full",
            self.base_url, message_id
        );
        debug!(url = %url, "getting Gmail message");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Gmail get_message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error ({}): {}", status, body);
        }

        let raw: RawGmailMessage = resp
            .json()
            .await
            .context("failed to parse Gmail raw message")?;

        Ok(parse_raw_message(raw))
    }

    /// Send an email.
    ///
    /// Constructs an RFC 2822 message and sends it via the Gmail API.
    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<EmailListEntry> {
        let url = format!("{}/messages/send", self.base_url);

        let raw_message = build_rfc2822(to, subject, body);
        let encoded = base64url_encode(raw_message.as_bytes());

        let payload = serde_json::json!({ "raw": encoded });

        debug!(url = %url, to = %to, subject = %subject, "sending Gmail email");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Gmail send_email request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Gmail send response")
    }

    /// Create a draft email.
    pub async fn create_draft(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<GmailDraft> {
        let url = format!("{}/drafts", self.base_url);

        let raw_message = build_rfc2822(to, subject, body);
        let encoded = base64url_encode(raw_message.as_bytes());

        let payload = serde_json::json!({
            "message": { "raw": encoded }
        });

        debug!(url = %url, to = %to, subject = %subject, "creating Gmail draft");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Gmail create_draft request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Gmail draft response")
    }

    /// Search emails using a Gmail search query.
    ///
    /// This is a convenience wrapper around `list_messages` that returns full message details.
    pub async fn search_emails(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<Vec<EmailMessage>> {
        let list = self.list_messages(Some(query), max_results).await?;

        let mut messages = Vec::with_capacity(list.messages.len());
        for entry in &list.messages {
            match self.get_message(&entry.id).await {
                Ok(msg) => messages.push(msg),
                Err(e) => {
                    debug!(id = %entry.id, error = %e, "skipping message that failed to fetch");
                }
            }
        }

        Ok(messages)
    }

    /// Delete (trash) a message by ID.
    pub async fn delete_message(&self, message_id: &str) -> Result<()> {
        let url = format!("{}/messages/{}/trash", self.base_url, message_id);
        debug!(url = %url, "trashing Gmail message");

        let resp = self
            .client
            .post(&url)
            .send()
            .await
            .context("Gmail delete_message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// Modify labels on a message (add and/or remove labels).
    pub async fn modify_labels(
        &self,
        message_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<()> {
        let url = format!("{}/messages/{}/modify", self.base_url, message_id);

        let payload = serde_json::json!({
            "addLabelIds": add_labels,
            "removeLabelIds": remove_labels,
        });

        debug!(url = %url, "modifying Gmail message labels");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Gmail modify_labels request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// List all labels for the authenticated user.
    pub async fn list_labels(&self) -> Result<Vec<GmailLabel>> {
        let url = format!("{}/labels", self.base_url);
        debug!(url = %url, "listing Gmail labels");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Gmail list_labels request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gmail API error ({}): {}", status, body);
        }

        let wrapper: LabelsListResponse = resp
            .json()
            .await
            .context("failed to parse Gmail labels response")?;

        Ok(wrapper.labels)
    }
}

// ── Helpers ───────────────────────────────────────────────────────

/// Extract a header value by name from a list of raw headers.
fn get_header(headers: &[RawHeader], name: &str) -> String {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.clone())
        .unwrap_or_default()
}

/// Parse a raw Gmail API message into our domain `EmailMessage`.
fn parse_raw_message(raw: RawGmailMessage) -> EmailMessage {
    let (from, to, subject, date, body) = if let Some(ref payload) = raw.payload {
        let from = get_header(&payload.headers, "From");
        let to = get_header(&payload.headers, "To");
        let subject = get_header(&payload.headers, "Subject");
        let date = get_header(&payload.headers, "Date");

        // Try to extract body: first from top-level payload body, then from parts
        let body = extract_body(payload);

        (from, to, subject, date, body)
    } else {
        (String::new(), String::new(), String::new(), String::new(), String::new())
    };

    EmailMessage {
        id: raw.id,
        thread_id: raw.thread_id,
        from,
        to,
        subject,
        body,
        date,
        labels: raw.label_ids,
        snippet: raw.snippet,
    }
}

/// Extract the body text from a Gmail payload, preferring text/plain from parts.
fn extract_body(payload: &RawPayload) -> String {
    // Check multipart parts first (prefer text/plain)
    for part in &payload.parts {
        if part.mime_type == "text/plain" {
            if let Some(ref body) = part.body {
                if let Some(ref data) = body.data {
                    return base64url_decode(data);
                }
            }
        }
    }

    // Fall back to top-level body
    if let Some(ref body) = payload.body {
        if let Some(ref data) = body.data {
            return base64url_decode(data);
        }
    }

    String::new()
}

/// Build a minimal RFC 2822 formatted email message.
fn build_rfc2822(to: &str, subject: &str, body: &str) -> String {
    format!(
        "To: {to}\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}"
    )
}

/// BASE64-URL encode without padding (for Gmail API raw messages).
fn base64url_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut buf = String::with_capacity(data.len() * 4 / 3 + 4);

    let mut i = 0;
    while i + 2 < data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        let _ = buf.write_char(TABLE[((n >> 18) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 12) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 6) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let remaining = data.len() - i;
    if remaining == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        let _ = buf.write_char(TABLE[((n >> 18) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 12) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 6) & 0x3F) as usize] as char);
    } else if remaining == 1 {
        let n = (data[i] as u32) << 16;
        let _ = buf.write_char(TABLE[((n >> 18) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 12) & 0x3F) as usize] as char);
    }

    // Convert standard base64 to base64url: '+' -> '-', '/' -> '_'
    buf.replace('+', "-").replace('/', "_")
}

/// Decode a base64url-encoded string (as returned by the Gmail API).
fn base64url_decode(input: &str) -> String {
    // Convert base64url back to standard base64
    let standard = input.replace('-', "+").replace('_', "/");

    // Add padding if needed
    let padded = match standard.len() % 4 {
        2 => format!("{standard}=="),
        3 => format!("{standard}="),
        _ => standard,
    };

    // Build reverse lookup table
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &b) in TABLE.iter().enumerate() {
        lookup[b as usize] = i as u8;
    }

    let bytes: Vec<u8> = padded.bytes().filter(|&b| b != b'=').collect();
    let mut output = Vec::with_capacity(bytes.len() * 3 / 4);

    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = lookup[bytes[i] as usize] as u32;
        let b = lookup[bytes[i + 1] as usize] as u32;
        let c = lookup[bytes[i + 2] as usize] as u32;
        let d = lookup[bytes[i + 3] as usize] as u32;
        let n = (a << 18) | (b << 12) | (c << 6) | d;
        output.push((n >> 16) as u8);
        output.push((n >> 8) as u8);
        output.push(n as u8);
        i += 4;
    }

    // Handle remaining bytes
    let remaining = bytes.len() - i;
    if remaining == 3 {
        let a = lookup[bytes[i] as usize] as u32;
        let b = lookup[bytes[i + 1] as usize] as u32;
        let c = lookup[bytes[i + 2] as usize] as u32;
        let n = (a << 18) | (b << 12) | (c << 6);
        output.push((n >> 16) as u8);
        output.push((n >> 8) as u8);
    } else if remaining == 2 {
        let a = lookup[bytes[i] as usize] as u32;
        let b = lookup[bytes[i + 1] as usize] as u32;
        let n = (a << 18) | (b << 12);
        output.push((n >> 16) as u8);
    }

    String::from_utf8(output).unwrap_or_default()
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

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_email_message_deserialization() {
        let json = r#"{
            "id": "msg123",
            "threadId": "thread456",
            "from": "alice@example.com",
            "to": "bob@example.com",
            "subject": "Hello",
            "body": "Hi Bob",
            "date": "Mon, 1 Jan 2026 00:00:00 +0000",
            "labels": ["INBOX", "UNREAD"],
            "snippet": "Hi Bob"
        }"#;
        let msg: EmailMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "msg123");
        assert_eq!(msg.thread_id, "thread456");
        assert_eq!(msg.from, "alice@example.com");
        assert_eq!(msg.to, "bob@example.com");
        assert_eq!(msg.subject, "Hello");
        assert_eq!(msg.body, "Hi Bob");
        assert_eq!(msg.labels, vec!["INBOX", "UNREAD"]);
        assert_eq!(msg.snippet, "Hi Bob");
    }

    #[test]
    fn test_email_message_default_fields() {
        let json = r#"{}"#;
        let msg: EmailMessage = serde_json::from_str(json).unwrap();
        assert!(msg.id.is_empty());
        assert!(msg.thread_id.is_empty());
        assert!(msg.from.is_empty());
        assert!(msg.labels.is_empty());
        assert!(msg.snippet.is_empty());
    }

    #[test]
    fn test_email_list_deserialization() {
        let json = r#"{
            "messages": [
                { "id": "msg1", "threadId": "t1" },
                { "id": "msg2", "threadId": "t2" }
            ],
            "nextPageToken": "page2",
            "resultSizeEstimate": 42
        }"#;
        let list: EmailList = serde_json::from_str(json).unwrap();
        assert_eq!(list.messages.len(), 2);
        assert_eq!(list.messages[0].id, "msg1");
        assert_eq!(list.messages[1].thread_id, "t2");
        assert_eq!(list.next_page_token.as_deref(), Some("page2"));
        assert_eq!(list.result_size_estimate, 42);
    }

    #[test]
    fn test_email_list_empty() {
        let json = r#"{}"#;
        let list: EmailList = serde_json::from_str(json).unwrap();
        assert!(list.messages.is_empty());
        assert!(list.next_page_token.is_none());
        assert_eq!(list.result_size_estimate, 0);
    }

    #[test]
    fn test_gmail_label_deserialization() {
        let json = r#"{
            "id": "INBOX",
            "name": "INBOX",
            "type": "system"
        }"#;
        let label: GmailLabel = serde_json::from_str(json).unwrap();
        assert_eq!(label.id, "INBOX");
        assert_eq!(label.name, "INBOX");
        assert_eq!(label.r#type, "system");
    }

    #[test]
    fn test_send_email_request_serialization() {
        let req = SendEmailRequest {
            to: "bob@example.com".into(),
            subject: "Test".into(),
            body: "Hello".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: SendEmailRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.to, "bob@example.com");
        assert_eq!(back.subject, "Test");
        assert_eq!(back.body, "Hello");
    }

    #[test]
    fn test_draft_request_serialization_roundtrip() {
        let draft = DraftRequest {
            to: "charlie@example.com".into(),
            subject: "Draft Subject".into(),
            body: "Draft body text".into(),
        };
        let json = serde_json::to_string(&draft).unwrap();
        let back: DraftRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.to, draft.to);
        assert_eq!(back.subject, draft.subject);
        assert_eq!(back.body, draft.body);
    }

    #[test]
    fn test_modify_labels_request_serialization() {
        let req = ModifyLabelsRequest {
            add_label_ids: vec!["STARRED".into()],
            remove_label_ids: vec!["UNREAD".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("STARRED"));
        assert!(json.contains("UNREAD"));
        let back: ModifyLabelsRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.add_label_ids, vec!["STARRED"]);
        assert_eq!(back.remove_label_ids, vec!["UNREAD"]);
    }

    #[test]
    fn test_client_default_base_url() {
        let client = GmailClient::new("tok");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url_strips_slash() {
        let client = GmailClient::with_base_url("tok", "https://gmail.test/v1/users/me/");
        assert_eq!(client.base_url(), "https://gmail.test/v1/users/me");
    }

    #[test]
    fn test_list_messages_url_construction() {
        let client = GmailClient::new("tok");
        let url = build_url(client.base_url(), "/messages?maxResults=10");
        assert!(url.starts_with(DEFAULT_BASE_URL));
        assert!(url.contains("maxResults=10"));
    }

    #[test]
    fn test_get_message_url_construction() {
        let client = GmailClient::new("tok");
        let url = build_url(client.base_url(), "/messages/msg123?format=full");
        assert!(url.contains("/messages/msg123"));
        assert!(url.contains("format=full"));
    }

    #[test]
    fn test_send_email_url_construction() {
        let client = GmailClient::new("tok");
        let url = build_url(client.base_url(), "/messages/send");
        assert!(url.contains("/messages/send"));
    }

    #[test]
    fn test_draft_url_construction() {
        let client = GmailClient::new("tok");
        let url = build_url(client.base_url(), "/drafts");
        assert!(url.ends_with("/drafts"));
    }

    #[test]
    fn test_delete_message_url_construction() {
        let client = GmailClient::new("tok");
        let url = build_url(client.base_url(), "/messages/msg123/trash");
        assert!(url.contains("/messages/msg123/trash"));
    }

    #[test]
    fn test_modify_labels_url_construction() {
        let client = GmailClient::new("tok");
        let url = build_url(client.base_url(), "/messages/msg123/modify");
        assert!(url.contains("/messages/msg123/modify"));
    }

    #[test]
    fn test_labels_url_construction() {
        let client = GmailClient::new("tok");
        let url = build_url(client.base_url(), "/labels");
        assert_eq!(
            url,
            "https://gmail.googleapis.com/gmail/v1/users/me/labels"
        );
    }

    #[test]
    fn test_email_message_serialization_roundtrip() {
        let msg = EmailMessage {
            id: "m1".into(),
            thread_id: "t1".into(),
            from: "alice@test.com".into(),
            to: "bob@test.com".into(),
            subject: "Roundtrip".into(),
            body: "Body text".into(),
            date: "2026-01-01".into(),
            labels: vec!["INBOX".into(), "IMPORTANT".into()],
            snippet: "Body text".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: EmailMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "m1");
        assert_eq!(back.thread_id, "t1");
        assert_eq!(back.from, "alice@test.com");
        assert_eq!(back.labels.len(), 2);
        assert_eq!(back.snippet, "Body text");
    }

    #[test]
    fn test_gmail_draft_deserialization() {
        let json = r#"{
            "id": "draft123",
            "message": { "id": "msg456", "threadId": "t789" }
        }"#;
        let draft: GmailDraft = serde_json::from_str(json).unwrap();
        assert_eq!(draft.id, "draft123");
        let msg = draft.message.unwrap();
        assert_eq!(msg.id, "msg456");
        assert_eq!(msg.thread_id, "t789");
    }

    #[test]
    fn test_labels_list_response_deserialization() {
        let json = r#"{
            "labels": [
                { "id": "INBOX", "name": "INBOX", "type": "system" },
                { "id": "Label_1", "name": "Work", "type": "user" }
            ]
        }"#;
        let resp: LabelsListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.labels.len(), 2);
        assert_eq!(resp.labels[0].id, "INBOX");
        assert_eq!(resp.labels[1].name, "Work");
    }

    #[test]
    fn test_labels_list_response_empty() {
        let json = r#"{}"#;
        let resp: LabelsListResponse = serde_json::from_str(json).unwrap();
        assert!(resp.labels.is_empty());
    }

    #[test]
    fn test_raw_message_parsing() {
        let raw = RawGmailMessage {
            id: "raw1".into(),
            thread_id: "t1".into(),
            label_ids: vec!["INBOX".into()],
            snippet: "Hello world".into(),
            payload: Some(RawPayload {
                headers: vec![
                    RawHeader { name: "From".into(), value: "sender@test.com".into() },
                    RawHeader { name: "To".into(), value: "receiver@test.com".into() },
                    RawHeader { name: "Subject".into(), value: "Test Subject".into() },
                    RawHeader { name: "Date".into(), value: "Mon, 1 Jan 2026 00:00:00 +0000".into() },
                ],
                body: Some(RawBody {
                    data: Some(base64url_encode(b"Hello world")),
                }),
                parts: vec![],
            }),
        };
        let msg = parse_raw_message(raw);
        assert_eq!(msg.id, "raw1");
        assert_eq!(msg.from, "sender@test.com");
        assert_eq!(msg.to, "receiver@test.com");
        assert_eq!(msg.subject, "Test Subject");
        assert_eq!(msg.body, "Hello world");
        assert_eq!(msg.labels, vec!["INBOX"]);
        assert_eq!(msg.snippet, "Hello world");
    }

    #[test]
    fn test_raw_message_parsing_with_parts() {
        let raw = RawGmailMessage {
            id: "raw2".into(),
            thread_id: "t2".into(),
            label_ids: vec![],
            snippet: "".into(),
            payload: Some(RawPayload {
                headers: vec![
                    RawHeader { name: "Subject".into(), value: "Multipart".into() },
                ],
                body: None,
                parts: vec![
                    RawPart {
                        mime_type: "text/plain".into(),
                        body: Some(RawBody {
                            data: Some(base64url_encode(b"Plain text body")),
                        }),
                    },
                    RawPart {
                        mime_type: "text/html".into(),
                        body: Some(RawBody {
                            data: Some(base64url_encode(b"<p>HTML body</p>")),
                        }),
                    },
                ],
            }),
        };
        let msg = parse_raw_message(raw);
        assert_eq!(msg.subject, "Multipart");
        assert_eq!(msg.body, "Plain text body");
    }

    #[test]
    fn test_raw_message_parsing_no_payload() {
        let raw = RawGmailMessage {
            id: "raw3".into(),
            thread_id: "t3".into(),
            label_ids: vec![],
            snippet: "snippet only".into(),
            payload: None,
        };
        let msg = parse_raw_message(raw);
        assert_eq!(msg.id, "raw3");
        assert!(msg.from.is_empty());
        assert!(msg.body.is_empty());
        assert_eq!(msg.snippet, "snippet only");
    }

    #[test]
    fn test_build_rfc2822() {
        let msg = build_rfc2822("bob@test.com", "Hello", "World");
        assert!(msg.starts_with("To: bob@test.com\r\n"));
        assert!(msg.contains("Subject: Hello\r\n"));
        assert!(msg.contains("Content-Type: text/plain; charset=utf-8\r\n"));
        assert!(msg.ends_with("\r\n\r\nWorld"));
    }

    #[test]
    fn test_base64url_encode_decode_roundtrip() {
        let original = "Hello, Gmail API! This is a test message.";
        let encoded = base64url_encode(original.as_bytes());
        // Ensure it is base64url (no +, /, or =)
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        let decoded = base64url_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_urlencod_preserves_unreserved() {
        assert_eq!(urlencod("abc-_.~XYZ019"), "abc-_.~XYZ019");
    }

    #[test]
    fn test_urlencod_encodes_special() {
        assert_eq!(urlencod("a b"), "a%20b");
        assert_eq!(urlencod("from:me"), "from%3Ame");
    }

    #[test]
    fn test_get_header_case_insensitive() {
        let headers = vec![
            RawHeader { name: "From".into(), value: "alice@test.com".into() },
            RawHeader { name: "subject".into(), value: "Lower case".into() },
        ];
        assert_eq!(get_header(&headers, "from"), "alice@test.com");
        assert_eq!(get_header(&headers, "Subject"), "Lower case");
        assert_eq!(get_header(&headers, "Missing"), "");
    }
}
