use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// MemoryCategory
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MemoryCategory {
    SuccessPattern,
    FailurePattern,
    ModelInsight,
    ConflictResolution,
    CodePattern,
    UserPreference,
    General,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SuccessPattern => "SuccessPattern",
            Self::FailurePattern => "FailurePattern",
            Self::ModelInsight => "ModelInsight",
            Self::ConflictResolution => "ConflictResolution",
            Self::CodePattern => "CodePattern",
            Self::UserPreference => "UserPreference",
            Self::General => "General",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "SuccessPattern" => Self::SuccessPattern,
            "FailurePattern" => Self::FailurePattern,
            "ModelInsight" => Self::ModelInsight,
            "ConflictResolution" => Self::ConflictResolution,
            "CodePattern" => Self::CodePattern,
            "UserPreference" => Self::UserPreference,
            _ => Self::General,
        }
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// MemoryEntry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryEntry {
    pub id: i64,
    pub category: MemoryCategory,
    pub content: String,
    pub tags: Vec<String>,
    pub source_run_id: Option<String>,
    pub source_team_id: Option<String>,
    pub relevance_score: f64,
    pub created_at: String,
    pub last_accessed: String,
    pub access_count: u64,
}

impl MemoryEntry {
    /// Convenience constructor with sensible defaults.
    pub fn new(category: MemoryCategory, content: impl Into<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: 0,
            category,
            content: content.into(),
            tags: Vec::new(),
            source_run_id: None,
            source_team_id: None,
            relevance_score: 1.0,
            created_at: now.clone(),
            last_accessed: now,
            access_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// MemoryStats
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub by_category: HashMap<MemoryCategory, usize>,
    pub avg_relevance: f64,
}

// ---------------------------------------------------------------------------
// CollectiveMemory
// ---------------------------------------------------------------------------

pub struct CollectiveMemory {
    conn: Mutex<Connection>,
}

impl CollectiveMemory {
    /// Open (or create) a SQLite database at `path`.
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("Failed to open database: {e}"))?;
        Self::init_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory SQLite database (useful for testing).
    pub fn in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory db: {e}"))?;
        Self::init_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // -- private -------------------------------------------------------------

