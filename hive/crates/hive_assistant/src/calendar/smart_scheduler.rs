use serde::{Deserialize, Serialize};

use crate::calendar::UnifiedEvent;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A suggestion for when to schedule a new event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingSuggestion {
    pub start: String,
    pub end: String,
    pub reasoning: String,
}

// ---------------------------------------------------------------------------
// Scheduling
// ---------------------------------------------------------------------------

/// Find available time slots given existing events and a desired duration.
///
/// Searches within the provided `search_start` to `search_end` window
/// for gaps large enough to fit `duration_minutes`. Returns up to
/// `max_suggestions` slots.
pub fn find_available_slots(
    events: &[UnifiedEvent],
    duration_minutes: i64,
    search_start: &str,
    search_end: &str,
    max_suggestions: usize,
) -> Vec<SchedulingSuggestion> {
    let Ok(window_start) = chrono::DateTime::parse_from_rfc3339(search_start) else {
        return Vec::new();
    };
    let Ok(window_end) = chrono::DateTime::parse_from_rfc3339(search_end) else {
        return Vec::new();
    };

    let duration = chrono::Duration::minutes(duration_minutes);

    // Parse and sort events by start time.
    let mut parsed: Vec<(
        chrono::DateTime<chrono::FixedOffset>,
        chrono::DateTime<chrono::FixedOffset>,
    )> = events
        .iter()
        .filter_map(|e| {
            let start = chrono::DateTime::parse_from_rfc3339(&e.start).ok()?;
            let end = chrono::DateTime::parse_from_rfc3339(&e.end).ok()?;
            Some((start, end))
        })
        .collect();
    parsed.sort_by_key(|(s, _)| *s);

    let mut suggestions = Vec::new();
    let mut cursor = window_start;

    for (event_start, event_end) in &parsed {
        // If the event is entirely before our cursor, skip it.
        if *event_end <= cursor {
            continue;
        }

        // If there's a gap before this event, check if it's big enough.
        if *event_start > cursor {
            let gap = *event_start - cursor;
            if gap >= duration {
                let slot_end = cursor + duration;
                suggestions.push(SchedulingSuggestion {
                    start: cursor.to_rfc3339(),
                    end: slot_end.to_rfc3339(),
                    reasoning: format!(
                        "Free slot of {} minutes before '{}'",
                        gap.num_minutes(),
                        events
                            .iter()
                            .find(|e| e.start == event_start.to_rfc3339())
                            .map(|e| e.title.as_str())
                            .unwrap_or("event")
                    ),
                });
                if suggestions.len() >= max_suggestions {
                    return suggestions;
                }
            }
        }

        // Move cursor past this event.
        if *event_end > cursor {
            cursor = *event_end;
        }
    }

    // Check for a gap after the last event.
    if cursor + duration <= window_end && suggestions.len() < max_suggestions {
        let slot_end = cursor + duration;
        suggestions.push(SchedulingSuggestion {
            start: cursor.to_rfc3339(),
            end: slot_end.to_rfc3339(),
            reasoning: "Free slot after all existing events".to_string(),
        });
    }

    suggestions
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::calendar::smart_scheduler::find_available_slots;
    use crate::calendar::{CalendarProvider, UnifiedEvent};

    fn make_event(id: &str, start: &str, end: &str) -> UnifiedEvent {
        UnifiedEvent {
            id: id.to_string(),
            title: format!("Event {id}"),
            start: start.to_string(),
            end: end.to_string(),
            location: None,
            provider: CalendarProvider::Google,
            attendees: Vec::new(),
            description: None,
        }
    }

    #[test]
    fn test_no_events_full_window_available() {
        let suggestions = find_available_slots(
            &[],
            30,
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T17:00:00+00:00",
            5,
        );
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].start.contains("09:00:00"));
    }

    #[test]
    fn test_gap_between_events() {
        let events = vec![
            make_event(
                "a",
                "2026-02-10T09:00:00+00:00",
                "2026-02-10T10:00:00+00:00",
            ),
            make_event(
                "b",
                "2026-02-10T11:00:00+00:00",
                "2026-02-10T12:00:00+00:00",
            ),
        ];

        let suggestions = find_available_slots(
            &events,
            30,
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T13:00:00+00:00",
            5,
        );

        // Should find: gap between a and b (10:00-11:00), gap after b (12:00-13:00)
        assert!(suggestions.len() >= 1);
        assert!(suggestions[0].start.contains("10:00:00"));
    }

    #[test]
    fn test_no_gap_large_enough() {
        let events = vec![
            make_event(
                "a",
                "2026-02-10T09:00:00+00:00",
                "2026-02-10T09:50:00+00:00",
            ),
            make_event(
                "b",
                "2026-02-10T09:50:00+00:00",
                "2026-02-10T10:00:00+00:00",
            ),
        ];

        // 60 min needed, but only 0 min gaps and some time after b
        let suggestions = find_available_slots(
            &events,
            60,
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T10:30:00+00:00",
            5,
        );

        // Not enough time after last event either (only 30 min left)
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_respects_max_suggestions() {
        let suggestions = find_available_slots(
            &[],
            30,
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T17:00:00+00:00",
            1,
        );
        assert_eq!(suggestions.len(), 1);
    }

    #[test]
    fn test_invalid_timestamps_return_empty() {
        let suggestions = find_available_slots(&[], 30, "not-a-date", "also-not-a-date", 5);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggestion_has_reasoning() {
        let suggestions = find_available_slots(
            &[],
            30,
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T17:00:00+00:00",
            1,
        );
        assert!(!suggestions[0].reasoning.is_empty());
    }

    #[test]
    fn test_slot_after_last_event() {
        let events = vec![make_event(
            "a",
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T10:00:00+00:00",
        )];

        let suggestions = find_available_slots(
            &events,
            60,
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T12:00:00+00:00",
            5,
        );

        // Should find a slot after event a (10:00-11:00)
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.start.contains("10:00:00")));
    }

    #[test]
    fn test_serialization() {
        let suggestions = find_available_slots(
            &[],
            30,
            "2026-02-10T09:00:00+00:00",
            "2026-02-10T17:00:00+00:00",
            1,
        );
        let json = serde_json::to_string(&suggestions).unwrap();
        assert!(json.contains("reasoning"));
    }
}
