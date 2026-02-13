use serde::{Deserialize, Serialize};

use crate::calendar::UnifiedEvent;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Severity of a scheduling conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictSeverity {
    /// Events partially overlap.
    Partial,
    /// One event is entirely contained within another.
    Full,
}

/// A pair of events that conflict with each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictPair {
    pub event_a: String,
    pub event_b: String,
    pub severity: ConflictSeverity,
}

/// Report of detected scheduling conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictReport {
    pub conflicts: Vec<ConflictPair>,
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Detect scheduling conflicts in a list of events.
///
/// Compares all pairs of events and returns conflicts where time ranges
/// overlap. Events are compared by their start/end RFC3339 timestamps.
pub fn detect_conflicts(events: &[UnifiedEvent]) -> ConflictReport {
    let mut conflicts = Vec::new();

    for i in 0..events.len() {
        for j in (i + 1)..events.len() {
            let a = &events[i];
            let b = &events[j];

            if let Some(severity) = check_overlap(a, b) {
                conflicts.push(ConflictPair {
                    event_a: a.id.clone(),
                    event_b: b.id.clone(),
                    severity,
                });
            }
        }
    }

    ConflictReport { conflicts }
}

/// Check if two events overlap. Returns the severity if they do.
fn check_overlap(a: &UnifiedEvent, b: &UnifiedEvent) -> Option<ConflictSeverity> {
    // Parse timestamps; if parsing fails, assume no overlap.
    let a_start = chrono::DateTime::parse_from_rfc3339(&a.start).ok()?;
    let a_end = chrono::DateTime::parse_from_rfc3339(&a.end).ok()?;
    let b_start = chrono::DateTime::parse_from_rfc3339(&b.start).ok()?;
    let b_end = chrono::DateTime::parse_from_rfc3339(&b.end).ok()?;

    // No overlap if one ends before the other starts.
    if a_end <= b_start || b_end <= a_start {
        return None;
    }

    // Check for full containment.
    let a_contains_b = a_start <= b_start && a_end >= b_end;
    let b_contains_a = b_start <= a_start && b_end >= a_end;

    if a_contains_b || b_contains_a {
        Some(ConflictSeverity::Full)
    } else {
        Some(ConflictSeverity::Partial)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::calendar::conflict_detector::{ConflictSeverity, detect_conflicts};
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
    fn test_no_conflicts_non_overlapping() {
        let events = vec![
            make_event("a", "2026-02-10T09:00:00Z", "2026-02-10T10:00:00Z"),
            make_event("b", "2026-02-10T10:00:00Z", "2026-02-10T11:00:00Z"),
            make_event("c", "2026-02-10T12:00:00Z", "2026-02-10T13:00:00Z"),
        ];

        let report = detect_conflicts(&events);
        assert!(report.conflicts.is_empty());
    }

    #[test]
    fn test_partial_overlap() {
        let events = vec![
            make_event("a", "2026-02-10T09:00:00Z", "2026-02-10T10:30:00Z"),
            make_event("b", "2026-02-10T10:00:00Z", "2026-02-10T11:00:00Z"),
        ];

        let report = detect_conflicts(&events);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].severity, ConflictSeverity::Partial);
        assert_eq!(report.conflicts[0].event_a, "a");
        assert_eq!(report.conflicts[0].event_b, "b");
    }

    #[test]
    fn test_full_containment() {
        let events = vec![
            make_event("outer", "2026-02-10T08:00:00Z", "2026-02-10T12:00:00Z"),
            make_event("inner", "2026-02-10T09:00:00Z", "2026-02-10T10:00:00Z"),
        ];

        let report = detect_conflicts(&events);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].severity, ConflictSeverity::Full);
    }

    #[test]
    fn test_multiple_conflicts() {
        let events = vec![
            make_event("a", "2026-02-10T09:00:00Z", "2026-02-10T10:30:00Z"),
            make_event("b", "2026-02-10T10:00:00Z", "2026-02-10T11:30:00Z"),
            make_event("c", "2026-02-10T11:00:00Z", "2026-02-10T12:00:00Z"),
        ];

        let report = detect_conflicts(&events);
        // a overlaps b, b overlaps c
        assert_eq!(report.conflicts.len(), 2);
    }

    #[test]
    fn test_empty_events() {
        let report = detect_conflicts(&[]);
        assert!(report.conflicts.is_empty());
    }

    #[test]
    fn test_single_event() {
        let events = vec![make_event(
            "only",
            "2026-02-10T09:00:00Z",
            "2026-02-10T10:00:00Z",
        )];
        let report = detect_conflicts(&events);
        assert!(report.conflicts.is_empty());
    }

    #[test]
    fn test_exact_same_time() {
        let events = vec![
            make_event("a", "2026-02-10T09:00:00Z", "2026-02-10T10:00:00Z"),
            make_event("b", "2026-02-10T09:00:00Z", "2026-02-10T10:00:00Z"),
        ];

        let report = detect_conflicts(&events);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].severity, ConflictSeverity::Full);
    }

    #[test]
    fn test_adjacent_events_no_conflict() {
        // Event A ends exactly when B starts -- no conflict.
        let events = vec![
            make_event("a", "2026-02-10T09:00:00Z", "2026-02-10T10:00:00Z"),
            make_event("b", "2026-02-10T10:00:00Z", "2026-02-10T11:00:00Z"),
        ];

        let report = detect_conflicts(&events);
        assert!(report.conflicts.is_empty());
    }

    #[test]
    fn test_conflict_report_serialization() {
        let events = vec![
            make_event("a", "2026-02-10T09:00:00Z", "2026-02-10T10:30:00Z"),
            make_event("b", "2026-02-10T10:00:00Z", "2026-02-10T11:00:00Z"),
        ];
        let report = detect_conflicts(&events);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("Partial"));
    }
}
