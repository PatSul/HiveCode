use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::PathBuf;
use tracing::info;

use crate::config::HiveConfig;

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

/// Summary row for a conversation, including its message count.
#[derive(Debug, Clone)]
pub struct ConversationRow {
    pub id: String,
    pub title: String,
    pub model: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
}

/// A single message belonging to a conversation.
#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub model: Option<String>,
    pub cost: Option<f64>,
    pub tokens: Option<u32>,
    pub created_at: String,
}

/// A key-value memory entry with category and timestamp.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub category: String,
    pub updated_at: String,
}

/// A persisted log entry.
#[derive(Debug, Clone)]
pub struct LogRow {
    pub id: i64,
    pub level: String,
    pub source: String,
    pub message: String,
    pub created_at: String,
}

/// Aggregated cost data for a single model.
#[derive(Debug, Clone)]
pub struct ModelCostRow {
    pub model: String,
    pub total_cost: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub request_count: u64,
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

/// SQLite database for conversations, messages, memory entries, and cost tracking.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens (or creates) the SQLite database at `~/.hive/memory.db`.
    pub fn open() -> Result<Self> {
        let db_path = HiveConfig::db_path()?;
        Self::open_at(db_path)
    }

    /// Opens (or creates) the SQLite database at the given path.
    pub fn open_at(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open database: {}", path.display()))?;

        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Self { conn };
        db.init_schema()?;
        info!("Database opened at {}", path.display());
        Ok(db)
    }

    /// Opens an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("Failed to open in-memory database")?;

        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Creates all tables and indices if they do not already exist.
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                model TEXT,
                cost REAL,
                tokens INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS memory_entries (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS cost_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                model TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cost_usd REAL NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                level TEXT NOT NULL,
                source TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation
                ON messages(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_memory_category
                ON memory_entries(category);
            CREATE INDEX IF NOT EXISTS idx_cost_records_created
                ON cost_records(created_at);
            CREATE INDEX IF NOT EXISTS idx_cost_records_model
                ON cost_records(model);
            CREATE INDEX IF NOT EXISTS idx_logs_created
                ON logs(created_at);
            CREATE INDEX IF NOT EXISTS idx_logs_level
                ON logs(level);

            -- FTS5 full-text index for fast conversation search across
            -- titles and message content.
            CREATE VIRTUAL TABLE IF NOT EXISTS conversations_fts USING fts5(
                conversation_id UNINDEXED,
                title,
                content,
                tokenize = 'porter unicode61'
            );
            ",
        )?;
        Ok(())
    }

    /// Returns a reference to the underlying connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // -----------------------------------------------------------------------
    // Conversations
    // -----------------------------------------------------------------------

    /// Inserts or replaces a conversation header and updates the FTS index.
    pub fn save_conversation(&self, id: &str, title: &str, model: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO conversations (id, title, model, created_at, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 title = excluded.title,
                 model = excluded.model,
                 updated_at = datetime('now')",
            params![id, title, model],
        )?;
        self.rebuild_fts_for(id)?;
        Ok(())
    }

