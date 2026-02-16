pub mod os_notifications;

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use hive_core::scheduler::CronSchedule;

use crate::storage::AssistantStorage;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// When a reminder should trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReminderTrigger {
    /// Fire at a specific date/time.
    At(DateTime<Utc>),
    /// Fire on a recurring cron schedule (cron expression string).
    Recurring(String),
    /// Fire when a named event occurs.
    OnEvent(String),
}

/// Current status of a reminder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderStatus {
    Active,
    Snoozed,
    Completed,
    Dismissed,
}

/// A reminder managed by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    pub title: String,
    pub description: String,
    pub trigger: ReminderTrigger,
    /// Optional project root this reminder belongs to.
    ///
    /// `None` means the reminder is global/unscoped.
    pub project_root: Option<String>,
    pub status: ReminderStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// A reminder that has been triggered and needs attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggeredReminder {
    pub reminder_id: String,
    pub title: String,
    pub triggered_at: String,
}

// ---------------------------------------------------------------------------
// ReminderService
// ---------------------------------------------------------------------------

/// Service for managing reminders with persistent storage.
pub struct ReminderService {
    storage: Arc<AssistantStorage>,
}

impl ReminderService {
    pub fn new(storage: Arc<AssistantStorage>) -> Self {
        Self { storage }
    }

    /// Create a one-time reminder that triggers at a specific time.
    pub fn create(
        &self,
        title: &str,
        description: &str,
        trigger_at: DateTime<Utc>,
    ) -> Result<Reminder, String> {
        self.create_for_project(title, description, trigger_at, None)
    }

    /// Create a one-time reminder for a specific project root.
    pub fn create_for_project(
        &self,
        title: &str,
        description: &str,
        trigger_at: DateTime<Utc>,
        project_root: Option<&str>,
    ) -> Result<Reminder, String> {
        let now = Utc::now();
        let reminder = Reminder {
            id: Uuid::new_v4().to_string(),
            title: title.to_string(),
            description: description.to_string(),
            trigger: ReminderTrigger::At(trigger_at),
            project_root: project_root.map(|p| p.to_string()),
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        self.storage.insert_reminder(&reminder)?;
        Ok(reminder)
    }

    /// Create a recurring reminder with a cron expression.
    pub fn create_recurring(
        &self,
        title: &str,
        description: &str,
        cron_expr: &str,
    ) -> Result<Reminder, String> {
        self.create_recurring_for_project(title, description, cron_expr, None)
    }

    /// Create a recurring reminder for a specific project root.
    pub fn create_recurring_for_project(
        &self,
        title: &str,
        description: &str,
        cron_expr: &str,
        project_root: Option<&str>,
    ) -> Result<Reminder, String> {
        let now = Utc::now();
        let reminder = Reminder {
            id: Uuid::new_v4().to_string(),
            title: title.to_string(),
            description: description.to_string(),
            trigger: ReminderTrigger::Recurring(cron_expr.to_string()),
            project_root: project_root.map(|p| p.to_string()),
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        self.storage.insert_reminder(&reminder)?;
        Ok(reminder)
    }

    /// Create an event-based reminder that triggers when a named event occurs.
    pub fn create_event_reminder(
        &self,
        title: &str,
        description: &str,
        event_name: &str,
    ) -> Result<Reminder, String> {
        self.create_event_reminder_for_project(title, description, event_name, None)
    }

    /// Create an event-based reminder for a specific project root.
    pub fn create_event_reminder_for_project(
        &self,
        title: &str,
        description: &str,
        event_name: &str,
        project_root: Option<&str>,
    ) -> Result<Reminder, String> {
        let now = Utc::now();
        let reminder = Reminder {
            id: Uuid::new_v4().to_string(),
            title: title.to_string(),
            description: description.to_string(),
            trigger: ReminderTrigger::OnEvent(event_name.to_string()),
            project_root: project_root.map(|p| p.to_string()),
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        self.storage.insert_reminder(&reminder)?;
        Ok(reminder)
    }

    /// Check all active reminders and return those that should trigger now.
    ///
    /// - `ReminderTrigger::At` triggers if the trigger time is at or before `now`.
    /// - `ReminderTrigger::Recurring` triggers if the cron expression matches `now`.
    /// - `ReminderTrigger::OnEvent` is intentionally a no-op here; use
    ///   [`check_event`](Self::check_event) to fire event-based reminders.
    pub fn tick(&self) -> Result<Vec<TriggeredReminder>, String> {
        let now = Utc::now();
        let active = self.storage.list_reminders_by_status("active")?;
        let mut triggered = Vec::new();

        for reminder in &active {
            match &reminder.trigger {
                ReminderTrigger::At(dt) => {
                    if *dt <= now {
                        triggered.push(TriggeredReminder {
                            reminder_id: reminder.id.clone(),
                            title: reminder.title.clone(),
                            triggered_at: now.to_rfc3339(),
                        });
                    }
                }
                ReminderTrigger::Recurring(cron_expr) => {
                    match CronSchedule::parse(cron_expr) {
                        Ok(cron) => {
                            if cron.matches(&now) {
                                triggered.push(TriggeredReminder {
                                    reminder_id: reminder.id.clone(),
                                    title: reminder.title.clone(),
                                    triggered_at: now.to_rfc3339(),
                                });
                            }
                        }
                        Err(e) => {
                            warn!(
                                reminder_id = %reminder.id,
                                cron_expr = %cron_expr,
                                "Failed to parse cron expression, skipping: {e}"
                            );
                        }
                    }
                }
                ReminderTrigger::OnEvent(_) => {
                    // Event-based reminders fire via check_event() method,
                    // not during periodic tick().
                }
            }
        }

        Ok(triggered)
    }

    /// Check event-based reminders against a specific event name.
    ///
    /// Call this method when an event occurs (e.g. `"pr_merged"`, `"build_failed"`)
    /// to trigger any active reminders whose `OnEvent` trigger matches the given
    /// event name.
    pub fn check_event(&self, event_name: &str) -> Result<Vec<TriggeredReminder>, String> {
        let now = Utc::now();
        let active = self.storage.list_reminders_by_status("active")?;
        let mut triggered = Vec::new();

        for reminder in &active {
            if let ReminderTrigger::OnEvent(ref trigger_event) = reminder.trigger
                && trigger_event == event_name {
                    triggered.push(TriggeredReminder {
                        reminder_id: reminder.id.clone(),
                        title: reminder.title.clone(),
                        triggered_at: now.to_rfc3339(),
                    });
                }
        }

        Ok(triggered)
    }

    /// Snooze a reminder (set status to Snoozed).
    pub fn snooze(&self, id: &str) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let updated = self.storage.update_reminder_status(id, "snoozed", &now)?;
        if !updated {
            return Err(format!("Reminder '{id}' not found"));
        }
        Ok(())
    }

    /// Mark a reminder as completed.
    pub fn complete(&self, id: &str) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let updated = self.storage.update_reminder_status(id, "completed", &now)?;
        if !updated {
            return Err(format!("Reminder '{id}' not found"));
        }
        Ok(())
    }

    /// Dismiss a reminder.
    pub fn dismiss(&self, id: &str) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let updated = self.storage.update_reminder_status(id, "dismissed", &now)?;
        if !updated {
            return Err(format!("Reminder '{id}' not found"));
        }
        Ok(())
    }

