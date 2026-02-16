//! Cross-channel memory service.
//!
//! Tracks conversations across messaging platforms, links channels and
//! threads, and provides unified search capabilities. This is not a
//! [`MessagingProvider`] itself but a coordination layer that works
//! alongside the [`MessagingHub`].

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::provider::{IncomingMessage, Platform};

// ── Types ────────────────────────────────────────────────────────

/// A link between channels on different platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelLink {
    pub id: String,
    pub platform_a: Platform,
    pub channel_a: String,
    pub platform_b: Platform,
    pub channel_b: String,
    pub created_at: DateTime<Utc>,
}

/// A thread link connecting threads across platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadLink {
    pub id: String,
    pub platform: Platform,
    pub channel_id: String,
    pub thread_id: String,
    pub linked_platform: Platform,
    pub linked_channel_id: String,
    pub linked_thread_id: String,
    pub created_at: DateTime<Utc>,
}

/// A conversation record tracking a message across platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationEntry {
    pub message_id: String,
    pub platform: Platform,
    pub channel_id: String,
    pub author: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// A unified search result across platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossSearchResult {
    pub platform: Platform,
    pub channel_id: String,
    pub message_id: String,
    pub author: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub score: f64,
}

// ── Service ────────────────────────────────────────────────────────

/// Inner state behind an `Arc<Mutex<_>>`.
#[derive(Serialize, Deserialize)]
struct CrossChannelState {
    channel_links: Vec<ChannelLink>,
    thread_links: Vec<ThreadLink>,
    conversations: Vec<ConversationEntry>,
    next_link_id: u64,
}

/// Cross-channel memory service for tracking conversations across
/// messaging platforms.
pub struct CrossChannelService {
    state: Arc<Mutex<CrossChannelState>>,
}

