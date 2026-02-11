use rusqlite::{Connection, params};
use std::sync::Mutex;

use crate::approval::{ApprovalLevel, ApprovalRequest};
use crate::email::{EmailDigest, EmailProvider};
use crate::reminders::{Reminder, ReminderStatus, ReminderTrigger};

/// SQLite-backed persistence for the assistant subsystem.
///
/// Stores reminders, email poll state, email digests, and approval logs.
/// Uses WAL mode for improved concurrent read performance.
pub struct AssistantStorage {
    conn: Mutex<Connection>,
}

impl AssistantStorage {
    /// Open (or create) an assistant database at the given file path.
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("Failed to open database: {e}"))?;
        Self::configure_and_init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory assistant database (useful for tests).
    pub fn in_memory() -> Result<Self, String> {
        let conn =
            Connection::open_in_memory().map_err(|e| format!("Failed to open in-memory db: {e}"))?;
        Self::configure_and_init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn configure_and_init(conn: &Connection) -> Result<(), String> {
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("Failed to set WAL mode: {e}"))?;
        Self::init_tables(conn)
    }

    fn init_tables(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS reminders (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                trigger_type TEXT NOT NULL,
                trigger_at TEXT,
                recurring_cron TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS email_poll_state (
                provider TEXT PRIMARY KEY,
                last_poll_at TEXT NOT NULL,
                last_message_id TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS email_digests (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                summary TEXT NOT NULL,
                email_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS approval_log (
                id TEXT PRIMARY KEY,
                action TEXT NOT NULL,
                resource TEXT NOT NULL,
                level TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                requested_by TEXT NOT NULL,
                decided_by TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                decided_at TEXT
            );
            ",
        )
        .map_err(|e| format!("Failed to initialize tables: {e}"))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Reminders
    // -----------------------------------------------------------------------

    /// Insert a new reminder.
    pub fn insert_reminder(&self, reminder: &Reminder) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let (trigger_type, trigger_at, recurring_cron) = serialize_trigger(&reminder.trigger);
        let status_str = serialize_status(&reminder.status);
        conn.execute(
            "INSERT INTO reminders (id, title, description, trigger_type, trigger_at, recurring_cron, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                reminder.id,
                reminder.title,
                reminder.description,
                trigger_type,
                trigger_at,
                recurring_cron,
                status_str,
                reminder.created_at,
                reminder.updated_at,
            ],
        )
        .map_err(|e| format!("Failed to insert reminder: {e}"))?;
        Ok(())
    }

    /// Get a reminder by ID.
    pub fn get_reminder(&self, id: &str) -> Result<Option<Reminder>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, trigger_type, trigger_at, recurring_cron, status, created_at, updated_at
                 FROM reminders WHERE id = ?1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(params![id], |row| row_to_reminder(row))
            .map_err(|e| format!("Failed to query reminder: {e}"))?;

        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| format!("Failed to read reminder: {e}"))?)),
            None => Ok(None),
        }
    }

    /// List reminders filtered by status.
    pub fn list_reminders_by_status(&self, status: &str) -> Result<Vec<Reminder>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, trigger_type, trigger_at, recurring_cron, status, created_at, updated_at
                 FROM reminders WHERE status = ?1 ORDER BY created_at ASC",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(params![status], |row| row_to_reminder(row))
            .map_err(|e| format!("Failed to query reminders: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read reminder row: {e}"))?);
        }
        Ok(results)
    }

    /// Update reminder status.
    pub fn update_reminder_status(
        &self,
        id: &str,
        status: &str,
        updated_at: &str,
    ) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let affected = conn
            .execute(
                "UPDATE reminders SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status, updated_at, id],
            )
            .map_err(|e| format!("Failed to update reminder status: {e}"))?;
        Ok(affected > 0)
    }

    // -----------------------------------------------------------------------
    // Email poll state
    // -----------------------------------------------------------------------

    /// Upsert the email poll state for a given provider.
    pub fn upsert_poll_state(
        &self,
        provider: &str,
        last_poll_at: &str,
        last_message_id: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO email_poll_state (provider, last_poll_at, last_message_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(provider) DO UPDATE SET
                last_poll_at = excluded.last_poll_at,
                last_message_id = excluded.last_message_id",
            params![provider, last_poll_at, last_message_id],
        )
        .map_err(|e| format!("Failed to upsert poll state: {e}"))?;
        Ok(())
    }

    /// Get the poll state for a given provider.
    pub fn get_poll_state(
        &self,
        provider: &str,
    ) -> Result<Option<(String, String)>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare("SELECT last_poll_at, last_message_id FROM email_poll_state WHERE provider = ?1")
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(params![provider], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("Failed to query poll state: {e}"))?;

        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| format!("Failed to read poll state: {e}"))?)),
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // Email digests
    // -----------------------------------------------------------------------

    /// Insert an email digest.
    pub fn insert_digest(&self, digest: &EmailDigest) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let provider_str = format!("{:?}", digest.provider);
        conn.execute(
            "INSERT INTO email_digests (id, provider, summary, email_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                digest.id,
                provider_str,
                digest.summary,
                digest.email_count as i64,
                digest.created_at,
            ],
        )
        .map_err(|e| format!("Failed to insert digest: {e}"))?;
        Ok(())
    }

    /// Get the most recent digest for a provider.
    pub fn latest_digest(&self, provider: &str) -> Result<Option<EmailDigest>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, provider, summary, email_count, created_at
                 FROM email_digests WHERE provider = ?1
                 ORDER BY created_at DESC LIMIT 1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(params![provider], |row| {
                Ok(EmailDigest {
                    id: row.get(0)?,
                    provider: parse_email_provider(&row.get::<_, String>(1)?),
                    summary: row.get(2)?,
                    email_count: row.get::<_, i64>(3)? as usize,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("Failed to query digest: {e}"))?;

        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| format!("Failed to read digest: {e}"))?)),
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // Approval log
    // -----------------------------------------------------------------------

    /// Insert an approval request into the log.
    pub fn insert_approval(&self, request: &ApprovalRequest) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let level_str = format!("{:?}", request.level);
        conn.execute(
            "INSERT INTO approval_log (id, action, resource, level, status, requested_by, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)",
            params![
                request.id,
                request.action,
                request.resource,
                level_str,
                request.requested_by,
                request.created_at,
            ],
        )
        .map_err(|e| format!("Failed to insert approval: {e}"))?;
        Ok(())
    }

    /// Update an approval's status and decision metadata.
    pub fn update_approval_decision(
        &self,
        id: &str,
        status: &str,
        decided_by: &str,
        decided_at: &str,
    ) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let affected = conn
            .execute(
                "UPDATE approval_log SET status = ?1, decided_by = ?2, decided_at = ?3 WHERE id = ?4",
                params![status, decided_by, decided_at, id],
            )
            .map_err(|e| format!("Failed to update approval: {e}"))?;
        Ok(affected > 0)
    }

    /// List approvals by status.
    pub fn list_approvals_by_status(&self, status: &str) -> Result<Vec<ApprovalRequest>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, action, resource, level, requested_by, created_at
                 FROM approval_log WHERE status = ?1 ORDER BY created_at ASC",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(params![status], |row| {
                Ok(ApprovalRequest {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    resource: row.get(2)?,
                    level: parse_approval_level(&row.get::<_, String>(3)?),
                    requested_by: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("Failed to query approvals: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read approval row: {e}"))?);
        }
        Ok(results)
    }

    /// Get an approval by ID.
    pub fn get_approval(&self, id: &str) -> Result<Option<ApprovalRequest>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, action, resource, level, requested_by, created_at
                 FROM approval_log WHERE id = ?1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(ApprovalRequest {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    resource: row.get(2)?,
                    level: parse_approval_level(&row.get::<_, String>(3)?),
                    requested_by: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("Failed to query approval: {e}"))?;

        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| format!("Failed to read approval: {e}"))?)),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn serialize_trigger(trigger: &ReminderTrigger) -> (String, Option<String>, Option<String>) {
    match trigger {
        ReminderTrigger::At(dt) => ("at".to_string(), Some(dt.to_rfc3339()), None),
        ReminderTrigger::Recurring(cron) => ("recurring".to_string(), None, Some(cron.clone())),
        ReminderTrigger::OnEvent(event) => ("on_event".to_string(), Some(event.clone()), None),
    }
}

