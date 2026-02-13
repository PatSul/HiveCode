//! Google Calendar API v3 client.
//!
//! Wraps the REST API at `https://www.googleapis.com/calendar/v3`
//! using `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://www.googleapis.com/calendar/v3";

/// A date-time or date value used in calendar events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDateTime {
    #[serde(default)]
    pub date_time: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub time_zone: Option<String>,
}

/// An attendee of a calendar event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attendee {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub response_status: Option<String>,
}

/// A single Google Calendar event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub start: Option<EventDateTime>,
    #[serde(default)]
    pub end: Option<EventDateTime>,
    #[serde(default)]
    pub attendees: Vec<Attendee>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub html_link: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
}

/// A single calendar entry from the calendar list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarListEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub primary: Option<bool>,
    #[serde(default)]
    pub access_role: Option<String>,
}

/// A paginated list of calendars.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarList {
    #[serde(default)]
    pub items: Vec<CalendarListEntry>,
    pub next_page_token: Option<String>,
}

/// A paginated list of events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventList {
    #[serde(default)]
    pub items: Vec<CalendarEvent>,
    pub next_page_token: Option<String>,
}

/// Request body for creating or updating a calendar event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEventRequest {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub start: Option<EventDateTime>,
    #[serde(default)]
    pub end: Option<EventDateTime>,
    #[serde(default)]
    pub attendees: Vec<Attendee>,
}

/// Request body for a FreeBusy query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeBusyRequest {
    pub time_min: String,
    pub time_max: String,
    pub items: Vec<FreeBusyCalendar>,
}

/// A calendar identifier for FreeBusy requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeBusyCalendar {
    pub id: String,
}

/// A time period during which a calendar is busy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimePeriod {
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
}

/// Busy information for a single calendar.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeBusyCalendarInfo {
    #[serde(default)]
    pub busy: Vec<TimePeriod>,
}

/// Response from a FreeBusy query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeBusyResponse {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub time_min: Option<String>,
    #[serde(default)]
    pub time_max: Option<String>,
    #[serde(default)]
    pub calendars: std::collections::HashMap<String, FreeBusyCalendarInfo>,
}

/// Client for the Google Calendar v3 REST API.
pub struct GoogleCalendarClient {
    base_url: String,
    client: Client,
}

impl GoogleCalendarClient {
    /// Create a new client using the given OAuth access token.
    pub fn new(access_token: &str) -> Self {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom base URL (useful for testing).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
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

    /// List events in a calendar within a date range.
    pub async fn list_events(
        &self,
        calendar_id: &str,
        time_min: Option<&str>,
        time_max: Option<&str>,
        max_results: Option<u32>,
    ) -> Result<EventList> {
        let mut url = format!(
            "{}/calendars/{}/events?singleEvents=true&orderBy=startTime",
            self.base_url,
            urlencod(calendar_id)
        );

        if let Some(t) = time_min {
            url.push_str(&format!("&timeMin={}", urlencod(t)));
        }
        if let Some(t) = time_max {
            url.push_str(&format!("&timeMax={}", urlencod(t)));
        }
        if let Some(max) = max_results {
            url.push_str(&format!("&maxResults={}", max));
        }

        debug!(url = %url, "listing Calendar events");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Calendar list_events request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Calendar API error ({}): {}", status, body);
        }

        resp.json()
            .await
            .context("failed to parse Calendar event list")
    }