    /// Lists conversations ordered by most recently updated, with pagination.
    pub fn list_conversations(&self, limit: usize, offset: usize) -> Result<Vec<ConversationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.title, c.model, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count
             FROM conversations c
             ORDER BY c.updated_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get::<_, i64>(5)? as usize,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read conversation row")?);
        }
        Ok(result)
    }

    /// Deletes a conversation, its messages (via ON DELETE CASCADE), and FTS index entries.
    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM conversations_fts WHERE conversation_id = ?1",
            params![id],
        )?;
        self.conn
            .execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Searches conversations using FTS5 full-text search across titles and
    /// message content.  Falls back to LIKE-based search if the FTS query
    /// syntax is invalid (e.g. unmatched quotes).
    pub fn search_conversations(&self, query: &str) -> Result<Vec<ConversationRow>> {
        // Try FTS5 first — much faster for large conversation stores.
        if let Ok(results) = self.search_conversations_fts(query) {
            return Ok(results);
        }

        // Fallback: plain LIKE search (handles edge cases in query syntax).
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.title, c.model, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count
             FROM conversations c
             WHERE c.title LIKE ?1 COLLATE NOCASE
                OR c.id IN (
                    SELECT DISTINCT m.conversation_id
                    FROM messages m
                    WHERE m.content LIKE ?1 COLLATE NOCASE
                )
             ORDER BY c.updated_at DESC",
        )?;

        let rows = stmt.query_map(params![pattern], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get::<_, i64>(5)? as usize,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read conversation row")?);
        }
        Ok(result)
    }

    /// FTS5-based search implementation.
    fn search_conversations_fts(&self, query: &str) -> Result<Vec<ConversationRow>> {
        // Quote the query terms for FTS5 to handle special characters safely.
        let fts_query = format!("\"{}\"", query.replace('"', "\"\""));

        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.title, c.model, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count
             FROM conversations_fts f
             JOIN conversations c ON c.id = f.conversation_id
             WHERE conversations_fts MATCH ?1
             ORDER BY c.updated_at DESC",
        )?;

        let rows = stmt.query_map(params![fts_query], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get::<_, i64>(5)? as usize,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read conversation row")?);
        }
        Ok(result)
    }

    /// Rebuild the FTS5 index entry for a single conversation.  Deletes the
    /// existing entry (if any) and re-inserts with the current title and all
    /// message content concatenated.
    fn rebuild_fts_for(&self, conversation_id: &str) -> Result<()> {
        // Remove stale entry.
        self.conn.execute(
            "DELETE FROM conversations_fts WHERE conversation_id = ?1",
            params![conversation_id],
        )?;

        // Fetch the current title.
        let title: Option<String> = self
            .conn
            .query_row(
                "SELECT title FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .ok();

        let title = match title {
            Some(t) => t,
            None => return Ok(()), // conversation doesn't exist yet
        };

        // Concatenate all message content (space separated).
        let mut stmt = self.conn.prepare(
            "SELECT content FROM messages WHERE conversation_id = ?1 ORDER BY id ASC",
        )?;
        let contents: Vec<String> = stmt
            .query_map(params![conversation_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        let all_content = contents.join(" ");

        self.conn.execute(
            "INSERT INTO conversations_fts (conversation_id, title, content) VALUES (?1, ?2, ?3)",
            params![conversation_id, title, all_content],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Messages
    // -----------------------------------------------------------------------

    /// Saves a message, updates the FTS index, and returns its auto-generated row ID.
    pub fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        model: Option<&str>,
        cost: Option<f64>,
        tokens: Option<u32>,
    ) -> Result<i64> {
        // Touch the conversation's updated_at timestamp
        self.conn.execute(
            "UPDATE conversations SET updated_at = datetime('now') WHERE id = ?1",
            params![conversation_id],
        )?;

        self.conn.execute(
            "INSERT INTO messages (conversation_id, role, content, model, cost, tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                conversation_id,
                role,
                content,
                model,
                cost,
                tokens.map(|t| t as i64)
            ],
        )?;

        let row_id = self.conn.last_insert_rowid();
        self.rebuild_fts_for(conversation_id)?;
        Ok(row_id)
    }

    /// Returns all messages for a conversation, ordered chronologically.
    pub fn get_messages(&self, conversation_id: &str) -> Result<Vec<MessageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, role, content, model, cost, tokens, created_at
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY id ASC",
        )?;

        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok(MessageRow {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                model: row.get(3)?,
                cost: row.get(4)?,
                tokens: row.get::<_, Option<i64>>(5)?.map(|v| v as u32),
                created_at: row.get(6)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read message row")?);
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Memory entries
    // -----------------------------------------------------------------------

    /// Inserts or replaces a memory entry (upsert on key).
    pub fn save_memory(&self, key: &str, value: &str, category: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO memory_entries (key, value, category, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET
                 value = excluded.value,
                 category = excluded.category,
                 updated_at = datetime('now')",
            params![key, value, category],
        )?;
        Ok(())
    }

    /// Returns the value for a memory key, or `None` if it does not exist.
    pub fn get_memory(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM memory_entries WHERE key = ?1")?;

        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;

        match rows.next() {
            Some(row) => Ok(Some(row.context("Failed to read memory value")?)),
            None => Ok(None),
        }
    }

    /// Searches memory entries by key or value (case-insensitive LIKE).
    pub fn search_memory(&self, query: &str) -> Result<Vec<MemoryEntry>> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT key, value, category, updated_at
             FROM memory_entries
             WHERE key LIKE ?1 COLLATE NOCASE
                OR value LIKE ?1 COLLATE NOCASE
             ORDER BY updated_at DESC",
        )?;

        let rows = stmt.query_map(params![pattern], |row| {
            Ok(MemoryEntry {
                key: row.get(0)?,
                value: row.get(1)?,
                category: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read memory entry")?);
        }
        Ok(result)
    }

    /// Deletes a memory entry by key.
    pub fn delete_memory(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM memory_entries WHERE key = ?1", params![key])?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Cost tracking
    // -----------------------------------------------------------------------

    /// Records a single cost event.
    pub fn record_cost(
        &self,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cost_records (model, input_tokens, output_tokens, cost_usd)
             VALUES (?1, ?2, ?3, ?4)",
            params![model, input_tokens as i64, output_tokens as i64, cost_usd],
        )?;
        Ok(())
    }

    /// Returns total cost for the current UTC day.
    pub fn daily_cost(&self) -> Result<f64> {
        let cost: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0)
             FROM cost_records
             WHERE date(created_at) = date('now')",
            [],
            |row| row.get(0),
        )?;
        Ok(cost)
    }

    /// Returns total cost for the current UTC month.
    pub fn monthly_cost(&self) -> Result<f64> {
        let cost: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0)
             FROM cost_records
             WHERE strftime('%Y-%m', created_at) = strftime('%Y-%m', 'now')",
            [],
            |row| row.get(0),
        )?;
        Ok(cost)
    }

    // -----------------------------------------------------------------------
    // Logs
    // -----------------------------------------------------------------------

    /// Saves a log entry and returns its auto-generated row ID.
    pub fn save_log(&self, level: &str, source: &str, message: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO logs (level, source, message) VALUES (?1, ?2, ?3)",
            params![level, source, message],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Returns the most recent log entries (newest first), with pagination.
    pub fn recent_logs(&self, limit: usize, offset: usize) -> Result<Vec<LogRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, level, source, message, created_at
             FROM logs
             ORDER BY id DESC
             LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            Ok(LogRow {
                id: row.get(0)?,
                level: row.get(1)?,
                source: row.get(2)?,
                message: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read log row")?);
        }
        Ok(result)
    }

    /// Deletes log entries older than the given ISO 8601 datetime string.
    /// Returns the number of rows deleted.
    pub fn delete_logs_before(&self, before: &str) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM logs WHERE created_at < ?1",
            params![before],
        )?;
        Ok(deleted)
    }

    /// Deletes all log entries. Returns the number of rows deleted.
    pub fn clear_logs(&self) -> Result<usize> {
        let deleted = self.conn.execute("DELETE FROM logs", [])?;
        Ok(deleted)
    }

    // -----------------------------------------------------------------------
    // Cost tracking
    // -----------------------------------------------------------------------

    /// Returns aggregated cost data grouped by model.
    pub fn cost_by_model(&self) -> Result<Vec<ModelCostRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT model,
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COUNT(*)
             FROM cost_records
             GROUP BY model
             ORDER BY SUM(cost_usd) DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ModelCostRow {
                model: row.get(0)?,
                total_cost: row.get(1)?,
                total_input_tokens: row.get::<_, i64>(2)? as u64,
                total_output_tokens: row.get::<_, i64>(3)? as u64,
                request_count: row.get::<_, i64>(4)? as u64,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read cost row")?);
        }
        Ok(result)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::open_in_memory().expect("Failed to open in-memory database")
    }

    // -----------------------------------------------------------------------
    // Schema
    // -----------------------------------------------------------------------

    #[test]
    fn test_schema_creates_tables() {
        let db = test_db();
        // Verify all five tables exist by querying sqlite_master
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table'
                   AND name IN ('conversations', 'messages', 'memory_entries', 'cost_records', 'logs')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_schema_is_idempotent() {
        let db = test_db();
        // Running init_schema again should not fail
        db.init_schema()
            .expect("Second init_schema call should succeed");
    }

    // -----------------------------------------------------------------------
    // Conversations
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_and_list_conversations() {
        let db = test_db();
        db.save_conversation("c1", "First Chat", "claude-sonnet")
            .unwrap();
        db.save_conversation("c2", "Second Chat", "gpt-4").unwrap();

        let convs = db.list_conversations(10, 0).unwrap();
        assert_eq!(convs.len(), 2);
        // Both may have the same updated_at (second precision), so just check both exist
        let ids: Vec<&str> = convs.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"c1"));
        assert!(ids.contains(&"c2"));
        let c2 = convs.iter().find(|c| c.id == "c2").unwrap();
        assert_eq!(c2.title, "Second Chat");
        assert_eq!(c2.model, "gpt-4");
        assert_eq!(c2.message_count, 0);
    }

    #[test]
    fn test_save_conversation_upsert() {
        let db = test_db();
        db.save_conversation("c1", "Original", "claude").unwrap();
        db.save_conversation("c1", "Updated Title", "gpt-4")
            .unwrap();

        let convs = db.list_conversations(10, 0).unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].title, "Updated Title");
        assert_eq!(convs[0].model, "gpt-4");
    }

    #[test]
    fn test_list_conversations_pagination() {
        let db = test_db();
        for i in 0..5 {
            db.save_conversation(&format!("c{i}"), &format!("Chat {i}"), "model")
                .unwrap();
        }

        let page1 = db.list_conversations(2, 0).unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = db.list_conversations(2, 2).unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = db.list_conversations(2, 4).unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn test_delete_conversation() {
        let db = test_db();
        db.save_conversation("c1", "Delete Me", "model").unwrap();
        db.save_message("c1", "user", "hello", None, None, None)
            .unwrap();

        db.delete_conversation("c1").unwrap();

        let convs = db.list_conversations(10, 0).unwrap();
        assert!(convs.is_empty());

        // Messages should be cascade-deleted
        let msgs = db.get_messages("c1").unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_delete_nonexistent_conversation() {
        let db = test_db();
        // Should not error
        db.delete_conversation("does-not-exist").unwrap();
    }

    #[test]
    fn test_search_conversations_by_title() {
        let db = test_db();
        db.save_conversation("c1", "Rust Programming", "claude")
            .unwrap();
        db.save_conversation("c2", "Cooking Tips", "gpt-4").unwrap();

        let results = db.search_conversations("rust").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "c1");
    }

    #[test]
    fn test_search_conversations_by_message_content() {
        let db = test_db();
        db.save_conversation("c1", "General Chat", "claude")
            .unwrap();
        db.save_message(
            "c1",
            "user",
            "Tell me about quantum physics",
            None,
            None,
            None,
        )
        .unwrap();

        db.save_conversation("c2", "Another Chat", "claude")
            .unwrap();
        db.save_message("c2", "user", "How to bake bread", None, None, None)
            .unwrap();

        let results = db.search_conversations("quantum").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "c1");
    }

    #[test]
    fn test_search_conversations_no_match() {
        let db = test_db();
        db.save_conversation("c1", "Hello", "model").unwrap();

        let results = db.search_conversations("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // Messages
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_and_get_messages() {
        let db = test_db();
        db.save_conversation("c1", "Chat", "claude").unwrap();

        let id1 = db
            .save_message("c1", "user", "Hello", None, None, None)
            .unwrap();
        let id2 = db
            .save_message(
                "c1",
                "assistant",
                "Hi there!",
                Some("claude-sonnet"),
                Some(0.003),
                Some(150),
            )
            .unwrap();

        assert!(id2 > id1);

        let msgs = db.get_messages("c1").unwrap();
        assert_eq!(msgs.len(), 2);

        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello");
        assert!(msgs[0].model.is_none());
        assert!(msgs[0].cost.is_none());
        assert!(msgs[0].tokens.is_none());

        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "Hi there!");
        assert_eq!(msgs[1].model.as_deref(), Some("claude-sonnet"));
        assert!((msgs[1].cost.unwrap() - 0.003).abs() < f64::EPSILON);
        assert_eq!(msgs[1].tokens, Some(150));
    }

    #[test]
    fn test_get_messages_empty() {
        let db = test_db();
        db.save_conversation("c1", "Empty", "model").unwrap();

        let msgs = db.get_messages("c1").unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_message_count_in_conversation_list() {
        let db = test_db();
        db.save_conversation("c1", "Chat", "claude").unwrap();
        db.save_message("c1", "user", "msg1", None, None, None)
            .unwrap();
        db.save_message("c1", "assistant", "msg2", None, None, None)
            .unwrap();
        db.save_message("c1", "user", "msg3", None, None, None)
            .unwrap();

        let convs = db.list_conversations(10, 0).unwrap();
        assert_eq!(convs[0].message_count, 3);
    }

    // -----------------------------------------------------------------------
    // Memory entries
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_and_get_memory() {
        let db = test_db();
        db.save_memory("user_name", "Alice", "profile").unwrap();

        let value = db.get_memory("user_name").unwrap();
        assert_eq!(value.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_get_memory_missing_key() {
        let db = test_db();
        let value = db.get_memory("nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_save_memory_upsert() {
        let db = test_db();
        db.save_memory("key1", "original", "general").unwrap();
        db.save_memory("key1", "updated", "profile").unwrap();

        let value = db.get_memory("key1").unwrap();
        assert_eq!(value.as_deref(), Some("updated"));
    }

    #[test]
    fn test_search_memory() {
        let db = test_db();
        db.save_memory("rust_notes", "Ownership and borrowing", "programming")
            .unwrap();
        db.save_memory("cooking_tip", "Add salt to pasta water", "cooking")
            .unwrap();
        db.save_memory("rust_book", "The Rust Programming Language", "books")
            .unwrap();

        let results = db.search_memory("rust").unwrap();
        assert_eq!(results.len(), 2);

        let keys: Vec<&str> = results.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"rust_notes"));
        assert!(keys.contains(&"rust_book"));
    }

    #[test]
    fn test_search_memory_by_value() {
        let db = test_db();
        db.save_memory("tip1", "Use RUST for systems programming", "tips")
            .unwrap();

        let results = db.search_memory("rust").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "tip1");
    }

    #[test]
    fn test_delete_memory() {
        let db = test_db();
        db.save_memory("temp", "temporary value", "general")
            .unwrap();
        assert!(db.get_memory("temp").unwrap().is_some());

        db.delete_memory("temp").unwrap();
        assert!(db.get_memory("temp").unwrap().is_none());
    }

    #[test]
    fn test_delete_memory_nonexistent() {
        let db = test_db();
        // Should not error
        db.delete_memory("ghost").unwrap();
    }

    // -----------------------------------------------------------------------
    // Cost tracking
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_and_daily_cost() {
        let db = test_db();
        db.record_cost("claude-sonnet", 1000, 500, 0.01).unwrap();
        db.record_cost("claude-sonnet", 2000, 1000, 0.02).unwrap();
        db.record_cost("gpt-4", 500, 200, 0.005).unwrap();

        let daily = db.daily_cost().unwrap();
        assert!((daily - 0.035).abs() < 1e-9);
    }

    #[test]
    fn test_monthly_cost() {
        let db = test_db();
        db.record_cost("claude", 100, 50, 0.10).unwrap();
        db.record_cost("claude", 200, 100, 0.20).unwrap();

        let monthly = db.monthly_cost().unwrap();
        assert!((monthly - 0.30).abs() < 1e-9);
    }

    #[test]
    fn test_daily_cost_empty() {
        let db = test_db();
        let daily = db.daily_cost().unwrap();
        assert!((daily - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cost_by_model() {
        let db = test_db();
        db.record_cost("claude-sonnet", 1000, 500, 0.01).unwrap();
        db.record_cost("claude-sonnet", 2000, 800, 0.02).unwrap();
        db.record_cost("gpt-4", 500, 200, 0.05).unwrap();

        let by_model = db.cost_by_model().unwrap();
        assert_eq!(by_model.len(), 2);

        // Sorted by total_cost DESC, so gpt-4 ($0.05) comes first
        assert_eq!(by_model[0].model, "gpt-4");
        assert!((by_model[0].total_cost - 0.05).abs() < 1e-9);
        assert_eq!(by_model[0].total_input_tokens, 500);
        assert_eq!(by_model[0].total_output_tokens, 200);
        assert_eq!(by_model[0].request_count, 1);

        assert_eq!(by_model[1].model, "claude-sonnet");
        assert!((by_model[1].total_cost - 0.03).abs() < 1e-9);
        assert_eq!(by_model[1].total_input_tokens, 3000);
        assert_eq!(by_model[1].total_output_tokens, 1300);
        assert_eq!(by_model[1].request_count, 2);
    }

    #[test]
    fn test_cost_by_model_empty() {
        let db = test_db();
        let by_model = db.cost_by_model().unwrap();
        assert!(by_model.is_empty());
    }

    // -----------------------------------------------------------------------
    // FTS5 search
    // -----------------------------------------------------------------------

    #[test]
    fn test_fts_search_by_title() {
        let db = test_db();
        db.save_conversation("c1", "Quantum Physics Discussion", "claude")
            .unwrap();
        db.save_conversation("c2", "Cooking Recipes", "gpt-4")
            .unwrap();

        let results = db.search_conversations("quantum").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "c1");
    }

    #[test]
    fn test_fts_search_by_message_content() {
        let db = test_db();
        db.save_conversation("c1", "General Chat", "claude")
            .unwrap();
        db.save_message("c1", "user", "Tell me about photosynthesis", None, None, None)
            .unwrap();

        db.save_conversation("c2", "Another Chat", "claude")
            .unwrap();
        db.save_message("c2", "user", "How to bake bread", None, None, None)
            .unwrap();

        let results = db.search_conversations("photosynthesis").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "c1");
    }

    #[test]
    fn test_fts_search_porter_stemming() {
        let db = test_db();
        db.save_conversation("c1", "Running Tips", "claude").unwrap();
        db.save_message("c1", "user", "I love programming in Rust", None, None, None)
            .unwrap();

        // Porter stemmer should match "programming" via "program"
        let results = db.search_conversations("program").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "c1");
    }

    #[test]
    fn test_fts_search_after_delete() {
        let db = test_db();
        db.save_conversation("c1", "Delete Me", "claude").unwrap();
        db.save_message("c1", "user", "unique search term xyzzy", None, None, None)
            .unwrap();

        let results = db.search_conversations("xyzzy").unwrap();
        assert_eq!(results.len(), 1);

        db.delete_conversation("c1").unwrap();

        let results = db.search_conversations("xyzzy").unwrap();
        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // Integration: full conversation lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_conversation_lifecycle() {
        let db = test_db();

        // Create conversation
        db.save_conversation("lifecycle", "My Chat", "claude-sonnet")
            .unwrap();

        // Add messages
        db.save_message("lifecycle", "user", "What is Rust?", None, None, None)
            .unwrap();
        db.save_message(
            "lifecycle",
            "assistant",
            "Rust is a systems programming language.",
            Some("claude-sonnet"),
            Some(0.002),
            Some(45),
        )
        .unwrap();

        // Verify messages
        let msgs = db.get_messages("lifecycle").unwrap();
        assert_eq!(msgs.len(), 2);

        // Verify conversation appears in list with correct count
        let convs = db.list_conversations(10, 0).unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].message_count, 2);

        // Search finds it
        let found = db.search_conversations("systems programming").unwrap();
        assert_eq!(found.len(), 1);

        // Delete cascade
        db.delete_conversation("lifecycle").unwrap();
        assert!(db.list_conversations(10, 0).unwrap().is_empty());
        assert!(db.get_messages("lifecycle").unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // Logs
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_and_recent_logs() {
        let db = test_db();
        db.save_log("info", "agent", "Agent started").unwrap();
        db.save_log("error", "network", "Connection lost").unwrap();
        db.save_log("debug", "ui", "Render complete").unwrap();

        let logs = db.recent_logs(10, 0).unwrap();
        assert_eq!(logs.len(), 3);
        // newest first
        assert_eq!(logs[0].source, "ui");
        assert_eq!(logs[1].source, "network");
        assert_eq!(logs[2].source, "agent");
    }

    #[test]
    fn test_recent_logs_pagination() {
        let db = test_db();
        for i in 0..5 {
            db.save_log("info", "src", &format!("msg {i}")).unwrap();
        }

        let page1 = db.recent_logs(2, 0).unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = db.recent_logs(2, 2).unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = db.recent_logs(2, 4).unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn test_clear_logs() {
        let db = test_db();
        db.save_log("info", "a", "msg1").unwrap();
        db.save_log("warning", "b", "msg2").unwrap();

        let deleted = db.clear_logs().unwrap();
        assert_eq!(deleted, 2);

        let logs = db.recent_logs(10, 0).unwrap();
        assert!(logs.is_empty());
    }

    #[test]
    fn test_delete_logs_before() {
        let db = test_db();
        // Insert two with explicit timestamps for deterministic testing.
        db.conn
            .execute(
                "INSERT INTO logs (level, source, message, created_at)
                 VALUES ('info', 'old', 'old msg', '2024-01-01 00:00:00')",
                [],
            )
            .unwrap();
        db.save_log("info", "new", "new msg").unwrap();

        let deleted = db.delete_logs_before("2025-01-01 00:00:00").unwrap();
        assert_eq!(deleted, 1);

        let logs = db.recent_logs(10, 0).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].source, "new");
    }

    #[test]
    fn test_recent_logs_empty() {
        let db = test_db();
        let logs = db.recent_logs(10, 0).unwrap();
        assert!(logs.is_empty());
    }
}
