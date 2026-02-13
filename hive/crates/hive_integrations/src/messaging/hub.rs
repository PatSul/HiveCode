//! Messaging hub that routes messages to platform-specific providers.
//!
//! The [`MessagingHub`] acts as a unified facade, holding a registry of
//! [`MessagingProvider`] implementations keyed by [`Platform`] and
//! delegating operations to the appropriate provider.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tracing::debug;

use super::provider::{Channel, IncomingMessage, MessagingProvider, Platform, SentMessage};

/// Central hub that manages and dispatches to messaging providers.
pub struct MessagingHub {
    providers: HashMap<Platform, Box<dyn MessagingProvider>>,
}

impl MessagingHub {
    /// Create a new empty hub with no providers registered.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider for its platform, replacing any previous one.
    pub fn register_provider(&mut self, provider: Box<dyn MessagingProvider>) {
        let platform = provider.platform();
        debug!(platform = %platform, "registering messaging provider");
        self.providers.insert(platform, provider);
    }

    /// Return the number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Check whether a provider is registered for the given platform.
    pub fn has_provider(&self, platform: Platform) -> bool {
        self.providers.contains_key(&platform)
    }

    /// Return the list of platforms that have registered providers.
    pub fn platforms(&self) -> Vec<Platform> {
        self.providers.keys().copied().collect()
    }

    /// Send a text message via the specified platform.
    pub async fn send_message(
        &self,
        platform: Platform,
        channel: &str,
        text: &str,
    ) -> Result<SentMessage> {
        let provider = self
            .providers
            .get(&platform)
            .context(format!("no provider registered for {platform}"))?;

        debug!(platform = %platform, channel = %channel, "sending message via hub");
        provider.send_message(channel, text).await
    }

    /// List channels visible on the given platform.
    pub async fn list_channels(&self, platform: Platform) -> Result<Vec<Channel>> {
        let provider = self
            .providers
            .get(&platform)
            .context(format!("no provider registered for {platform}"))?;

        debug!(platform = %platform, "listing channels via hub");
        provider.list_channels().await
    }

    /// Retrieve recent messages from a channel on the given platform.
    pub async fn get_messages(
        &self,
        platform: Platform,
        channel: &str,
        limit: u32,
    ) -> Result<Vec<IncomingMessage>> {
        let provider = self
            .providers
            .get(&platform)
            .context(format!("no provider registered for {platform}"))?;

        debug!(platform = %platform, channel = %channel, limit = limit, "getting messages via hub");
        provider.get_messages(channel, limit).await
    }

    /// Add a reaction to a message on the given platform.
    pub async fn add_reaction(
        &self,
        platform: Platform,
        channel: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        let provider = self
            .providers
            .get(&platform)
            .context(format!("no provider registered for {platform}"))?;

        debug!(platform = %platform, channel = %channel, message_id = %message_id, "adding reaction via hub");
        provider.add_reaction(channel, message_id, emoji).await
    }

    /// Search messages on the given platform.
    pub async fn search_messages(
        &self,
        platform: Platform,
        query: &str,
        limit: u32,
    ) -> Result<Vec<IncomingMessage>> {
        let provider = self
            .providers
            .get(&platform)
            .context(format!("no provider registered for {platform}"))?;

        debug!(platform = %platform, query = %query, "searching messages via hub");
        provider.search_messages(query, limit).await
    }

    /// Broadcast a message to multiple channels across multiple platforms.
    ///
    /// `channels_by_platform` maps each target platform to a list of channel
    /// IDs. Returns a `Vec` of `(Platform, Result<SentMessage>)` for each
    /// send attempt, preserving the order of platforms and channels.
    pub async fn broadcast(
        &self,
        channels_by_platform: &HashMap<Platform, Vec<String>>,
        text: &str,
    ) -> Vec<(Platform, String, Result<SentMessage>)> {
        let mut results = Vec::new();

        for (platform, channels) in channels_by_platform {
            for channel in channels {
                let result = match self.providers.get(platform) {
                    Some(provider) => {
                        debug!(
                            platform = %platform,
                            channel = %channel,
                            "broadcasting message"
                        );
                        provider.send_message(channel, text).await
                    }
                    None => Err(anyhow::anyhow!("no provider registered for {platform}")),
                };
                results.push((*platform, channel.clone(), result));
            }
        }

        results
    }
}

