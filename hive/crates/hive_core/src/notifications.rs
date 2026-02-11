use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppNotification {
    pub id: String,
    pub notification_type: NotificationType,
    pub title: Option<String>,
    pub message: String,
    pub read: bool,
    pub timestamp: DateTime<Utc>,
}

impl AppNotification {
    pub fn new(notification_type: NotificationType, message: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            notification_type,
            title: None,
            message: message.into(),
            read: false,
            timestamp: Utc::now(),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// In-memory notification store.
pub struct NotificationStore {
    notifications: Vec<AppNotification>,
    max_notifications: usize,
}

impl NotificationStore {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            max_notifications: 100,
        }
    }

    pub fn push(&mut self, notification: AppNotification) {
        self.notifications.insert(0, notification);
        if self.notifications.len() > self.max_notifications {
            self.notifications.truncate(self.max_notifications);
        }
    }

    pub fn mark_read(&mut self, id: &str) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.read = true;
        }
    }

    pub fn mark_all_read(&mut self) {
        for n in &mut self.notifications {
            n.read = true;
        }
    }

    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    pub fn all(&self) -> &[AppNotification] {
        &self.notifications
    }

    pub fn clear(&mut self) {
        self.notifications.clear();
    }
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_notification() {
        let n = AppNotification::new(NotificationType::Info, "Test message");
        assert_eq!(n.message, "Test message");
        assert_eq!(n.notification_type, NotificationType::Info);
        assert!(!n.read);
        assert!(n.title.is_none());
    }

    #[test]
    fn notification_with_title() {
        let n = AppNotification::new(NotificationType::Success, "Done")
            .with_title("Operation Complete");
        assert_eq!(n.title.as_deref(), Some("Operation Complete"));
    }

    #[test]
    fn store_push_and_count() {
        let mut store = NotificationStore::new();
        assert_eq!(store.unread_count(), 0);
        assert!(store.all().is_empty());

        store.push(AppNotification::new(NotificationType::Info, "msg1"));
        store.push(AppNotification::new(NotificationType::Warning, "msg2"));
        assert_eq!(store.all().len(), 2);
        assert_eq!(store.unread_count(), 2);
    }

    #[test]
    fn store_newest_first() {
        let mut store = NotificationStore::new();
        store.push(AppNotification::new(NotificationType::Info, "first"));
        store.push(AppNotification::new(NotificationType::Info, "second"));
        assert_eq!(store.all()[0].message, "second");
        assert_eq!(store.all()[1].message, "first");
    }

    #[test]
    fn store_mark_read() {
        let mut store = NotificationStore::new();
        store.push(AppNotification::new(NotificationType::Info, "msg"));
        let id = store.all()[0].id.clone();

        assert_eq!(store.unread_count(), 1);
        store.mark_read(&id);
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn store_mark_all_read() {
        let mut store = NotificationStore::new();
        store.push(AppNotification::new(NotificationType::Info, "a"));
        store.push(AppNotification::new(NotificationType::Info, "b"));
        store.push(AppNotification::new(NotificationType::Info, "c"));
        assert_eq!(store.unread_count(), 3);

        store.mark_all_read();
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn store_truncates_at_max() {
        let mut store = NotificationStore::new();
        store.max_notifications = 3;

        for i in 0..5 {
            store.push(AppNotification::new(
                NotificationType::Info,
                format!("msg{i}"),
            ));
        }
        assert_eq!(store.all().len(), 3);
        // Most recent should be first
        assert_eq!(store.all()[0].message, "msg4");
    }

    #[test]
    fn store_clear() {
        let mut store = NotificationStore::new();
        store.push(AppNotification::new(NotificationType::Error, "err"));
        store.clear();
        assert!(store.all().is_empty());
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn store_mark_read_nonexistent() {
        let mut store = NotificationStore::new();
        store.push(AppNotification::new(NotificationType::Info, "msg"));
        // Should not panic
        store.mark_read("nonexistent-id");
        assert_eq!(store.unread_count(), 1);
    }

    #[test]
    fn notification_types() {
        let types = [
            NotificationType::Info,
            NotificationType::Success,
            NotificationType::Warning,
            NotificationType::Error,
        ];
        for t in types {
            let n = AppNotification::new(t, "test");
            assert_eq!(n.notification_type, t);
        }
    }

    #[test]
    fn notification_serde_roundtrip() {
        let n = AppNotification::new(NotificationType::Warning, "test")
            .with_title("Title");
        let json = serde_json::to_string(&n).unwrap();
        let parsed: AppNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.message, "test");
        assert_eq!(parsed.title.as_deref(), Some("Title"));
        assert_eq!(parsed.notification_type, NotificationType::Warning);
    }
}
