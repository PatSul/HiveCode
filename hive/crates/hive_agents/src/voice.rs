//! Voice Assistant — process voice commands and manage wake word detection.
//!
//! Mirrors the Electron app's `voice-assistant.ts`, `voice-command-router.ts`,
//! and `wake-word-service.ts` features: state management, intent classification
//! via keyword matching, wake word detection, and command history tracking.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

use hive_ai::tts::service::TtsService;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Current operational state of the voice assistant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceState {
    Idle,
    Listening,
    Processing,
    Speaking,
    Error,
}

/// Classified intent of a voice command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceIntent {
    SendMessage,
    SearchFiles,
    RunCommand,
    OpenPanel,
    CreateTask,
    ReadNotifications,
    CheckSchedule,
    Unknown,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// A parsed voice command with classified intent and confidence score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCommand {
    pub id: String,
    pub text: String,
    pub intent: VoiceIntent,
    pub confidence: f32,
    pub timestamp: DateTime<Utc>,
}

/// Configuration for wake word detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordConfig {
    pub wake_words: Vec<String>,
    pub sensitivity: f32,
    pub enabled: bool,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            wake_words: vec!["hey hive".to_string(), "ok hive".to_string()],
            sensitivity: 0.5,
            enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// VoiceAssistant
// ---------------------------------------------------------------------------

/// In-memory voice assistant with intent classification and wake word support.
///
/// Processes text input into classified [`VoiceCommand`]s using keyword
/// matching, manages wake word configuration, and maintains a command history.
pub struct VoiceAssistant {
    state: VoiceState,
    wake_word_config: WakeWordConfig,
    command_history: Vec<VoiceCommand>,
    tts: Option<Arc<TtsService>>,
}

impl VoiceAssistant {
    /// Create a new voice assistant with default wake words ("hey hive", "ok hive").
    pub fn new() -> Self {
        Self {
            state: VoiceState::Idle,
            wake_word_config: WakeWordConfig::default(),
            command_history: Vec::new(),
            tts: None,
        }
    }

    /// Wire a TTS service into the voice assistant for audio output.
    pub fn set_tts(&mut self, tts: Arc<TtsService>) {
        self.tts = Some(tts);
    }

    /// Speak the given text using the configured TTS service.
    ///
    /// Transitions to [`VoiceState::Speaking`] while synthesising, and back
    /// to [`VoiceState::Idle`] upon completion (or [`VoiceState::Error`] on failure).
    pub async fn speak(&mut self, text: &str) -> Result<Vec<u8>, String> {
        let tts = self
            .tts
            .clone()
            .ok_or_else(|| "TTS service not configured".to_string())?;

        self.set_state(VoiceState::Speaking);

        match tts.speak(text).await {
            Ok(audio) => {
                self.set_state(VoiceState::Idle);
                Ok(audio.bytes)
            }
            Err(e) => {
                self.set_state(VoiceState::Error);
                Err(e.to_string())
            }
        }
    }

    /// Update the current voice assistant state.
    pub fn set_state(&mut self, state: VoiceState) {
        debug!(?state, "Voice assistant state changed");
        self.state = state;
    }

    /// Return the current voice assistant state.
    pub fn state(&self) -> VoiceState {
        self.state
    }

    /// Process raw text input into a classified [`VoiceCommand`].
    ///
    /// The text is analysed against keyword patterns to determine the most
    /// likely [`VoiceIntent`]. The resulting command is appended to the
    /// internal history and returned.
    pub fn process_text(&mut self, text: &str) -> VoiceCommand {
        let intent = Self::classify_intent(text);
        let confidence = if intent == VoiceIntent::Unknown {
            0.0
        } else {
            Self::compute_confidence(text, intent)
        };

        let command = VoiceCommand {
            id: Uuid::new_v4().to_string(),
            text: text.to_string(),
            intent,
            confidence,
            timestamp: Utc::now(),
        };

        debug!(
            id = %command.id,
            intent = ?command.intent,
            confidence = command.confidence,
            "Processed voice command"
        );

        self.command_history.push(command.clone());
        command
    }

    /// Add a new wake word to the configuration.
    pub fn add_wake_word(&mut self, word: impl Into<String>) {
        let word = word.into().to_lowercase();
        if !self.wake_word_config.wake_words.contains(&word) {
            debug!(%word, "Added wake word");
            self.wake_word_config.wake_words.push(word);
        }
    }

    /// Remove a wake word from the configuration. Returns `true` if the word
    /// was present and removed.
    pub fn remove_wake_word(&mut self, word: &str) -> bool {
        let word_lower = word.to_lowercase();
        let before = self.wake_word_config.wake_words.len();
        self.wake_word_config
            .wake_words
            .retain(|w| w != &word_lower);
        let removed = self.wake_word_config.wake_words.len() < before;
        if removed {
            debug!(word, "Removed wake word");
        }
        removed
    }