impl Default for MessagingHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::provider::SentMessage;
    use async_trait::async_trait;
    use chrono::Utc;

    /// A fake provider for testing hub routing logic.
    struct FakeProvider {
        plat: Platform,
    }

    impl FakeProvider {
        fn new(plat: Platform) -> Self {
            Self { plat }
        }
    }

    #[async_trait]
    impl MessagingProvider for FakeProvider {
        fn platform(&self) -> Platform {
            self.plat
        }

        async fn send_message(&self, channel: &str, _text: &str) -> Result<SentMessage> {
            Ok(SentMessage {
                id: format!("sent-{}-{}", self.plat, channel),
                channel_id: channel.to_string(),
                timestamp: Utc::now(),
            })
        }

        async fn list_channels(&self) -> Result<Vec<Channel>> {
            Ok(vec![
                Channel {
                    id: "ch-1".into(),
                    name: "general".into(),
                    platform: self.plat,
                },
                Channel {
                    id: "ch-2".into(),
                    name: "random".into(),
                    platform: self.plat,
                },
            ])
        }

        async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
            let mut msgs = Vec::new();
            for i in 0..limit.min(3) {
                msgs.push(IncomingMessage {
                    id: format!("msg-{i}"),
                    channel_id: channel.to_string(),
                    author: "testuser".into(),
                    content: format!("Message {i}"),
                    timestamp: Utc::now(),
                    attachments: vec![],
                    platform: self.plat,
                });
            }
            Ok(msgs)
        }

        async fn add_reaction(
            &self,
            _channel: &str,
            _message_id: &str,
            _emoji: &str,
        ) -> Result<()> {
            Ok(())
        }

        async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
            let mut msgs = Vec::new();
            for i in 0..limit.min(2) {
                msgs.push(IncomingMessage {
                    id: format!("search-{i}"),
                    channel_id: "ch-1".into(),
                    author: "searcher".into(),
                    content: format!("Result for '{query}' #{i}"),
                    timestamp: Utc::now(),
                    attachments: vec![],
                    platform: self.plat,
                });
            }
            Ok(msgs)
        }
    }

    #[test]
    fn test_hub_new_is_empty() {
        let hub = MessagingHub::new();
        assert_eq!(hub.provider_count(), 0);
        assert!(hub.platforms().is_empty());
    }

    #[test]
    fn test_hub_default_is_empty() {
        let hub = MessagingHub::default();
        assert_eq!(hub.provider_count(), 0);
    }

    #[test]
    fn test_register_provider() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));
        assert_eq!(hub.provider_count(), 1);
        assert!(hub.has_provider(Platform::Slack));
        assert!(!hub.has_provider(Platform::Discord));
    }

    #[test]
    fn test_register_multiple_providers() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));
        hub.register_provider(Box::new(FakeProvider::new(Platform::Discord)));
        assert_eq!(hub.provider_count(), 2);
        assert!(hub.has_provider(Platform::Slack));
        assert!(hub.has_provider(Platform::Discord));
    }

    #[test]
    fn test_register_replaces_existing() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));
        assert_eq!(hub.provider_count(), 1);
    }

    #[test]
    fn test_platforms_returns_registered() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));
        hub.register_provider(Box::new(FakeProvider::new(Platform::Discord)));
        let platforms = hub.platforms();
        assert_eq!(platforms.len(), 2);
        assert!(platforms.contains(&Platform::Slack));
        assert!(platforms.contains(&Platform::Discord));
    }

    #[tokio::test]
    async fn test_send_message_routes_to_provider() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));

        let result = hub.send_message(Platform::Slack, "general", "hello").await;
        assert!(result.is_ok());
        let sent = result.unwrap();
        assert_eq!(sent.channel_id, "general");
        assert!(sent.id.contains("slack"));
    }

    #[tokio::test]
    async fn test_send_message_missing_provider() {
        let hub = MessagingHub::new();
        let result = hub.send_message(Platform::Telegram, "chan", "hi").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no provider"));
    }

    #[tokio::test]
    async fn test_list_channels() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Discord)));

        let channels = hub.list_channels(Platform::Discord).await.unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].name, "general");
    }

    #[tokio::test]
    async fn test_get_messages() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));

        let msgs = hub.get_messages(Platform::Slack, "ch-1", 5).await.unwrap();
        assert_eq!(msgs.len(), 3); // FakeProvider caps at 3
    }

    #[tokio::test]
    async fn test_broadcast() {
        let mut hub = MessagingHub::new();
        hub.register_provider(Box::new(FakeProvider::new(Platform::Slack)));
        hub.register_provider(Box::new(FakeProvider::new(Platform::Discord)));

        let mut targets = HashMap::new();
        targets.insert(Platform::Slack, vec!["general".into(), "random".into()]);
        targets.insert(Platform::Discord, vec!["lobby".into()]);

        let results = hub.broadcast(&targets, "Hello everyone!").await;
        assert_eq!(results.len(), 3);
        for (_, _, result) in &results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_broadcast_missing_provider() {
        let hub = MessagingHub::new();
        let mut targets = HashMap::new();
        targets.insert(Platform::Telegram, vec!["chat".into()]);

        let results = hub.broadcast(&targets, "Hello!").await;
        assert_eq!(results.len(), 1);
        assert!(results[0].2.is_err());
    }
}
