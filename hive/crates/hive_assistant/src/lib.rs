pub mod storage;
pub mod approval;
pub mod plugin;
pub mod email;
pub mod calendar;
pub mod reminders;

use std::sync::Arc;

use storage::AssistantStorage;
use email::EmailService;
use calendar::CalendarService;
use calendar::daily_brief::{DailyBriefing, generate_briefing};
use reminders::{ReminderService, TriggeredReminder};
use approval::ApprovalService;

// Re-export key types at crate root for convenience.
pub use approval::{ApprovalLevel, ApprovalRequest, ApprovalStatus};
pub use calendar::UnifiedEvent;
pub use email::{EmailDigest, EmailProvider, UnifiedEmail};
pub use plugin::{AssistantCapability, AssistantPlugin};
pub use reminders::{Reminder, ReminderStatus, ReminderTrigger, TriggeredReminder as TriggeredRem};

/// The central coordination point for the assistant subsystem.
///
/// `AssistantService` owns EmailService, CalendarService, ReminderService,
/// and ApprovalService, providing a unified API for:
/// - Daily briefings combining calendar, email, and reminders
/// - Reminder management and tick-based triggering
/// - Approval workflows
/// - Email operations (stubs for Phase 2)
/// - Calendar operations (stubs for Phase 2)
pub struct AssistantService {
    #[allow(dead_code)]
    storage: Arc<AssistantStorage>,
    pub email_service: EmailService,
    pub calendar_service: CalendarService,
    pub reminder_service: ReminderService,
    pub approval_service: ApprovalService,
}

impl AssistantService {
    /// Open a persistent assistant database at the given path.
    pub fn open(db_path: &str) -> Result<Self, String> {
        let storage = Arc::new(AssistantStorage::open(db_path)?);
        Ok(Self::from_storage(storage))
    }

    /// Create an in-memory assistant service (useful for tests).
    pub fn in_memory() -> Result<Self, String> {
        let storage = Arc::new(AssistantStorage::in_memory()?);
        Ok(Self::from_storage(storage))
    }

    fn from_storage(storage: Arc<AssistantStorage>) -> Self {
        Self {
            email_service: EmailService::new(),
            calendar_service: CalendarService::new(),
            reminder_service: ReminderService::new(Arc::clone(&storage)),
            approval_service: ApprovalService::new(Arc::clone(&storage)),
            storage,
        }
    }

    /// Generate a daily briefing combining calendar events, email digest,
    /// and active reminders.
    pub fn daily_briefing(&self) -> DailyBriefing {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // Fetch today's events (stub returns empty).
        let events = self
            .calendar_service
            .today_events()
            .unwrap_or_default();

        // Build email digest from empty inbox (stub).
        let gmail_emails = self
            .email_service
            .fetch_gmail_inbox()
            .unwrap_or_default();
        let email_digest = if gmail_emails.is_empty() {
            None
        } else {
            Some(
                self.email_service
                    .build_digest(&gmail_emails, &email::EmailProvider::Gmail),
            )
        };

        // Get active reminders.
        let reminders = self
            .reminder_service
            .list_active()
            .unwrap_or_default();

        generate_briefing(&today, events, email_digest, reminders)
    }

    /// Tick all active reminders and return those that triggered.
    pub fn tick_reminders(&self) -> Vec<TriggeredReminder> {
        self.reminder_service.tick().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::AssistantService;
    use crate::approval::ApprovalLevel;

    #[test]
    fn test_in_memory_creation() {
        let service = AssistantService::in_memory().unwrap();
        let briefing = service.daily_briefing();
        assert!(!briefing.date.is_empty());
        assert!(briefing.events.is_empty());
        assert!(briefing.email_summary.is_none());
    }

    #[test]
    fn test_tick_reminders_empty() {
        let service = AssistantService::in_memory().unwrap();
        let triggered = service.tick_reminders();
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_tick_reminders_with_past_reminder() {
        let service = AssistantService::in_memory().unwrap();
        let past = Utc::now() - Duration::hours(1);

        service
            .reminder_service
            .create("Overdue", "Should trigger", past)
            .unwrap();

        let triggered = service.tick_reminders();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].title, "Overdue");
    }

    #[test]
    fn test_daily_briefing_with_reminders() {
        let service = AssistantService::in_memory().unwrap();
        let future = Utc::now() + Duration::hours(1);

        service
            .reminder_service
            .create("Review PR", "", future)
            .unwrap();

        let briefing = service.daily_briefing();
        assert_eq!(briefing.active_reminders.len(), 1);
        assert!(briefing.action_items.iter().any(|a| a.contains("Review PR")));
    }

    #[test]
    fn test_approval_workflow_through_service() {
        let service = AssistantService::in_memory().unwrap();

        let request = service
            .approval_service
            .submit("deploy", "prod", ApprovalLevel::High, "bot")
            .unwrap();

        let pending = service.approval_service.list_pending().unwrap();
        assert_eq!(pending.len(), 1);

        service.approval_service.approve(&request.id, "admin").unwrap();

        let pending_after = service.approval_service.list_pending().unwrap();
        assert!(pending_after.is_empty());
    }

    #[test]
    fn test_email_service_accessible() {
        let service = AssistantService::in_memory().unwrap();
        let emails = service.email_service.fetch_gmail_inbox().unwrap();
        assert!(emails.is_empty());
    }

    #[test]
    fn test_calendar_service_accessible() {
        let service = AssistantService::in_memory().unwrap();
        let events = service.calendar_service.today_events().unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_full_lifecycle() {
        let service = AssistantService::in_memory().unwrap();

        // 1. Create reminders.
        let past = Utc::now() - Duration::minutes(5);
        let future = Utc::now() + Duration::hours(2);

        let r1 = service
            .reminder_service
            .create("Overdue task", "Should trigger", past)
            .unwrap();
        service
            .reminder_service
            .create("Future task", "Should not trigger", future)
            .unwrap();

        // 2. Tick reminders.
        let triggered = service.tick_reminders();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].title, "Overdue task");

        // 3. Complete the overdue one.
        service.reminder_service.complete(&r1.id).unwrap();

        // 4. Get briefing.
        let briefing = service.daily_briefing();
        assert_eq!(briefing.active_reminders.len(), 1);
        assert!(briefing.action_items.iter().any(|a| a.contains("Future task")));

        // 5. Submit and approve something.
        let approval = service
            .approval_service
            .submit("restart", "web-server", ApprovalLevel::Medium, "agent")
            .unwrap();
        service
            .approval_service
            .approve(&approval.id, "user")
            .unwrap();

        assert!(service.approval_service.list_pending().unwrap().is_empty());
    }
}