    fn init_tables(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                category        TEXT    NOT NULL,
                content         TEXT    NOT NULL,
                tags            TEXT    NOT NULL DEFAULT '[]',
                source_run_id   TEXT,
                source_team_id  TEXT,
                relevance_score REAL    NOT NULL DEFAULT 1.0,
                created_at      TEXT    NOT NULL,
                last_accessed   TEXT    NOT NULL,
                access_count    INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_memories_category
                ON memories(category);

            CREATE INDEX IF NOT EXISTS idx_memories_relevance
                ON memories(relevance_score DESC);",
        )
        .map_err(|e| format!("Failed to initialise tables: {e}"))
    }

    /// Parse a row from the memories table into a `MemoryEntry`.
    fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<MemoryEntry, rusqlite::Error> {
        let id: i64 = row.get(0)?;
        let cat_str: String = row.get(1)?;
        let content: String = row.get(2)?;
        let tags_json: String = row.get(3)?;
        let source_run_id: Option<String> = row.get(4)?;
        let source_team_id: Option<String> = row.get(5)?;
        let relevance_score: f64 = row.get(6)?;
        let created_at: String = row.get(7)?;
        let last_accessed: String = row.get(8)?;
        let access_count: i64 = row.get(9)?;

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

        Ok(MemoryEntry {
            id,
            category: MemoryCategory::from_str(&cat_str),
            content,
            tags,
            source_run_id,
            source_team_id,
            relevance_score,
            created_at,
            last_accessed,
            access_count: access_count as u64,
        })
    }

    // -- public API ----------------------------------------------------------

    /// Insert a new memory entry. Returns the new row id.
    pub fn remember(&self, entry: &MemoryEntry) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let tags_json =
            serde_json::to_string(&entry.tags).map_err(|e| format!("JSON error: {e}"))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memories (category, content, tags, source_run_id, source_team_id,
                                   relevance_score, created_at, last_accessed, access_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.category.as_str(),
                entry.content,
                tags_json,
                entry.source_run_id,
                entry.source_team_id,
                entry.relevance_score,
                now,
                now,
                entry.access_count as i64,
            ],
        )
        .map_err(|e| format!("Insert error: {e}"))?;

        Ok(conn.last_insert_rowid())
    }

    /// Query memories.
    ///
    /// - `query`    — substring match against `content` (case-insensitive via LIKE).
    /// - `category` — optional filter on `MemoryCategory`.
    /// - `tags`     — optional: every supplied tag must appear in the stored JSON array.
    /// - `limit`    — max rows to return.
    pub fn recall(
        &self,
        query: &str,
        category: Option<MemoryCategory>,
        tags: Option<&[String]>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let mut sql = String::from(
            "SELECT id, category, content, tags, source_run_id, source_team_id,
                    relevance_score, created_at, last_accessed, access_count
             FROM memories
             WHERE content LIKE ?1",
        );

        if category.is_some() {
            sql.push_str(" AND category = ?2");
        }

        // Tag filtering: each required tag must appear as a JSON element.
        // We use `tags LIKE '%"tag"%'` for every tag — simple and sufficient for
        // JSON-encoded arrays of strings.
        let tag_clauses: Vec<String> = if let Some(t) = tags {
            t.iter()
                .enumerate()
                .map(|(i, _tag)| {
                    let param_idx = if category.is_some() { 3 + i } else { 2 + i };
                    format!(" AND tags LIKE ?{param_idx}")
                })
                .collect()
        } else {
            Vec::new()
        };
        for clause in &tag_clauses {
            sql.push_str(clause);
        }

        sql.push_str(" ORDER BY relevance_score DESC");
        sql.push_str(&format!(" LIMIT {limit}"));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Prepare error: {e}"))?;

        // Build a vector of boxed dyn ToSql so we can handle a dynamic number of params.
        let like_query = format!("%{query}%");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        param_values.push(Box::new(like_query));

        if let Some(cat) = category {
            param_values.push(Box::new(cat.as_str().to_string()));
        }

        if let Some(t) = tags {
            for tag in t {
                param_values.push(Box::new(format!("%\"{tag}\"%")));
            }
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), Self::row_to_entry)
            .map_err(|e| format!("Query error: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }

        Ok(results)
    }

    /// Bump a memory's access metadata.
    ///
    /// Sets `last_accessed` to now, increments `access_count`, and gives a tiny
    /// relevance boost (x 1.01).
    pub fn touch(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE memories
             SET last_accessed   = ?1,
                 access_count    = access_count + 1,
                 relevance_score = relevance_score * 1.01
             WHERE id = ?2",
            params![now, id],
        )
        .map_err(|e| format!("Touch error: {e}"))?;

        Ok(())
    }

    /// Multiply every entry's `relevance_score` by `factor` (typically < 1.0 to
    /// decay stale memories). Returns the number of rows affected.
    pub fn decay_scores(&self, factor: f64) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let changed = conn
            .execute(
                "UPDATE memories SET relevance_score = relevance_score * ?1",
                params![factor],
            )
            .map_err(|e| format!("Decay error: {e}"))?;

        Ok(changed)
    }

    /// Delete all entries whose `relevance_score` is below `min_relevance`.
    /// Returns the number of rows deleted.
    pub fn prune(&self, min_relevance: f64) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let deleted = conn
            .execute(
                "DELETE FROM memories WHERE relevance_score < ?1",
                params![min_relevance],
            )
            .map_err(|e| format!("Prune error: {e}"))?;

        Ok(deleted)
    }

    /// Aggregate statistics across the memory store.
    pub fn stats(&self) -> Result<MemoryStats, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let total_entries: usize = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .map_err(|e| format!("Count error: {e}"))?;

        let avg_relevance: f64 = if total_entries == 0 {
            0.0
        } else {
            conn.query_row("SELECT AVG(relevance_score) FROM memories", [], |row| {
                row.get(0)
            })
            .map_err(|e| format!("Avg error: {e}"))?
        };

        let mut by_category: HashMap<MemoryCategory, usize> = HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT category, COUNT(*) FROM memories GROUP BY category")
                .map_err(|e| format!("Prepare error: {e}"))?;

            let rows = stmt
                .query_map([], |row| {
                    let cat_str: String = row.get(0)?;
                    let count: usize = row.get(1)?;
                    Ok((cat_str, count))
                })
                .map_err(|e| format!("Query error: {e}"))?;

            for row in rows {
                let (cat_str, count) = row.map_err(|e| format!("Row error: {e}"))?;
                by_category.insert(MemoryCategory::from_str(&cat_str), count);
            }
        }

        Ok(MemoryStats {
            total_entries,
            by_category,
            avg_relevance,
        })
    }

    /// Return the total number of entries in the store.
    pub fn entry_count(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .map_err(|e| format!("Count error: {e}"))?;

        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(category: MemoryCategory, content: &str) -> MemoryEntry {
        MemoryEntry::new(category, content)
    }

    fn make_tagged_entry(category: MemoryCategory, content: &str, tags: &[&str]) -> MemoryEntry {
        let mut e = MemoryEntry::new(category, content);
        e.tags = tags.iter().map(|s| s.to_string()).collect();
        e
    }

    #[test]
    fn remember_and_recall_roundtrip() {
        let mem = CollectiveMemory::in_memory().unwrap();
        let entry = make_entry(
            MemoryCategory::SuccessPattern,
            "Use batch inserts for speed",
        );

        let id = mem.remember(&entry).unwrap();
        assert!(id > 0);

        let results = mem.recall("batch", None, None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Use batch inserts for speed");
        assert_eq!(results[0].category, MemoryCategory::SuccessPattern);
    }

    #[test]
    fn recall_with_category_filter() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "pattern A"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::FailurePattern, "pattern B"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "pattern C"))
            .unwrap();

        let success_only = mem
            .recall("pattern", Some(MemoryCategory::SuccessPattern), None, 10)
            .unwrap();
        assert_eq!(success_only.len(), 2);
        for entry in &success_only {
            assert_eq!(entry.category, MemoryCategory::SuccessPattern);
        }

        let failure_only = mem
            .recall("pattern", Some(MemoryCategory::FailurePattern), None, 10)
            .unwrap();
        assert_eq!(failure_only.len(), 1);
        assert_eq!(failure_only[0].content, "pattern B");
    }

    #[test]
    fn recall_with_query_filter() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "alpha beta gamma"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "delta epsilon"))
            .unwrap();

        let hits = mem.recall("beta", None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "alpha beta gamma");

        let hits = mem.recall("epsilon", None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "delta epsilon");

        let hits = mem.recall("nonexistent", None, None, 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn recall_with_tag_filter() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_tagged_entry(
            MemoryCategory::CodePattern,
            "use iterators",
            &["rust", "performance"],
        ))
        .unwrap();
        mem.remember(&make_tagged_entry(
            MemoryCategory::CodePattern,
            "use channels",
            &["rust", "concurrency"],
        ))
        .unwrap();
        mem.remember(&make_tagged_entry(
            MemoryCategory::CodePattern,
            "use promises",
            &["javascript"],
        ))
        .unwrap();

        let rust_tags = vec!["rust".to_string()];
        let hits = mem.recall("use", None, Some(&rust_tags), 10).unwrap();
        assert_eq!(hits.len(), 2);

        let perf_tags = vec!["performance".to_string()];
        let hits = mem.recall("use", None, Some(&perf_tags), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "use iterators");
    }

    #[test]
    fn touch_boosts_access_count_and_relevance() {
        let mem = CollectiveMemory::in_memory().unwrap();
        let entry = make_entry(MemoryCategory::ModelInsight, "gpt-4 good at reasoning");
        let id = mem.remember(&entry).unwrap();

        // Before touch
        let before = mem.recall("reasoning", None, None, 1).unwrap();
        assert_eq!(before[0].access_count, 0);
        let score_before = before[0].relevance_score;

        // Touch twice
        mem.touch(id).unwrap();
        mem.touch(id).unwrap();

        let after = mem.recall("reasoning", None, None, 1).unwrap();
        assert_eq!(after[0].access_count, 2);
        assert!(after[0].relevance_score > score_before);
    }

    #[test]
    fn decay_scores_reduces_all() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "entry one"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "entry two"))
            .unwrap();

        let affected = mem.decay_scores(0.5).unwrap();
        assert_eq!(affected, 2);

        let entries = mem.recall("entry", None, None, 10).unwrap();
        for e in &entries {
            assert!((e.relevance_score - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn prune_removes_low_relevance() {
        let mem = CollectiveMemory::in_memory().unwrap();

        // Insert a high-relevance entry and a default one.
        let mut keeper = make_entry(MemoryCategory::General, "keeper");
        keeper.relevance_score = 5.0;
        mem.remember(&keeper).unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "doomed"))
            .unwrap();

        // Decay all scores by 0.1 => keeper becomes 0.5, doomed becomes 0.1
        mem.decay_scores(0.1).unwrap();

        // Prune anything below 0.15 — only "doomed" (0.1) should be deleted.
        let deleted = mem.prune(0.15).unwrap();
        assert_eq!(deleted, 1);

        let remaining = mem.recall("", None, None, 10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content, "keeper");
    }

    #[test]
    fn stats_returns_correct_counts() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "a"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "b"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::FailurePattern, "c"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "d"))
            .unwrap();

        let s = mem.stats().unwrap();
        assert_eq!(s.total_entries, 4);
        assert_eq!(s.by_category[&MemoryCategory::SuccessPattern], 2);
        assert_eq!(s.by_category[&MemoryCategory::FailurePattern], 1);
        assert_eq!(s.by_category[&MemoryCategory::General], 1);
        assert!((s.avg_relevance - 1.0).abs() < 0.001);
    }

    #[test]
    fn empty_database_recall_returns_empty() {
        let mem = CollectiveMemory::in_memory().unwrap();
        let results = mem.recall("anything", None, None, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn entry_count_works() {
        let mem = CollectiveMemory::in_memory().unwrap();
        assert_eq!(mem.entry_count().unwrap(), 0);

        mem.remember(&make_entry(MemoryCategory::General, "one"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "two"))
            .unwrap();

        assert_eq!(mem.entry_count().unwrap(), 2);
    }

    #[test]
    fn memory_category_display_and_roundtrip() {
        let cats = [
            MemoryCategory::SuccessPattern,
            MemoryCategory::FailurePattern,
            MemoryCategory::ModelInsight,
            MemoryCategory::ConflictResolution,
            MemoryCategory::CodePattern,
            MemoryCategory::UserPreference,
            MemoryCategory::General,
        ];
        for cat in &cats {
            let s = cat.to_string();
            let back = MemoryCategory::from_str(&s);
            assert_eq!(*cat, back);
        }
        // Unknown string falls back to General.
        assert_eq!(MemoryCategory::from_str("bogus"), MemoryCategory::General);
    }
}
