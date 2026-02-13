use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://graph.microsoft.com/v1.0";

/// A calendar event returned from the Microsoft Graph API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub subject: String,
    pub start: EventDateTime,
    pub end: EventDateTime,
    pub location: Option<String>,
    pub is_all_day: bool,
}

/// A date-time value with its associated time zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDateTime {
    pub date_time: String,
    pub time_zone: String,
}

/// Input struct for creating a new calendar event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCalendarEvent {
    pub subject: String,
    pub start: EventDateTime,
    pub end: EventDateTime,
    pub body: Option<String>,
    pub location: Option<String>,
}

/// Microsoft Graph API client for Outlook calendar operations.
///
/// Wraps the `/me/calendarView` and `/me/events` endpoints
/// of the Microsoft Graph v1.0 REST API.
pub struct OutlookCalendarClient {
    client: Client,
    access_token: String,
    base_url: String,
}

impl OutlookCalendarClient {
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

    /// List calendar events within a time range.
    ///
    /// `start` and `end` should be ISO 8601 date-time strings
    /// (e.g. `"2026-02-01T00:00:00"`). Returns up to `top` events.
    pub async fn list_events(
        &self,
        start: &str,
        end: &str,
        top: u32,
    ) -> Result<Vec<CalendarEvent>> {
        let url = format!(
            "{}/me/calendarView?startDateTime={}&endDateTime={}&$top={}",
            self.base_url, start, end, top
        );
        debug!(url = %url, "listing Outlook calendar events");

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Outlook list events request failed")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to parse Outlook calendar response")?;

        if !status.is_success() {
            anyhow::bail!("Microsoft Graph error ({}): {}", status, body);
        }

        let events: Vec<CalendarEvent> =
            serde_json::from_value(body.get("value").cloned().unwrap_or_default())
                .context("failed to deserialize calendar events")?;

        Ok(events)
    }

    /// Create a new calendar event.
    pub async fn create_event(&self, event: &NewCalendarEvent) -> Result<CalendarEvent> {
        let url = format!("{}/me/events", self.base_url);

        let mut payload = serde_json::json!({
            "subject": event.subject,
            "start": {
                "dateTime": event.start.date_time,
                "timeZone": event.start.time_zone
            },
            "end": {
                "dateTime": event.end.date_time,
                "timeZone": event.end.time_zone
            }
        });

        if let Some(ref body) = event.body {
            payload["body"] = serde_json::json!({
                "contentType": "Text",
                "content": body
            });
        }
        if let Some(ref loc) = event.location {
            payload["location"] = serde_json::json!({ "displayName": loc });
        }

        debug!(url = %url, subject = %event.subject, "creating Outlook calendar event");

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&payload)
            .send()
            .await
            .context("Outlook create event request failed")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to parse Outlook calendar response")?;

        if !status.is_success() {
            anyhow::bail!("Microsoft Graph error ({}): {}", status, body);
        }

        let created: CalendarEvent =
            serde_json::from_value(body).context("failed to deserialize created event")?;
        Ok(created)
    }

    /// Delete a calendar event by ID.
    pub async fn delete_event(&self, event_id: &str) -> Result<()> {
        let url = format!("{}/me/events/{}", self.base_url, event_id);
        debug!(url = %url, "deleting Outlook calendar event");

        let response = self
            .client
            .delete(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Outlook delete event request failed")?;

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
        let client = OutlookCalendarClient::new("test_token");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url_strips_trailing_slash() {
        let client = OutlookCalendarClient::with_base_url("tok", "https://graph.test.com/v1.0/");
        assert_eq!(client.base_url(), "https://graph.test.com/v1.0");
    }

    #[test]
    fn test_calendar_event_serde_roundtrip() {
        let event = CalendarEvent {
            id: "evt-1".into(),
            subject: "Team standup".into(),
            start: EventDateTime {
                date_time: "2026-02-10T09:00:00".into(),
                time_zone: "UTC".into(),
            },
            end: EventDateTime {
                date_time: "2026-02-10T09:30:00".into(),
                time_zone: "UTC".into(),
            },
            location: Some("Room A".into()),
            is_all_day: false,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: CalendarEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "evt-1");
        assert_eq!(deserialized.subject, "Team standup");
        assert_eq!(deserialized.start.date_time, "2026-02-10T09:00:00");
        assert_eq!(deserialized.location.as_deref(), Some("Room A"));
        assert!(!deserialized.is_all_day);
    }

    #[test]
    fn test_new_calendar_event_serde() {
        let event = NewCalendarEvent {
            subject: "Launch meeting".into(),
            start: EventDateTime {
                date_time: "2026-03-01T14:00:00".into(),
                time_zone: "America/New_York".into(),
            },
            end: EventDateTime {
                date_time: "2026-03-01T15:00:00".into(),
                time_zone: "America/New_York".into(),
            },
            body: Some("Discuss launch plan".into()),
            location: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: NewCalendarEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.subject, "Launch meeting");
        assert_eq!(parsed.body.as_deref(), Some("Discuss launch plan"));
        assert!(parsed.location.is_none());
    }

    #[test]
    fn test_event_datetime_serde() {
        let dt = EventDateTime {
            date_time: "2026-02-07T12:00:00".into(),
            time_zone: "Europe/London".into(),
        };
        let json = serde_json::to_string(&dt).unwrap();
        assert!(json.contains("2026-02-07T12:00:00"));
        assert!(json.contains("Europe/London"));

        let parsed: EventDateTime = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.time_zone, "Europe/London");
    }

    #[test]
    fn test_calendar_view_url_construction() {
        let client = OutlookCalendarClient::new("tok");
        let url = build_url(client.base_url(), "/me/calendarView");
        assert_eq!(url, "https://graph.microsoft.com/v1.0/me/calendarView");
    }

    #[test]
    fn test_events_url_construction() {
        let client = OutlookCalendarClient::new("tok");
        let url = build_url(client.base_url(), "/me/events");
        assert_eq!(url, "https://graph.microsoft.com/v1.0/me/events");
    }

    #[test]
    fn test_create_event_payload_structure() {
        let event = NewCalendarEvent {
            subject: "Demo".into(),
            start: EventDateTime {
                date_time: "2026-02-10T10:00:00".into(),
                time_zone: "UTC".into(),
            },
            end: EventDateTime {
                date_time: "2026-02-10T11:00:00".into(),
                time_zone: "UTC".into(),
            },
            body: Some("Show progress".into()),
            location: Some("Zoom".into()),
        };

        let mut payload = serde_json::json!({
            "subject": event.subject,
            "start": { "dateTime": event.start.date_time, "timeZone": event.start.time_zone },
            "end": { "dateTime": event.end.date_time, "timeZone": event.end.time_zone }
        });
        if let Some(ref body) = event.body {
            payload["body"] = serde_json::json!({ "contentType": "Text", "content": body });
        }
        if let Some(ref loc) = event.location {
            payload["location"] = serde_json::json!({ "displayName": loc });
        }

        assert_eq!(payload["subject"], "Demo");
        assert_eq!(payload["start"]["dateTime"], "2026-02-10T10:00:00");
        assert_eq!(payload["body"]["content"], "Show progress");
        assert_eq!(payload["location"]["displayName"], "Zoom");
    }
}
