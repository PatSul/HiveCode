pub mod conflict_detector;
pub mod smart_scheduler;
pub mod daily_brief;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Supported calendar providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarProvider {
    Google,
    Outlook,
    CalDav(String),
}

/// A unified calendar event representation across all providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedEvent {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub provider: CalendarProvider,
    pub attendees: Vec<String>,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// CalendarService
// ---------------------------------------------------------------------------

/// Service for managing calendar operations across providers.
///
/// Current methods are stubs. Actual provider API integration is planned
/// for Phase 2.
pub struct CalendarService;

impl CalendarService {
    pub fn new() -> Self {
        Self
    }

    /// Get today's events.
    ///
    /// TODO: implement with actual calendar API integration
    pub fn today_events(&self) -> Result<Vec<UnifiedEvent>, String> {
        // TODO: implement with actual calendar API integration
        Ok(Vec::new())
    }

    /// Get events within a date range.
    ///
    /// TODO: implement with actual calendar API integration
    pub fn events_in_range(&self, _start: &str, _end: &str) -> Result<Vec<UnifiedEvent>, String> {
        // TODO: implement with actual calendar API integration
        Ok(Vec::new())
    }

    /// Create a new calendar event.
    ///
    /// TODO: implement with actual calendar API integration
    pub fn create_event(&self, event: &UnifiedEvent) -> Result<String, String> {
        // TODO: implement with actual calendar API integration
        tracing::info!(
            title = event.title.as_str(),
            start = event.start.as_str(),
            end = event.end.as_str(),
            "Calendar event creation requested (stub)"
        );
        Ok(event.id.clone())
    }
}

impl Default for CalendarService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::calendar::{CalendarProvider, CalendarService, UnifiedEvent};

    fn make_event(id: &str, title: &str, start: &str, end: &str) -> UnifiedEvent {
        UnifiedEvent {
            id: id.to_string(),
            title: title.to_string(),
            start: start.to_string(),
            end: end.to_string(),
            location: None,
            provider: CalendarProvider::Google,
            attendees: Vec::new(),
            description: None,
        }
    }

    #[test]
    fn test_today_events_returns_empty_stub() {
        let service = CalendarService::new();
        let events = service.today_events().unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_events_in_range_returns_empty_stub() {
        let service = CalendarService::new();
        let events = service
            .events_in_range("2026-02-10T00:00:00Z", "2026-02-10T23:59:59Z")
            .unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_create_event_returns_id() {
        let service = CalendarService::new();
        let event = make_event("ev-1", "Team standup", "2026-02-10T09:00:00Z", "2026-02-10T09:30:00Z");
        let id = service.create_event(&event).unwrap();
        assert_eq!(id, "ev-1");
    }

    #[test]
    fn test_unified_event_serialization() {
        let event = UnifiedEvent {
            id: "ev-ser".to_string(),
            title: "Meeting".to_string(),
            start: "2026-02-10T10:00:00Z".to_string(),
            end: "2026-02-10T11:00:00Z".to_string(),
            location: Some("Room 42".to_string()),
            provider: CalendarProvider::Outlook,
            attendees: vec!["alice@example.com".to_string(), "bob@example.com".to_string()],
            description: Some("Weekly sync".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: UnifiedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "ev-ser");
        assert_eq!(deserialized.attendees.len(), 2);
        assert_eq!(deserialized.location, Some("Room 42".to_string()));
    }

    #[test]
    fn test_calendar_provider_serialization() {
        let providers = vec![
            CalendarProvider::Google,
            CalendarProvider::Outlook,
            CalendarProvider::CalDav("https://cal.example.com".to_string()),
        ];
        let json = serde_json::to_string(&providers).unwrap();
        let deserialized: Vec<CalendarProvider> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, providers);
    }

    #[test]
    fn test_default_calendar_service() {
        let service = CalendarService::default();
        assert!(service.today_events().unwrap().is_empty());
    }
}