    /// Get a single event by ID.
    pub async fn get_event(&self, calendar_id: &str, event_id: &str) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.base_url,
            urlencod(calendar_id),
            urlencod(event_id)
        );
        debug!(url = %url, "getting Calendar event");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Calendar get_event request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Calendar API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Calendar event")
    }

    /// Create a new event in the specified calendar.
    pub async fn create_event(
        &self,
        calendar_id: &str,
        event: &CreateEventRequest,
    ) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendars/{}/events",
            self.base_url,
            urlencod(calendar_id)
        );

        debug!(url = %url, "creating Calendar event");

        let resp = self
            .client
            .post(&url)
            .json(event)
            .send()
            .await
            .context("Calendar create_event request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Calendar API error ({}): {}", status, body);
        }

        resp.json()
            .await
            .context("failed to parse created Calendar event")
    }

    /// Update an existing event.
    pub async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: &CreateEventRequest,
    ) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.base_url,
            urlencod(calendar_id),
            urlencod(event_id)
        );

        debug!(url = %url, "updating Calendar event");

        let resp = self
            .client
            .put(&url)
            .json(event)
            .send()
            .await
            .context("Calendar update_event request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Calendar API error ({}): {}", status, body);
        }

        resp.json()
            .await
            .context("failed to parse updated Calendar event")
    }

    /// Delete an event by ID.
    pub async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<()> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.base_url,
            urlencod(calendar_id),
            urlencod(event_id)
        );
        debug!(url = %url, "deleting Calendar event");

        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .context("Calendar delete_event request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Calendar API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// List all calendars for the authenticated user.
    pub async fn list_calendars(&self) -> Result<CalendarList> {
        let url = format!("{}/users/me/calendarList", self.base_url);
        debug!(url = %url, "listing calendars");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Calendar list_calendars request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Calendar API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Calendar list")
    }

    /// Query free/busy information for one or more calendars.
    pub async fn free_busy(
        &self,
        time_min: &str,
        time_max: &str,
        calendars: &[String],
    ) -> Result<FreeBusyResponse> {
        let url = format!("{}/freeBusy", self.base_url);

        let request = FreeBusyRequest {
            time_min: time_min.to_string(),
            time_max: time_max.to_string(),
            items: calendars
                .iter()
                .map(|id| FreeBusyCalendar { id: id.clone() })
                .collect(),
        };

        debug!(url = %url, "querying free/busy");

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Calendar free_busy request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Calendar API error ({}): {}", status, body);
        }

        resp.json()
            .await
            .context("failed to parse free/busy response")
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_calendar_event_deserialization() {
        let json = r#"{
            "id": "evt1",
            "summary": "Team standup",
            "description": "Daily sync",
            "location": "Room 42",
            "start": { "dateTime": "2025-01-15T09:00:00-05:00", "timeZone": "America/New_York" },
            "end": { "dateTime": "2025-01-15T09:30:00-05:00", "timeZone": "America/New_York" },
            "attendees": [
                { "email": "alice@example.com", "displayName": "Alice", "responseStatus": "accepted" }
            ],
            "status": "confirmed",
            "htmlLink": "https://calendar.google.com/event?id=evt1",
            "created": "2025-01-10T12:00:00Z",
            "updated": "2025-01-14T08:00:00Z"
        }"#;
        let event: CalendarEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.id, "evt1");
        assert_eq!(event.summary.as_deref(), Some("Team standup"));
        assert_eq!(event.description.as_deref(), Some("Daily sync"));
        assert_eq!(event.location.as_deref(), Some("Room 42"));
        assert_eq!(event.status.as_deref(), Some("confirmed"));
        assert_eq!(event.attendees.len(), 1);
        assert_eq!(event.attendees[0].email, "alice@example.com");
        assert_eq!(event.attendees[0].display_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_calendar_event_minimal() {
        let json = r#"{ "id": "evt_min" }"#;
        let event: CalendarEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.id, "evt_min");
        assert!(event.summary.is_none());
        assert!(event.start.is_none());
        assert!(event.attendees.is_empty());
    }

    #[test]
    fn test_event_date_time_with_date_only() {
        let json = r#"{ "date": "2025-01-15" }"#;
        let dt: EventDateTime = serde_json::from_str(json).unwrap();
        assert_eq!(dt.date.as_deref(), Some("2025-01-15"));
        assert!(dt.date_time.is_none());
    }

    #[test]
    fn test_event_date_time_with_datetime() {
        let json = r#"{ "dateTime": "2025-01-15T09:00:00-05:00", "timeZone": "America/New_York" }"#;
        let dt: EventDateTime = serde_json::from_str(json).unwrap();
        assert_eq!(dt.date_time.as_deref(), Some("2025-01-15T09:00:00-05:00"));
        assert_eq!(dt.time_zone.as_deref(), Some("America/New_York"));
        assert!(dt.date.is_none());
    }

    #[test]
    fn test_attendee_deserialization() {
        let json = r#"{
            "email": "bob@example.com",
            "displayName": "Bob",
            "responseStatus": "tentative"
        }"#;
        let attendee: Attendee = serde_json::from_str(json).unwrap();
        assert_eq!(attendee.email, "bob@example.com");
        assert_eq!(attendee.display_name.as_deref(), Some("Bob"));
        assert_eq!(attendee.response_status.as_deref(), Some("tentative"));
    }

    #[test]
    fn test_event_list_deserialization() {
        let json = r#"{
            "items": [
                { "id": "e1", "summary": "Meeting A" },
                { "id": "e2", "summary": "Meeting B" }
            ],
            "nextPageToken": "page2"
        }"#;
        let list: EventList = serde_json::from_str(json).unwrap();
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].id, "e1");
        assert_eq!(list.next_page_token.as_deref(), Some("page2"));
    }

    #[test]
    fn test_event_list_empty() {
        let json = r#"{ "items": [] }"#;
        let list: EventList = serde_json::from_str(json).unwrap();
        assert!(list.items.is_empty());
        assert!(list.next_page_token.is_none());
    }

    #[test]
    fn test_calendar_list_deserialization() {
        let json = r#"{
            "items": [
                { "id": "cal1", "summary": "Work", "primary": true, "accessRole": "owner" },
                { "id": "cal2", "summary": "Personal" }
            ]
        }"#;
        let list: CalendarList = serde_json::from_str(json).unwrap();
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].id, "cal1");
        assert_eq!(list.items[0].primary, Some(true));
        assert_eq!(list.items[0].access_role.as_deref(), Some("owner"));
    }

    #[test]
    fn test_create_event_request_serialization() {
        let req = CreateEventRequest {
            summary: Some("Lunch".into()),
            description: Some("Team lunch".into()),
            location: Some("Cafe".into()),
            start: Some(EventDateTime {
                date_time: Some("2025-01-15T12:00:00Z".into()),
                date: None,
                time_zone: Some("UTC".into()),
            }),
            end: Some(EventDateTime {
                date_time: Some("2025-01-15T13:00:00Z".into()),
                date: None,
                time_zone: Some("UTC".into()),
            }),
            attendees: vec![Attendee {
                email: "alice@example.com".into(),
                display_name: Some("Alice".into()),
                response_status: None,
            }],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"summary\":\"Lunch\""));
        assert!(json.contains("\"location\":\"Cafe\""));
        assert!(json.contains("alice@example.com"));
    }

    #[test]
    fn test_free_busy_response_deserialization() {
        let json = r#"{
            "kind": "calendar#freeBusy",
            "timeMin": "2025-01-15T00:00:00Z",
            "timeMax": "2025-01-16T00:00:00Z",
            "calendars": {
                "primary": {
                    "busy": [
                        { "start": "2025-01-15T09:00:00Z", "end": "2025-01-15T10:00:00Z" },
                        { "start": "2025-01-15T14:00:00Z", "end": "2025-01-15T15:00:00Z" }
                    ]
                }
            }
        }"#;
        let resp: FreeBusyResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.kind, "calendar#freeBusy");
        assert_eq!(resp.time_min.as_deref(), Some("2025-01-15T00:00:00Z"));
        let primary = resp.calendars.get("primary").unwrap();
        assert_eq!(primary.busy.len(), 2);
        assert_eq!(
            primary.busy[0].start.as_deref(),
            Some("2025-01-15T09:00:00Z")
        );
    }

    #[test]
    fn test_free_busy_request_serialization() {
        let req = FreeBusyRequest {
            time_min: "2025-01-15T00:00:00Z".into(),
            time_max: "2025-01-16T00:00:00Z".into(),
            items: vec![
                FreeBusyCalendar {
                    id: "primary".into(),
                },
                FreeBusyCalendar {
                    id: "work@example.com".into(),
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"timeMin\":\"2025-01-15T00:00:00Z\""));
        assert!(json.contains("\"primary\""));
        assert!(json.contains("work@example.com"));
    }

    #[test]
    fn test_client_default_base_url() {
        let client = GoogleCalendarClient::new("tok");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url_strips_slash() {
        let client = GoogleCalendarClient::with_base_url("tok", "https://calendar.test/v3/");
        assert_eq!(client.base_url(), "https://calendar.test/v3");
    }

    #[test]
    fn test_list_events_url_construction() {
        let client = GoogleCalendarClient::new("tok");
        let url = build_url(
            client.base_url(),
            "/calendars/primary/events?singleEvents=true&orderBy=startTime",
        );
        assert!(url.starts_with(DEFAULT_BASE_URL));
        assert!(url.contains("/calendars/primary/events"));
        assert!(url.contains("singleEvents=true"));
    }

    #[test]
    fn test_get_event_url_construction() {
        let client = GoogleCalendarClient::new("tok");
        let url = build_url(client.base_url(), "/calendars/primary/events/evt123");
        assert!(url.contains("/calendars/primary/events/evt123"));
    }

    #[test]
    fn test_list_calendars_url_construction() {
        let client = GoogleCalendarClient::new("tok");
        let url = build_url(client.base_url(), "/users/me/calendarList");
        assert_eq!(
            url,
            "https://www.googleapis.com/calendar/v3/users/me/calendarList"
        );
    }

    #[test]
    fn test_free_busy_url_construction() {
        let client = GoogleCalendarClient::new("tok");
        let url = build_url(client.base_url(), "/freeBusy");
        assert!(url.ends_with("/freeBusy"));
    }

    #[test]
    fn test_delete_event_url_construction() {
        let client = GoogleCalendarClient::new("tok");
        let url = build_url(client.base_url(), "/calendars/primary/events/evt_del");
        assert!(url.contains("/calendars/primary/events/evt_del"));
    }

    #[test]
    fn test_calendar_event_serialization_roundtrip() {
        let event = CalendarEvent {
            id: "rt1".into(),
            summary: Some("Roundtrip test".into()),
            description: Some("Checking serde".into()),
            location: Some("Virtual".into()),
            start: Some(EventDateTime {
                date_time: Some("2025-01-15T10:00:00Z".into()),
                date: None,
                time_zone: Some("UTC".into()),
            }),
            end: Some(EventDateTime {
                date_time: Some("2025-01-15T11:00:00Z".into()),
                date: None,
                time_zone: Some("UTC".into()),
            }),
            attendees: vec![Attendee {
                email: "test@example.com".into(),
                display_name: Some("Test".into()),
                response_status: Some("accepted".into()),
            }],
            status: Some("confirmed".into()),
            html_link: Some("https://calendar.google.com/event?id=rt1".into()),
            created: Some("2025-01-10T00:00:00Z".into()),
            updated: Some("2025-01-14T00:00:00Z".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: CalendarEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "rt1");
        assert_eq!(back.summary.as_deref(), Some("Roundtrip test"));
        assert_eq!(back.attendees.len(), 1);
        assert_eq!(back.attendees[0].email, "test@example.com");
    }
}
