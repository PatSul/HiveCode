use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://graph.microsoft.com/v1.0";

/// An email message returned from the Microsoft Graph API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailMessage {
    pub id: String,
    pub subject: String,
    pub from: Option<EmailAddress>,
    pub to_recipients: Vec<EmailAddress>,
    pub body_preview: String,
    pub received_at: Option<String>,
    pub is_read: bool,
}

/// An email address with an optional display name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddress {
    pub name: Option<String>,
    pub address: String,
}

/// Microsoft Graph API client for Outlook email operations.
///
/// Wraps the `/me/mailFolders/{folder}/messages` and `/me/sendMail`
/// endpoints of the Microsoft Graph v1.0 REST API.
pub struct OutlookEmailClient {
    client: Client,
    access_token: String,
    base_url: String,
}

impl OutlookEmailClient {
    /// Create a new client using the default Microsoft Graph base URL.
    pub fn new(access_token: &str) -> Self {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom base URL (useful for tests).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        Self {
            client: Client::new(),
            access_token: access_token.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// List messages in the given mail folder (e.g. `"inbox"`).
    ///
    /// Returns up to `top` messages ordered by received date descending.
    pub async fn list_messages(&self, folder: &str, top: u32) -> Result<Vec<EmailMessage>> {
        let url = format!(
            "{}/me/mailFolders/{}/messages?$top={}&$orderby=receivedDateTime desc",
            self.base_url, folder, top
        );
        debug!(url = %url, "listing Outlook messages");

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Outlook list messages request failed")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to parse Outlook response")?;

        if !status.is_success() {
            anyhow::bail!("Microsoft Graph error ({}): {}", status, body);
        }

        let messages: Vec<EmailMessage> =
            serde_json::from_value(body.get("value").cloned().unwrap_or_default())
                .context("failed to deserialize email messages")?;

        Ok(messages)
    }

    /// Get a single message by ID.
    pub async fn get_message(&self, message_id: &str) -> Result<EmailMessage> {
        let url = format!("{}/me/messages/{}", self.base_url, message_id);
        debug!(url = %url, "getting Outlook message");

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Outlook get message request failed")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to parse Outlook response")?;

        if !status.is_success() {
            anyhow::bail!("Microsoft Graph error ({}): {}", status, body);
        }

        let msg: EmailMessage =
            serde_json::from_value(body).context("failed to deserialize email message")?;
        Ok(msg)
    }

    /// Send an email to the given recipients.
    pub async fn send_message(&self, to: &[&str], subject: &str, body: &str) -> Result<()> {
        let url = format!("{}/me/sendMail", self.base_url);

        let recipients: Vec<serde_json::Value> = to
            .iter()
            .map(|addr| {
                serde_json::json!({
                    "emailAddress": { "address": addr }
                })
            })
            .collect();

        let payload = serde_json::json!({
            "message": {
                "subject": subject,
                "body": {
                    "contentType": "Text",
                    "content": body
                },
                "toRecipients": recipients
            }
        });

        debug!(url = %url, subject = %subject, "sending Outlook email");

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&payload)
            .send()
            .await
            .context("Outlook send message request failed")?;

        let status = response.status();
        if !status.is_success() {
            let err_body: serde_json::Value = response
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "unknown"}));
            anyhow::bail!("Microsoft Graph send error ({}): {}", status, err_body);
        }

        Ok(())
    }

    /// Delete a message by ID (moves it to Deleted Items).
    pub async fn delete_message(&self, message_id: &str) -> Result<()> {
        let url = format!("{}/me/messages/{}", self.base_url, message_id);
        debug!(url = %url, "deleting Outlook message");

        let response = self
            .client
            .delete(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Outlook delete message request failed")?;

        let status = response.status();
        if !status.is_success() {
            let err_body: serde_json::Value = response
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "unknown"}));
            anyhow::bail!("Microsoft Graph delete error ({}): {}", status, err_body);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_client_default_base_url() {
        let client = OutlookEmailClient::new("test_token");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url_strips_trailing_slash() {
        let client = OutlookEmailClient::with_base_url("tok", "https://graph.test.com/v1.0/");
        assert_eq!(client.base_url(), "https://graph.test.com/v1.0");
    }

    #[test]
    fn test_email_message_serde_roundtrip() {
        let msg = EmailMessage {
            id: "msg-1".into(),
            subject: "Hello".into(),
            from: Some(EmailAddress {
                name: Some("Alice".into()),
                address: "alice@example.com".into(),
            }),
            to_recipients: vec![EmailAddress {
                name: None,
                address: "bob@example.com".into(),
            }],
            body_preview: "Hi Bob".into(),
            received_at: Some("2026-01-15T10:00:00Z".into()),
            is_read: false,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: EmailMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "msg-1");
        assert_eq!(deserialized.subject, "Hello");
        assert_eq!(deserialized.from.unwrap().address, "alice@example.com");
        assert_eq!(deserialized.to_recipients.len(), 1);
        assert!(!deserialized.is_read);
    }

    #[test]
    fn test_email_address_serde() {
        let addr = EmailAddress {
            name: Some("Test User".into()),
            address: "test@example.com".into(),
        };
        let json = serde_json::to_string(&addr).unwrap();
        assert!(json.contains("test@example.com"));
        assert!(json.contains("Test User"));

        let parsed: EmailAddress = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.address, "test@example.com");
    }

    #[test]
    fn test_list_messages_url_construction() {
        let client = OutlookEmailClient::new("tok");
        let url = build_url(client.base_url(), "/me/mailFolders/inbox/messages");
        assert_eq!(
            url,
            "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages"
        );
    }

    #[test]
    fn test_send_mail_url_construction() {
        let client = OutlookEmailClient::new("tok");
        let url = build_url(client.base_url(), "/me/sendMail");
        assert_eq!(url, "https://graph.microsoft.com/v1.0/me/sendMail");
    }

    #[test]
    fn test_send_mail_payload_structure() {
        let to = ["alice@example.com", "bob@example.com"];
        let recipients: Vec<serde_json::Value> = to
            .iter()
            .map(|addr| {
                serde_json::json!({
                    "emailAddress": { "address": addr }
                })
            })
            .collect();

        let payload = serde_json::json!({
            "message": {
                "subject": "Test",
                "body": { "contentType": "Text", "content": "Hello" },
                "toRecipients": recipients
            }
        });

        assert_eq!(payload["message"]["subject"], "Test");
        let recips = payload["message"]["toRecipients"].as_array().unwrap();
        assert_eq!(recips.len(), 2);
        assert_eq!(recips[0]["emailAddress"]["address"], "alice@example.com");
    }
}
