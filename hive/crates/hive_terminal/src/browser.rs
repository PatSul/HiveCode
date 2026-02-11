use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// BrowserAction
// ---------------------------------------------------------------------------

/// An action to execute within a browser instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrowserAction {
    /// Navigate the browser to the given URL.
    Navigate { url: String },
    /// Click on the element matching the given CSS selector.
    Click { selector: String },
    /// Type text into the element matching the given CSS selector.
    Type { selector: String, text: String },
    /// Take a screenshot and save it to the given path.
    Screenshot { path: String },
    /// Wait for an element matching the selector to appear, with a timeout.
    WaitForSelector {
        selector: String,
        timeout_ms: u64,
    },
    /// Evaluate a JavaScript expression in the page context.
    Evaluate { script: String },
    /// Get the text content of the element matching the given CSS selector.
    GetText { selector: String },
    /// Get the value of an attribute on the element matching the given CSS
    /// selector.
    GetAttribute { selector: String, attribute: String },
}

// ---------------------------------------------------------------------------
// ActionResult
// ---------------------------------------------------------------------------

/// The result of executing a [`BrowserAction`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Whether the action completed successfully.
    pub success: bool,
    /// An optional return value (e.g. extracted text, attribute value,
    /// evaluated script result).
    pub value: Option<String>,
    /// An optional error message when `success` is `false`.
    pub error: Option<String>,
    /// Wall-clock duration of the action in milliseconds.
    pub duration_ms: u64,
}

impl ActionResult {
    /// Create a successful result with an optional value.
    fn ok(value: Option<String>, duration_ms: u64) -> Self {
        Self {
            success: true,
            value,
            error: None,
            duration_ms,
        }
    }

    /// Create a failed result with an error message.
    #[allow(dead_code)]
    fn fail(error: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            success: false,
            value: None,
            error: Some(error.into()),
            duration_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// BrowserInstance
// ---------------------------------------------------------------------------

/// Represents a single browser instance managed by the pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserInstance {
    /// Unique identifier for this instance.
    pub id: String,
    /// Timestamp when the instance was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the instance was last used.
    pub last_used: DateTime<Utc>,
    /// Whether the instance is currently executing an action.
    pub is_busy: bool,
    /// The URL currently loaded in this instance, if any.
    pub current_url: Option<String>,
}

impl BrowserInstance {
    /// Create a new idle browser instance with a random ID.
    fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            last_used: now,
            is_busy: false,
            current_url: None,
        }
    }
}

// ---------------------------------------------------------------------------
// BrowserPoolConfig
// ---------------------------------------------------------------------------

/// Configuration for the browser instance pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserPoolConfig {
    /// Maximum number of simultaneous browser instances.
    pub max_instances: usize,
    /// Seconds an instance may sit idle before being eligible for cleanup.
    pub idle_timeout_secs: u64,
    /// Whether to run browsers in headless mode.
    pub headless: bool,
}

impl Default for BrowserPoolConfig {
    fn default() -> Self {
        Self {
            max_instances: 3,
            idle_timeout_secs: 300,
            headless: true,
        }
    }
}

// ---------------------------------------------------------------------------
// BrowserPool
// ---------------------------------------------------------------------------

/// An in-memory pool of [`BrowserInstance`]s.
///
/// The pool enforces a maximum number of concurrent instances and supports
/// idle-timeout cleanup. This is a data-structure stub; no real browser
/// processes are spawned.
#[derive(Debug, Clone)]
pub struct BrowserPool {
    config: BrowserPoolConfig,
    instances: HashMap<String, BrowserInstance>,
}

impl Default for BrowserPool {
    fn default() -> Self {
        Self::new(BrowserPoolConfig::default())
    }
}

impl BrowserPool {
    /// Create a new pool with the given configuration.
    pub fn new(config: BrowserPoolConfig) -> Self {
        debug!(
            max = config.max_instances,
            idle_timeout = config.idle_timeout_secs,
            headless = config.headless,
            "browser pool created"
        );
        Self {
            config,
            instances: HashMap::new(),
        }
    }