impl CrossChannelService {
    /// Create a new, empty cross-channel service.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CrossChannelState {
                channel_links: Vec::new(),
                thread_links: Vec::new(),
                conversations: Vec::new(),
                next_link_id: 1,
            })),
        }
    }

    // ── Channel linking ──────────────────────────────────────────

    /// Link two channels across platforms.
    pub fn link_channels(
        &self,
        platform_a: Platform,
        channel_a: &str,
        platform_b: Platform,
        channel_b: &str,
    ) -> ChannelLink {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let id = format!("cl-{}", state.next_link_id);
        state.next_link_id += 1;

        let link = ChannelLink {
            id: id.clone(),
            platform_a,
            channel_a: channel_a.to_string(),
            platform_b,
            channel_b: channel_b.to_string(),
            created_at: Utc::now(),
        };

        debug!(
            id = %id,
            platform_a = %platform_a,
            channel_a = %channel_a,
            platform_b = %platform_b,
            channel_b = %channel_b,
            "linked channels across platforms"
        );

        state.channel_links.push(link.clone());
        link
    }

    /// List all channel links.
    pub fn list_channel_links(&self) -> Vec<ChannelLink> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).channel_links.clone()
    }

    /// Find channels linked to the given platform+channel.
    pub fn find_linked_channels(
        &self,
        platform: Platform,
        channel: &str,
    ) -> Vec<(Platform, String)> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut results = Vec::new();

        for link in &state.channel_links {
            if link.platform_a == platform && link.channel_a == channel {
                results.push((link.platform_b, link.channel_b.clone()));
            } else if link.platform_b == platform && link.channel_b == channel {
                results.push((link.platform_a, link.channel_a.clone()));
            }
        }

        results
    }

    /// Remove a channel link by its ID.
    pub fn unlink_channels(&self, link_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let before = state.channel_links.len();
        state.channel_links.retain(|l| l.id != link_id);
        state.channel_links.len() < before
    }

    // ── Thread linking ───────────────────────────────────────────

    /// Link two threads across platforms.
    pub fn link_threads(
        &self,
        platform: Platform,
        channel_id: &str,
        thread_id: &str,
        linked_platform: Platform,
        linked_channel_id: &str,
        linked_thread_id: &str,
    ) -> ThreadLink {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let id = format!("tl-{}", state.next_link_id);
        state.next_link_id += 1;

        let link = ThreadLink {
            id: id.clone(),
            platform,
            channel_id: channel_id.to_string(),
            thread_id: thread_id.to_string(),
            linked_platform,
            linked_channel_id: linked_channel_id.to_string(),
            linked_thread_id: linked_thread_id.to_string(),
            created_at: Utc::now(),
        };

        debug!(
            id = %id,
            platform = %platform,
            thread_id = %thread_id,
            linked_platform = %linked_platform,
            linked_thread_id = %linked_thread_id,
            "linked threads across platforms"
        );

        state.thread_links.push(link.clone());
        link
    }

    /// List all thread links.
    pub fn list_thread_links(&self) -> Vec<ThreadLink> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).thread_links.clone()
    }

    // ── Conversation tracking ────────────────────────────────────

    /// Record a message in the cross-channel conversation history.
    pub fn track_message(&self, message: &IncomingMessage) {
        let entry = ConversationEntry {
            message_id: message.id.clone(),
            platform: message.platform,
            channel_id: message.channel_id.clone(),
            author: message.author.clone(),
            content: message.content.clone(),
            timestamp: message.timestamp,
        };

        debug!(
            platform = %message.platform,
            channel = %message.channel_id,
            message_id = %message.id,
            "tracking cross-channel message"
        );

        self.state.lock().unwrap_or_else(|e| e.into_inner()).conversations.push(entry);
    }

    /// Track multiple messages at once.
    pub fn track_messages(&self, messages: &[IncomingMessage]) {
        for msg in messages {
            self.track_message(msg);
        }
    }

    /// Return the total number of tracked conversation entries.
    pub fn conversation_count(&self) -> usize {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).conversations.len()
    }

    /// Get recent conversation entries, newest first.
    pub fn recent_conversations(&self, limit: usize) -> Vec<ConversationEntry> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .conversations
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get conversation entries for a specific platform.
    pub fn conversations_by_platform(&self, platform: Platform) -> Vec<ConversationEntry> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .conversations
            .iter()
            .filter(|e| e.platform == platform)
            .cloned()
            .collect()
    }

    // ── Cross-platform search ────────────────────────────────────

    /// Search conversations across all tracked platforms.
    pub fn search_all(&self, query: &str, limit: usize) -> Vec<CrossSearchResult> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        debug!(query = %query, "searching across all platforms");

        let mut results: Vec<CrossSearchResult> = state
            .conversations
            .iter()
            .filter_map(|entry| {
                let content_lower = entry.content.to_lowercase();

                // Score based on how many query words are found.
                let matched = query_words
                    .iter()
                    .filter(|w| content_lower.contains(**w))
                    .count();

                if matched == 0 {
                    return None;
                }

                let score = matched as f64 / query_words.len().max(1) as f64;

                Some(CrossSearchResult {
                    platform: entry.platform,
                    channel_id: entry.channel_id.clone(),
                    message_id: entry.message_id.clone(),
                    author: entry.author.clone(),
                    content: entry.content.clone(),
                    timestamp: entry.timestamp,
                    score,
                })
            })
            .collect();

        // Sort by score descending, then by timestamp descending.
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });

        results.truncate(limit);
        results
    }

    /// Search conversations filtered to specific platforms.
    pub fn search_platforms(
        &self,
        query: &str,
        platforms: &[Platform],
        limit: usize,
    ) -> Vec<CrossSearchResult> {
        let all = self.search_all(query, limit * 2);
        all.into_iter()
            .filter(|r| platforms.contains(&r.platform))
            .take(limit)
            .collect()
    }

    // ── Statistics ────────────────────────────────────────────────

    /// Return per-platform message counts.
    pub fn platform_stats(&self) -> HashMap<Platform, usize> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut stats = HashMap::new();
        for entry in &state.conversations {
            *stats.entry(entry.platform).or_insert(0) += 1;
        }
        stats
    }

    /// Clear all data.
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.channel_links.clear();
        state.thread_links.clear();
        state.conversations.clear();
    }

    // ── Persistence ─────────────────────────────────────────────

    /// Persist the cross-channel service state to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let state = self.state.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let json = serde_json::to_string_pretty(&*state)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a cross-channel service from a JSON file. Returns an empty
    /// service if the file does not exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path)?;
        let state: CrossChannelState = serde_json::from_str(&json)?;
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }
}