fn deserialize_trigger(
    trigger_type: &str,
    trigger_at: Option<String>,
    recurring_cron: Option<String>,
) -> ReminderTrigger {
    match trigger_type {
        "at" => {
            if let Some(ref at_str) = trigger_at {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(at_str) {
                    return ReminderTrigger::At(dt.with_timezone(&chrono::Utc));
                }
            }
            // Fallback: use epoch if parsing fails
            ReminderTrigger::At(chrono::DateTime::UNIX_EPOCH)
        }
        "recurring" => {
            ReminderTrigger::Recurring(recurring_cron.unwrap_or_default())
        }
        "on_event" => {
            ReminderTrigger::OnEvent(trigger_at.unwrap_or_default())
        }
        _ => ReminderTrigger::OnEvent("unknown".to_string()),
    }
}

fn serialize_status(status: &ReminderStatus) -> String {
    match status {
        ReminderStatus::Active => "active".to_string(),
        ReminderStatus::Snoozed => "snoozed".to_string(),
        ReminderStatus::Completed => "completed".to_string(),
        ReminderStatus::Dismissed => "dismissed".to_string(),
    }
}

fn deserialize_status(s: &str) -> ReminderStatus {
    match s {
        "active" => ReminderStatus::Active,
        "snoozed" => ReminderStatus::Snoozed,
        "completed" => ReminderStatus::Completed,
        "dismissed" => ReminderStatus::Dismissed,
        _ => ReminderStatus::Active,
    }
}

