use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::HiveConfig;

/// Session state for save/restore on crash recovery.
///
/// Persisted to `~/.hive/session.json`. On startup the workspace loads this
/// to resume where the user left off (conversation, panel, window size).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SessionState {
    pub active_conversation_id: Option<String>,
    pub active_panel: String,
    pub window_size: Option<[u32; 2]>,
    pub working_directory: Option<String>,
    pub open_files: Vec<String>,
    pub chat_draft: Option<String>,
}

impl SessionState {
    fn session_path() -> Result<PathBuf> {
        Ok(HiveConfig::base_dir()?.join("session.json"))
    }

    /// Persist session state to `~/.hive/session.json`.
    pub fn save(&self) -> Result<()> {
        let path = Self::session_path()?;
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to save session: {}", path.display()))?;
        Ok(())
    }

    /// Load session state from disk. Returns `Default` if the file is missing
    /// or corrupt (never errors on bad JSON).
    pub fn load() -> Result<Self> {
        let path = Self::session_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read session: {}", path.display()))?;
            let state: Self = serde_json::from_str(&content).unwrap_or_default();
            Ok(state)
        } else {
            Ok(Self::default())
        }
    }

    /// Load session from an explicit path (for testing without `~/.hive/`).
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read session: {}", path.display()))?;
            let state: Self = serde_json::from_str(&content).unwrap_or_default();
            Ok(state)
        } else {
            Ok(Self::default())
        }
    }

    /// Save session to an explicit path (for testing without `~/.hive/`).
    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to save session: {}", path.display()))?;
        Ok(())
    }

    /// Delete the session file.
    pub fn clear() -> Result<()> {
        let path = Self::session_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    // -- Convenience helpers ------------------------------------------------

    /// Quick save: persist only the last conversation ID (plus panel) without
    /// touching other fields. Reads the existing session first to preserve
    /// unrelated state.
    pub fn save_last_conversation_id(id: &str) -> Result<()> {
        let mut state = Self::load().unwrap_or_default();
        state.active_conversation_id = Some(id.to_string());
        state.save()
    }

    /// Quick load: return just the last conversation ID, or `None` if no
    /// session exists or no conversation was active.
    pub fn load_last_conversation_id() -> Option<String> {
        Self::load().ok().and_then(|s| s.active_conversation_id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn session_path_in(dir: &TempDir) -> PathBuf {
        dir.path().join("session.json")
    }

    #[test]
    fn test_save_and_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let path = session_path_in(&tmp);

        let state = SessionState {
            active_conversation_id: Some("abc-123".into()),
            active_panel: "Files".into(),
            window_size: Some([1920, 1080]),
            working_directory: Some("/home/user/project".into()),
            open_files: vec!["main.rs".into(), "lib.rs".into()],
            chat_draft: Some("half-typed message".into()),
        };

        state.save_to(&path).unwrap();
        let loaded = SessionState::load_from(&path).unwrap();

        assert_eq!(loaded.active_conversation_id.as_deref(), Some("abc-123"));
        assert_eq!(loaded.active_panel, "Files");
        assert_eq!(loaded.window_size, Some([1920, 1080]));
        assert_eq!(
            loaded.working_directory.as_deref(),
            Some("/home/user/project")
        );
        assert_eq!(loaded.open_files, vec!["main.rs", "lib.rs"]);
        assert_eq!(loaded.chat_draft.as_deref(), Some("half-typed message"));
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let tmp = TempDir::new().unwrap();
        let path = session_path_in(&tmp);

        let loaded = SessionState::load_from(&path).unwrap();

        assert!(loaded.active_conversation_id.is_none());
        assert!(loaded.active_panel.is_empty());
        assert!(loaded.window_size.is_none());
        assert!(loaded.open_files.is_empty());
        assert!(loaded.chat_draft.is_none());
    }

    #[test]
    fn test_load_corrupt_json_returns_default() {
        let tmp = TempDir::new().unwrap();
        let path = session_path_in(&tmp);

        std::fs::write(&path, "NOT VALID JSON {{{{").unwrap();

        let loaded = SessionState::load_from(&path).unwrap();

        assert!(loaded.active_conversation_id.is_none());
        assert!(loaded.active_panel.is_empty());
    }

    #[test]
    fn test_load_partial_json_fills_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = session_path_in(&tmp);

        // Only some fields present; others should get default values.
        std::fs::write(
            &path,
            r#"{ "active_panel": "Settings", "window_size": [800, 600] }"#,
        )
        .unwrap();

        let loaded = SessionState::load_from(&path).unwrap();

        assert!(loaded.active_conversation_id.is_none());
        assert_eq!(loaded.active_panel, "Settings");
        assert_eq!(loaded.window_size, Some([800, 600]));
        assert!(loaded.open_files.is_empty());
        assert!(loaded.chat_draft.is_none());
    }

    #[test]
    fn test_save_overwrites_previous_session() {
        let tmp = TempDir::new().unwrap();
        let path = session_path_in(&tmp);

        let state1 = SessionState {
            active_conversation_id: Some("first".into()),
            active_panel: "Chat".into(),
            ..Default::default()
        };
        state1.save_to(&path).unwrap();

        let state2 = SessionState {
            active_conversation_id: Some("second".into()),
            active_panel: "Monitor".into(),
            ..Default::default()
        };
        state2.save_to(&path).unwrap();

        let loaded = SessionState::load_from(&path).unwrap();
        assert_eq!(loaded.active_conversation_id.as_deref(), Some("second"));
        assert_eq!(loaded.active_panel, "Monitor");
    }

    #[test]
    fn test_conversation_id_none_when_empty_session() {
        let tmp = TempDir::new().unwrap();
        let path = session_path_in(&tmp);

        let state = SessionState::default();
        state.save_to(&path).unwrap();

        let loaded = SessionState::load_from(&path).unwrap();
        assert!(loaded.active_conversation_id.is_none());
    }

    #[test]
    fn test_window_size_persistence() {
        let tmp = TempDir::new().unwrap();
        let path = session_path_in(&tmp);

        let state = SessionState {
            window_size: Some([1280, 800]),
            ..Default::default()
        };
        state.save_to(&path).unwrap();

        let loaded = SessionState::load_from(&path).unwrap();
        assert_eq!(loaded.window_size, Some([1280, 800]));
    }
}