    /// Check whether the given text matches any configured wake word
    /// (case-insensitive).
    pub fn is_wake_word(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase().trim().to_string();
        self.wake_word_config
            .wake_words
            .iter()
            .any(|w| text_lower == *w)
    }

    /// Return the full command history.
    pub fn command_history(&self) -> &[VoiceCommand] {
        &self.command_history
    }

    /// Clear the command history.
    pub fn clear_history(&mut self) {
        debug!(
            count = self.command_history.len(),
            "Clearing voice command history"
        );
        self.command_history.clear();
    }

    /// Return a reference to the current wake word configuration.
    pub fn wake_word_config(&self) -> &WakeWordConfig {
        &self.wake_word_config
    }

    // -- private helpers ----------------------------------------------------

    /// Classify text into a [`VoiceIntent`] using keyword matching.
    fn classify_intent(text: &str) -> VoiceIntent {
        let lower = text.to_lowercase();

        // Order matters: more specific patterns first.
        if lower.contains("send")
            || lower.contains("message")
            || lower.contains("email")
        {
            return VoiceIntent::SendMessage;
        }

        if lower.contains("search")
            || lower.contains("find")
            || lower.contains("look for")
        {
            return VoiceIntent::SearchFiles;
        }

        if lower.contains("run")
            || lower.contains("execute")
            || lower.contains("terminal")
        {
            return VoiceIntent::RunCommand;
        }

        if lower.contains("open")
            || lower.contains("show")
            || lower.contains("switch to")
        {
            return VoiceIntent::OpenPanel;
        }

        if lower.contains("create")
            || lower.contains("add")
            || lower.contains("new task")
        {
            return VoiceIntent::CreateTask;
        }

        if lower.contains("read")
            || lower.contains("notifications")
            || lower.contains("alerts")
        {
            return VoiceIntent::ReadNotifications;
        }

        if lower.contains("schedule")
            || lower.contains("calendar")
            || lower.contains("meeting")
        {
            return VoiceIntent::CheckSchedule;
        }

        VoiceIntent::Unknown
    }

    /// Compute a confidence score based on how many matching keywords appear.
    fn compute_confidence(text: &str, intent: VoiceIntent) -> f32 {
        let lower = text.to_lowercase();
        let keywords: &[&str] = match intent {
            VoiceIntent::SendMessage => &["send", "message", "email"],
            VoiceIntent::SearchFiles => &["search", "find", "look for"],
            VoiceIntent::RunCommand => &["run", "execute", "terminal"],
            VoiceIntent::OpenPanel => &["open", "show", "switch to"],
            VoiceIntent::CreateTask => &["create", "add", "new task"],
            VoiceIntent::ReadNotifications => &["read", "notifications", "alerts"],
            VoiceIntent::CheckSchedule => &["schedule", "calendar", "meeting"],
            VoiceIntent::Unknown => return 0.0,
        };

        let matched = keywords
            .iter()
            .filter(|kw| lower.contains(**kw))
            .count();

        // Base confidence 0.6 for one match, up to 1.0 for all three.
        let score = 0.6 + (matched as f32 - 1.0) * 0.2;
        score.clamp(0.0, 1.0)
    }
}

impl Default for VoiceAssistant {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- construction -------------------------------------------------------

    #[test]
    fn new_assistant_starts_idle() {
        let va = VoiceAssistant::new();
        assert_eq!(va.state(), VoiceState::Idle);
        assert!(va.command_history().is_empty());
    }

    #[test]
    fn new_assistant_has_default_wake_words() {
        let va = VoiceAssistant::new();
        let cfg = va.wake_word_config();
        assert_eq!(cfg.wake_words.len(), 2);
        assert!(cfg.wake_words.contains(&"hey hive".to_string()));
        assert!(cfg.wake_words.contains(&"ok hive".to_string()));
        assert!(cfg.enabled);
        assert!((cfg.sensitivity - 0.5).abs() < f32::EPSILON);
    }

    // -- state management ---------------------------------------------------

    #[test]
    fn set_and_get_state() {
        let mut va = VoiceAssistant::new();

        va.set_state(VoiceState::Listening);
        assert_eq!(va.state(), VoiceState::Listening);

        va.set_state(VoiceState::Processing);
        assert_eq!(va.state(), VoiceState::Processing);

        va.set_state(VoiceState::Speaking);
        assert_eq!(va.state(), VoiceState::Speaking);

        va.set_state(VoiceState::Error);
        assert_eq!(va.state(), VoiceState::Error);

        va.set_state(VoiceState::Idle);
        assert_eq!(va.state(), VoiceState::Idle);
    }

    // -- intent classification ----------------------------------------------

