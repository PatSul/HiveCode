use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Capabilities that an assistant plugin can provide.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssistantCapability {
    Email,
    Calendar,
    Reminders,
    Research,
    Approvals,
}

// ---------------------------------------------------------------------------
// AssistantPlugin trait
// ---------------------------------------------------------------------------

/// Trait that all assistant plugins must implement.
///
/// Plugins extend the assistant with additional capabilities such as
/// email integration, calendar sync, research tools, etc.
#[async_trait]
pub trait AssistantPlugin: Send + Sync {
    /// Human-readable name of the plugin.
    fn name(&self) -> &str;

    /// The set of capabilities this plugin provides.
    fn capabilities(&self) -> Vec<AssistantCapability>;

    /// Initialize the plugin. Called once at startup.
    async fn initialize(&mut self) -> Result<(), String>;

    /// Shut down the plugin gracefully. Called once at app exit.
    async fn shutdown(&mut self) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::plugin::{AssistantCapability, AssistantPlugin};

    struct MockPlugin {
        initialized: bool,
        shut_down: bool,
    }

    impl MockPlugin {
        fn new() -> Self {
            Self {
                initialized: false,
                shut_down: false,
            }
        }
    }

    #[async_trait::async_trait]
    impl AssistantPlugin for MockPlugin {
        fn name(&self) -> &str {
            "mock_plugin"
        }

        fn capabilities(&self) -> Vec<AssistantCapability> {
            vec![AssistantCapability::Email, AssistantCapability::Calendar]
        }

        async fn initialize(&mut self) -> Result<(), String> {
            self.initialized = true;
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), String> {
            self.shut_down = true;
            Ok(())
        }
    }

    #[test]
    fn test_mock_plugin_name() {
        let plugin = MockPlugin::new();
        assert_eq!(plugin.name(), "mock_plugin");
    }

    #[test]
    fn test_mock_plugin_capabilities() {
        let plugin = MockPlugin::new();
        let caps = plugin.capabilities();
        assert_eq!(caps.len(), 2);
        assert!(caps.contains(&AssistantCapability::Email));
        assert!(caps.contains(&AssistantCapability::Calendar));
    }

    #[tokio::test]
    async fn test_mock_plugin_lifecycle() {
        let mut plugin = MockPlugin::new();
        assert!(!plugin.initialized);
        assert!(!plugin.shut_down);

        plugin.initialize().await.unwrap();
        assert!(plugin.initialized);

        plugin.shutdown().await.unwrap();
        assert!(plugin.shut_down);
    }

    #[test]
    fn test_capability_serialization() {
        let cap = AssistantCapability::Email;
        let json = serde_json::to_string(&cap).unwrap();
        assert_eq!(json, "\"Email\"");

        let deserialized: AssistantCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, AssistantCapability::Email);
    }

    #[test]
    fn test_all_capabilities_serialize() {
        let caps = vec![
            AssistantCapability::Email,
            AssistantCapability::Calendar,
            AssistantCapability::Reminders,
            AssistantCapability::Research,
            AssistantCapability::Approvals,
        ];
        let json = serde_json::to_string(&caps).unwrap();
        let deserialized: Vec<AssistantCapability> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, caps);
    }
}
