use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// Severity level of an application notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationType {
    /// Informational message.
    Info,
    /// Successful operation.
    Success,
    /// Non-critical warning.
    Warning,
    /// Error requiring attention.
    Error,
}

/// A user-facing notification with optional title and read/unread state.
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
    /// Creates a new unread notification with the given type and message.
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

    /// Attaches a title to this notification (builder pattern).
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// In-memory notification store.
#[derive(Serialize, Deserialize)]
pub struct NotificationStore {
    notifications: Vec<AppNotification>,
    max_notifications: usize,
}

impl NotificationStore {
    /// Creates a new, empty notification store with a default capacity of 100.
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            max_notifications: 100,
        }
    }

    /// Inserts a notification at the front and truncates if over capacity.
    pub fn push(&mut self, notification: AppNotification) {
        self.notifications.insert(0, notification);
        if self.notifications.len() > self.max_notifications {
            self.notifications.truncate(self.max_notifications);
        }
    }

    /// Marks a single notification as read by its ID. No-op if not found.
    pub fn mark_read(&mut self, id: &str) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.read = true;
        }
    }

    /// Marks every notification in the store as read.
    pub fn mark_all_read(&mut self) {
        for n in &mut self.notifications {
            n.read = true;
        }
    }

    /// Returns the number of unread notifications.
    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    /// Returns a slice of all notifications, newest first.
    pub fn all(&self) -> &[AppNotification] {
        &self.notifications
    }

    /// Removes all notifications from the store.
    pub fn clear(&mut self) {
        self.notifications.clear();
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Persist the notification store to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a notification store from a JSON file. Returns an empty store if
    /// the file does not exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
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
        let n = AppNotification::new(NotificationType::Warning, "test").with_title("Title");
        let json = serde_json::to_string(&n).unwrap();
        let parsed: AppNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.message, "test");
        assert_eq!(parsed.title.as_deref(), Some("Title"));
        assert_eq!(parsed.notification_type, NotificationType::Warning);
    }

    #[test]
    fn save_and_load_file_round_trip() {
        let dir = std::env::temp_dir().join("hive-notifications-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("notifications.json");

        let mut store = NotificationStore::new();
        store.push(AppNotification::new(NotificationType::Info, "msg1"));
        store.push(
            AppNotification::new(NotificationType::Error, "msg2").with_title("Alert"),
        );
        store.mark_read(&store.all()[0].id.clone());

        store.save_to_file(&path).unwrap();
        let loaded = NotificationStore::load_from_file(&path).unwrap();

        assert_eq!(loaded.all().len(), 2);
        assert_eq!(loaded.unread_count(), 1);
        assert_eq!(loaded.all()[0].message, "msg2");
        assert_eq!(loaded.all()[0].title.as_deref(), Some("Alert"));
        assert_eq!(loaded.all()[0].notification_type, NotificationType::Error);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_empty_store() {
        let path = std::env::temp_dir().join("nonexistent-hive-notifications.json");
        let store = NotificationStore::load_from_file(&path).unwrap();
        assert!(store.all().is_empty());
    }
}