    #[test]
    fn classify_send_message() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("send a message to John");
        assert_eq!(cmd.intent, VoiceIntent::SendMessage);
        assert!(cmd.confidence > 0.0);
    }

    #[test]
    fn classify_search_files() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("search for the config file");
        assert_eq!(cmd.intent, VoiceIntent::SearchFiles);
        assert!(cmd.confidence >= 0.6);
    }

    #[test]
    fn classify_run_command() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("run the build script");
        assert_eq!(cmd.intent, VoiceIntent::RunCommand);
        assert!(cmd.confidence >= 0.6);
    }

    #[test]
    fn classify_open_panel() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("open the settings panel");
        assert_eq!(cmd.intent, VoiceIntent::OpenPanel);
        assert!(cmd.confidence >= 0.6);
    }

    #[test]
    fn classify_create_task() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("create a new task for the sprint");
        assert_eq!(cmd.intent, VoiceIntent::CreateTask);
        assert!(cmd.confidence >= 0.6);
    }

    #[test]
    fn classify_read_notifications() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("read my notifications");
        assert_eq!(cmd.intent, VoiceIntent::ReadNotifications);
        assert!(cmd.confidence >= 0.6);
    }

    #[test]
    fn classify_check_schedule() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("check my schedule for today");
        assert_eq!(cmd.intent, VoiceIntent::CheckSchedule);
        assert!(cmd.confidence >= 0.6);
    }

    #[test]
    fn classify_unknown_intent() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("hello world");
        assert_eq!(cmd.intent, VoiceIntent::Unknown);
        assert!((cmd.confidence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn classify_with_multiple_keywords_has_higher_confidence() {
        let mut va = VoiceAssistant::new();
        // "send" + "message" = two keywords for SendMessage
        let cmd = va.process_text("send a message");
        assert_eq!(cmd.intent, VoiceIntent::SendMessage);
        assert!(cmd.confidence >= 0.8);
    }

    // -- wake words ---------------------------------------------------------

    #[test]
    fn is_wake_word_case_insensitive() {
        let va = VoiceAssistant::new();
        assert!(va.is_wake_word("hey hive"));
        assert!(va.is_wake_word("Hey Hive"));
        assert!(va.is_wake_word("HEY HIVE"));
        assert!(va.is_wake_word("ok hive"));
        assert!(va.is_wake_word("OK HIVE"));
    }

    #[test]
    fn is_wake_word_rejects_non_wake_words() {
        let va = VoiceAssistant::new();
        assert!(!va.is_wake_word("hello hive"));
        assert!(!va.is_wake_word("hi there"));
        assert!(!va.is_wake_word(""));
    }

    #[test]
    fn add_wake_word() {
        let mut va = VoiceAssistant::new();
        va.add_wake_word("yo hive");
        assert!(va.is_wake_word("yo hive"));
        assert!(va.is_wake_word("Yo Hive"));
        assert_eq!(va.wake_word_config().wake_words.len(), 3);
    }

    #[test]
    fn add_duplicate_wake_word_is_noop() {
        let mut va = VoiceAssistant::new();
        va.add_wake_word("hey hive");
        assert_eq!(va.wake_word_config().wake_words.len(), 2);
    }

    #[test]
    fn remove_wake_word() {
        let mut va = VoiceAssistant::new();
        let removed = va.remove_wake_word("hey hive");
        assert!(removed);
        assert!(!va.is_wake_word("hey hive"));
        assert_eq!(va.wake_word_config().wake_words.len(), 1);
    }

    #[test]
    fn remove_nonexistent_wake_word_returns_false() {
        let mut va = VoiceAssistant::new();
        let removed = va.remove_wake_word("not a wake word");
        assert!(!removed);
        assert_eq!(va.wake_word_config().wake_words.len(), 2);
    }

    // -- command history ----------------------------------------------------

    #[test]
    fn command_history_tracks_processed_commands() {
        let mut va = VoiceAssistant::new();
        va.process_text("send a message");
        va.process_text("search for files");
        va.process_text("hello world");

        let history = va.command_history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].intent, VoiceIntent::SendMessage);
        assert_eq!(history[1].intent, VoiceIntent::SearchFiles);
        assert_eq!(history[2].intent, VoiceIntent::Unknown);
    }

    #[test]
    fn clear_history_empties_command_log() {
        let mut va = VoiceAssistant::new();
        va.process_text("run tests");
        va.process_text("open panel");
        assert_eq!(va.command_history().len(), 2);

        va.clear_history();
        assert!(va.command_history().is_empty());
    }

    // -- serde round trip ---------------------------------------------------

    #[test]
    fn voice_command_serde_round_trip() {
        let cmd = VoiceCommand {
            id: Uuid::new_v4().to_string(),
            text: "send a message to the team".to_string(),
            intent: VoiceIntent::SendMessage,
            confidence: 0.85,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string_pretty(&cmd).unwrap();
        let parsed: VoiceCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, cmd.id);
        assert_eq!(parsed.text, cmd.text);
        assert_eq!(parsed.intent, VoiceIntent::SendMessage);
        assert!((parsed.confidence - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn voice_state_serde_round_trip() {
        let states = vec![
            VoiceState::Idle,
            VoiceState::Listening,
            VoiceState::Processing,
            VoiceState::Speaking,
            VoiceState::Error,
        ];
        for state in &states {
            let json = serde_json::to_string(state).unwrap();
            let parsed: VoiceState = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, state);
        }
    }

    #[test]
    fn voice_intent_serde_round_trip() {
        let intents = vec![
            VoiceIntent::SendMessage,
            VoiceIntent::SearchFiles,
            VoiceIntent::RunCommand,
            VoiceIntent::OpenPanel,
            VoiceIntent::CreateTask,
            VoiceIntent::ReadNotifications,
            VoiceIntent::CheckSchedule,
            VoiceIntent::Unknown,
        ];
        for intent in &intents {
            let json = serde_json::to_string(intent).unwrap();
            let parsed: VoiceIntent = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, intent);
        }
    }

    #[test]
    fn wake_word_config_serde_round_trip() {
        let cfg = WakeWordConfig {
            wake_words: vec!["hey hive".into(), "ok hive".into(), "yo hive".into()],
            sensitivity: 0.7,
            enabled: true,
        };
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let parsed: WakeWordConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.wake_words, cfg.wake_words);
        assert!((parsed.sensitivity - 0.7).abs() < f32::EPSILON);
        assert!(parsed.enabled);
    }

    // -- edge cases ---------------------------------------------------------

    #[test]
    fn process_empty_text_returns_unknown() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("");
        assert_eq!(cmd.intent, VoiceIntent::Unknown);
        assert!((cmd.confidence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn process_text_assigns_unique_ids() {
        let mut va = VoiceAssistant::new();
        let cmd1 = va.process_text("send message");
        let cmd2 = va.process_text("send message");
        assert_ne!(cmd1.id, cmd2.id);
    }

    #[test]
    fn default_impl_matches_new() {
        let va = VoiceAssistant::default();
        assert_eq!(va.state(), VoiceState::Idle);
        assert!(va.command_history().is_empty());
        assert_eq!(va.wake_word_config().wake_words.len(), 2);
    }

    #[test]
    fn classify_email_as_send_message() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("email the report to the manager");
        assert_eq!(cmd.intent, VoiceIntent::SendMessage);
    }

    #[test]
    fn classify_find_as_search() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("find the README file");
        assert_eq!(cmd.intent, VoiceIntent::SearchFiles);
    }

    #[test]
    fn classify_execute_as_run_command() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("execute the deployment script");
        assert_eq!(cmd.intent, VoiceIntent::RunCommand);
    }

    #[test]
    fn classify_terminal_as_run_command() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("open the terminal");
        // "open" matches OpenPanel first, but "terminal" also matches RunCommand.
        // Since OpenPanel keywords are checked after RunCommand... actually
        // let's check the actual order:  "terminal" triggers RunCommand before
        // "open" triggers OpenPanel because RunCommand is checked first.
        // Wait — "open" is checked in OpenPanel block. Let's see what actually
        // happens: the lower text is "open the terminal". It does NOT contain
        // "send", "message", or "email" → skip SendMessage. It does NOT contain
        // "search", "find", or "look for" → skip SearchFiles. It DOES contain
        // "run"? No. "execute"? No. "terminal"? YES → RunCommand.
        assert_eq!(cmd.intent, VoiceIntent::RunCommand);
    }

    #[test]
    fn classify_show_as_open_panel() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("show me the dashboard");
        assert_eq!(cmd.intent, VoiceIntent::OpenPanel);
    }

    #[test]
    fn classify_calendar_as_check_schedule() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("what's on my calendar today");
        assert_eq!(cmd.intent, VoiceIntent::CheckSchedule);
    }

    #[test]
    fn classify_alerts_as_read_notifications() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("any new alerts?");
        assert_eq!(cmd.intent, VoiceIntent::ReadNotifications);
    }

    #[test]
    fn classify_meeting_as_check_schedule() {
        let mut va = VoiceAssistant::new();
        let cmd = va.process_text("do I have a meeting tomorrow");
        assert_eq!(cmd.intent, VoiceIntent::CheckSchedule);
    }

    #[test]
    fn is_wake_word_trims_whitespace() {
        let va = VoiceAssistant::new();
        assert!(va.is_wake_word("  hey hive  "));
        assert!(va.is_wake_word("  OK HIVE  "));
    }
}
