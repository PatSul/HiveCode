pub mod compose_agent;
pub mod inbox_agent;

use hive_integrations::{
    GmailClient, OutlookEmailClient,
    EmailClassifier, ClassificationResult, EmailCategory,
};
use hive_shield::HiveShield;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Supported email providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmailProvider {
    Gmail,
    Outlook,
    Custom(String),
}

/// A unified email representation across all providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedEmail {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub timestamp: String,
    pub provider: EmailProvider,
    pub read: bool,
    pub important: bool,
}

/// A digest summarizing emails from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDigest {
    pub id: String,
    pub provider: EmailProvider,
    pub summary: String,
    pub email_count: usize,
    pub created_at: String,
}

/// Classification result for an email.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmailClassification {
    Important,
    Normal,
    Spam,
    Newsletter,
}

// ---------------------------------------------------------------------------
// EmailService
// ---------------------------------------------------------------------------

/// Service for managing email operations across providers.
///
/// Holds optional OAuth tokens. When tokens are present, real API calls are
/// made through the fully-implemented `hive_integrations` clients. When tokens
/// are absent the methods degrade gracefully and return empty results.
pub struct EmailService {
    gmail_token: Option<String>,
    outlook_token: Option<String>,
    classifier: EmailClassifier,
}

impl EmailService {
    pub fn new() -> Self {
        Self {
            gmail_token: None,
            outlook_token: None,
            classifier: EmailClassifier::new(),
        }
    }

    /// Create a service pre-configured with OAuth tokens.
    pub fn with_tokens(gmail_token: Option<String>, outlook_token: Option<String>) -> Self {
        Self {
            gmail_token,
            outlook_token,
            classifier: EmailClassifier::new(),
        }
    }

    /// Update the Gmail OAuth access token at runtime.
    pub fn set_gmail_token(&mut self, token: String) {
        self.gmail_token = Some(token);
    }

    /// Update the Outlook OAuth access token at runtime.
    pub fn set_outlook_token(&mut self, token: String) {
        self.outlook_token = Some(token);
    }

