use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::IpAddr;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;

/// A registered webhook that receives event notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

impl Webhook {
    /// Create a new webhook with a generated UUID.
    pub fn new(name: impl Into<String>, url: impl Into<String>, events: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            url: url.into(),
            events,
            active: true,
            created_at: Utc::now(),
        }
    }

    /// Check whether this webhook is subscribed to the given event.
    pub fn is_subscribed_to(&self, event: &str) -> bool {
        self.active && self.events.iter().any(|e| e == event)
    }
}

/// Validate that a webhook URL is safe to deliver to.
///
/// Enforces HTTPS and blocks private/internal IP addresses to prevent SSRF.
fn validate_webhook_url(raw: &str) -> Result<(), String> {
    let parsed = Url::parse(raw).map_err(|e| format!("Invalid URL: {e}"))?;

    if parsed.scheme() != "https" {
        return Err("Webhook URLs must use HTTPS".into());
    }

    if let Some(host) = parsed.host_str() {
        let blocked_hosts = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]"];
        if blocked_hosts.contains(&host) {
            return Err(format!("Webhook URL blocked: {host} is a local address"));
        }

        if let Ok(ip) = host.parse::<IpAddr>() {
            match ip {
                IpAddr::V4(v4)
                    if v4.is_private()
                        || v4.is_loopback()
                        || v4.is_link_local()
                        || v4.is_broadcast()
                        || v4.is_unspecified() =>
                {
                    return Err(format!(
                        "Webhook URL blocked: {ip} is a private/reserved address"
                    ));
                }
                IpAddr::V6(v6) if v6.is_loopback() || v6.is_unspecified() => {
                    return Err(format!(
                        "Webhook URL blocked: {ip} is a private/reserved address"
                    ));
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Registry that manages webhook subscriptions and dispatches events.
pub struct WebhookRegistry {
    webhooks: Vec<Webhook>,
    client: Client,
}

impl WebhookRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            webhooks: Vec::new(),
            client: Client::new(),
        }
    }

    /// Register a new webhook and return its ID.
    ///
    /// Returns an error if the webhook URL fails validation (non-HTTPS or
    /// targets a private/local address).
    pub fn register(&mut self, webhook: Webhook) -> Result<String, String> {
        validate_webhook_url(&webhook.url)?;
        let id = webhook.id.clone();
        debug!(id = %id, name = %webhook.name, "registering webhook");
        self.webhooks.push(webhook);
        Ok(id)
    }

    /// Unregister a webhook by ID. Returns `true` if it was found and removed.
    pub fn unregister(&mut self, id: &str) -> bool {
        let before = self.webhooks.len();
        self.webhooks.retain(|w| w.id != id);
        let removed = self.webhooks.len() < before;
        if removed {
            debug!(id = %id, "unregistered webhook");
        } else {
            warn!(id = %id, "webhook not found for unregister");
        }
        removed
    }

    /// List all registered webhooks.
    pub fn list(&self) -> &[Webhook] {
        &self.webhooks
    }

    /// Return the number of registered webhooks.
    pub fn len(&self) -> usize {
        self.webhooks.len()
    }

    /// Return whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.webhooks.is_empty()
    }

    /// Find a webhook by ID.
    pub fn get(&self, id: &str) -> Option<&Webhook> {
        self.webhooks.iter().find(|w| w.id == id)
    }

    /// Trigger an event, sending the payload to all subscribed active webhooks.
    ///
    /// Returns the number of webhooks that were successfully notified.
    /// Delivery failures are logged but do not stop other deliveries.
    pub async fn trigger(&self, event: &str, payload: &Value) -> usize {
        let subscribers: Vec<&Webhook> = self
            .webhooks
            .iter()
            .filter(|w| w.is_subscribed_to(event))
            .collect();

        if subscribers.is_empty() {
            debug!(event = %event, "no subscribers for event");
            return 0;
        }

        let body = serde_json::json!({
            "event": event,
            "payload": payload,
            "timestamp": Utc::now().to_rfc3339(),
        });

        let mut success_count = 0;

        for webhook in &subscribers {
            debug!(
                id = %webhook.id,
                name = %webhook.name,
                url = %webhook.url,
                event = %event,
                "delivering webhook"
            );

            let result = self.client.post(&webhook.url).json(&body).send().await;

            match result {
                Ok(response) if response.status().is_success() => {
                    success_count += 1;
                }
                Ok(response) => {
                    warn!(
                        id = %webhook.id,
                        status = %response.status(),
                        "webhook delivery returned non-success status"
                    );
                }
                Err(err) => {
                    warn!(
                        id = %webhook.id,
                        error = %err,
                        "webhook delivery failed"
                    );
                }
            }
        }

        success_count
    }
}