fn parse_approval_level(s: &str) -> ApprovalLevel {
    match s {
        "Low" => ApprovalLevel::Low,
        "Medium" => ApprovalLevel::Medium,
        "High" => ApprovalLevel::High,
        "Critical" => ApprovalLevel::Critical,
        _ => ApprovalLevel::Low,
    }
}

fn parse_email_provider(s: &str) -> EmailProvider {
    match s {
        "Gmail" => EmailProvider::Gmail,
        "Outlook" => EmailProvider::Outlook,
        _ => {
            // Try to parse Custom(name) format
            if s.starts_with("Custom(") && s.ends_with(')') {
                let name = s[7..s.len() - 1].trim_matches('"').to_string();
                EmailProvider::Custom(name)
            } else {
                EmailProvider::Custom(s.to_string())
            }
        }
    }
}

fn row_to_reminder(row: &rusqlite::Row) -> rusqlite::Result<Reminder> {
    let trigger_type: String = row.get(3)?;
    let trigger_at: Option<String> = row.get(4)?;
    let recurring_cron: Option<String> = row.get(5)?;
    let status_str: String = row.get(6)?;

    Ok(Reminder {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        trigger: deserialize_trigger(&trigger_type, trigger_at, recurring_cron),
        status: deserialize_status(&status_str),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use crate::approval::{ApprovalLevel, ApprovalRequest};
    use crate::email::{EmailDigest, EmailProvider};
    use crate::reminders::{Reminder, ReminderStatus, ReminderTrigger};
    use crate::storage::AssistantStorage;

    fn make_storage() -> AssistantStorage {
        AssistantStorage::in_memory().unwrap()
    }

    // -- Reminder tests --

    #[test]
    fn test_insert_and_get_reminder() {
        let storage = make_storage();
        let now = chrono::Utc::now();
        let reminder = Reminder {
            id: "rem-1".to_string(),
            title: "Standup meeting".to_string(),
            description: "Daily standup at 9am".to_string(),
            trigger: ReminderTrigger::At(now),
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };

        storage.insert_reminder(&reminder).unwrap();
        let fetched = storage.get_reminder("rem-1").unwrap().unwrap();
        assert_eq!(fetched.title, "Standup meeting");
        assert_eq!(fetched.description, "Daily standup at 9am");
    }

    #[test]
    fn test_get_nonexistent_reminder() {
        let storage = make_storage();
        assert!(storage.get_reminder("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_list_reminders_by_status() {
        let storage = make_storage();
        let now = chrono::Utc::now();

        for i in 0..3 {
            let reminder = Reminder {
                id: format!("rem-{i}"),
                title: format!("Task {i}"),
                description: String::new(),
                trigger: ReminderTrigger::At(now),
                status: if i < 2 {
                    ReminderStatus::Active
                } else {
                    ReminderStatus::Completed
                },
                created_at: now.to_rfc3339(),
                updated_at: now.to_rfc3339(),
            };
            storage.insert_reminder(&reminder).unwrap();
        }

        let active = storage.list_reminders_by_status("active").unwrap();
        assert_eq!(active.len(), 2);

        let completed = storage.list_reminders_by_status("completed").unwrap();
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_update_reminder_status() {
        let storage = make_storage();
        let now = chrono::Utc::now();

        let reminder = Reminder {
            id: "rem-upd".to_string(),
            title: "Update me".to_string(),
            description: String::new(),
            trigger: ReminderTrigger::At(now),
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        storage.insert_reminder(&reminder).unwrap();

        let updated = storage
            .update_reminder_status("rem-upd", "completed", &now.to_rfc3339())
            .unwrap();
        assert!(updated);

        let fetched = storage.get_reminder("rem-upd").unwrap().unwrap();
        assert!(matches!(fetched.status, ReminderStatus::Completed));
    }

    #[test]
    fn test_update_nonexistent_reminder() {
        let storage = make_storage();
        let updated = storage
            .update_reminder_status("nope", "completed", "2026-01-01T00:00:00Z")
            .unwrap();
        assert!(!updated);
    }

    #[test]
    fn test_reminder_recurring_trigger() {
        let storage = make_storage();
        let now = chrono::Utc::now();
        let reminder = Reminder {
            id: "rem-cron".to_string(),
            title: "Weekly review".to_string(),
            description: String::new(),
            trigger: ReminderTrigger::Recurring("0 9 * * MON".to_string()),
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        storage.insert_reminder(&reminder).unwrap();

        let fetched = storage.get_reminder("rem-cron").unwrap().unwrap();
        if let ReminderTrigger::Recurring(cron) = &fetched.trigger {
            assert_eq!(cron, "0 9 * * MON");
        } else {
            panic!("Expected Recurring trigger");
        }
    }

    #[test]
    fn test_reminder_on_event_trigger() {
        let storage = make_storage();
        let now = chrono::Utc::now();
        let reminder = Reminder {
            id: "rem-ev".to_string(),
            title: "After deploy".to_string(),
            description: String::new(),
            trigger: ReminderTrigger::OnEvent("deploy_complete".to_string()),
            status: ReminderStatus::Active,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        storage.insert_reminder(&reminder).unwrap();

        let fetched = storage.get_reminder("rem-ev").unwrap().unwrap();
        if let ReminderTrigger::OnEvent(event) = &fetched.trigger {
            assert_eq!(event, "deploy_complete");
        } else {
            panic!("Expected OnEvent trigger");
        }
    }

    // -- Email poll state tests --

    #[test]
    fn test_upsert_and_get_poll_state() {
        let storage = make_storage();
        storage
            .upsert_poll_state("gmail", "2026-02-10T12:00:00Z", "msg-100")
            .unwrap();

        let state = storage.get_poll_state("gmail").unwrap().unwrap();
        assert_eq!(state.0, "2026-02-10T12:00:00Z");
        assert_eq!(state.1, "msg-100");

        // Upsert again
        storage
            .upsert_poll_state("gmail", "2026-02-10T13:00:00Z", "msg-200")
            .unwrap();

        let state2 = storage.get_poll_state("gmail").unwrap().unwrap();
        assert_eq!(state2.0, "2026-02-10T13:00:00Z");
        assert_eq!(state2.1, "msg-200");
    }

    #[test]
    fn test_get_poll_state_nonexistent() {
        let storage = make_storage();
        assert!(storage.get_poll_state("unknown").unwrap().is_none());
    }

    // -- Email digest tests --

    #[test]
    fn test_insert_and_get_digest() {
        let storage = make_storage();
        let digest = EmailDigest {
            id: "dig-1".to_string(),
            provider: EmailProvider::Gmail,
            summary: "3 important emails about project updates".to_string(),
            email_count: 3,
            created_at: "2026-02-10T12:00:00Z".to_string(),
        };
        storage.insert_digest(&digest).unwrap();

        let fetched = storage.latest_digest("Gmail").unwrap().unwrap();
        assert_eq!(fetched.summary, "3 important emails about project updates");
        assert_eq!(fetched.email_count, 3);
    }

    #[test]
    fn test_latest_digest_returns_most_recent() {
        let storage = make_storage();
        for i in 0..3 {
            let digest = EmailDigest {
                id: format!("dig-{i}"),
                provider: EmailProvider::Gmail,
                summary: format!("Digest {i}"),
                email_count: i + 1,
                created_at: format!("2026-02-10T{:02}:00:00Z", 10 + i),
            };
            storage.insert_digest(&digest).unwrap();
        }

        let latest = storage.latest_digest("Gmail").unwrap().unwrap();
        assert_eq!(latest.summary, "Digest 2");
    }

    #[test]
    fn test_latest_digest_nonexistent() {
        let storage = make_storage();
        assert!(storage.latest_digest("unknown").unwrap().is_none());
    }

    // -- Approval log tests --

    #[test]
    fn test_insert_and_get_approval() {
        let storage = make_storage();
        let request = ApprovalRequest {
            id: "apr-1".to_string(),
            action: "deploy".to_string(),
            resource: "prod-server".to_string(),
            level: ApprovalLevel::High,
            requested_by: "bot".to_string(),
            created_at: "2026-02-10T12:00:00Z".to_string(),
        };
        storage.insert_approval(&request).unwrap();

        let fetched = storage.get_approval("apr-1").unwrap().unwrap();
        assert_eq!(fetched.action, "deploy");
        assert_eq!(fetched.resource, "prod-server");
        assert!(matches!(fetched.level, ApprovalLevel::High));
    }

    #[test]
    fn test_get_nonexistent_approval() {
        let storage = make_storage();
        assert!(storage.get_approval("nope").unwrap().is_none());
    }

    #[test]
    fn test_list_pending_approvals() {
        let storage = make_storage();
        for i in 0..3 {
            let request = ApprovalRequest {
                id: format!("apr-{i}"),
                action: format!("action-{i}"),
                resource: "resource".to_string(),
                level: ApprovalLevel::Medium,
                requested_by: "bot".to_string(),
                created_at: format!("2026-02-10T{:02}:00:00Z", 10 + i),
            };
            storage.insert_approval(&request).unwrap();
        }

        let pending = storage.list_approvals_by_status("pending").unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[test]
    fn test_update_approval_decision() {
        let storage = make_storage();
        let request = ApprovalRequest {
            id: "apr-dec".to_string(),
            action: "delete".to_string(),
            resource: "database".to_string(),
            level: ApprovalLevel::Critical,
            requested_by: "bot".to_string(),
            created_at: "2026-02-10T12:00:00Z".to_string(),
        };
        storage.insert_approval(&request).unwrap();

        let updated = storage
            .update_approval_decision(
                "apr-dec",
                "approved",
                "admin",
                "2026-02-10T13:00:00Z",
            )
            .unwrap();
        assert!(updated);

        // Verify it is no longer in pending
        let pending = storage.list_approvals_by_status("pending").unwrap();
        assert!(pending.is_empty());

        let approved = storage.list_approvals_by_status("approved").unwrap();
        assert_eq!(approved.len(), 1);
    }

    #[test]
    fn test_update_nonexistent_approval() {
        let storage = make_storage();
        let updated = storage
            .update_approval_decision("nope", "approved", "admin", "2026-02-10T12:00:00Z")
            .unwrap();
        assert!(!updated);
    }
}