    /// Acquire an idle browser instance, or create a new one if the pool is
    /// not at capacity.
    ///
    /// Returns the instance ID on success. Returns an error if all instances
    /// are busy and the pool is at maximum capacity.
    pub fn acquire(&mut self) -> Result<String> {
        // First, try to find an idle instance.
        if let Some(id) = self
            .instances
            .values()
            .find(|inst| !inst.is_busy)
            .map(|inst| inst.id.clone())
        {
            let inst = self.instances.get_mut(&id).unwrap();
            inst.is_busy = true;
            inst.last_used = Utc::now();
            debug!(id = %id, "acquired existing browser instance");
            return Ok(id);
        }

        // No idle instance available; try to create one.
        if self.instances.len() >= self.config.max_instances {
            bail!(
                "Browser pool at maximum capacity ({} instances, all busy)",
                self.config.max_instances
            );
        }

        let mut instance = BrowserInstance::new();
        instance.is_busy = true;
        let id = instance.id.clone();
        debug!(id = %id, "created new browser instance");
        self.instances.insert(id.clone(), instance);
        Ok(id)
    }

    /// Release a browser instance, marking it as no longer busy.
    ///
    /// Returns an error if the instance ID is unknown.
    pub fn release(&mut self, instance_id: &str) -> Result<()> {
        let inst = self
            .instances
            .get_mut(instance_id)
            .with_context(|| format!("Unknown browser instance: {instance_id}"))?;
        inst.is_busy = false;
        inst.last_used = Utc::now();
        debug!(id = %instance_id, "released browser instance");
        Ok(())
    }

    /// Remove an instance from the pool entirely.
    ///
    /// Returns an error if the instance ID is unknown.
    pub fn remove(&mut self, instance_id: &str) -> Result<()> {
        if self.instances.remove(instance_id).is_none() {
            bail!("Unknown browser instance: {instance_id}");
        }
        debug!(id = %instance_id, "removed browser instance");
        Ok(())
    }

    /// Number of instances that are currently busy.
    pub fn active_count(&self) -> usize {
        self.instances.values().filter(|i| i.is_busy).count()
    }

    /// Number of instances that are currently idle.
    pub fn idle_count(&self) -> usize {
        self.instances.values().filter(|i| !i.is_busy).count()
    }

    /// Total number of instances in the pool.
    pub fn total_count(&self) -> usize {
        self.instances.len()
    }

    /// Remove all instances that have been idle longer than the configured
    /// timeout. Returns the number of instances removed.
    pub fn cleanup_idle(&mut self) -> usize {
        let cutoff = Utc::now()
            - chrono::Duration::seconds(self.config.idle_timeout_secs as i64);
        let to_remove: Vec<String> = self
            .instances
            .values()
            .filter(|i| !i.is_busy && i.last_used < cutoff)
            .map(|i| i.id.clone())
            .collect();
        let count = to_remove.len();
        for id in &to_remove {
            self.instances.remove(id);
        }
        if count > 0 {
            debug!(removed = count, "cleaned up idle browser instances");
        }
        count
    }

    /// Return a reference to a specific instance by ID, if it exists.
    pub fn get(&self, instance_id: &str) -> Option<&BrowserInstance> {
        self.instances.get(instance_id)
    }

    /// Return a mutable reference to a specific instance by ID, if it exists.
    pub fn get_mut(&mut self, instance_id: &str) -> Option<&mut BrowserInstance> {
        self.instances.get_mut(instance_id)
    }

    /// Return the pool configuration.
    pub fn config(&self) -> &BrowserPoolConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// BrowserAutomation
// ---------------------------------------------------------------------------

/// High-level browser automation facade.
///
/// Wraps a [`BrowserPool`] and provides methods to execute browser actions.
/// Since no real browser engine is embedded, actions return simulated
/// results. The data structures and API shape match the Electron
/// `browser-automation.ts` / `browser-pool.ts` design.
#[derive(Debug, Clone)]
pub struct BrowserAutomation {
    pool: BrowserPool,
}

impl BrowserAutomation {
    /// Create a new automation controller with the given pool config.
    pub fn new(pool_config: BrowserPoolConfig) -> Self {
        Self {
            pool: BrowserPool::new(pool_config),
        }
    }