impl Default for WebhookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_webhook(name: &str, events: Vec<&str>) -> Webhook {
        Webhook::new(
            name,
            format!("https://example.com/hooks/{name}"),
            events.into_iter().map(String::from).collect(),
        )
    }

    #[test]
    fn test_webhook_creation() {
        let wh = Webhook::new("test", "https://example.com/hook", vec!["push".into()]);
        assert!(!wh.id.is_empty());
        assert_eq!(wh.name, "test");
        assert_eq!(wh.url, "https://example.com/hook");
        assert!(wh.active);
        assert_eq!(wh.events, vec!["push"]);
    }

    #[test]
    fn test_is_subscribed_to() {
        let wh = sample_webhook("ci", vec!["push", "pr"]);
        assert!(wh.is_subscribed_to("push"));
        assert!(wh.is_subscribed_to("pr"));
        assert!(!wh.is_subscribed_to("issue"));
    }

    #[test]
    fn test_inactive_webhook_not_subscribed() {
        let mut wh = sample_webhook("ci", vec!["push"]);
        wh.active = false;
        assert!(!wh.is_subscribed_to("push"));
    }

    #[test]
    fn test_register_and_list() {
        let mut registry = WebhookRegistry::new();
        assert!(registry.is_empty());

        let wh = sample_webhook("deploy", vec!["deploy"]);
        let id = registry.register(wh).unwrap();

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let listed = registry.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
    }

    #[test]
    fn test_unregister() {
        let mut registry = WebhookRegistry::new();
        let wh = sample_webhook("temp", vec!["test"]);
        let id = registry.register(wh).unwrap();
        assert_eq!(registry.len(), 1);

        assert!(registry.unregister(&id));
        assert_eq!(registry.len(), 0);

        // Unregistering again returns false.
        assert!(!registry.unregister(&id));
    }

    #[test]
    fn test_get_by_id() {
        let mut registry = WebhookRegistry::new();
        let wh = sample_webhook("finder", vec!["event"]);
        let id = registry.register(wh).unwrap();

        let found = registry.get(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "finder");

        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_webhook_serialization_roundtrip() {
        let wh = sample_webhook("serial", vec!["push", "tag"]);
        let json = serde_json::to_string(&wh).unwrap();
        let deserialized: Webhook = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, wh.id);
        assert_eq!(deserialized.name, wh.name);
        assert_eq!(deserialized.url, wh.url);
        assert_eq!(deserialized.events, wh.events);
        assert_eq!(deserialized.active, wh.active);
    }

    #[test]
    fn test_multiple_webhooks_different_events() {
        let mut registry = WebhookRegistry::new();
        registry
            .register(sample_webhook("a", vec!["push"]))
            .unwrap();
        registry
            .register(sample_webhook("b", vec!["deploy"]))
            .unwrap();
        registry
            .register(sample_webhook("c", vec!["push", "deploy"]))
            .unwrap();

        assert_eq!(registry.len(), 3);

        let push_subscribers: Vec<_> = registry
            .list()
            .iter()
            .filter(|w| w.is_subscribed_to("push"))
            .collect();
        assert_eq!(push_subscribers.len(), 2);

        let deploy_subscribers: Vec<_> = registry
            .list()
            .iter()
            .filter(|w| w.is_subscribed_to("deploy"))
            .collect();
        assert_eq!(deploy_subscribers.len(), 2);
    }

    #[tokio::test]
    async fn test_trigger_no_subscribers_returns_zero() {
        let registry = WebhookRegistry::new();
        let count = registry
            .trigger("unknown_event", &serde_json::json!({}))
            .await;
        assert_eq!(count, 0);
    }

    // -----------------------------------------------------------------------
    // URL validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_rejects_http() {
        let wh = Webhook::new("bad", "http://example.com/hook", vec!["push".into()]);
        let mut registry = WebhookRegistry::new();
        let err = registry.register(wh).unwrap_err();
        assert!(err.contains("HTTPS"), "expected HTTPS error, got: {err}");
    }

    #[test]
    fn test_validate_rejects_localhost() {
        let wh = Webhook::new("bad", "https://localhost/hook", vec!["push".into()]);
        let mut registry = WebhookRegistry::new();
        let err = registry.register(wh).unwrap_err();
        assert!(
            err.contains("local address"),
            "expected local address error, got: {err}"
        );
    }

    #[test]
    fn test_validate_rejects_private_ip() {
        for url in [
            "https://127.0.0.1/hook",
            "https://10.0.0.1/hook",
            "https://192.168.1.1/hook",
            "https://172.16.0.1/hook",
        ] {
            let wh = Webhook::new("bad", url, vec!["push".into()]);
            let mut registry = WebhookRegistry::new();
            let result = registry.register(wh);
            assert!(result.is_err(), "expected rejection for {url}");
        }
    }

    #[test]
    fn test_validate_rejects_link_local() {
        let wh = Webhook::new("bad", "https://169.254.169.254/hook", vec!["push".into()]);
        let mut registry = WebhookRegistry::new();
        assert!(registry.register(wh).is_err());
    }

    #[test]
    fn test_validate_accepts_valid_https() {
        let wh = Webhook::new(
            "good",
            "https://hooks.example.com/event",
            vec!["push".into()],
        );
        let mut registry = WebhookRegistry::new();
        assert!(registry.register(wh).is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_url() {
        let wh = Webhook::new("bad", "not a url", vec!["push".into()]);
        let mut registry = WebhookRegistry::new();
        let err = registry.register(wh).unwrap_err();
        assert!(
            err.contains("Invalid URL"),
            "expected Invalid URL error, got: {err}"
        );
    }
}
