use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::email::{EmailProvider, UnifiedEmail};
use crate::storage::AssistantStorage;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Notification about an important email that needs attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailNotification {
    pub email_id: String,
    pub from: String,
    pub subject: String,
    pub provider: EmailProvider,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// InboxAgent
// ---------------------------------------------------------------------------

/// Agent that periodically polls for new emails and surfaces important ones.
pub struct InboxAgent {
    pub poll_interval: Duration,
    storage: Arc<AssistantStorage>,
}

impl InboxAgent {
    pub fn new(storage: Arc<AssistantStorage>, poll_interval: Duration) -> Self {
        Self {
            poll_interval,
            storage,
        }
    }

    /// Poll for new emails and return notifications for important ones.
    ///
    /// This filters the provided emails to find only important ones and
    /// updates the poll state in storage.
    ///
    /// TODO: implement with actual inbox polling via provider APIs
    pub fn poll(&self, emails: &[UnifiedEmail], provider: &str) -> Result<Vec<EmailNotification>, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let last_message_id = emails.last().map(|e| e.id.as_str()).unwrap_or("");

        self.storage
            .upsert_poll_state(provider, &now, last_message_id)?;

        let notifications: Vec<EmailNotification> = emails
            .iter()
            .filter(|e| e.important && !e.read)
            .map(|e| EmailNotification {
                email_id: e.id.clone(),
                from: e.from.clone(),
                subject: e.subject.clone(),
                provider: e.provider.clone(),
                reason: "Marked as important".to_string(),
            })
            .collect();

        Ok(notifications)
    }

    /// Get the last poll time for a provider.
    pub fn last_poll_time(&self, provider: &str) -> Result<Option<String>, String> {
        let state = self.storage.get_poll_state(provider)?;
        Ok(state.map(|(poll_at, _)| poll_at))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::email::inbox_agent::InboxAgent;
    use crate::email::{EmailProvider, UnifiedEmail};
    use crate::storage::AssistantStorage;

    fn make_agent() -> InboxAgent {
        let storage = Arc::new(AssistantStorage::in_memory().unwrap());
        InboxAgent::new(storage, Duration::from_secs(60))
    }

    fn make_email(id: &str, important: bool, read: bool) -> UnifiedEmail {
        UnifiedEmail {
            id: id.to_string(),
            from: "sender@example.com".to_string(),
            to: "me@example.com".to_string(),
            subject: format!("Subject for {id}"),
            body: "Body text".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            provider: EmailProvider::Gmail,
            read,
            important,
        }
    }

    #[test]
    fn test_poll_empty_inbox() {
        let agent = make_agent();
        let notifications = agent.poll(&[], "gmail").unwrap();
        assert!(notifications.is_empty());
    }

    #[test]
    fn test_poll_returns_important_unread() {
        let agent = make_agent();
        let emails = vec![
            make_email("e1", true, false),  // important, unread -> notification
            make_email("e2", false, false), // not important -> skip
            make_email("e3", true, true),   // important, read -> skip
            make_email("e4", true, false),  // important, unread -> notification
        ];

        let notifications = agent.poll(&emails, "gmail").unwrap();
        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].email_id, "e1");
        assert_eq!(notifications[1].email_id, "e4");
    }

    #[test]
    fn test_poll_updates_state() {
        let agent = make_agent();
        let emails = vec![make_email("e-last", false, false)];

        agent.poll(&emails, "gmail").unwrap();
        let last_poll = agent.last_poll_time("gmail").unwrap();
        assert!(last_poll.is_some());
    }

    #[test]
    fn test_last_poll_time_none_before_poll() {
        let agent = make_agent();
        assert!(agent.last_poll_time("gmail").unwrap().is_none());
    }

    #[test]
    fn test_notification_fields() {
        let agent = make_agent();
        let emails = vec![make_email("e-check", true, false)];

        let notifications = agent.poll(&emails, "gmail").unwrap();
        assert_eq!(notifications.len(), 1);

        let n = &notifications[0];
        assert_eq!(n.email_id, "e-check");
        assert_eq!(n.from, "sender@example.com");
        assert_eq!(n.subject, "Subject for e-check");
        assert!(matches!(n.provider, EmailProvider::Gmail));
        assert_eq!(n.reason, "Marked as important");
    }

    #[test]
    fn test_poll_interval_accessible() {
        let agent = make_agent();
        assert_eq!(agent.poll_interval, Duration::from_secs(60));
    }
}