impl Default for CrossChannelService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> CrossChannelService {
        CrossChannelService::new()
    }

    fn make_message(
        id: &str,
        platform: Platform,
        channel: &str,
        author: &str,
        content: &str,
    ) -> IncomingMessage {
        IncomingMessage {
            id: id.into(),
            channel_id: channel.into(),
            author: author.into(),
            content: content.into(),
            timestamp: Utc::now(),
            attachments: vec![],
            platform,
        }
    }

    #[test]
    fn test_new_service_is_empty() {
        let svc = make_service();
        assert_eq!(svc.conversation_count(), 0);
        assert!(svc.list_channel_links().is_empty());
        assert!(svc.list_thread_links().is_empty());
    }

    #[test]
    fn test_default_service_is_empty() {
        let svc = CrossChannelService::default();
        assert_eq!(svc.conversation_count(), 0);
    }

    #[test]
    fn test_link_channels() {
        let svc = make_service();
        let link = svc.link_channels(Platform::Slack, "C01", Platform::Discord, "D01");
        assert!(link.id.starts_with("cl-"));
        assert_eq!(link.platform_a, Platform::Slack);
        assert_eq!(link.channel_a, "C01");
        assert_eq!(link.platform_b, Platform::Discord);
        assert_eq!(link.channel_b, "D01");
    }

    #[test]
    fn test_list_channel_links() {
        let svc = make_service();
        svc.link_channels(Platform::Slack, "C01", Platform::Discord, "D01");
        svc.link_channels(Platform::Telegram, "T01", Platform::Matrix, "M01");
        let links = svc.list_channel_links();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_find_linked_channels() {
        let svc = make_service();
        svc.link_channels(Platform::Slack, "C01", Platform::Discord, "D01");
        svc.link_channels(Platform::Slack, "C01", Platform::Telegram, "T01");

        let linked = svc.find_linked_channels(Platform::Slack, "C01");
        assert_eq!(linked.len(), 2);

        // Reverse lookup also works.
        let linked_reverse = svc.find_linked_channels(Platform::Discord, "D01");
        assert_eq!(linked_reverse.len(), 1);
        assert_eq!(linked_reverse[0].0, Platform::Slack);
        assert_eq!(linked_reverse[0].1, "C01");
    }

    #[test]
    fn test_unlink_channels() {
        let svc = make_service();
        let link = svc.link_channels(Platform::Slack, "C01", Platform::Discord, "D01");
        assert_eq!(svc.list_channel_links().len(), 1);

        let removed = svc.unlink_channels(&link.id);
        assert!(removed);
        assert!(svc.list_channel_links().is_empty());

        // Removing non-existent link returns false.
        let removed_again = svc.unlink_channels("non-existent");
        assert!(!removed_again);
    }

    #[test]
    fn test_link_threads() {
        let svc = make_service();
        let link = svc.link_threads(
            Platform::Slack,
            "C01",
            "thread-1",
            Platform::Discord,
            "D01",
            "thread-2",
        );
        assert!(link.id.starts_with("tl-"));
        assert_eq!(link.platform, Platform::Slack);
        assert_eq!(link.thread_id, "thread-1");
        assert_eq!(link.linked_platform, Platform::Discord);
        assert_eq!(link.linked_thread_id, "thread-2");
    }

    #[test]
    fn test_list_thread_links() {
        let svc = make_service();
        svc.link_threads(Platform::Slack, "C01", "t1", Platform::Discord, "D01", "t2");
        assert_eq!(svc.list_thread_links().len(), 1);
    }

    #[test]
    fn test_track_message() {
        let svc = make_service();
        let msg = make_message("msg-1", Platform::Slack, "C01", "alice", "Hello");
        svc.track_message(&msg);
        assert_eq!(svc.conversation_count(), 1);
    }

    #[test]
    fn test_track_messages_batch() {
        let svc = make_service();
        let messages = vec![
            make_message("msg-1", Platform::Slack, "C01", "alice", "Hello"),
            make_message("msg-2", Platform::Discord, "D01", "bob", "Hi"),
            make_message("msg-3", Platform::Telegram, "T01", "charlie", "Hey"),
        ];
        svc.track_messages(&messages);
        assert_eq!(svc.conversation_count(), 3);
    }

    #[test]
    fn test_recent_conversations() {
        let svc = make_service();
        svc.track_message(&make_message("msg-1", Platform::Slack, "C01", "a", "First"));
        svc.track_message(&make_message(
            "msg-2",
            Platform::Discord,
            "D01",
            "b",
            "Second",
        ));
        svc.track_message(&make_message(
            "msg-3",
            Platform::Telegram,
            "T01",
            "c",
            "Third",
        ));

        let recent = svc.recent_conversations(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].message_id, "msg-3"); // newest first
        assert_eq!(recent[1].message_id, "msg-2");
    }

    #[test]
    fn test_conversations_by_platform() {
        let svc = make_service();
        svc.track_message(&make_message("msg-1", Platform::Slack, "C01", "a", "Hello"));
        svc.track_message(&make_message("msg-2", Platform::Discord, "D01", "b", "Hi"));
        svc.track_message(&make_message("msg-3", Platform::Slack, "C02", "c", "Hey"));

        let slack_msgs = svc.conversations_by_platform(Platform::Slack);
        assert_eq!(slack_msgs.len(), 2);

        let discord_msgs = svc.conversations_by_platform(Platform::Discord);
        assert_eq!(discord_msgs.len(), 1);

        let tg_msgs = svc.conversations_by_platform(Platform::Telegram);
        assert!(tg_msgs.is_empty());
    }

    #[test]
    fn test_search_all() {
        let svc = make_service();
        svc.track_message(&make_message(
            "msg-1",
            Platform::Slack,
            "C01",
            "alice",
            "Hello world from Slack",
        ));
        svc.track_message(&make_message(
            "msg-2",
            Platform::Discord,
            "D01",
            "bob",
            "Goodbye world from Discord",
        ));
        svc.track_message(&make_message(
            "msg-3",
            Platform::Telegram,
            "T01",
            "charlie",
            "Nothing relevant here",
        ));

        let results = svc.search_all("hello world", 10);
        assert!(!results.is_empty());
        // msg-1 should score highest (both "hello" and "world" match).
        assert_eq!(results[0].message_id, "msg-1");
        assert!(results[0].score > 0.5);
    }

    #[test]
    fn test_search_all_no_results() {
        let svc = make_service();
        svc.track_message(&make_message("msg-1", Platform::Slack, "C01", "a", "Hello"));

        let results = svc.search_all("zzzzz", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_platforms_filter() {
        let svc = make_service();
        svc.track_message(&make_message(
            "msg-1",
            Platform::Slack,
            "C01",
            "alice",
            "Hello world",
        ));
        svc.track_message(&make_message(
            "msg-2",
            Platform::Discord,
            "D01",
            "bob",
            "Hello world",
        ));

        let results = svc.search_platforms("hello", &[Platform::Slack], 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].platform, Platform::Slack);
    }

    #[test]
    fn test_platform_stats() {
        let svc = make_service();
        svc.track_message(&make_message("1", Platform::Slack, "C01", "a", "m1"));
        svc.track_message(&make_message("2", Platform::Slack, "C02", "b", "m2"));
        svc.track_message(&make_message("3", Platform::Discord, "D01", "c", "m3"));

        let stats = svc.platform_stats();
        assert_eq!(stats.get(&Platform::Slack), Some(&2));
        assert_eq!(stats.get(&Platform::Discord), Some(&1));
        assert_eq!(stats.get(&Platform::Telegram), None);
    }

    #[test]
    fn test_clear() {
        let svc = make_service();
        svc.link_channels(Platform::Slack, "C01", Platform::Discord, "D01");
        svc.track_message(&make_message("1", Platform::Slack, "C01", "a", "hello"));
        svc.link_threads(Platform::Slack, "C01", "t1", Platform::Discord, "D01", "t2");

        svc.clear();
        assert!(svc.list_channel_links().is_empty());
        assert!(svc.list_thread_links().is_empty());
        assert_eq!(svc.conversation_count(), 0);
    }

    #[test]
    fn test_channel_link_serialization_roundtrip() {
        let link = ChannelLink {
            id: "cl-1".into(),
            platform_a: Platform::Slack,
            channel_a: "C01".into(),
            platform_b: Platform::Discord,
            channel_b: "D01".into(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&link).unwrap();
        let back: ChannelLink = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "cl-1");
        assert_eq!(back.platform_a, Platform::Slack);
        assert_eq!(back.platform_b, Platform::Discord);
    }

    #[test]
    fn test_thread_link_serialization_roundtrip() {
        let link = ThreadLink {
            id: "tl-1".into(),
            platform: Platform::Slack,
            channel_id: "C01".into(),
            thread_id: "t1".into(),
            linked_platform: Platform::Discord,
            linked_channel_id: "D01".into(),
            linked_thread_id: "t2".into(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&link).unwrap();
        let back: ThreadLink = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "tl-1");
        assert_eq!(back.thread_id, "t1");
        assert_eq!(back.linked_thread_id, "t2");
    }

    #[test]
    fn test_conversation_entry_serialization() {
        let entry = ConversationEntry {
            message_id: "msg-1".into(),
            platform: Platform::Slack,
            channel_id: "C01".into(),
            author: "alice".into(),
            content: "Hello".into(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: ConversationEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message_id, "msg-1");
        assert_eq!(back.platform, Platform::Slack);
    }

    #[test]
    fn test_cross_search_result_serialization() {
        let result = CrossSearchResult {
            platform: Platform::Discord,
            channel_id: "D01".into(),
            message_id: "msg-1".into(),
            author: "bob".into(),
            content: "Hello world".into(),
            timestamp: Utc::now(),
            score: 0.75,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: CrossSearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.platform, Platform::Discord);
        assert_eq!(back.score, 0.75);
    }
}
