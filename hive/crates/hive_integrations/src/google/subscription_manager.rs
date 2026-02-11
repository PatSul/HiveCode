//! Subscription tracking and unsubscribe management.
//!
//! Tracks recurring senders (newsletters, marketing, etc.), parses
//! RFC 8058 / RFC 2369 unsubscribe headers, and provides bulk
//! subscription management operations.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::email_classifier::EmailCategory;

// ── Types ─────────────────────────────────────────────────────────

/// A tracked email subscription (recurring sender).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub sender_email: String,
    pub sender_name: Option<String>,
    pub category: EmailCategory,
    pub unsubscribe_method: Option<UnsubscribeMethod>,
    pub last_received: DateTime<Utc>,
    pub frequency: Option<String>,
    pub is_subscribed: bool,
    pub added_at: DateTime<Utc>,
}

/// How to unsubscribe from this sender.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UnsubscribeMethod {
    /// RFC 8058 one-click unsubscribe via POST.
    OneClick { url: String },
    /// RFC 2369 mailto: unsubscribe.
    MailTo { address: String },
    /// Standard web link.
    Link { url: String },
}

/// Aggregate statistics about tracked subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubscriptionStats {
    pub total: usize,
    pub active: usize,
    pub unsubscribed: usize,
    pub newsletters: usize,
    pub marketing: usize,
}

// ── Manager ───────────────────────────────────────────────────────

/// Manages a local list of tracked subscriptions.
pub struct SubscriptionManager {
    subscriptions: Vec<Subscription>,
}

impl SubscriptionManager {
    /// Create a new, empty manager.
    pub fn new() -> Self {
        Self {
            subscriptions: Vec::new(),
        }
    }

    /// Track a new subscription or update an existing one (matched by sender
    /// email, case-insensitive). Returns a reference to the subscription.
    pub fn track_subscription(
        &mut self,
        sender_email: &str,
        sender_name: Option<&str>,
        category: EmailCategory,
        unsubscribe_method: Option<UnsubscribeMethod>,
    ) -> &Subscription {
        let lower = sender_email.to_lowercase();

        // Update if already tracked.
        if let Some(pos) = self
            .subscriptions
            .iter()
            .position(|s| s.sender_email.to_lowercase() == lower)
        {
            let sub = &mut self.subscriptions[pos];
            sub.last_received = Utc::now();
            sub.category = category;
            if let Some(name) = sender_name {
                sub.sender_name = Some(name.to_string());
            }
            if unsubscribe_method.is_some() {
                sub.unsubscribe_method = unsubscribe_method;
            }
            return &self.subscriptions[pos];
        }

        // Otherwise, insert new.
        let now = Utc::now();
        let sub = Subscription {
            id: Uuid::new_v4().to_string(),
            sender_email: sender_email.to_string(),
            sender_name: sender_name.map(String::from),
            category,
            unsubscribe_method,
            last_received: now,
            frequency: None,
            is_subscribed: true,
            added_at: now,
        };
        self.subscriptions.push(sub);
        self.subscriptions.last().expect("just pushed")
    }

