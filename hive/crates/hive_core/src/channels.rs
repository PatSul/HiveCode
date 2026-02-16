//! AI Agent Messaging Channels — Telegram/WhatsApp-style channels where users
//! interact with multiple AI agents in persistent, threaded conversations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

use crate::config::HiveConfig;

// ---------------------------------------------------------------------------
// Data Model
// ---------------------------------------------------------------------------

/// Who authored a channel message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MessageAuthor {
    User,
    Agent { persona: String },
    System,
}

/// A single message in a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: String,
    pub author: MessageAuthor,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub thread_id: Option<String>,
    pub model: Option<String>,
    pub cost: Option<f64>,
}

/// A threaded conversation within a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelThread {
    pub id: String,
    pub root_message_id: String,
    pub title: Option<String>,
    pub message_count: usize,
    pub last_activity: DateTime<Utc>,
}

/// An AI agent channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChannel {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub assigned_agents: Vec<String>,
    pub messages: Vec<ChannelMessage>,
    pub threads: Vec<ChannelThread>,
    pub pinned_files: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ChannelStore
// ---------------------------------------------------------------------------

/// Persistence layer for agent channels. Stores each channel as a JSON file
/// under `~/.hive/channels/{id}.json`.
#[derive(Debug)]
pub struct ChannelStore {
    channels_dir: PathBuf,
    channels: Vec<AgentChannel>,
}

impl Default for ChannelStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelStore {
    /// Create a new store, loading all channels from disk.
    pub fn new() -> Self {
        let channels_dir = HiveConfig::base_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("channels");

        if !channels_dir.exists() {
            let _ = std::fs::create_dir_all(&channels_dir);
        }

        let mut store = Self {
            channels_dir,
            channels: Vec::new(),
        };
        store.load_all();
        store
    }

    /// Ensure the default channels exist. Called once on startup.
    pub fn ensure_default_channels(&mut self) {
        let defaults = vec![
            ("general", "#general", "\u{1F4AC}", "Main channel — all agents available", vec![
                "Investigate", "Implement", "Verify", "Critique", "Debug", "CodeReview",
            ]),
            ("code-review", "#code-review", "\u{1F50D}", "Code review discussions with review agents", vec![
                "CodeReview", "Critique",
            ]),
            ("debug", "#debug", "\u{1F41B}", "Debugging sessions with debug agents", vec![
                "Debug", "Investigate",
            ]),
            ("research", "#research", "\u{1F4D6}", "Research and investigation tasks", vec![
                "Investigate", "Implement",
            ]),
        ];

        for (id, name, icon, desc, agents) in defaults {
            if !self.channels.iter().any(|c| c.id == id) {
                let channel = AgentChannel {
                    id: id.to_string(),
                    name: name.to_string(),
                    icon: icon.to_string(),
                    description: desc.to_string(),
                    assigned_agents: agents.into_iter().map(String::from).collect(),
                    messages: Vec::new(),
                    threads: Vec::new(),
                    pinned_files: Vec::new(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.channels.push(channel.clone());
                let _ = self.save_channel(&channel);
                info!("Created default channel: {name}");
            }
        }
    }

    /// List all channels.
    pub fn list_channels(&self) -> &[AgentChannel] {
        &self.channels
    }

    /// Get a channel by ID.
    pub fn get_channel(&self, id: &str) -> Option<&AgentChannel> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// Get a mutable reference to a channel.
    pub fn get_channel_mut(&mut self, id: &str) -> Option<&mut AgentChannel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }

    /// Create a new custom channel.
    pub fn create_channel(
        &mut self,
        name: &str,
        icon: &str,
        description: &str,
        agents: Vec<String>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let channel = AgentChannel {
            id: id.clone(),
            name: name.to_string(),
            icon: icon.to_string(),
            description: description.to_string(),
            assigned_agents: agents,
            messages: Vec::new(),
            threads: Vec::new(),
            pinned_files: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let _ = self.save_channel(&channel);
        self.channels.push(channel);
        id
    }

    /// Add a message to a channel and persist.
    pub fn add_message(&mut self, channel_id: &str, message: ChannelMessage) {
        if let Some(idx) = self.channels.iter().position(|c| c.id == channel_id) {
            self.channels[idx].messages.push(message);
            self.channels[idx].updated_at = Utc::now();
            let path = self.channels_dir.join(format!("{}.json", self.channels[idx].id));
            let json = serde_json::to_string_pretty(&self.channels[idx]).unwrap_or_default();
            let _ = std::fs::write(path, json);
        }
    }

    /// Save a single channel to disk.
    fn save_channel(&self, channel: &AgentChannel) -> anyhow::Result<()> {
        let path = self.channels_dir.join(format!("{}.json", channel.id));
        let json = serde_json::to_string_pretty(channel)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load all channels from disk.
    fn load_all(&mut self) {
        self.channels.clear();
        if let Ok(entries) = std::fs::read_dir(&self.channels_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => match serde_json::from_str::<AgentChannel>(&content) {
                            Ok(channel) => self.channels.push(channel),
                            Err(e) => warn!("Failed to parse channel {}: {e}", path.display()),
                        },
                        Err(e) => warn!("Failed to read channel {}: {e}", path.display()),
                    }
                }
            }
        }
        // Sort by name
        self.channels.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Persist all channels to disk after modification.
    pub fn save_all(&self) {
        for channel in &self.channels {
            let _ = self.save_channel(channel);
        }
    }
}