    /// Execute a single [`BrowserAction`] against the given instance.
    ///
    /// Returns a simulated [`ActionResult`]. In a production implementation
    /// this would drive a real browser via CDP or WebDriver.
    pub fn execute_action(
        &mut self,
        instance_id: &str,
        action: &BrowserAction,
    ) -> Result<ActionResult> {
        // Verify the instance exists.
        let instance = self
            .pool
            .get_mut(instance_id)
            .with_context(|| format!("Unknown browser instance: {instance_id}"))?;

        let start = Instant::now();

        // Simulate the action.
        let result = match action {
            BrowserAction::Navigate { url } => {
                debug!(id = %instance_id, url = %url, "navigate");
                instance.current_url = Some(url.clone());
                ActionResult::ok(None, start.elapsed().as_millis() as u64)
            }
            BrowserAction::Click { selector } => {
                debug!(id = %instance_id, selector = %selector, "click");
                ActionResult::ok(None, start.elapsed().as_millis() as u64)
            }
            BrowserAction::Type { selector, text } => {
                debug!(id = %instance_id, selector = %selector, len = text.len(), "type");
                ActionResult::ok(None, start.elapsed().as_millis() as u64)
            }
            BrowserAction::Screenshot { path } => {
                debug!(id = %instance_id, path = %path, "screenshot");
                ActionResult::ok(Some(path.clone()), start.elapsed().as_millis() as u64)
            }
            BrowserAction::WaitForSelector {
                selector,
                timeout_ms,
            } => {
                debug!(
                    id = %instance_id,
                    selector = %selector,
                    timeout_ms = timeout_ms,
                    "wait_for_selector"
                );
                ActionResult::ok(None, start.elapsed().as_millis() as u64)
            }
            BrowserAction::Evaluate { script } => {
                debug!(id = %instance_id, script_len = script.len(), "evaluate");
                ActionResult::ok(
                    Some("undefined".to_string()),
                    start.elapsed().as_millis() as u64,
                )
            }
            BrowserAction::GetText { selector } => {
                debug!(id = %instance_id, selector = %selector, "get_text");
                ActionResult::ok(
                    Some(String::new()),
                    start.elapsed().as_millis() as u64,
                )
            }
            BrowserAction::GetAttribute {
                selector,
                attribute,
            } => {
                debug!(
                    id = %instance_id,
                    selector = %selector,
                    attribute = %attribute,
                    "get_attribute"
                );
                ActionResult::ok(None, start.elapsed().as_millis() as u64)
            }
        };

        // Update last-used timestamp.
        if let Some(inst) = self.pool.get_mut(instance_id) {
            inst.last_used = Utc::now();
        }

        Ok(result)
    }

    /// Execute a sequence of actions in order. Stops at the first failure and
    /// returns all results collected so far.
    pub fn execute_sequence(
        &mut self,
        instance_id: &str,
        actions: &[BrowserAction],
    ) -> Result<Vec<ActionResult>> {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            let result = self.execute_action(instance_id, action)?;
            let failed = !result.success;
            results.push(result);
            if failed {
                break;
            }
        }
        Ok(results)
    }

    /// Convenience: navigate to a URL.
    pub fn navigate(&mut self, instance_id: &str, url: &str) -> Result<ActionResult> {
        self.execute_action(
            instance_id,
            &BrowserAction::Navigate {
                url: url.to_string(),
            },
        )
    }

    /// Convenience: take a screenshot.
    pub fn screenshot(&mut self, instance_id: &str, path: &str) -> Result<ActionResult> {
        self.execute_action(
            instance_id,
            &BrowserAction::Screenshot {
                path: path.to_string(),
            },
        )
    }

    /// Access the underlying browser pool.
    pub fn pool(&self) -> &BrowserPool {
        &self.pool
    }

    /// Mutable access to the underlying browser pool.
    pub fn pool_mut(&mut self) -> &mut BrowserPool {
        &mut self.pool
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- BrowserPoolConfig defaults -----------------------------------------

    #[test]
    fn pool_config_defaults() {
        let cfg = BrowserPoolConfig::default();
        assert_eq!(cfg.max_instances, 3);
        assert_eq!(cfg.idle_timeout_secs, 300);
        assert!(cfg.headless);
    }

    #[test]
    fn pool_config_serialization_roundtrip() {
        let cfg = BrowserPoolConfig {
            max_instances: 5,
            idle_timeout_secs: 600,
            headless: false,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: BrowserPoolConfig =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.max_instances, 5);
        assert_eq!(restored.idle_timeout_secs, 600);
        assert!(!restored.headless);
    }

    // -- BrowserInstance ----------------------------------------------------

    #[test]
    fn instance_new_is_idle() {
        let inst = BrowserInstance::new();
        assert!(!inst.is_busy);
        assert!(inst.current_url.is_none());
        assert!(!inst.id.is_empty());
    }

    #[test]
    fn instance_serialization_roundtrip() {
        let inst = BrowserInstance::new();
        let json = serde_json::to_string(&inst).expect("serialize");
        let restored: BrowserInstance =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.id, inst.id);
        assert_eq!(restored.is_busy, inst.is_busy);
    }

    // -- BrowserPool -------------------------------------------------------