    /// Parse `List-Unsubscribe` and optional `List-Unsubscribe-Post` headers
    /// into an `UnsubscribeMethod`.
    ///
    /// Follows RFC 8058 (one-click) and RFC 2369 (mailto / http).
    pub fn parse_unsubscribe_headers(
        list_unsubscribe: &str,
        list_unsubscribe_post: Option<&str>,
    ) -> Option<UnsubscribeMethod> {
        let trimmed = list_unsubscribe.trim();
        if trimmed.is_empty() {
            return None;
        }

        // The header value may contain multiple entries wrapped in angle
        // brackets and separated by commas, e.g.:
        //   <mailto:unsub@example.com>, <https://example.com/unsub>
        let entries: Vec<&str> = trimmed
            .split(',')
            .map(|s| s.trim().trim_start_matches('<').trim_end_matches('>').trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut http_url: Option<&str> = None;
        let mut mailto_addr: Option<&str> = None;

        for entry in &entries {
            let lower = entry.to_lowercase();
            if lower.starts_with("https://") || lower.starts_with("http://") {
                http_url = Some(entry);
            } else if lower.starts_with("mailto:") {
                // Strip the "mailto:" prefix
                mailto_addr = Some(&entry["mailto:".len()..]);
            }
        }

        // RFC 8058: if List-Unsubscribe-Post is present and contains
        // "List-Unsubscribe=One-Click" alongside an HTTPS URL, prefer
        // one-click unsubscribe.
        if let Some(post) = list_unsubscribe_post {
            if post
                .to_lowercase()
                .contains("list-unsubscribe=one-click")
            {
                if let Some(url) = http_url {
                    return Some(UnsubscribeMethod::OneClick {
                        url: url.to_string(),
                    });
                }
            }
        }

        // Prefer HTTP link over mailto.
        if let Some(url) = http_url {
            return Some(UnsubscribeMethod::Link {
                url: url.to_string(),
            });
        }

        if let Some(addr) = mailto_addr {
            return Some(UnsubscribeMethod::MailTo {
                address: addr.to_string(),
            });
        }

        None
    }

    /// Look up a subscription by sender email (case-insensitive).
    pub fn get_subscription(&self, sender_email: &str) -> Option<&Subscription> {
        let lower = sender_email.to_lowercase();
        self.subscriptions
            .iter()
            .find(|s| s.sender_email.to_lowercase() == lower)
    }

    /// Return all tracked subscriptions.
    pub fn list_subscriptions(&self) -> &[Subscription] {
        &self.subscriptions
    }

    /// Mark a subscription as unsubscribed.
    pub fn mark_unsubscribed(&mut self, sender_email: &str) -> Result<()> {
        let lower = sender_email.to_lowercase();
        let sub = self
            .subscriptions
            .iter_mut()
            .find(|s| s.sender_email.to_lowercase() == lower);

        match sub {
            Some(s) => {
                s.is_subscribed = false;
                Ok(())
            }
            None => bail!("subscription not found for {sender_email}"),
        }
    }

    /// Return only newsletter subscriptions.
    pub fn list_newsletters(&self) -> Vec<&Subscription> {
        self.subscriptions
            .iter()
            .filter(|s| s.category == EmailCategory::Newsletter)
            .collect()
    }

    /// Compute aggregate statistics.
    pub fn stats(&self) -> SubscriptionStats {
        let total = self.subscriptions.len();
        let active = self.subscriptions.iter().filter(|s| s.is_subscribed).count();
        let unsubscribed = total - active;
        let newsletters = self
            .subscriptions
            .iter()
            .filter(|s| s.category == EmailCategory::Newsletter)
            .count();
        let marketing = self
            .subscriptions
            .iter()
            .filter(|s| s.category == EmailCategory::Marketing)
            .count();

        SubscriptionStats {
            total,
            active,
            unsubscribed,
            newsletters,
            marketing,
        }
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> SubscriptionManager {
        SubscriptionManager::new()
    }

    // 1. Track new subscription
    #[test]
    fn test_track_new_subscription() {
        let mut mgr = manager();
        let sub = mgr.track_subscription(
            "news@example.com",
            Some("Example News"),
            EmailCategory::Newsletter,
            None,
        );
        assert_eq!(sub.sender_email, "news@example.com");
        assert_eq!(sub.sender_name.as_deref(), Some("Example News"));
        assert_eq!(sub.category, EmailCategory::Newsletter);
        assert!(sub.is_subscribed);
    }

    // 2. Track updates existing subscription
    #[test]
    fn test_track_updates_existing() {
        let mut mgr = manager();
        mgr.track_subscription("news@example.com", Some("Old Name"), EmailCategory::Newsletter, None);
        let sub = mgr.track_subscription(
            "NEWS@example.com", // case-insensitive match
            Some("New Name"),
            EmailCategory::Marketing,
            None,
        );
        assert_eq!(sub.sender_name.as_deref(), Some("New Name"));
        assert_eq!(sub.category, EmailCategory::Marketing);
        assert_eq!(mgr.list_subscriptions().len(), 1);
    }

    // 3. Parse one-click unsubscribe (RFC 8058)
    #[test]
    fn test_parse_one_click_unsubscribe() {
        let method = SubscriptionManager::parse_unsubscribe_headers(
            "<https://example.com/unsub?id=123>",
            Some("List-Unsubscribe=One-Click"),
        );
        assert_eq!(
            method,
            Some(UnsubscribeMethod::OneClick {
                url: "https://example.com/unsub?id=123".into(),
            })
        );
    }

    // 4. Parse mailto unsubscribe
    #[test]
    fn test_parse_mailto_unsubscribe() {
        let method = SubscriptionManager::parse_unsubscribe_headers(
            "<mailto:unsub@example.com>",
            None,
        );
        assert_eq!(
            method,
            Some(UnsubscribeMethod::MailTo {
                address: "unsub@example.com".into(),
            })
        );
    }

    // 5. Parse link unsubscribe (HTTP without one-click post)
    #[test]
    fn test_parse_link_unsubscribe() {
        let method = SubscriptionManager::parse_unsubscribe_headers(
            "<https://example.com/unsubscribe>",
            None,
        );
        assert_eq!(
            method,
            Some(UnsubscribeMethod::Link {
                url: "https://example.com/unsubscribe".into(),
            })
        );
    }

    // 6. Parse header with both mailto and http prefers http
    #[test]
    fn test_parse_combined_header_prefers_http() {
        let method = SubscriptionManager::parse_unsubscribe_headers(
            "<mailto:unsub@example.com>, <https://example.com/unsub>",
            None,
        );
        assert_eq!(
            method,
            Some(UnsubscribeMethod::Link {
                url: "https://example.com/unsub".into(),
            })
        );
    }

    // 7. Parse empty header returns None
    #[test]
    fn test_parse_empty_header() {
        assert!(SubscriptionManager::parse_unsubscribe_headers("", None).is_none());
        assert!(SubscriptionManager::parse_unsubscribe_headers("   ", None).is_none());
    }

    // 8. Get subscription by email
    #[test]
    fn test_get_subscription() {
        let mut mgr = manager();
        mgr.track_subscription("test@example.com", None, EmailCategory::Marketing, None);
        assert!(mgr.get_subscription("test@example.com").is_some());
        assert!(mgr.get_subscription("TEST@EXAMPLE.COM").is_some());
        assert!(mgr.get_subscription("other@example.com").is_none());
    }

    // 9. Mark unsubscribed
    #[test]
    fn test_mark_unsubscribed() {
        let mut mgr = manager();
        mgr.track_subscription("news@example.com", None, EmailCategory::Newsletter, None);
        assert!(mgr.get_subscription("news@example.com").unwrap().is_subscribed);

        mgr.mark_unsubscribed("news@example.com").unwrap();
        assert!(!mgr.get_subscription("news@example.com").unwrap().is_subscribed);
    }

    // 10. Mark unsubscribed fails for unknown sender
    #[test]
    fn test_mark_unsubscribed_not_found() {
        let mut mgr = manager();
        let result = mgr.mark_unsubscribed("unknown@example.com");
        assert!(result.is_err());
    }

    // 11. List newsletters filters correctly
    #[test]
    fn test_list_newsletters() {
        let mut mgr = manager();
        mgr.track_subscription("news@example.com", None, EmailCategory::Newsletter, None);
        mgr.track_subscription("promo@shop.com", None, EmailCategory::Marketing, None);
        mgr.track_subscription("digest@tech.com", None, EmailCategory::Newsletter, None);

        let newsletters = mgr.list_newsletters();
        assert_eq!(newsletters.len(), 2);
        assert!(newsletters.iter().all(|s| s.category == EmailCategory::Newsletter));
    }

    // 12. Stats computation
    #[test]
    fn test_stats() {
        let mut mgr = manager();
        mgr.track_subscription("a@example.com", None, EmailCategory::Newsletter, None);
        mgr.track_subscription("b@example.com", None, EmailCategory::Marketing, None);
        mgr.track_subscription("c@example.com", None, EmailCategory::Newsletter, None);
        mgr.mark_unsubscribed("b@example.com").unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.active, 2);
        assert_eq!(stats.unsubscribed, 1);
        assert_eq!(stats.newsletters, 2);
        assert_eq!(stats.marketing, 1);
    }

    // 13. Stats on empty manager
    #[test]
    fn test_stats_empty() {
        let mgr = manager();
        let stats = mgr.stats();
        assert_eq!(
            stats,
            SubscriptionStats {
                total: 0,
                active: 0,
                unsubscribed: 0,
                newsletters: 0,
                marketing: 0,
            }
        );
    }

    // 14. Subscription serialization roundtrip
    #[test]
    fn test_subscription_serialization() {
        let sub = Subscription {
            id: "test-id".into(),
            sender_email: "sender@example.com".into(),
            sender_name: Some("Sender".into()),
            category: EmailCategory::Newsletter,
            unsubscribe_method: Some(UnsubscribeMethod::OneClick {
                url: "https://example.com/unsub".into(),
            }),
            last_received: Utc::now(),
            frequency: Some("weekly".into()),
            is_subscribed: true,
            added_at: Utc::now(),
        };
        let json = serde_json::to_string(&sub).unwrap();
        let back: Subscription = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test-id");
        assert_eq!(back.sender_email, "sender@example.com");
        assert_eq!(back.category, EmailCategory::Newsletter);
        assert!(back.is_subscribed);
    }

    // 15. UnsubscribeMethod serialization
    #[test]
    fn test_unsubscribe_method_serialization() {
        let methods = vec![
            UnsubscribeMethod::OneClick { url: "https://example.com".into() },
            UnsubscribeMethod::MailTo { address: "unsub@example.com".into() },
            UnsubscribeMethod::Link { url: "https://example.com/link".into() },
        ];
        for method in &methods {
            let json = serde_json::to_string(method).unwrap();
            let back: UnsubscribeMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, method);
        }
    }

    // 16. SubscriptionStats serialization
    #[test]
    fn test_stats_serialization() {
        let stats = SubscriptionStats {
            total: 10,
            active: 7,
            unsubscribed: 3,
            newsletters: 4,
            marketing: 2,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: SubscriptionStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stats);
    }

    // 17. Tracking with unsubscribe method
    #[test]
    fn test_track_with_unsubscribe() {
        let mut mgr = manager();
        let method = UnsubscribeMethod::Link {
            url: "https://example.com/unsub".into(),
        };
        let sub = mgr.track_subscription(
            "news@example.com",
            None,
            EmailCategory::Newsletter,
            Some(method),
        );
        assert!(sub.unsubscribe_method.is_some());
        match &sub.unsubscribe_method {
            Some(UnsubscribeMethod::Link { url }) => {
                assert_eq!(url, "https://example.com/unsub");
            }
            _ => panic!("expected Link method"),
        }
    }

    // 18. Default trait implementation
    #[test]
    fn test_default_manager() {
        let mgr = SubscriptionManager::default();
        assert!(mgr.list_subscriptions().is_empty());
    }
}