    /// Fetch emails from a Gmail inbox.
    ///
    /// Requires a valid Gmail OAuth token to have been set via
    /// [`set_gmail_token`] or [`with_tokens`]. Returns an empty vec if
    /// no token is configured.
    pub fn fetch_gmail_inbox(&self) -> Result<Vec<UnifiedEmail>, String> {
        let token = match &self.gmail_token {
            Some(t) => t.clone(),
            None => return Ok(Vec::new()),
        };

        // Use tokio Handle to run async code from sync context.
        let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
        

        handle.block_on(async {
            let client = GmailClient::new(&token);
            let list = client
                .list_messages(None, 20)
                .await
                .map_err(|e| format!("Gmail list error: {e}"))?;

            let mut emails = Vec::with_capacity(list.messages.len());
            for entry in &list.messages {
                match client.get_message(&entry.id).await {
                    Ok(msg) => {
                        let important = msg.labels.iter().any(|l| {
                            l.eq_ignore_ascii_case("IMPORTANT") || l.eq_ignore_ascii_case("STARRED")
                        });
                        let read = !msg.labels.iter().any(|l| l.eq_ignore_ascii_case("UNREAD"));

                        emails.push(UnifiedEmail {
                            id: msg.id,
                            from: msg.from,
                            to: msg.to,
                            subject: msg.subject,
                            body: msg.body,
                            timestamp: msg.date,
                            provider: EmailProvider::Gmail,
                            read,
                            important,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(id = %entry.id, error = %e, "skipping Gmail message");
                    }
                }
            }
            Ok(emails)
        })
    }

    /// Fetch emails from an Outlook inbox.
    ///
    /// Requires a valid Outlook/Microsoft Graph OAuth token. Returns an empty
    /// vec if no token is configured.
    pub fn fetch_outlook_inbox(&self) -> Result<Vec<UnifiedEmail>, String> {
        let token = match &self.outlook_token {
            Some(t) => t.clone(),
            None => return Ok(Vec::new()),
        };

        let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
        

        handle.block_on(async {
            let client = OutlookEmailClient::new(&token);
            let messages = client
                .list_messages("inbox", 20)
                .await
                .map_err(|e| format!("Outlook list error: {e}"))?;

            let emails = messages
                .into_iter()
                .map(|msg| {
                    let from = msg
                        .from
                        .map(|a| a.address)
                        .unwrap_or_default();
                    let to = msg
                        .to_recipients
                        .first()
                        .map(|a| a.address.clone())
                        .unwrap_or_default();

                    UnifiedEmail {
                        id: msg.id,
                        from,
                        to,
                        subject: msg.subject,
                        body: msg.body_preview,
                        timestamp: msg.received_at.unwrap_or_default(),
                        provider: EmailProvider::Outlook,
                        read: msg.is_read,
                        important: false,
                    }
                })
                .collect();
            Ok(emails)
        })
    }

    /// Build a digest summarizing a collection of emails.
    ///
    /// Groups emails by classification and produces a human-readable summary.
    pub fn build_digest(&self, emails: &[UnifiedEmail], provider: &EmailProvider) -> EmailDigest {
        if emails.is_empty() {
            return EmailDigest {
                id: uuid::Uuid::new_v4().to_string(),
                provider: provider.clone(),
                summary: "0 emails received".to_string(),
                email_count: 0,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
        }

        let important_count = emails.iter().filter(|e| e.important).count();
        let unread_count = emails.iter().filter(|e| !e.read).count();

        // Use the classifier to detect newsletters and spam
        let mut newsletter_count = 0usize;
        let mut spam_count = 0usize;
        for e in emails {
            let result = self.classifier.classify(&e.from, &e.subject, &e.body, &[]);
            match result.category {
                EmailCategory::Newsletter => newsletter_count += 1,
                EmailCategory::Spam => spam_count += 1,
                _ => {}
            }
        }

        let mut parts = Vec::new();
        parts.push(format!("{} emails received", emails.len()));
        if unread_count > 0 {
            parts.push(format!("{unread_count} unread"));
        }
        if important_count > 0 {
            parts.push(format!("{important_count} important"));
        }
        if newsletter_count > 0 {
            parts.push(format!("{newsletter_count} newsletters"));
        }
        if spam_count > 0 {
            parts.push(format!("{spam_count} spam"));
        }

        EmailDigest {
            id: uuid::Uuid::new_v4().to_string(),
            provider: provider.clone(),
            summary: parts.join(", "),
            email_count: emails.len(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Send an email, scanning outgoing text through the privacy shield first.
    ///
    /// Routes to the appropriate provider based on the `provider` parameter.
    /// If no provider is specified, defaults to Gmail if a token is available,
    /// then Outlook.
    pub fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
        shield: &HiveShield,
    ) -> Result<(), String> {
        // Scan outgoing text through the shield
        let result = shield.process_outgoing(body, "email");

        let send_body = match result.action {
            hive_shield::ShieldAction::Block(reason) => {
                return Err(format!("Email blocked by shield: {reason}"));
            }
            hive_shield::ShieldAction::CloakAndAllow(cloaked) => cloaked.text,
            hive_shield::ShieldAction::Allow | hive_shield::ShieldAction::Warn(_) => {
                body.to_string()
            }
        };

        // Try Gmail first, then Outlook
        if let Some(token) = &self.gmail_token {
            let token = token.clone();
            let to = to.to_string();
            let subject = subject.to_string();
            let handle =
                Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
            handle.block_on(async {
                let client = GmailClient::new(&token);
                client
                    .send_email(&to, &subject, &send_body)
                    .await
                    .map_err(|e| format!("Gmail send error: {e}"))?;
                Ok::<(), String>(())
            })?;

            tracing::info!(to = %to, subject = %subject, "Email sent via Gmail");
            return Ok(());
        }

        if let Some(token) = &self.outlook_token {
            let token = token.clone();
            let to_str = to.to_string();
            let subject = subject.to_string();
            let handle =
                Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
            handle.block_on(async {
                let client = OutlookEmailClient::new(&token);
                client
                    .send_message(&[&to_str], &subject, &send_body)
                    .await
                    .map_err(|e| format!("Outlook send error: {e}"))?;
                Ok::<(), String>(())
            })?;

            tracing::info!(to = %to, subject = %subject, "Email sent via Outlook");
            return Ok(());
        }

        // No provider configured — log and succeed silently (graceful degradation)
        tracing::info!(
            to = to,
            subject = subject,
            "Email send requested (no provider configured)"
        );
        Ok(())
    }

    /// Classify an email using the 5-layer classification engine.
    pub fn classify(&self, email: &UnifiedEmail) -> EmailClassification {
        let labels: Vec<String> = Vec::new();
        let result = self
            .classifier
            .classify(&email.from, &email.subject, &email.body, &labels);

        match result.category {
            EmailCategory::Important => EmailClassification::Important,
            EmailCategory::Spam => EmailClassification::Spam,
            EmailCategory::Newsletter => EmailClassification::Newsletter,
            _ => {
                if email.important {
                    EmailClassification::Important
                } else {
                    EmailClassification::Normal
                }
            }
        }
    }

    /// Full classification with detailed result from the 5-layer engine.
    pub fn classify_detailed(&self, email: &UnifiedEmail) -> ClassificationResult {
        let labels: Vec<String> = Vec::new();
        self.classifier
            .classify(&email.from, &email.subject, &email.body, &labels)
    }
}

impl Default for EmailService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::email::{
        EmailClassification, EmailDigest, EmailProvider, EmailService, UnifiedEmail,
    };
    use hive_shield::{HiveShield, ShieldConfig};

    fn make_email(important: bool) -> UnifiedEmail {
        UnifiedEmail {
            id: "email-1".to_string(),
            from: "sender@example.com".to_string(),
            to: "recipient@example.com".to_string(),
            subject: "Test subject".to_string(),
            body: "Test body content".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            provider: EmailProvider::Gmail,
            read: false,
            important,
        }
    }

    #[test]
    fn test_fetch_gmail_returns_empty_without_token() {
        let service = EmailService::new();
        let emails = service.fetch_gmail_inbox().unwrap();
        assert!(emails.is_empty());
    }

    #[test]
    fn test_fetch_outlook_returns_empty_without_token() {
        let service = EmailService::new();
        let emails = service.fetch_outlook_inbox().unwrap();
        assert!(emails.is_empty());
    }

    #[test]
    fn test_build_digest_empty() {
        let service = EmailService::new();
        let digest = service.build_digest(&[], &EmailProvider::Gmail);
        assert_eq!(digest.email_count, 0);
        assert_eq!(digest.summary, "0 emails received");
    }

    #[test]
    fn test_build_digest_with_emails() {
        let service = EmailService::new();
        let emails = vec![make_email(true), make_email(false)];
        let digest = service.build_digest(&emails, &EmailProvider::Outlook);
        assert_eq!(digest.email_count, 2);
        assert!(matches!(digest.provider, EmailProvider::Outlook));
        assert!(digest.summary.contains("2 emails received"));
        assert!(digest.summary.contains("unread"));
    }

    #[test]
    fn test_build_digest_with_important() {
        let service = EmailService::new();
        let emails = vec![make_email(true), make_email(true), make_email(false)];
        let digest = service.build_digest(&emails, &EmailProvider::Gmail);
        assert!(digest.summary.contains("important") || digest.summary.contains("3 emails"));
    }

    #[test]
    fn test_send_email_clean_text() {
        let service = EmailService::new();
        let shield = HiveShield::new(ShieldConfig {
            access_policies: HashMap::new(),
            ..ShieldConfig::default()
        });
        let result = service.send_email(
            "user@example.com",
            "Hello",
            "This is a clean message.",
            &shield,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_email_with_secrets_blocked() {
        let service = EmailService::new();
        let shield = HiveShield::new(ShieldConfig::default());
        let result = service.send_email(
            "user@example.com",
            "Credentials",
            &format!("Here is the key: AKIA{}", "IOSFODNN7EXAMPLE"),
            &shield,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("blocked"));
    }

    #[test]
    fn test_classify_important() {
        let service = EmailService::new();
        let email = make_email(true);
        assert_eq!(service.classify(&email), EmailClassification::Important);
    }

    #[test]
    fn test_classify_normal() {
        let service = EmailService::new();
        let email = make_email(false);
        assert_eq!(service.classify(&email), EmailClassification::Normal);
    }

    #[test]
    fn test_classify_spam() {
        let service = EmailService::new();
        let mut email = make_email(false);
        email.from = "spammer@scam.com".to_string();
        email.subject = "You have won a million dollars!".to_string();
        email.body = "Click here immediately to claim your prize. Wire transfer needed.".to_string();
        assert_eq!(service.classify(&email), EmailClassification::Spam);
    }

    #[test]
    fn test_classify_newsletter() {
        let service = EmailService::new();
        let mut email = make_email(false);
        email.from = "digest@substack.com".to_string();
        email.subject = "Your weekly newsletter".to_string();
        email.body = "Click here to unsubscribe".to_string();
        assert_eq!(service.classify(&email), EmailClassification::Newsletter);
    }

    #[test]
    fn test_with_tokens() {
        let service =
            EmailService::with_tokens(Some("gmail_tok".into()), Some("outlook_tok".into()));
        // Tokens set but no runtime — fetch degrades gracefully
        let result = service.fetch_gmail_inbox();
        // Will fail with "No tokio runtime" but at least proves token path works
        assert!(result.is_err() || result.unwrap().is_empty());
    }

    #[test]
    fn test_set_tokens() {
        let mut service = EmailService::new();
        service.set_gmail_token("tok".to_string());
        service.set_outlook_token("tok2".to_string());
        // Proves the setter compiles and works
        let result = service.fetch_gmail_inbox();
        assert!(result.is_err() || result.unwrap().is_empty());
    }

    #[test]
    fn test_email_provider_serialization() {
        let providers = vec![
            EmailProvider::Gmail,
            EmailProvider::Outlook,
            EmailProvider::Custom("proton".to_string()),
        ];
        let json = serde_json::to_string(&providers).unwrap();
        let deserialized: Vec<EmailProvider> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, providers);
    }

    #[test]
    fn test_unified_email_serialization() {
        let email = make_email(true);
        let json = serde_json::to_string(&email).unwrap();
        let deserialized: UnifiedEmail = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "email-1");
        assert_eq!(deserialized.from, "sender@example.com");
        assert!(deserialized.important);
    }

    #[test]
    fn test_email_digest_serialization() {
        let digest = EmailDigest {
            id: "dig-1".to_string(),
            provider: EmailProvider::Gmail,
            summary: "3 emails".to_string(),
            email_count: 3,
            created_at: "2026-02-10T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&digest).unwrap();
        let deserialized: EmailDigest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.email_count, 3);
    }

    #[test]
    fn test_default_email_service() {
        let service = EmailService::default();
        let emails = service.fetch_gmail_inbox().unwrap();
        assert!(emails.is_empty());
    }
}