    /// List all active reminders.
    pub fn list_active(&self) -> Result<Vec<Reminder>, String> {
        self.storage.list_reminders_by_status("active")
    }

    /// List active reminders scoped to a specific project.
    ///
    /// `project_root` can be a canonical path or project identifier. `None`
    /// returns all active reminders (matching current behavior).
    pub fn list_active_for_project(
        &self,
        project_root: Option<&str>,
    ) -> Result<Vec<Reminder>, String> {
        self.storage
            .list_reminders_by_project_root("active", project_root)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, Utc};

    use crate::reminders::{ReminderService, ReminderStatus};
    use crate::storage::AssistantStorage;

    fn make_service() -> ReminderService {
        let storage = Arc::new(AssistantStorage::in_memory().unwrap());
        ReminderService::new(storage)
    }

    #[test]
    fn test_create_and_list_active() {
        let service = make_service();

        let future = Utc::now() + Duration::hours(1);
        service
            .create("Standup", "Daily standup meeting", future)
            .unwrap();

        let active = service.list_active().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].title, "Standup");
    }

    #[test]
    fn test_create_for_project_scopes_active() {
        let service = make_service();
        let future = Utc::now() + Duration::hours(1);

        service
            .create_for_project(
                "Personal reminders",
                "Personal reminders",
                future,
                Some("/Users/example/personal-reminders"),
            )
            .unwrap();
        service
            .create_for_project(
                "Work reminders",
                "Work reminders",
                future,
                Some("/Users/example/workspace"),
            )
            .unwrap();

        let personal = service
            .list_active_for_project(Some("/Users/example/personal-reminders"))
            .unwrap();
        assert_eq!(personal.len(), 1);
        assert_eq!(personal[0].title, "Personal reminders");

        let workspace = service
            .list_active_for_project(Some("/Users/example/workspace"))
            .unwrap();
        assert_eq!(workspace.len(), 1);
        assert_eq!(workspace[0].title, "Work reminders");
    }

    #[test]
    fn test_create_recurring() {
        let service = make_service();

        let reminder = service
            .create_recurring("Weekly review", "End of week review", "0 17 * * FRI")
            .unwrap();

        assert_eq!(reminder.title, "Weekly review");
        let active = service.list_active().unwrap();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_tick_triggers_past_reminders() {
        let service = make_service();

        // Create a reminder in the past.
        let past = Utc::now() - Duration::hours(1);
        service.create("Overdue task", "", past).unwrap();

        let triggered = service.tick().unwrap();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].title, "Overdue task");
    }

    #[test]
    fn test_tick_does_not_trigger_future_reminders() {
        let service = make_service();

        let future = Utc::now() + Duration::hours(1);
        service.create("Future task", "", future).unwrap();

        let triggered = service.tick().unwrap();
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_snooze() {
        let service = make_service();
        let future = Utc::now() + Duration::hours(1);
        let reminder = service.create("Snooze me", "", future).unwrap();

        service.snooze(&reminder.id).unwrap();

        let active = service.list_active().unwrap();
        assert!(active.is_empty());
    }

    #[test]
    fn test_complete() {
        let service = make_service();
        let future = Utc::now() + Duration::hours(1);
        let reminder = service.create("Complete me", "", future).unwrap();

        service.complete(&reminder.id).unwrap();

        let active = service.list_active().unwrap();
        assert!(active.is_empty());
    }

    #[test]
    fn test_dismiss() {
        let service = make_service();
        let future = Utc::now() + Duration::hours(1);
        let reminder = service.create("Dismiss me", "", future).unwrap();

        service.dismiss(&reminder.id).unwrap();

        let active = service.list_active().unwrap();
        assert!(active.is_empty());
    }

    #[test]
    fn test_snooze_nonexistent_errors() {
        let service = make_service();
        let result = service.snooze("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_complete_nonexistent_errors() {
        let service = make_service();
        let result = service.complete("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_dismiss_nonexistent_errors() {
        let service = make_service();
        let result = service.dismiss("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_reminders_mixed_status() {
        let service = make_service();
        let future = Utc::now() + Duration::hours(1);
        let past = Utc::now() - Duration::hours(1);

        let r1 = service.create("Active 1", "", future).unwrap();
        let _r2 = service.create("Active 2", "", future).unwrap();
        let _r3 = service.create("Overdue", "", past).unwrap();

        // Complete one.
        service.complete(&r1.id).unwrap();

        let active = service.list_active().unwrap();
        assert_eq!(active.len(), 2);

        // Tick should trigger the overdue one.
        let triggered = service.tick().unwrap();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].title, "Overdue");
    }

    #[test]
    fn test_tick_fires_recurring_every_minute_cron() {
        // "* * * * *" matches every minute, so tick() should always trigger.
        let service = make_service();
        service
            .create_recurring("Always fires", "Fires every minute", "* * * * *")
            .unwrap();

        let triggered = service.tick().unwrap();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].title, "Always fires");
    }

    #[test]
    fn test_tick_does_not_fire_recurring_wrong_cron() {
        // Use a cron that only fires at minute 59, hour 23 — extremely unlikely
        // to match the current second unless the test runs at exactly 23:59.
        // We use a more reliable approach: pick a cron for a specific minute
        // that is NOT the current minute.
        let service = make_service();
        let now = Utc::now();
        // Pick a minute that is definitely not the current one
        let wrong_minute = (now.format("%M").to_string().parse::<u32>().unwrap() + 30) % 60;
        let cron_expr = format!("{wrong_minute} 23 31 12 0");
        service
            .create_recurring("Never fires", "Should not trigger", &cron_expr)
            .unwrap();

        let triggered = service.tick().unwrap();
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_tick_skips_invalid_cron_expression() {
        let service = make_service();
        service
            .create_recurring("Bad cron", "Has invalid cron", "not a cron")
            .unwrap();

        // Should not crash, just skip the invalid cron
        let triggered = service.tick().unwrap();
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_check_event_fires_matching_reminders() {
        let service = make_service();
        service
            .create_event_reminder("PR merged alert", "Notify on merge", "pr_merged")
            .unwrap();
        service
            .create_event_reminder("Build failed alert", "Notify on failure", "build_failed")
            .unwrap();

        let triggered = service.check_event("pr_merged").unwrap();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].title, "PR merged alert");
    }

    #[test]
    fn test_check_event_does_not_fire_unmatched() {
        let service = make_service();
        service
            .create_event_reminder("PR merged alert", "Notify on merge", "pr_merged")
            .unwrap();

        let triggered = service.check_event("build_failed").unwrap();
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_check_event_fires_multiple_matching() {
        let service = make_service();
        service
            .create_event_reminder("Alert 1", "First", "deploy_complete")
            .unwrap();
        service
            .create_event_reminder("Alert 2", "Second", "deploy_complete")
            .unwrap();
        service
            .create_event_reminder("Unrelated", "Different event", "pr_merged")
            .unwrap();

        let triggered = service.check_event("deploy_complete").unwrap();
        assert_eq!(triggered.len(), 2);
    }

    #[test]
    fn test_tick_does_not_fire_event_reminders() {
        // Event-based reminders should not fire during tick()
        let service = make_service();
        service
            .create_event_reminder("Event only", "Should not tick", "some_event")
            .unwrap();

        let triggered = service.tick().unwrap();
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_reminder_status_variants() {
        assert_eq!(ReminderStatus::Active, ReminderStatus::Active);
        assert_ne!(ReminderStatus::Active, ReminderStatus::Snoozed);
        assert_ne!(ReminderStatus::Completed, ReminderStatus::Dismissed);
    }

    #[test]
    fn test_triggered_reminder_serialization() {
        use crate::reminders::TriggeredReminder;
        let tr = TriggeredReminder {
            reminder_id: "rem-1".to_string(),
            title: "Test".to_string(),
            triggered_at: "2026-02-10T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&tr).unwrap();
        let deserialized: TriggeredReminder = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.reminder_id, "rem-1");
    }
}
