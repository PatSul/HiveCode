use serde::{Deserialize, Serialize};

use crate::calendar::UnifiedEvent;
use crate::email::EmailDigest;
use crate::reminders::Reminder;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A daily briefing combining calendar, email, and reminder data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBriefing {
    pub date: String,
    pub events: Vec<UnifiedEvent>,
    pub email_summary: Option<EmailDigest>,
    pub active_reminders: Vec<Reminder>,
    pub action_items: Vec<String>,
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Generate a daily briefing from calendar events, email digest, and reminders.
pub fn generate_briefing(
    date: &str,
    events: Vec<UnifiedEvent>,
    email_digest: Option<EmailDigest>,
    reminders: Vec<Reminder>,
) -> DailyBriefing {
    let mut action_items = Vec::new();

    // Generate action items from events.
    for event in &events {
        action_items.push(format!(
            "Attend: {} ({} - {})",
            event.title, event.start, event.end
        ));
    }

    // Generate action items from email digest.
    if let Some(ref digest) = email_digest
        && digest.email_count > 0 {
            action_items.push(format!(
                "Review {} email(s): {}",
                digest.email_count, digest.summary
            ));
        }

    // Generate action items from reminders.
    for reminder in &reminders {
        action_items.push(format!("Reminder: {}", reminder.title));
    }

    DailyBriefing {
        date: date.to_string(),
        events,
        email_summary: email_digest,
        active_reminders: reminders,
        action_items,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::calendar::daily_brief::generate_briefing;
    use crate::calendar::{CalendarProvider, UnifiedEvent};
    use crate::email::{EmailDigest, EmailProvider};
    use crate::reminders::{Reminder, ReminderStatus, ReminderTrigger};

    fn make_event(title: &str) -> UnifiedEvent {
        UnifiedEvent {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            start: "2026-02-10T09:00:00Z".to_string(),
            end: "2026-02-10T10:00:00Z".to_string(),
            location: None,
            provider: CalendarProvider::Google,
            attendees: Vec::new(),
            description: None,
        }
    }

    fn make_digest() -> EmailDigest {
        EmailDigest {
            id: "dig-brief".to_string(),
            provider: EmailProvider::Gmail,
            summary: "5 important messages about project deadline".to_string(),
            email_count: 5,
            created_at: "2026-02-10T08:00:00Z".to_string(),
        }
    }

    fn make_reminder(title: &str) -> Reminder {
        let now = chrono::Utc::now();
        Reminder {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            description: String::new(),
            trigger: ReminderTrigger::At(now),
            project_root: None,
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        }
    }

    #[test]
    fn test_empty_briefing() {
        let briefing = generate_briefing("2026-02-10", Vec::new(), None, Vec::new());
        assert_eq!(briefing.date, "2026-02-10");
        assert!(briefing.events.is_empty());
        assert!(briefing.email_summary.is_none());
        assert!(briefing.active_reminders.is_empty());
        assert!(briefing.action_items.is_empty());
    }

    #[test]
    fn test_briefing_with_events() {
        let events = vec![make_event("Standup"), make_event("1:1 with Manager")];
        let briefing = generate_briefing("2026-02-10", events, None, Vec::new());

        assert_eq!(briefing.events.len(), 2);
        assert_eq!(briefing.action_items.len(), 2);
        assert!(briefing.action_items[0].contains("Standup"));
        assert!(briefing.action_items[1].contains("1:1 with Manager"));
    }

    #[test]
    fn test_briefing_with_email_digest() {
        let digest = make_digest();
        let briefing = generate_briefing("2026-02-10", Vec::new(), Some(digest), Vec::new());

        assert!(briefing.email_summary.is_some());
        assert_eq!(briefing.action_items.len(), 1);
        assert!(briefing.action_items[0].contains("5 email"));
    }

    #[test]
    fn test_briefing_with_reminders() {
        let reminders = vec![make_reminder("Deploy v2.0"), make_reminder("Call dentist")];
        let briefing = generate_briefing("2026-02-10", Vec::new(), None, reminders);

        assert_eq!(briefing.active_reminders.len(), 2);
        assert_eq!(briefing.action_items.len(), 2);
        assert!(briefing.action_items[0].contains("Deploy v2.0"));
    }

    #[test]
    fn test_full_briefing() {
        let events = vec![make_event("Team sync")];
        let digest = make_digest();
        let reminders = vec![make_reminder("Review PR")];

        let briefing = generate_briefing("2026-02-10", events, Some(digest), reminders);

        assert_eq!(briefing.events.len(), 1);
        assert!(briefing.email_summary.is_some());
        assert_eq!(briefing.active_reminders.len(), 1);
        // 1 event + 1 email + 1 reminder = 3 action items
        assert_eq!(briefing.action_items.len(), 3);
    }

    #[test]
    fn test_briefing_serialization() {
        let briefing = generate_briefing("2026-02-10", Vec::new(), None, Vec::new());
        let json = serde_json::to_string(&briefing).unwrap();
        assert!(json.contains("2026-02-10"));
        let deserialized: crate::calendar::daily_brief::DailyBriefing =
            serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.date, "2026-02-10");
    }
}
