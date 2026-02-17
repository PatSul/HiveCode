use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::warn;

use crate::config::HiveConfig;

// ---------------------------------------------------------------------------
// Data types — JSON-compatible with the Electron reference (main.ts)
// Field names use camelCase in JSON to match the Electron format.
// ---------------------------------------------------------------------------

/// A single message stored inside a conversation file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "tokenCount")]
    pub tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

/// Full conversation persisted as `{id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<StoredMessage>,
    pub model: String,
    pub total_cost: f64,
    pub total_tokens: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Lightweight structs for partial deserialization (perf: avoids parsing
// full message content just to build summaries or run searches).
// Unknown JSON fields are silently ignored via serde defaults.
// ---------------------------------------------------------------------------

/// Minimal message representation — skips `thinking`, heavy optional fields.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageMeta {
    #[allow(dead_code)]
    role: String,
    content: String,
}

/// Lightweight conversation header for list/search — skips full StoredMessage
/// deserialization (timestamps, model, cost, tokens, thinking per message).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationMeta {
    id: String,
    title: String,
    messages: Vec<MessageMeta>,
    model: String,
    #[serde(default)]
    total_cost: f64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Lightweight summary returned by `list_summaries` / `search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    /// First ~100 characters of the last message.
    pub preview: String,
    pub message_count: usize,
    pub total_cost: f64,
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Title generation
// ---------------------------------------------------------------------------

/// Generates a title from the first user message, truncated to 50 chars.
pub fn generate_title(messages: &[StoredMessage]) -> String {
    let first_user = messages.iter().find(|m| m.role == "user");
    match first_user {
        Some(msg) => {
            let trimmed = msg.content.trim();
            if trimmed.len() <= 50 {
                trimmed.to_string()
            } else {
                // Find a safe char boundary at or before 50
                let boundary = trimmed
                    .char_indices()
                    .take_while(|(i, _)| *i < 50)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(50);
                format!("{}...", &trimmed[..boundary])
            }
        }
        None => "New Conversation".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Preview helper
// ---------------------------------------------------------------------------

fn make_preview(content: &str, max_len: usize) -> String {
    let trimmed = content.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        let boundary = trimmed
            .char_indices()
            .take_while(|(i, _)| *i < max_len)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(max_len);
        format!("{}...", &trimmed[..boundary])
    }
}

// ---------------------------------------------------------------------------
// ConversationStore
// ---------------------------------------------------------------------------

/// File-based conversation store. Each conversation is a JSON file in
/// `~/.hive/conversations/{id}.json`.
pub struct ConversationStore {
    dir: PathBuf,
}

impl ConversationStore {
    /// Creates a new store backed by `HiveConfig::conversations_dir()`.
    pub fn new() -> Result<Self> {
        let dir = HiveConfig::conversations_dir()?;
        if !dir.exists() {
            std::fs::create_dir_all(&dir).with_context(|| {
                format!("Failed to create conversations dir: {}", dir.display())
            })?;
        }
        Ok(Self { dir })
    }

    /// Creates a store rooted at an arbitrary directory (useful for tests).
    pub fn new_at(dir: PathBuf) -> Result<Self> {
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create dir: {}", dir.display()))?;
        }
        Ok(Self { dir })
    }

    /// Returns the file path for a given conversation ID.
    /// IDs are sanitised to prevent path traversal.
    fn path_for(&self, id: &str) -> Result<PathBuf> {
        let safe_id: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if safe_id.is_empty() {
            anyhow::bail!("Invalid conversation ID");
        }
        let resolved = self.dir.join(format!("{safe_id}.json"));
        // Double-check it is still inside the conversations dir
        let canonical_dir = std::fs::canonicalize(&self.dir).unwrap_or_else(|_| self.dir.clone());
        let canonical_file = if resolved.exists() {
            std::fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone())
        } else {
            // For new files, resolve the parent then append filename
            let parent = std::fs::canonicalize(resolved.parent().unwrap_or(&self.dir))
                .unwrap_or_else(|_| self.dir.clone());
            parent.join(format!("{safe_id}.json"))
        };
        if !canonical_file.starts_with(&canonical_dir) {
            anyhow::bail!("Invalid conversation ID (path traversal)");
        }
        Ok(resolved)
    }

    /// Saves (creates or overwrites) a conversation to disk.
    pub fn save(&self, conversation: &Conversation) -> Result<()> {
        let path = self.path_for(&conversation.id)?;
        let json = serde_json::to_string_pretty(conversation)
            .context("Failed to serialize conversation")?;
        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write conversation: {}", path.display()))?;
        Ok(())
    }

    /// Loads a single conversation by ID.
    pub fn load(&self, id: &str) -> Result<Conversation> {
        let path = self.path_for(id)?;
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Conversation not found: {}", path.display()))?;
        let conv: Conversation = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse conversation: {}", path.display()))?;
        Ok(conv)
    }

    /// Deletes a conversation file.
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.path_for(id)?;
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to delete conversation: {}", path.display()))?;
        }
        Ok(())
    }

    /// Lists summaries of all conversations, sorted by `updated_at` descending.
    pub fn list_summaries(&self) -> Result<Vec<ConversationSummary>> {
        let mut summaries = Vec::new();

        let entries = std::fs::read_dir(&self.dir)
            .with_context(|| format!("Failed to read conversations dir: {}", self.dir.display()))?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Skipping unreadable dir entry: {e}");
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            match self.load_summary_from_path(&path) {
                Ok(summary) => summaries.push(summary),
                Err(e) => {
                    warn!("Skipping corrupt conversation file {}: {e}", path.display());
                    continue;
                }
            }
        }

        // Sort newest first
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    /// Case-insensitive search across title and message content.
    ///
    /// For large conversation stores, prefer `Database::search_conversations`
    /// which uses an FTS5 full-text index.  This file-scanning fallback
    /// remains for offline / DB-less operation.
    pub fn search(&self, query: &str) -> Result<Vec<ConversationSummary>> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        let entries = std::fs::read_dir(&self.dir)
            .with_context(|| format!("Failed to read conversations dir: {}", self.dir.display()))?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let meta: ConversationMeta = match serde_json::from_str(&content) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Short-circuit: check title first, then stop at the first matching message.
            let matched = meta.title.to_lowercase().contains(&query_lower)
                || meta
                    .messages
                    .iter()
                    .any(|m| m.content.to_lowercase().contains(&query_lower));

            if matched {
                let last_msg = meta.messages.last();
                let preview = last_msg
                    .map(|m| make_preview(&m.content, 100))
                    .unwrap_or_default();

                results.push(ConversationSummary {
                    id: meta.id,
                    title: meta.title,
                    preview,
                    message_count: meta.messages.len(),
                    total_cost: meta.total_cost,
                    model: meta.model,
                    created_at: meta.created_at,
                    updated_at: meta.updated_at,
                });
            }
        }

        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn load_summary_from_path(&self, path: &std::path::Path) -> Result<ConversationSummary> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let meta: ConversationMeta = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        let last_msg = meta.messages.last();
        let preview = last_msg
            .map(|m| make_preview(&m.content, 100))
            .unwrap_or_default();

        Ok(ConversationSummary {
            id: meta.id,
            title: meta.title,
            preview,
            message_count: meta.messages.len(),
            total_cost: meta.total_cost,
            model: meta.model,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        })
    }
}

// ---------------------------------------------------------------------------
// Convenience: create a new Conversation with a fresh UUID
// ---------------------------------------------------------------------------

impl Conversation {
    /// Creates a new empty conversation with a fresh UUID.
    pub fn new(model: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: "New Conversation".to_string(),
            messages: Vec::new(),
            model: model.into(),
            total_cost: 0.0,
            total_tokens: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Appends a message and updates the title / timestamps.
    pub fn add_message(&mut self, msg: StoredMessage) {
        if let Some(cost) = msg.cost {
            self.total_cost += cost;
        }
        if let Some(tokens) = msg.tokens {
            self.total_tokens += tokens;
        }
        self.messages.push(msg);
        self.title = generate_title(&self.messages);
        self.updated_at = Utc::now();
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a store in a temp directory.
    fn temp_store() -> (ConversationStore, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("Failed to create tempdir");
        let store =
            ConversationStore::new_at(tmp.path().to_path_buf()).expect("Failed to create store");
        (store, tmp)
    }

    /// Helper: build a minimal conversation.
    fn make_conversation(
        id: &str,
        title: &str,
        user_msg: &str,
        updated_at: DateTime<Utc>,
    ) -> Conversation {
        Conversation {
            id: id.to_string(),
            title: title.to_string(),
            messages: vec![StoredMessage {
                role: "user".into(),
                content: user_msg.into(),
                timestamp: Utc::now(),
                model: None,
                cost: Some(0.01),
                tokens: Some(100),
                thinking: None,
            }],
            model: "test-model".into(),
            total_cost: 0.01,
            total_tokens: 100,
            created_at: Utc::now() - chrono::Duration::hours(1),
            updated_at,
        }
    }

    // -----------------------------------------------------------------------
    // save / load round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_load_round_trip() {
        let (store, _tmp) = temp_store();
        let conv = make_conversation("conv-001", "Hello", "Hello world", Utc::now());

        store.save(&conv).expect("save failed");
        let loaded = store.load("conv-001").expect("load failed");

        assert_eq!(loaded.id, conv.id);
        assert_eq!(loaded.title, conv.title);
        assert_eq!(loaded.messages.len(), conv.messages.len());
        assert_eq!(loaded.messages[0].role, "user");
        assert_eq!(loaded.messages[0].content, "Hello world");
        assert_eq!(loaded.model, "test-model");
        assert!((loaded.total_cost - 0.01).abs() < f64::EPSILON);
        assert_eq!(loaded.total_tokens, 100);
    }

    // -----------------------------------------------------------------------
    // list_summaries: 3 conversations, sorted by updated_at desc
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_summaries_sorted() {
        let (store, _tmp) = temp_store();

        let oldest = make_conversation(
            "conv-old",
            "Old",
            "Old message",
            Utc::now() - chrono::Duration::hours(3),
        );
        let middle = make_conversation(
            "conv-mid",
            "Middle",
            "Middle message",
            Utc::now() - chrono::Duration::hours(1),
        );
        let newest = make_conversation("conv-new", "Newest", "New message", Utc::now());

        // Save in random order
        store.save(&middle).unwrap();
        store.save(&oldest).unwrap();
        store.save(&newest).unwrap();

        let summaries = store.list_summaries().expect("list failed");
        assert_eq!(summaries.len(), 3);
        assert_eq!(summaries[0].id, "conv-new");
        assert_eq!(summaries[1].id, "conv-mid");
        assert_eq!(summaries[2].id, "conv-old");

        // Verify preview field
        assert_eq!(summaries[0].preview, "New message");
        assert_eq!(summaries[0].message_count, 1);
    }

    // -----------------------------------------------------------------------
    // delete
    // -----------------------------------------------------------------------

    #[test]
    fn test_delete() {
        let (store, _tmp) = temp_store();
        let conv = make_conversation("conv-del", "Delete me", "bye", Utc::now());
        store.save(&conv).unwrap();

        assert!(store.load("conv-del").is_ok());
        store.delete("conv-del").unwrap();
        assert!(store.load("conv-del").is_err());

        // Deleting again should not fail
        store.delete("conv-del").unwrap();
    }

    // -----------------------------------------------------------------------
    // search (case-insensitive)
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_case_insensitive() {
        let (store, _tmp) = temp_store();

        let conv1 = Conversation {
            id: "s1".into(),
            title: "Rust Programming".into(),
            messages: vec![StoredMessage {
                role: "user".into(),
                content: "How do I use cargo?".into(),
                timestamp: Utc::now(),
                model: None,
                cost: None,
                tokens: None,
                thinking: None,
            }],
            model: "gpt-4".into(),
            total_cost: 0.0,
            total_tokens: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let conv2 = Conversation {
            id: "s2".into(),
            title: "Cooking Tips".into(),
            messages: vec![StoredMessage {
                role: "user".into(),
                content: "Best pasta recipe?".into(),
                timestamp: Utc::now(),
                model: None,
                cost: None,
                tokens: None,
                thinking: None,
            }],
            model: "claude".into(),
            total_cost: 0.0,
            total_tokens: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let conv3 = Conversation {
            id: "s3".into(),
            title: "More cooking".into(),
            messages: vec![StoredMessage {
                role: "user".into(),
                content: "Tell me about RUST safety features".into(),
                timestamp: Utc::now(),
                model: None,
                cost: None,
                tokens: None,
                thinking: None,
            }],
            model: "claude".into(),
            total_cost: 0.0,
            total_tokens: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.save(&conv1).unwrap();
        store.save(&conv2).unwrap();
        store.save(&conv3).unwrap();

        // Search "rust" should match conv1 (title) and conv3 (message content "RUST")
        let results = store.search("rust").unwrap();
        assert_eq!(results.len(), 2);
        let ids: Vec<&str> = results.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"s1"));
        assert!(ids.contains(&"s3"));

        // Search "pasta" should match conv2 only
        let results = store.search("PASTA").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "s2");

        // Search "nonexistent" should return empty
        let results = store.search("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // generate_title
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_title_short() {
        let msgs = vec![StoredMessage {
            role: "user".into(),
            content: "Hello".into(),
            timestamp: Utc::now(),
            model: None,
            cost: None,
            tokens: None,
            thinking: None,
        }];
        assert_eq!(generate_title(&msgs), "Hello");
    }

    #[test]
    fn test_generate_title_truncated() {
        let long = "a".repeat(80);
        let msgs = vec![StoredMessage {
            role: "user".into(),
            content: long,
            timestamp: Utc::now(),
            model: None,
            cost: None,
            tokens: None,
            thinking: None,
        }];
        let title = generate_title(&msgs);
        assert!(title.ends_with("..."));
        // The part before "..." should be at most 50 chars
        let prefix = title.trim_end_matches("...");
        assert!(prefix.len() <= 50);
    }

    #[test]
    fn test_generate_title_no_user_message() {
        let msgs = vec![StoredMessage {
            role: "assistant".into(),
            content: "I am the assistant".into(),
            timestamp: Utc::now(),
            model: None,
            cost: None,
            tokens: None,
            thinking: None,
        }];
        assert_eq!(generate_title(&msgs), "New Conversation");
    }

    #[test]
    fn test_generate_title_empty() {
        assert_eq!(generate_title(&[]), "New Conversation");
    }

    // -----------------------------------------------------------------------
    // Corrupt JSON file handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_corrupt_file_skipped_in_list() {
        let (store, _tmp) = temp_store();

        // Save a valid conversation
        let conv = make_conversation("valid", "Valid", "Hello", Utc::now());
        store.save(&conv).unwrap();

        // Write a corrupt JSON file
        let corrupt_path = store.dir.join("corrupt.json");
        fs::write(&corrupt_path, "{ not valid json !!!").unwrap();

        // list_summaries should return only the valid one
        let summaries = store.list_summaries().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "valid");
    }

    #[test]
    fn test_corrupt_file_skipped_in_search() {
        let (store, _tmp) = temp_store();

        let conv = make_conversation("good", "Good", "findme", Utc::now());
        store.save(&conv).unwrap();

        let corrupt_path = store.dir.join("bad.json");
        fs::write(&corrupt_path, "garbage").unwrap();

        let results = store.search("findme").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "good");
    }

    // -----------------------------------------------------------------------
    // Empty conversations directory
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_dir() {
        let (store, _tmp) = temp_store();
        let summaries = store.list_summaries().unwrap();
        assert!(summaries.is_empty());

        let results = store.search("anything").unwrap();
        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // Conversation::new and add_message
    // -----------------------------------------------------------------------

    #[test]
    fn test_conversation_new_and_add_message() {
        let mut conv = Conversation::new("claude-sonnet");
        assert_eq!(conv.title, "New Conversation");
        assert!(conv.messages.is_empty());
        assert!(!conv.id.is_empty());

        conv.add_message(StoredMessage {
            role: "user".into(),
            content: "Write a poem".into(),
            timestamp: Utc::now(),
            model: None,
            cost: Some(0.005),
            tokens: Some(50),
            thinking: None,
        });

        assert_eq!(conv.messages.len(), 1);
        assert_eq!(conv.title, "Write a poem");
        assert!((conv.total_cost - 0.005).abs() < f64::EPSILON);
        assert_eq!(conv.total_tokens, 50);
    }

    // -----------------------------------------------------------------------
    // Preview truncation
    // -----------------------------------------------------------------------

    #[test]
    fn test_preview_truncation() {
        let (store, _tmp) = temp_store();
        let long_content = "x".repeat(200);
        let conv = Conversation {
            id: "prev".into(),
            title: "Preview test".into(),
            messages: vec![StoredMessage {
                role: "user".into(),
                content: long_content,
                timestamp: Utc::now(),
                model: None,
                cost: None,
                tokens: None,
                thinking: None,
            }],
            model: "test".into(),
            total_cost: 0.0,
            total_tokens: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.save(&conv).unwrap();

        let summaries = store.list_summaries().unwrap();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].preview.ends_with("..."));
        let prefix = summaries[0].preview.trim_end_matches("...");
        assert!(prefix.len() <= 100);
    }

    // -----------------------------------------------------------------------
    // Path traversal prevention
    // -----------------------------------------------------------------------

    #[test]
    fn test_path_traversal_blocked() {
        let (store, _tmp) = temp_store();
        assert!(store.load("../../../etc/passwd").is_err());
        assert!(store.load("").is_err());
    }
}