    #[test]
    fn pool_default_is_empty() {
        let pool = BrowserPool::default();
        assert_eq!(pool.total_count(), 0);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.idle_count(), 0);
    }

    #[test]
    fn pool_acquire_creates_instance() {
        let mut pool = BrowserPool::default();
        let id = pool.acquire().expect("should acquire");
        assert_eq!(pool.total_count(), 1);
        assert_eq!(pool.active_count(), 1);
        assert_eq!(pool.idle_count(), 0);
        assert!(pool.get(&id).is_some());
        assert!(pool.get(&id).unwrap().is_busy);
    }

    #[test]
    fn pool_release_marks_idle() {
        let mut pool = BrowserPool::default();
        let id = pool.acquire().expect("should acquire");
        pool.release(&id).expect("should release");
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.idle_count(), 1);
        assert!(!pool.get(&id).unwrap().is_busy);
    }

    #[test]
    fn pool_acquire_reuses_idle_instance() {
        let mut pool = BrowserPool::default();
        let id1 = pool.acquire().expect("first acquire");
        pool.release(&id1).expect("release");
        let id2 = pool.acquire().expect("second acquire");
        // Should reuse the same instance.
        assert_eq!(id1, id2);
        assert_eq!(pool.total_count(), 1);
    }

    #[test]
    fn pool_acquire_fails_at_capacity() {
        let config = BrowserPoolConfig {
            max_instances: 2,
            ..Default::default()
        };
        let mut pool = BrowserPool::new(config);
        pool.acquire().expect("first");
        pool.acquire().expect("second");
        let result = pool.acquire();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("maximum capacity"),
            "expected capacity error, got: {msg}"
        );
    }

    #[test]
    fn pool_remove_instance() {
        let mut pool = BrowserPool::default();
        let id = pool.acquire().expect("acquire");
        assert_eq!(pool.total_count(), 1);
        pool.remove(&id).expect("remove");
        assert_eq!(pool.total_count(), 0);
        assert!(pool.get(&id).is_none());
    }

    #[test]
    fn pool_remove_unknown_instance_errors() {
        let mut pool = BrowserPool::default();
        let result = pool.remove("nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn pool_release_unknown_instance_errors() {
        let mut pool = BrowserPool::default();
        let result = pool.release("nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn pool_cleanup_idle_removes_expired() {
        let config = BrowserPoolConfig {
            idle_timeout_secs: 0, // everything is immediately expired
            ..Default::default()
        };
        let mut pool = BrowserPool::new(config);
        let id = pool.acquire().expect("acquire");
        pool.release(&id).expect("release");
        // Force the last_used timestamp into the past.
        pool.get_mut(&id).unwrap().last_used =
            Utc::now() - chrono::Duration::seconds(10);
        let removed = pool.cleanup_idle();
        assert_eq!(removed, 1);
        assert_eq!(pool.total_count(), 0);
    }

    #[test]
    fn pool_cleanup_idle_keeps_busy() {
        let config = BrowserPoolConfig {
            idle_timeout_secs: 0,
            ..Default::default()
        };
        let mut pool = BrowserPool::new(config);
        let _id = pool.acquire().expect("acquire");
        // Instance is busy, so cleanup should not remove it even with
        // zero timeout.
        let removed = pool.cleanup_idle();
        assert_eq!(removed, 0);
        assert_eq!(pool.total_count(), 1);
    }

    // -- BrowserAction serialization ----------------------------------------

    #[test]
    fn action_navigate_serialization() {
        let action = BrowserAction::Navigate {
            url: "https://example.com".into(),
        };
        let json = serde_json::to_string(&action).expect("serialize");
        let restored: BrowserAction =
            serde_json::from_str(&json).expect("deserialize");
        match restored {
            BrowserAction::Navigate { url } => {
                assert_eq!(url, "https://example.com");
            }
            other => panic!("expected Navigate, got {other:?}"),
        }
    }

    #[test]
    fn action_type_serialization() {
        let action = BrowserAction::Type {
            selector: "#input".into(),
            text: "hello".into(),
        };
        let json = serde_json::to_string(&action).expect("serialize");
        assert!(json.contains("hello"));
        let restored: BrowserAction =
            serde_json::from_str(&json).expect("deserialize");
        match restored {
            BrowserAction::Type { selector, text } => {
                assert_eq!(selector, "#input");
                assert_eq!(text, "hello");
            }
            other => panic!("expected Type, got {other:?}"),
        }
    }

    #[test]
    fn action_get_attribute_serialization() {
        let action = BrowserAction::GetAttribute {
            selector: "img.logo".into(),
            attribute: "src".into(),
        };
        let json = serde_json::to_string(&action).expect("serialize");
        let restored: BrowserAction =
            serde_json::from_str(&json).expect("deserialize");
        match restored {
            BrowserAction::GetAttribute {
                selector,
                attribute,
            } => {
                assert_eq!(selector, "img.logo");
                assert_eq!(attribute, "src");
            }
            other => panic!("expected GetAttribute, got {other:?}"),
        }
    }

    // -- ActionResult -------------------------------------------------------

    #[test]
    fn action_result_ok_helper() {
        let r = ActionResult::ok(Some("value".into()), 42);
        assert!(r.success);
        assert_eq!(r.value.as_deref(), Some("value"));
        assert!(r.error.is_none());
        assert_eq!(r.duration_ms, 42);
    }

    #[test]
    fn action_result_fail_helper() {
        let r = ActionResult::fail("something broke", 7);
        assert!(!r.success);
        assert!(r.value.is_none());
        assert_eq!(r.error.as_deref(), Some("something broke"));
        assert_eq!(r.duration_ms, 7);
    }

    #[test]
    fn action_result_serialization_roundtrip() {
        let r = ActionResult::ok(Some("hello".into()), 100);
        let json = serde_json::to_string(&r).expect("serialize");
        let restored: ActionResult =
            serde_json::from_str(&json).expect("deserialize");
        assert!(restored.success);
        assert_eq!(restored.value.as_deref(), Some("hello"));
        assert_eq!(restored.duration_ms, 100);
    }

    // -- BrowserAutomation --------------------------------------------------

    #[test]
    fn automation_navigate_updates_url() {
        let mut auto = BrowserAutomation::new(BrowserPoolConfig::default());
        let id = auto.pool_mut().acquire().expect("acquire");
        let result = auto
            .navigate(&id, "https://example.com")
            .expect("navigate");
        assert!(result.success);
        assert_eq!(
            auto.pool().get(&id).unwrap().current_url.as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn automation_screenshot_returns_path() {
        let mut auto = BrowserAutomation::new(BrowserPoolConfig::default());
        let id = auto.pool_mut().acquire().expect("acquire");
        let result = auto
            .screenshot(&id, "/tmp/shot.png")
            .expect("screenshot");
        assert!(result.success);
        assert_eq!(result.value.as_deref(), Some("/tmp/shot.png"));
    }

    #[test]
    fn automation_execute_action_unknown_instance() {
        let mut auto = BrowserAutomation::new(BrowserPoolConfig::default());
        let result = auto.execute_action(
            "no-such-id",
            &BrowserAction::Click {
                selector: "#btn".into(),
            },
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unknown"), "got: {msg}");
    }

    #[test]
    fn automation_execute_sequence_all_succeed() {
        let mut auto = BrowserAutomation::new(BrowserPoolConfig::default());
        let id = auto.pool_mut().acquire().expect("acquire");
        let actions = vec![
            BrowserAction::Navigate {
                url: "https://example.com".into(),
            },
            BrowserAction::Click {
                selector: "#btn".into(),
            },
            BrowserAction::GetText {
                selector: "h1".into(),
            },
        ];
        let results = auto
            .execute_sequence(&id, &actions)
            .expect("sequence");
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.success));
    }

    #[test]
    fn automation_evaluate_returns_value() {
        let mut auto = BrowserAutomation::new(BrowserPoolConfig::default());
        let id = auto.pool_mut().acquire().expect("acquire");
        let result = auto
            .execute_action(
                &id,
                &BrowserAction::Evaluate {
                    script: "document.title".into(),
                },
            )
            .expect("evaluate");
        assert!(result.success);
        assert!(result.value.is_some());
    }

    #[test]
    fn automation_pool_accessor() {
        let auto = BrowserAutomation::new(BrowserPoolConfig::default());
        assert_eq!(auto.pool().total_count(), 0);
        assert_eq!(auto.pool().config().max_instances, 3);
    }

    #[test]
    fn automation_wait_for_selector() {
        let mut auto = BrowserAutomation::new(BrowserPoolConfig::default());
        let id = auto.pool_mut().acquire().expect("acquire");
        let result = auto
            .execute_action(
                &id,
                &BrowserAction::WaitForSelector {
                    selector: ".loaded".into(),
                    timeout_ms: 5000,
                },
            )
            .expect("wait");
        assert!(result.success);
    }

    #[test]
    fn automation_type_action() {
        let mut auto = BrowserAutomation::new(BrowserPoolConfig::default());
        let id = auto.pool_mut().acquire().expect("acquire");
        let result = auto
            .execute_action(
                &id,
                &BrowserAction::Type {
                    selector: "#search".into(),
                    text: "rust lang".into(),
                },
            )
            .expect("type");
        assert!(result.success);
    }
}
