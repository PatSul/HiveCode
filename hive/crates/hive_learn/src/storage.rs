use rusqlite::{Connection, params};
use std::sync::Mutex;

use crate::types::{
    CodePattern, LearningLogEntry, OutcomeRecord, PromptVersion, RoutingHistoryEntry,
    UserPreference,
};

/// SQLite-backed persistence for all learning data.
pub struct LearningStorage {
    conn: Mutex<Connection>,
}

impl LearningStorage {
    /// Open (or create) a learning database at the given file path.
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("Failed to open database: {e}"))?;
        Self::init_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory learning database (useful for tests).
    pub fn in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory db: {e}"))?;
        Self::init_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn init_tables(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS learning_outcomes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                task_type TEXT NOT NULL,
                tier TEXT NOT NULL,
                persona TEXT,
                outcome TEXT NOT NULL,
                edit_distance REAL,
                follow_up_count INTEGER NOT NULL DEFAULT 0,
                quality_score REAL NOT NULL DEFAULT 0.0,
                cost REAL NOT NULL DEFAULT 0.0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS routing_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_type TEXT NOT NULL,
                classified_tier TEXT NOT NULL,
                actual_tier_needed TEXT,
                model_id TEXT NOT NULL,
                quality_score REAL NOT NULL DEFAULT 0.0,
                cost REAL NOT NULL DEFAULT 0.0,
                timestamp TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS user_preferences (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.0,
                observation_count INTEGER NOT NULL DEFAULT 1,
                last_updated TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS prompt_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                persona TEXT NOT NULL,
                version INTEGER NOT NULL,
                prompt_text TEXT NOT NULL,
                avg_quality REAL NOT NULL DEFAULT 0.0,
                sample_count INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS code_patterns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                language TEXT NOT NULL,
                category TEXT NOT NULL,
                description TEXT NOT NULL,
                quality_score REAL NOT NULL DEFAULT 0.0,
                use_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS learning_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                description TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT '',
                reversible INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| format!("Failed to initialize tables: {e}"))?;
        Ok(())
    }

    /// Record an outcome from an AI interaction.
    pub fn record_outcome(&self, record: &OutcomeRecord) -> Result<i64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let outcome_str = serde_json::to_value(&record.outcome)
            .map_err(|e| format!("Failed to serialize outcome: {e}"))?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        conn.execute(
            "INSERT INTO learning_outcomes
                (conversation_id, message_id, model_id, task_type, tier, persona,
                 outcome, edit_distance, follow_up_count, quality_score, cost, latency_ms, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.conversation_id,
                record.message_id,
                record.model_id,
                record.task_type,
                record.tier,
                record.persona,
                outcome_str,
                record.edit_distance,
                record.follow_up_count,
                record.quality_score,
                record.cost,
                record.latency_ms as i64,
                record.timestamp,
            ],
        )
        .map_err(|e| format!("Failed to insert outcome: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    /// Record a routing decision.
    pub fn record_routing(&self, entry: &RoutingHistoryEntry) -> Result<i64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO routing_history
                (task_type, classified_tier, actual_tier_needed, model_id, quality_score, cost, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.task_type,
                entry.classified_tier,
                entry.actual_tier_needed,
                entry.model_id,
                entry.quality_score,
                entry.cost,
                entry.timestamp,
            ],
        )
        .map_err(|e| format!("Failed to insert routing entry: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    /// Retrieve outcome records, optionally filtered by model and recency.
    pub fn get_outcomes(
        &self,
        model_id: Option<&str>,
        days: u32,
        limit: usize,
    ) -> Result<Vec<OutcomeRecord>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(i64::from(days)))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();

        let (sql, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match model_id {
            Some(mid) => (
                "SELECT conversation_id, message_id, model_id, task_type, tier, persona,
                        outcome, edit_distance, follow_up_count, quality_score, cost, latency_ms, timestamp
                 FROM learning_outcomes
                 WHERE model_id = ?1 AND timestamp >= ?2
                 ORDER BY timestamp DESC
                 LIMIT ?3"
                    .to_string(),
                vec![
                    Box::new(mid.to_string()),
                    Box::new(cutoff),
                    Box::new(limit as i64),
                ],
            ),
            None => (
                "SELECT conversation_id, message_id, model_id, task_type, tier, persona,
                        outcome, edit_distance, follow_up_count, quality_score, cost, latency_ms, timestamp
                 FROM learning_outcomes
                 WHERE timestamp >= ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2"
                    .to_string(),
                vec![Box::new(cutoff), Box::new(limit as i64)],
            ),
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let outcome_str: String = row.get(6)?;
                let outcome = serde_json::from_value(serde_json::Value::String(outcome_str))
                    .unwrap_or(crate::types::Outcome::Unknown);
                Ok(OutcomeRecord {
                    conversation_id: row.get(0)?,
                    message_id: row.get(1)?,
                    model_id: row.get(2)?,
                    task_type: row.get(3)?,
                    tier: row.get(4)?,
                    persona: row.get(5)?,
                    outcome,
                    edit_distance: row.get(7)?,
                    follow_up_count: row.get(8)?,
                    quality_score: row.get(9)?,
                    cost: row.get(10)?,
                    latency_ms: row.get::<_, i64>(11)? as u64,
                    timestamp: row.get(12)?,
                })
            })
            .map_err(|e| format!("Failed to query outcomes: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read outcome row: {e}"))?);
        }
        Ok(results)
    }

    /// Retrieve routing history, optionally filtered by task type.
    pub fn get_routing_history(
        &self,
        task_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RoutingHistoryEntry>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        let (sql, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match task_type {
            Some(tt) => (
                "SELECT task_type, classified_tier, actual_tier_needed, model_id, quality_score, cost, timestamp
                 FROM routing_history
                 WHERE task_type = ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2"
                    .to_string(),
                vec![Box::new(tt.to_string()), Box::new(limit as i64)],
            ),
            None => (
                "SELECT task_type, classified_tier, actual_tier_needed, model_id, quality_score, cost, timestamp
                 FROM routing_history
                 ORDER BY timestamp DESC
                 LIMIT ?1"
                    .to_string(),
                vec![Box::new(limit as i64)],
            ),
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(RoutingHistoryEntry {
                    task_type: row.get(0)?,
                    classified_tier: row.get(1)?,
                    actual_tier_needed: row.get(2)?,
                    model_id: row.get(3)?,
                    quality_score: row.get(4)?,
                    cost: row.get(5)?,
                    timestamp: row.get(6)?,
                })
            })
            .map_err(|e| format!("Failed to query routing history: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read routing row: {e}"))?);
        }
        Ok(results)
    }

    /// Retrieve a single user preference by key.
    pub fn get_preference(&self, key: &str) -> Result<Option<UserPreference>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT key, value, confidence, observation_count, last_updated
                 FROM user_preferences WHERE key = ?1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(params![key], |row| {
                Ok(UserPreference {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    confidence: row.get(2)?,
                    observation_count: row.get(3)?,
                    last_updated: row.get(4)?,
                })
            })
            .map_err(|e| format!("Failed to query preference: {e}"))?;

        match rows.next() {
            Some(row) => Ok(Some(
                row.map_err(|e| format!("Failed to read preference: {e}"))?,
            )),
            None => Ok(None),
        }
    }

    /// Insert or update a user preference.
    pub fn set_preference(&self, pref: &UserPreference) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO user_preferences (key, value, confidence, observation_count, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                confidence = excluded.confidence,
                observation_count = excluded.observation_count,
                last_updated = excluded.last_updated",
            params![
                pref.key,
                pref.value,
                pref.confidence,
                pref.observation_count,
                pref.last_updated,
            ],
        )
        .map_err(|e| format!("Failed to set preference: {e}"))?;
        Ok(())
    }

    /// Delete a user preference. Returns true if a row was deleted.
    pub fn delete_preference(&self, key: &str) -> Result<bool, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let affected = conn
            .execute("DELETE FROM user_preferences WHERE key = ?1", params![key])
            .map_err(|e| format!("Failed to delete preference: {e}"))?;
        Ok(affected > 0)
    }

    /// Retrieve all user preferences.
    pub fn all_preferences(&self) -> Result<Vec<UserPreference>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT key, value, confidence, observation_count, last_updated
                 FROM user_preferences ORDER BY key",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(UserPreference {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    confidence: row.get(2)?,
                    observation_count: row.get(3)?,
                    last_updated: row.get(4)?,
                })
            })
            .map_err(|e| format!("Failed to query preferences: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read preference row: {e}"))?);
        }
        Ok(results)
    }

    /// Save a prompt version.
    pub fn save_prompt_version(&self, pv: &PromptVersion) -> Result<i64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO prompt_versions
                (persona, version, prompt_text, avg_quality, sample_count, is_active, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                pv.persona,
                pv.version,
                pv.prompt_text,
                pv.avg_quality,
                pv.sample_count,
                pv.is_active as i32,
                pv.created_at,
            ],
        )
        .map_err(|e| format!("Failed to save prompt version: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    /// Get the currently active prompt for a persona.
    pub fn get_active_prompt(&self, persona: &str) -> Result<Option<PromptVersion>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT persona, version, prompt_text, avg_quality, sample_count, is_active, created_at
                 FROM prompt_versions
                 WHERE persona = ?1 AND is_active = 1
                 ORDER BY version DESC
                 LIMIT 1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(params![persona], |row| {
                Ok(PromptVersion {
                    persona: row.get(0)?,
                    version: row.get(1)?,
                    prompt_text: row.get(2)?,
                    avg_quality: row.get(3)?,
                    sample_count: row.get(4)?,
                    is_active: row.get::<_, i32>(5)? != 0,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| format!("Failed to query active prompt: {e}"))?;

        match rows.next() {
            Some(row) => {
                Ok(Some(row.map_err(|e| {
                    format!("Failed to read prompt version: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// Get all prompt versions for a persona, ordered by version descending.
    pub fn get_prompt_versions(&self, persona: &str) -> Result<Vec<PromptVersion>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT persona, version, prompt_text, avg_quality, sample_count, is_active, created_at
                 FROM prompt_versions
                 WHERE persona = ?1
                 ORDER BY version DESC",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(params![persona], |row| {
                Ok(PromptVersion {
                    persona: row.get(0)?,
                    version: row.get(1)?,
                    prompt_text: row.get(2)?,
                    avg_quality: row.get(3)?,
                    sample_count: row.get(4)?,
                    is_active: row.get::<_, i32>(5)? != 0,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| format!("Failed to query prompt versions: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read prompt version row: {e}"))?);
        }
        Ok(results)
    }

    /// Activate a specific prompt version for a persona, deactivating all others.
    pub fn activate_prompt_version(&self, persona: &str, version: u32) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "UPDATE prompt_versions SET is_active = 0 WHERE persona = ?1",
            params![persona],
        )
        .map_err(|e| format!("Failed to deactivate prompts: {e}"))?;
        conn.execute(
            "UPDATE prompt_versions SET is_active = 1 WHERE persona = ?1 AND version = ?2",
            params![persona, version],
        )
        .map_err(|e| format!("Failed to activate prompt version: {e}"))?;
        Ok(())
    }

    /// Save a code pattern.
    pub fn save_pattern(&self, pattern: &CodePattern) -> Result<i64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO code_patterns
                (pattern, language, category, description, quality_score, use_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                pattern.pattern,
                pattern.language,
                pattern.category,
                pattern.description,
                pattern.quality_score,
                pattern.use_count,
                pattern.created_at,
            ],
        )
        .map_err(|e| format!("Failed to save pattern: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    /// Search code patterns by substring match on pattern, description, or category.
    pub fn search_patterns(&self, query: &str, limit: usize) -> Result<Vec<CodePattern>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let like_query = format!("%{query}%");
        let mut stmt = conn
            .prepare(
                "SELECT id, pattern, language, category, description, quality_score, use_count, created_at
                 FROM code_patterns
                 WHERE pattern LIKE ?1 OR description LIKE ?1 OR category LIKE ?1
                 ORDER BY quality_score DESC, use_count DESC
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(params![like_query, limit as i64], |row| {
                Ok(CodePattern {
                    id: row.get(0)?,
                    pattern: row.get(1)?,
                    language: row.get(2)?,
                    category: row.get(3)?,
                    description: row.get(4)?,
                    quality_score: row.get(5)?,
                    use_count: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| format!("Failed to search patterns: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read pattern row: {e}"))?);
        }
        Ok(results)
    }

    /// Add an entry to the transparent learning log.
    pub fn log_learning(&self, entry: &LearningLogEntry) -> Result<i64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO learning_log (event_type, description, details, reversible, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.event_type,
                entry.description,
                entry.details,
                entry.reversible as i32,
                entry.timestamp,
            ],
        )
        .map_err(|e| format!("Failed to log learning entry: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    /// Retrieve popular code patterns sorted by use_count descending.
    pub fn popular_patterns(&self, limit: usize) -> Result<Vec<CodePattern>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, pattern, language, category, description, quality_score, use_count, created_at
                 FROM code_patterns
                 ORDER BY use_count DESC, quality_score DESC
                 LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(CodePattern {
                    id: row.get(0)?,
                    pattern: row.get(1)?,
                    language: row.get(2)?,
                    category: row.get(3)?,
                    description: row.get(4)?,
                    quality_score: row.get(5)?,
                    use_count: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| format!("Failed to query popular patterns: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read pattern row: {e}"))?);
        }
        Ok(results)
    }

    /// Get distinct (task_type, tier) combos with their count and average quality.
    pub fn task_tier_stats(&self) -> Result<Vec<(String, String, u32, f64)>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT task_type, tier, COUNT(*), AVG(quality_score)
                 FROM learning_outcomes GROUP BY task_type, tier",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            })
            .map_err(|e| format!("Failed to query task-tier stats: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read stats row: {e}"))?);
        }
        Ok(results)
    }

    /// Get quality scores for a (task_type, tier) ordered by timestamp desc.
    pub fn task_tier_quality_scores(
        &self,
        task_type: &str,
        tier: &str,
        limit: usize,
    ) -> Result<Vec<f64>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT quality_score FROM learning_outcomes
                 WHERE task_type = ?1 AND tier = ?2
                 ORDER BY timestamp DESC LIMIT ?3",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(params![task_type, tier, limit as i64], |row| {
                row.get::<_, f64>(0)
            })
            .map_err(|e| format!("Failed to query quality scores: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }
        Ok(results)
    }

    /// Update avg_quality and sample_count for the active prompt of a persona.
    pub fn update_prompt_quality(
        &self,
        persona: &str,
        avg_quality: f64,
        sample_count: u32,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "UPDATE prompt_versions SET avg_quality = ?1, sample_count = ?2
             WHERE persona = ?3 AND is_active = 1",
            params![avg_quality, sample_count, persona],
        )
        .map_err(|e| format!("Failed to update prompt quality: {e}"))?;
        Ok(())
    }

    /// Get the maximum version number for a persona, or 0 if none exist.
    pub fn max_prompt_version(&self, persona: &str) -> Result<u32, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let result: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM prompt_versions WHERE persona = ?1",
                params![persona],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to get max version: {e}"))?;
        Ok(result as u32)
    }

    /// Get all active prompt versions across all personas.
    pub fn all_active_prompts(&self) -> Result<Vec<PromptVersion>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT persona, version, prompt_text, avg_quality, sample_count, is_active, created_at
                 FROM prompt_versions WHERE is_active = 1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PromptVersion {
                    persona: row.get(0)?,
                    version: row.get(1)?,
                    prompt_text: row.get(2)?,
                    avg_quality: row.get(3)?,
                    sample_count: row.get(4)?,
                    is_active: row.get::<_, i32>(5)? != 0,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| format!("Failed to query active prompts: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }
        Ok(results)
    }

    /// Delete all user preferences.
    pub fn reset_preferences(&self) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute("DELETE FROM user_preferences", [])
            .map_err(|e| format!("Failed to reset preferences: {e}"))?;
        Ok(())
    }

    /// Save a routing adjustment.
    pub fn save_routing_adjustment(
        &self,
        adj: &crate::types::RoutingAdjustment,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO routing_history (task_type, classified_tier, actual_tier_needed, model_id, quality_score, cost, timestamp)
             VALUES (?1, ?2, ?3, 'routing_adjustment', 0.0, 0.0, ?4)",
            params![
                adj.task_type,
                adj.from_tier,
                adj.to_tier,
                chrono::Utc::now().to_rfc3339(),
            ],
        )
        .map_err(|e| format!("Failed to save routing adjustment: {e}"))?;
        Ok(())
    }

    /// Get a routing adjustment for a (task_type, from_tier) pair.
    pub fn get_routing_adjustment(
        &self,
        task_type: &str,
        classified_tier: &str,
    ) -> Result<Option<String>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT actual_tier_needed FROM routing_history
                 WHERE task_type = ?1 AND classified_tier = ?2 AND model_id = 'routing_adjustment'
                 ORDER BY timestamp DESC LIMIT 1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(params![task_type, classified_tier], |row| {
                row.get::<_, Option<String>>(0)
            })
            .map_err(|e| format!("Failed to query adjustment: {e}"))?;

        match rows.next() {
            Some(row) => {
                let val = row.map_err(|e| format!("Row error: {e}"))?;
                Ok(val)
            }
            None => Ok(None),
        }
    }

    /// Clear all routing adjustments.
    pub fn clear_routing_adjustments(&self) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        conn.execute(
            "DELETE FROM routing_history WHERE model_id = 'routing_adjustment'",
            [],
        )
        .map_err(|e| format!("Failed to clear routing adjustments: {e}"))?;
        Ok(())
    }

    /// Average quality of the most recent N outcomes.
    pub fn avg_quality_recent(&self, n: usize) -> Result<f64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let result: f64 = conn
            .query_row(
                "SELECT COALESCE(AVG(quality_score), 0.0)
                 FROM (SELECT quality_score FROM learning_outcomes ORDER BY timestamp DESC LIMIT ?1)",
                params![n as i64],
                |row| row.get(0),
            )
            .map_err(|e| format!("Query error: {e}"))?;
        Ok(result)
    }

    /// Average quality of outcomes at offset..offset+n (ordered by timestamp DESC).
    pub fn avg_quality_at_offset(&self, offset: usize, n: usize) -> Result<f64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let result: f64 = conn
            .query_row(
                "SELECT COALESCE(AVG(quality_score), 0.0)
                 FROM (SELECT quality_score FROM learning_outcomes ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2)",
                params![n as i64, offset as i64],
                |row| row.get(0),
            )
            .map_err(|e| format!("Query error: {e}"))?;
        Ok(result)
    }

    /// Model-level quality stats: (model_id, count, avg_quality) for models with 5+ outcomes.
    pub fn model_quality_stats(&self) -> Result<Vec<(String, u32, f64)>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT model_id, COUNT(*), AVG(quality_score)
                 FROM learning_outcomes GROUP BY model_id HAVING COUNT(*) >= 5",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })
            .map_err(|e| format!("Failed to query model stats: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }
        Ok(results)
    }

    /// Total cost and total quality points.
    pub fn cost_quality_totals(&self) -> Result<(f64, f64), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let result = conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0.0), COALESCE(SUM(quality_score), 0.0) FROM learning_outcomes",
                [],
                |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
            )
            .map_err(|e| format!("Query error: {e}"))?;
        Ok(result)
    }

    /// Task-type quality stats: (task_type, count, avg_quality).
    pub fn task_type_quality_stats(&self) -> Result<Vec<(String, u32, f64)>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT task_type, COUNT(*), AVG(quality_score)
                 FROM learning_outcomes GROUP BY task_type",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })
            .map_err(|e| format!("Failed to query task stats: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }
        Ok(results)
    }

    /// Outcome distribution: count per outcome type.
    pub fn outcome_distribution(&self) -> Result<Vec<(String, u32)>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare("SELECT outcome, COUNT(*) FROM learning_outcomes GROUP BY outcome")
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(|e| format!("Failed to query distribution: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }
        Ok(results)
    }

    /// Misroute rate: % of routing entries where actual_tier_needed != classified_tier.
    pub fn misroute_rate(&self) -> Result<f64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM routing_history
                 WHERE actual_tier_needed IS NOT NULL AND model_id != 'routing_adjustment'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("Query error: {e}"))?;
        if total == 0 {
            return Ok(0.0);
        }
        let misrouted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM routing_history
                 WHERE actual_tier_needed IS NOT NULL AND model_id != 'routing_adjustment'
                   AND actual_tier_needed != classified_tier",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("Query error: {e}"))?;
        Ok(misrouted as f64 / total as f64)
    }

    /// Retrieve recent learning log entries.
    pub fn get_learning_log(&self, limit: usize) -> Result<Vec<LearningLogEntry>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, event_type, description, details, reversible, timestamp
                 FROM learning_log
                 ORDER BY timestamp DESC
                 LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(LearningLogEntry {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    description: row.get(2)?,
                    details: row.get(3)?,
                    reversible: row.get::<_, i32>(4)? != 0,
                    timestamp: row.get(5)?,
                })
            })
            .map_err(|e| format!("Failed to query learning log: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Failed to read learning log row: {e}"))?);
        }
        Ok(results)
    }

    /// Get the average quality score for a model over the last N days.
    /// Returns 0.0 if no data is available.
    pub fn model_quality(&self, model_id: &str, days: u32) -> Result<f64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(i64::from(days)))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();

        let result: f64 = conn
            .query_row(
                "SELECT COALESCE(AVG(quality_score), 0.0)
                 FROM learning_outcomes
                 WHERE model_id = ?1 AND timestamp >= ?2",
                params![model_id, cutoff],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to query model quality: {e}"))?;

        Ok(result)
    }

    /// Get the average quality score for a task type + tier combination over the last N days.
    /// Returns 0.0 if no data is available.
    pub fn task_tier_quality(&self, task_type: &str, tier: &str, days: u32) -> Result<f64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(i64::from(days)))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();

        let result: f64 = conn
            .query_row(
                "SELECT COALESCE(AVG(quality_score), 0.0)
                 FROM learning_outcomes
                 WHERE task_type = ?1 AND tier = ?2 AND timestamp >= ?3",
                params![task_type, tier, cutoff],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to query task-tier quality: {e}"))?;

        Ok(result)
    }

    /// Get the total number of outcome records.
    pub fn outcome_count(&self) -> Result<u64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM learning_outcomes", [], |row| {
                row.get(0)
            })
            .map_err(|e| format!("Failed to count outcomes: {e}"))?;
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Outcome;

    fn make_outcome(model_id: &str, quality: f64, outcome: Outcome) -> OutcomeRecord {
        OutcomeRecord {
            conversation_id: "conv-test".to_string(),
            message_id: format!("msg-{}", uuid::Uuid::new_v4()),
            model_id: model_id.to_string(),
            task_type: "code_generation".to_string(),
            tier: "premium".to_string(),
            persona: Some("coder".to_string()),
            outcome,
            edit_distance: Some(0.1),
            follow_up_count: 0,
            quality_score: quality,
            cost: 0.002,
            latency_ms: 800,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn make_routing_entry(task_type: &str, tier: &str, quality: f64) -> RoutingHistoryEntry {
        RoutingHistoryEntry {
            task_type: task_type.to_string(),
            classified_tier: tier.to_string(),
            actual_tier_needed: None,
            model_id: "gpt-4o".to_string(),
            quality_score: quality,
            cost: 0.003,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_record_outcome_and_get_outcomes() {
        let storage = LearningStorage::in_memory().unwrap();
        let record = make_outcome("gpt-4o", 0.85, Outcome::Accepted);
        let id = storage.record_outcome(&record).unwrap();
        assert!(id > 0);

        let outcomes = storage.get_outcomes(Some("gpt-4o"), 30, 100).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].model_id, "gpt-4o");
        assert_eq!(outcomes[0].outcome, Outcome::Accepted);
        assert!((outcomes[0].quality_score - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_outcomes_no_filter() {
        let storage = LearningStorage::in_memory().unwrap();
        storage
            .record_outcome(&make_outcome("gpt-4o", 0.9, Outcome::Accepted))
            .unwrap();
        storage
            .record_outcome(&make_outcome("claude-3", 0.7, Outcome::Corrected))
            .unwrap();

        let outcomes = storage.get_outcomes(None, 30, 100).unwrap();
        assert_eq!(outcomes.len(), 2);
    }

    #[test]
    fn test_get_outcomes_filtered_by_model() {
        let storage = LearningStorage::in_memory().unwrap();
        storage
            .record_outcome(&make_outcome("gpt-4o", 0.9, Outcome::Accepted))
            .unwrap();
        storage
            .record_outcome(&make_outcome("claude-3", 0.7, Outcome::Corrected))
            .unwrap();

        let outcomes = storage.get_outcomes(Some("claude-3"), 30, 100).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].model_id, "claude-3");
    }

    #[test]
    fn test_get_outcomes_respects_limit() {
        let storage = LearningStorage::in_memory().unwrap();
        for _ in 0..5 {
            storage
                .record_outcome(&make_outcome("gpt-4o", 0.8, Outcome::Accepted))
                .unwrap();
        }

        let outcomes = storage.get_outcomes(None, 30, 3).unwrap();
        assert_eq!(outcomes.len(), 3);
    }

    #[test]
    fn test_record_routing_and_get_history() {
        let storage = LearningStorage::in_memory().unwrap();
        let entry = make_routing_entry("code_generation", "premium", 0.8);
        let id = storage.record_routing(&entry).unwrap();
        assert!(id > 0);

        let history = storage
            .get_routing_history(Some("code_generation"), 100)
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].classified_tier, "premium");
    }

    #[test]
    fn test_get_routing_history_no_filter() {
        let storage = LearningStorage::in_memory().unwrap();
        storage
            .record_routing(&make_routing_entry("code_generation", "premium", 0.8))
            .unwrap();
        storage
            .record_routing(&make_routing_entry("chat", "standard", 0.6))
            .unwrap();

        let history = storage.get_routing_history(None, 100).unwrap();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_preference_crud() {
        let storage = LearningStorage::in_memory().unwrap();

        // Initially empty
        assert!(storage.get_preference("theme").unwrap().is_none());

        // Set
        let pref = UserPreference {
            key: "theme".to_string(),
            value: "dark".to_string(),
            confidence: 0.9,
            observation_count: 5,
            last_updated: "2026-02-10T12:00:00Z".to_string(),
        };
        storage.set_preference(&pref).unwrap();

        // Get
        let fetched = storage.get_preference("theme").unwrap().unwrap();
        assert_eq!(fetched.value, "dark");
        assert!((fetched.confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(fetched.observation_count, 5);

        // Update (upsert)
        let pref2 = UserPreference {
            key: "theme".to_string(),
            value: "light".to_string(),
            confidence: 0.95,
            observation_count: 10,
            last_updated: "2026-02-10T13:00:00Z".to_string(),
        };
        storage.set_preference(&pref2).unwrap();

        let fetched2 = storage.get_preference("theme").unwrap().unwrap();
        assert_eq!(fetched2.value, "light");
        assert_eq!(fetched2.observation_count, 10);

        // Delete
        let deleted = storage.delete_preference("theme").unwrap();
        assert!(deleted);
        assert!(storage.get_preference("theme").unwrap().is_none());

        // Delete non-existent
        let deleted2 = storage.delete_preference("nonexistent").unwrap();
        assert!(!deleted2);
    }

    #[test]
    fn test_all_preferences() {
        let storage = LearningStorage::in_memory().unwrap();

        let prefs = vec![
            UserPreference {
                key: "editor".to_string(),
                value: "vim".to_string(),
                confidence: 0.8,
                observation_count: 3,
                last_updated: "2026-02-10T12:00:00Z".to_string(),
            },
            UserPreference {
                key: "theme".to_string(),
                value: "dark".to_string(),
                confidence: 0.9,
                observation_count: 5,
                last_updated: "2026-02-10T12:00:00Z".to_string(),
            },
        ];

        for p in &prefs {
            storage.set_preference(p).unwrap();
        }

        let all = storage.all_preferences().unwrap();
        assert_eq!(all.len(), 2);
        // Ordered by key
        assert_eq!(all[0].key, "editor");
        assert_eq!(all[1].key, "theme");
    }

    #[test]
    fn test_prompt_version_save_and_get_active() {
        let storage = LearningStorage::in_memory().unwrap();

        let pv1 = PromptVersion {
            persona: "coder".to_string(),
            version: 1,
            prompt_text: "You are a coding assistant.".to_string(),
            avg_quality: 0.7,
            sample_count: 50,
            is_active: true,
            created_at: "2026-02-10T10:00:00Z".to_string(),
        };
        storage.save_prompt_version(&pv1).unwrap();

        let active = storage.get_active_prompt("coder").unwrap().unwrap();
        assert_eq!(active.version, 1);
        assert!(active.is_active);
        assert_eq!(active.prompt_text, "You are a coding assistant.");

        // No active prompt for unknown persona
        assert!(storage.get_active_prompt("unknown").unwrap().is_none());
    }

    #[test]
    fn test_prompt_version_activate() {
        let storage = LearningStorage::in_memory().unwrap();

        let pv1 = PromptVersion {
            persona: "coder".to_string(),
            version: 1,
            prompt_text: "Version 1 prompt.".to_string(),
            avg_quality: 0.7,
            sample_count: 50,
            is_active: true,
            created_at: "2026-02-10T10:00:00Z".to_string(),
        };
        let pv2 = PromptVersion {
            persona: "coder".to_string(),
            version: 2,
            prompt_text: "Version 2 prompt.".to_string(),
            avg_quality: 0.85,
            sample_count: 30,
            is_active: false,
            created_at: "2026-02-10T11:00:00Z".to_string(),
        };
        storage.save_prompt_version(&pv1).unwrap();
        storage.save_prompt_version(&pv2).unwrap();

        // v1 is active
        let active = storage.get_active_prompt("coder").unwrap().unwrap();
        assert_eq!(active.version, 1);

        // Activate v2
        storage.activate_prompt_version("coder", 2).unwrap();
        let active = storage.get_active_prompt("coder").unwrap().unwrap();
        assert_eq!(active.version, 2);
        assert_eq!(active.prompt_text, "Version 2 prompt.");

        // v1 should no longer be active
        let all = storage.get_prompt_versions("coder").unwrap();
        assert_eq!(all.len(), 2);
        for pv in &all {
            if pv.version == 1 {
                assert!(!pv.is_active);
            }
            if pv.version == 2 {
                assert!(pv.is_active);
            }
        }
    }

    #[test]
    fn test_get_prompt_versions() {
        let storage = LearningStorage::in_memory().unwrap();

        for v in 1..=3 {
            let pv = PromptVersion {
                persona: "writer".to_string(),
                version: v,
                prompt_text: format!("Writer prompt v{v}"),
                avg_quality: 0.5 + (v as f64) * 0.1,
                sample_count: v * 10,
                is_active: v == 3,
                created_at: format!("2026-02-10T{:02}:00:00Z", 10 + v),
            };
            storage.save_prompt_version(&pv).unwrap();
        }

        let versions = storage.get_prompt_versions("writer").unwrap();
        assert_eq!(versions.len(), 3);
        // Ordered by version DESC
        assert_eq!(versions[0].version, 3);
        assert_eq!(versions[1].version, 2);
        assert_eq!(versions[2].version, 1);
    }

    #[test]
    fn test_pattern_save_and_search() {
        let storage = LearningStorage::in_memory().unwrap();

        let pattern1 = CodePattern {
            id: 0,
            pattern: "fn error_handler(err: &Error) -> Response".to_string(),
            language: "rust".to_string(),
            category: "error_handling".to_string(),
            description: "Standard error handler pattern for HTTP responses".to_string(),
            quality_score: 0.9,
            use_count: 15,
            created_at: "2026-02-10T12:00:00Z".to_string(),
        };
        let pattern2 = CodePattern {
            id: 0,
            pattern: "struct AppState { db: Pool<Postgres> }".to_string(),
            language: "rust".to_string(),
            category: "architecture".to_string(),
            description: "Application state with database pool".to_string(),
            quality_score: 0.85,
            use_count: 10,
            created_at: "2026-02-10T12:00:00Z".to_string(),
        };

        let id1 = storage.save_pattern(&pattern1).unwrap();
        let id2 = storage.save_pattern(&pattern2).unwrap();
        assert!(id1 > 0);
        assert!(id2 > 0);
        assert_ne!(id1, id2);

        // Search by pattern text
        let results = storage.search_patterns("error_handler", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, "error_handling");

        // Search by description
        let results = storage.search_patterns("database", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, "architecture");

        // Search by category
        let results = storage.search_patterns("error_handling", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Search with no matches
        let results = storage.search_patterns("nonexistent_xyz", 10).unwrap();
        assert!(results.is_empty());

        // Search returning multiple
        let results = storage.search_patterns("rust", 10).unwrap();
        assert_eq!(results.len(), 0); // "rust" is in the language field, not in the searched columns
    }

    #[test]
    fn test_learning_log() {
        let storage = LearningStorage::in_memory().unwrap();

        let entry1 = LearningLogEntry {
            id: 0,
            event_type: "routing_adjustment".to_string(),
            description: "Upgraded code_review from standard to premium".to_string(),
            details: "{\"task_type\":\"code_review\"}".to_string(),
            reversible: true,
            timestamp: "2026-02-10T12:00:00Z".to_string(),
        };
        let entry2 = LearningLogEntry {
            id: 0,
            event_type: "preference_learned".to_string(),
            description: "User prefers dark theme".to_string(),
            details: "{\"key\":\"theme\",\"value\":\"dark\"}".to_string(),
            reversible: true,
            timestamp: "2026-02-10T13:00:00Z".to_string(),
        };

        let id1 = storage.log_learning(&entry1).unwrap();
        let id2 = storage.log_learning(&entry2).unwrap();
        assert!(id1 > 0);
        assert!(id2 > 0);

        let log = storage.get_learning_log(10).unwrap();
        assert_eq!(log.len(), 2);
        // Most recent first
        assert_eq!(log[0].event_type, "preference_learned");
        assert_eq!(log[1].event_type, "routing_adjustment");
        assert!(log[0].reversible);
    }

    #[test]
    fn test_learning_log_respects_limit() {
        let storage = LearningStorage::in_memory().unwrap();

        for i in 0..5 {
            let entry = LearningLogEntry {
                id: 0,
                event_type: "test".to_string(),
                description: format!("Entry {i}"),
                details: String::new(),
                reversible: false,
                timestamp: format!("2026-02-10T{:02}:00:00Z", 10 + i),
            };
            storage.log_learning(&entry).unwrap();
        }

        let log = storage.get_learning_log(3).unwrap();
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn test_model_quality_calculation() {
        let storage = LearningStorage::in_memory().unwrap();

        // Insert three outcomes with different quality scores
        for quality in [0.6, 0.8, 1.0] {
            storage
                .record_outcome(&make_outcome("test-model", quality, Outcome::Accepted))
                .unwrap();
        }

        let avg = storage.model_quality("test-model", 30).unwrap();
        assert!((avg - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_model_quality_no_data() {
        let storage = LearningStorage::in_memory().unwrap();
        let avg = storage.model_quality("nonexistent", 30).unwrap();
        assert!((avg - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_task_tier_quality() {
        let storage = LearningStorage::in_memory().unwrap();

        let mut record = make_outcome("gpt-4o", 0.7, Outcome::Accepted);
        record.task_type = "debugging".to_string();
        record.tier = "standard".to_string();
        storage.record_outcome(&record).unwrap();

        let mut record2 = make_outcome("gpt-4o", 0.9, Outcome::Accepted);
        record2.task_type = "debugging".to_string();
        record2.tier = "standard".to_string();
        storage.record_outcome(&record2).unwrap();

        let avg = storage
            .task_tier_quality("debugging", "standard", 30)
            .unwrap();
        assert!((avg - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_task_tier_quality_no_data() {
        let storage = LearningStorage::in_memory().unwrap();
        let avg = storage.task_tier_quality("unknown", "unknown", 30).unwrap();
        assert!((avg - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_outcome_count() {
        let storage = LearningStorage::in_memory().unwrap();
        assert_eq!(storage.outcome_count().unwrap(), 0);

        storage
            .record_outcome(&make_outcome("m1", 0.8, Outcome::Accepted))
            .unwrap();
        storage
            .record_outcome(&make_outcome("m2", 0.7, Outcome::Corrected))
            .unwrap();
        assert_eq!(storage.outcome_count().unwrap(), 2);
    }

    #[test]
    fn test_outcome_with_none_persona() {
        let storage = LearningStorage::in_memory().unwrap();
        let mut record = make_outcome("gpt-4o", 0.8, Outcome::Accepted);
        record.persona = None;
        storage.record_outcome(&record).unwrap();

        let outcomes = storage.get_outcomes(Some("gpt-4o"), 30, 10).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].persona, None);
    }

    #[test]
    fn test_routing_entry_with_actual_tier() {
        let storage = LearningStorage::in_memory().unwrap();
        let mut entry = make_routing_entry("code_review", "standard", 0.5);
        entry.actual_tier_needed = Some("premium".to_string());
        storage.record_routing(&entry).unwrap();

        let history = storage
            .get_routing_history(Some("code_review"), 10)
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].actual_tier_needed, Some("premium".to_string()));
    }

    #[test]
    fn test_empty_queries_return_defaults() {
        let storage = LearningStorage::in_memory().unwrap();

        // Empty outcomes
        let outcomes = storage.get_outcomes(None, 30, 100).unwrap();
        assert!(outcomes.is_empty());

        // Empty routing history
        let history = storage.get_routing_history(None, 100).unwrap();
        assert!(history.is_empty());

        // Empty preferences
        let prefs = storage.all_preferences().unwrap();
        assert!(prefs.is_empty());

        // Empty prompt versions
        let versions = storage.get_prompt_versions("any").unwrap();
        assert!(versions.is_empty());

        // No active prompt
        assert!(storage.get_active_prompt("any").unwrap().is_none());

        // Empty pattern search
        let patterns = storage.search_patterns("anything", 10).unwrap();
        assert!(patterns.is_empty());

        // Empty learning log
        let log = storage.get_learning_log(10).unwrap();
        assert!(log.is_empty());

        // Zero model quality
        assert!((storage.model_quality("any", 30).unwrap() - 0.0).abs() < f64::EPSILON);

        // Zero task-tier quality
        assert!((storage.task_tier_quality("any", "any", 30).unwrap() - 0.0).abs() < f64::EPSILON);

        // Zero outcome count
        assert_eq!(storage.outcome_count().unwrap(), 0);
    }
}
