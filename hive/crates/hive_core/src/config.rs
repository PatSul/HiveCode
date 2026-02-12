use anyhow::{Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

use crate::secure_storage::SecureStorage;

// ---------------------------------------------------------------------------
// Secure key storage file helpers
// ---------------------------------------------------------------------------

/// Storage key names for each provider's API key.
const KEY_ANTHROPIC: &str = "api_key_anthropic";
const KEY_OPENAI: &str = "api_key_openai";
const KEY_OPENROUTER: &str = "api_key_openrouter";
const KEY_GOOGLE: &str = "api_key_google";
const KEY_GROQ: &str = "api_key_groq";
const KEY_HUGGINGFACE: &str = "api_key_huggingface";
const KEY_LITELLM: &str = "api_key_litellm";
const KEY_ELEVENLABS: &str = "api_key_elevenlabs";
const KEY_TELNYX: &str = "api_key_telnyx";

/// Path to the encrypted key store: `~/.hive/keys.enc`
fn keys_file_path() -> Result<PathBuf> {
    Ok(HiveConfig::base_dir()?.join("keys.enc"))
}

/// Load encrypted key map from disk. Returns an empty map if the file is
/// missing or unreadable (graceful degradation).
fn load_key_map(path: &PathBuf) -> HashMap<String, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Save encrypted key map to disk.
fn save_key_map(path: &PathBuf, map: &HashMap<String, String>) -> Result<()> {
    let content = serde_json::to_string_pretty(map)?;
    std::fs::write(path, content)
        .with_context(|| format!("Failed to write key store: {}", path.display()))?;
    Ok(())
}

/// Retrieve a single API key from the encrypted store, decrypting with the
/// given `SecureStorage`. Returns `None` on any error (missing key, decryption
/// failure, etc.) so callers degrade gracefully.
fn get_secure_key(
    storage: &SecureStorage,
    map: &HashMap<String, String>,
    name: &str,
) -> Option<String> {
    let encrypted = map.get(name)?;
    match storage.decrypt(encrypted) {
        Ok(plaintext) if !plaintext.is_empty() => Some(plaintext),
        _ => None,
    }
}

/// Encrypt and store a single API key into the map. If the value is `None` or
/// empty the entry is removed.
fn set_secure_key(
    storage: &SecureStorage,
    map: &mut HashMap<String, String>,
    name: &str,
    value: &Option<String>,
) -> Result<()> {
    match value {
        Some(v) if !v.is_empty() => {
            let encrypted = storage.encrypt(v)?;
            map.insert(name.to_string(), encrypted);
        }
        _ => {
            map.remove(name);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HiveConfig
// ---------------------------------------------------------------------------

/// Application configuration stored at `~/.hive/config.json`.
///
/// API keys are **never** written to the JSON config file. They are stored
/// separately via `SecureStorage` in `~/.hive/keys.enc` (AES-256-GCM encrypted).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HiveConfig {
    // API keys -- skipped during JSON serialization.
    // Loaded from / saved to SecureStorage by ConfigManager.
    #[serde(skip)]
    pub anthropic_api_key: Option<String>,
    #[serde(skip)]
    pub openai_api_key: Option<String>,
    #[serde(skip)]
    pub openrouter_api_key: Option<String>,
    #[serde(skip)]
    pub google_api_key: Option<String>,
    #[serde(skip)]
    pub groq_api_key: Option<String>,
    #[serde(skip)]
    pub huggingface_api_key: Option<String>,
    #[serde(skip)]
    pub litellm_api_key: Option<String>,
    #[serde(skip)]
    pub elevenlabs_api_key: Option<String>,
    #[serde(skip)]
    pub telnyx_api_key: Option<String>,

    // Voice & TTS
    pub tts_provider: String,
    pub tts_voice_id: Option<String>,
    pub tts_speed: f32,
    pub tts_enabled: bool,
    pub tts_auto_speak: bool,
    pub clawdtalk_enabled: bool,
    pub clawdtalk_bot_pin: Option<String>,

    // Local AI / Proxy
    pub ollama_url: String,
    pub lmstudio_url: String,
    pub litellm_url: Option<String>,
    pub local_provider_url: Option<String>,
    pub privacy_mode: bool,

    // Model routing
    pub default_model: String,
    pub auto_routing: bool,

    // Budget
    pub daily_budget_usd: f64,
    pub monthly_budget_usd: f64,

    // UI
    pub theme: String,
    pub font_size: u32,

    // General
    pub auto_update: bool,
    pub notifications_enabled: bool,
    pub log_level: String,
}

impl Default for HiveConfig {
    fn default() -> Self {
        Self {
            anthropic_api_key: None,
            openai_api_key: None,
            openrouter_api_key: None,
            google_api_key: None,
            groq_api_key: None,
            huggingface_api_key: None,
            litellm_api_key: None,
            elevenlabs_api_key: None,
            telnyx_api_key: None,
            tts_provider: "qwen3".into(),
            tts_voice_id: None,
            tts_speed: 1.0,
            tts_enabled: false,
            tts_auto_speak: false,
            clawdtalk_enabled: false,
            clawdtalk_bot_pin: None,
            ollama_url: "http://localhost:11434".into(),
            lmstudio_url: "http://localhost:1234".into(),
            litellm_url: None,
            local_provider_url: None,
            privacy_mode: false,
            default_model: String::new(),
            auto_routing: true,
            daily_budget_usd: 10.0,
            monthly_budget_usd: 100.0,
            theme: "dark".into(),
            font_size: 14,
            auto_update: true,
            notifications_enabled: true,
            log_level: "info".into(),
        }
    }
}

impl HiveConfig {
    /// Returns the base config directory: `~/.hive/`
    pub fn base_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".hive"))
    }

    /// Returns the config file path: `~/.hive/config.json`
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::base_dir()?.join("config.json"))
    }

    /// Returns the conversations directory: `~/.hive/conversations/`
    pub fn conversations_dir() -> Result<PathBuf> {
        Ok(Self::base_dir()?.join("conversations"))
    }

    /// Returns the logs directory: `~/.hive/logs/`
    pub fn logs_dir() -> Result<PathBuf> {
        Ok(Self::base_dir()?.join("logs"))
    }

    /// Returns the database path: `~/.hive/memory.db`
    pub fn db_path() -> Result<PathBuf> {
        Ok(Self::base_dir()?.join("memory.db"))
    }

    /// Ensures all required directories exist.
    pub fn ensure_dirs() -> Result<()> {
        let dirs = [
            Self::base_dir()?,
            Self::conversations_dir()?,
            Self::logs_dir()?,
        ];
        for dir in &dirs {
            if !dir.exists() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
            }
        }
        Ok(())
    }

    /// Loads config from disk, or creates default if missing.
    ///
    /// **Note**: This loads only the non-secret fields. API keys are populated
    /// separately by [`ConfigManager`] via `SecureStorage`. Callers that need
    /// API keys should use `ConfigManager::get()` instead.
    pub fn load() -> Result<Self> {
        Self::ensure_dirs()?;
        let path = Self::config_path()?;
        Self::load_from_path(&path)
    }

    /// Load config from a specific file path.
    pub fn load_from_path(path: &PathBuf) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config: {}", path.display()))?;
            let config: Self = serde_json::from_str(&content)
                .with_context(|| "Failed to parse config.json")?;
            info!("Loaded config from {}", path.display());
            Ok(config)
        } else {
            let config = Self::default();
            config.save_to_path(path)?;
            info!("Created default config at {}", path.display());
            Ok(config)
        }
    }

    /// Saves config to disk (API keys are excluded via `#[serde(skip)]`).
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save_to_path(&path)
    }

    /// Save config to a specific file path.
    pub fn save_to_path(&self, path: &PathBuf) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config: {}", path.display()))?;
        Ok(())
    }

    /// Migrates from old `~/.hivecode/` directory if it exists.
    pub fn migrate_from_hivecode() -> Result<bool> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        let old_dir = home.join(".hivecode");
        let new_dir = Self::base_dir()?;

        if old_dir.exists() && !new_dir.exists() {
            info!("Migrating config from ~/.hivecode/ to ~/.hive/");
            std::fs::rename(&old_dir, &new_dir)
                .with_context(|| "Failed to migrate ~/.hivecode/ to ~/.hive/")?;
            return Ok(true);
        }
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Plaintext migration helper
// ---------------------------------------------------------------------------

/// A shadow struct that deserializes API keys from the raw JSON for migration
/// purposes. When users upgrade from old versions, keys may still be present
/// in config.json as plaintext -- we detect, migrate, and strip them.
#[derive(Deserialize, Default)]
struct LegacyKeys {
    #[serde(default)]
    anthropic_api_key: Option<String>,
    #[serde(default)]
    openai_api_key: Option<String>,
    #[serde(default)]
    openrouter_api_key: Option<String>,
    #[serde(default)]
    google_api_key: Option<String>,
}

impl LegacyKeys {
    fn has_any(&self) -> bool {
        [
            &self.anthropic_api_key,
            &self.openai_api_key,
            &self.openrouter_api_key,
            &self.google_api_key,
        ]
        .iter()
        .any(|k| k.as_ref().map_or(false, |v| !v.is_empty()))
    }
}

/// Detect and migrate plaintext API keys from a raw JSON config string.
/// Returns the extracted keys and a cleaned JSON string with key fields removed.
fn migrate_plaintext_keys(raw_json: &str) -> Result<(LegacyKeys, String)> {
    let legacy: LegacyKeys =
        serde_json::from_str(raw_json).unwrap_or_default();

    if legacy.has_any() {
        // Parse as a mutable JSON value and strip the key fields
        let mut value: serde_json::Value =
            serde_json::from_str(raw_json).context("Failed to parse config JSON for migration")?;
        if let Some(obj) = value.as_object_mut() {
            obj.remove("anthropic_api_key");
            obj.remove("openai_api_key");
            obj.remove("openrouter_api_key");
            obj.remove("google_api_key");
        }
        let cleaned = serde_json::to_string_pretty(&value)?;
        Ok((legacy, cleaned))
    } else {
        Ok((legacy, raw_json.to_string()))
    }
}

// ---------------------------------------------------------------------------
// ConfigManager
// ---------------------------------------------------------------------------

/// Thread-safe config holder with file watcher for hot reload.
///
/// API keys are stored encrypted via `SecureStorage` in `~/.hive/keys.enc`
/// and are **never** written to `config.json`.
pub struct ConfigManager {
    config: Arc<RwLock<HiveConfig>>,
    secure_storage: Option<SecureStorage>,
    keys_path: PathBuf,
    _watcher: Option<RecommendedWatcher>,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        HiveConfig::migrate_from_hivecode()?;

        let config_path = HiveConfig::config_path()?;
        let keys_path = keys_file_path()?;

        // Initialize secure storage (graceful if it fails -- keys just won't be available)
        let secure_storage = match SecureStorage::new() {
            Ok(ss) => Some(ss),
            Err(e) => {
                warn!("SecureStorage init failed ({e}); API keys will not be available");
                None
            }
        };

        // Load the config, handling backward-compatible migration of plaintext keys
        let config = Self::load_with_migration(&config_path, &keys_path, &secure_storage)?;
        let config = Arc::new(RwLock::new(config));

        // Reuse the same derived key for hot-reload (avoids a second Argon2 derivation).
        let reload_keys_path = keys_path.clone();
        let reload_ss = secure_storage.as_ref().map(|ss| ss.duplicate());
        let watcher = Self::setup_watcher(Arc::clone(&config), reload_keys_path, reload_ss)?;

        Ok(Self {
            config,
            secure_storage,
            keys_path,
            _watcher: Some(watcher),
        })
    }

    /// Load config, migrating any plaintext keys to SecureStorage.
    fn load_with_migration(
        config_path: &PathBuf,
        keys_path: &PathBuf,
        secure_storage: &Option<SecureStorage>,
    ) -> Result<HiveConfig> {
        HiveConfig::ensure_dirs()?;

        if config_path.exists() {
            let raw_json = std::fs::read_to_string(config_path)
                .with_context(|| format!("Failed to read config: {}", config_path.display()))?;

            // Check for plaintext keys that need migration
            let (legacy, cleaned_json) = migrate_plaintext_keys(&raw_json)?;

            if legacy.has_any() {
                info!("Migrating plaintext API keys from config.json to SecureStorage");

                if let Some(ss) = secure_storage {
                    // Load existing encrypted keys (if any), then merge legacy keys
                    let mut key_map = load_key_map(keys_path);
                    let _ = set_secure_key(ss, &mut key_map, KEY_ANTHROPIC, &legacy.anthropic_api_key);
                    let _ = set_secure_key(ss, &mut key_map, KEY_OPENAI, &legacy.openai_api_key);
                    let _ = set_secure_key(ss, &mut key_map, KEY_OPENROUTER, &legacy.openrouter_api_key);
                    let _ = set_secure_key(ss, &mut key_map, KEY_GOOGLE, &legacy.google_api_key);
                    save_key_map(keys_path, &key_map)?;
                    info!("Plaintext keys migrated to SecureStorage");
                } else {
                    warn!("SecureStorage unavailable; plaintext keys cannot be migrated");
                }

                // Overwrite config.json with the cleaned version (keys stripped)
                std::fs::write(config_path, &cleaned_json)
                    .with_context(|| "Failed to write cleaned config.json during migration")?;
                info!("Stripped plaintext keys from config.json");
            }

            // Parse the (potentially cleaned) JSON into HiveConfig
            let mut config: HiveConfig = serde_json::from_str(&cleaned_json)
                .with_context(|| "Failed to parse config.json")?;

            // Populate API keys from SecureStorage
            Self::populate_keys_from_storage(&mut config, keys_path, secure_storage);

            info!("Loaded config from {}", config_path.display());
            Ok(config)
        } else {
            let config = HiveConfig::default();
            config.save_to_path(config_path)?;
            info!("Created default config at {}", config_path.display());
            Ok(config)
        }
    }

    /// Read API keys from the encrypted key store into the config struct.
    fn populate_keys_from_storage(
        config: &mut HiveConfig,
        keys_path: &PathBuf,
        secure_storage: &Option<SecureStorage>,
    ) {
        if let Some(ss) = secure_storage {
            let key_map = load_key_map(keys_path);
            config.anthropic_api_key = get_secure_key(ss, &key_map, KEY_ANTHROPIC);
            config.openai_api_key = get_secure_key(ss, &key_map, KEY_OPENAI);
            config.openrouter_api_key = get_secure_key(ss, &key_map, KEY_OPENROUTER);
            config.google_api_key = get_secure_key(ss, &key_map, KEY_GOOGLE);
            config.groq_api_key = get_secure_key(ss, &key_map, KEY_GROQ);
            config.huggingface_api_key = get_secure_key(ss, &key_map, KEY_HUGGINGFACE);
            config.litellm_api_key = get_secure_key(ss, &key_map, KEY_LITELLM);
            config.elevenlabs_api_key = get_secure_key(ss, &key_map, KEY_ELEVENLABS);
            config.telnyx_api_key = get_secure_key(ss, &key_map, KEY_TELNYX);
        }
    }

    /// Get a clone of the current config (including decrypted API keys).
    pub fn get(&self) -> HiveConfig {
        self.config.read().clone()
    }

    /// Update the config. The closure receives a mutable reference to the
    /// config. After mutation, non-secret fields are saved to `config.json`
    /// and API keys are saved to SecureStorage.
    pub fn update(&self, f: impl FnOnce(&mut HiveConfig)) -> Result<()> {
        let mut config = self.config.write();
        f(&mut config);
        config.save()?;
        self.save_api_keys(&config)?;
        Ok(())
    }

    /// Get a specific API key by provider name.
    pub fn get_api_key(&self, provider: &str) -> Option<String> {
        let config = self.config.read();
        match provider {
            "anthropic" => config.anthropic_api_key.clone(),
            "openai" => config.openai_api_key.clone(),
            "openrouter" => config.openrouter_api_key.clone(),
            "google" => config.google_api_key.clone(),
            "groq" => config.groq_api_key.clone(),
            "huggingface" => config.huggingface_api_key.clone(),
            "litellm" => config.litellm_api_key.clone(),
            "elevenlabs" => config.elevenlabs_api_key.clone(),
            "telnyx" => config.telnyx_api_key.clone(),
            _ => None,
        }
    }

    /// Set a specific API key by provider name. Persists to SecureStorage
    /// immediately.
    pub fn set_api_key(&self, provider: &str, key: Option<String>) -> Result<()> {
        {
            let mut config = self.config.write();
            match provider {
                "anthropic" => config.anthropic_api_key = key.clone(),
                "openai" => config.openai_api_key = key.clone(),
                "openrouter" => config.openrouter_api_key = key.clone(),
                "google" => config.google_api_key = key.clone(),
                "groq" => config.groq_api_key = key.clone(),
                "huggingface" => config.huggingface_api_key = key.clone(),
                "litellm" => config.litellm_api_key = key.clone(),
                "elevenlabs" => config.elevenlabs_api_key = key.clone(),
                "telnyx" => config.telnyx_api_key = key.clone(),
                _ => anyhow::bail!("Unknown provider: {provider}"),
            }
        }
        // Persist only keys to SecureStorage (config.json is not touched)
        let config = self.config.read();
        self.save_api_keys(&config)
    }

    /// Persist all API keys to the encrypted key store.
    fn save_api_keys(&self, config: &HiveConfig) -> Result<()> {
        let Some(ss) = &self.secure_storage else {
            warn!("SecureStorage unavailable; API keys not persisted");
            anyhow::bail!("SecureStorage unavailable; API keys cannot be saved");
        };
        let mut key_map = load_key_map(&self.keys_path);
        set_secure_key(ss, &mut key_map, KEY_ANTHROPIC, &config.anthropic_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_OPENAI, &config.openai_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_OPENROUTER, &config.openrouter_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_GOOGLE, &config.google_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_GROQ, &config.groq_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_HUGGINGFACE, &config.huggingface_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_LITELLM, &config.litellm_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_ELEVENLABS, &config.elevenlabs_api_key)?;
        set_secure_key(ss, &mut key_map, KEY_TELNYX, &config.telnyx_api_key)?;
        save_key_map(&self.keys_path, &key_map)
    }

    fn setup_watcher(
        config: Arc<RwLock<HiveConfig>>,
        keys_path: PathBuf,
        secure_storage: Option<SecureStorage>,
    ) -> Result<RecommendedWatcher> {
        let config_path = HiveConfig::config_path()?;
        let watch_dir = config_path.parent().unwrap().to_path_buf();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                if event.paths.iter().any(|p| p.ends_with("config.json")) {
                    match HiveConfig::load() {
                        Ok(mut new_config) => {
                            // Re-populate keys from SecureStorage on hot reload
                            Self::populate_keys_from_storage(
                                &mut new_config,
                                &keys_path,
                                &secure_storage,
                            );
                            *config.write() = new_config;
                            info!("Config hot-reloaded");
                        }
                        Err(e) => warn!("Failed to hot-reload config: {e}"),
                    }
                }
            }
        })?;

        watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;
        Ok(watcher)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a HiveConfig, write it to a temp dir, and exercise
    /// round-trip load/save.
    fn make_temp_config_dir() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        let keys_path = tmp.path().join("keys.enc");
        (tmp, config_path, keys_path)
    }

    // -----------------------------------------------------------------------
    // 1. API key round-trip through SecureStorage
    // -----------------------------------------------------------------------

    #[test]
    fn api_key_roundtrip_via_secure_storage() {
        let (_tmp, _config_path, keys_path) = make_temp_config_dir();
        let ss = SecureStorage::new().unwrap();

        // Store keys
        let mut map = HashMap::new();
        set_secure_key(&ss, &mut map, KEY_ANTHROPIC, &Some("sk-ant-secret".into())).unwrap();
        set_secure_key(&ss, &mut map, KEY_OPENAI, &Some("sk-openai-123".into())).unwrap();
        set_secure_key(&ss, &mut map, KEY_OPENROUTER, &Some("sk-or-456".into())).unwrap();
        set_secure_key(&ss, &mut map, KEY_GOOGLE, &Some("AIza-google".into())).unwrap();
        save_key_map(&keys_path, &map).unwrap();

        // Reload and decrypt
        let loaded_map = load_key_map(&keys_path);
        assert_eq!(get_secure_key(&ss, &loaded_map, KEY_ANTHROPIC).unwrap(), "sk-ant-secret");
        assert_eq!(get_secure_key(&ss, &loaded_map, KEY_OPENAI).unwrap(), "sk-openai-123");
        assert_eq!(get_secure_key(&ss, &loaded_map, KEY_OPENROUTER).unwrap(), "sk-or-456");
        assert_eq!(get_secure_key(&ss, &loaded_map, KEY_GOOGLE).unwrap(), "AIza-google");
    }

    // -----------------------------------------------------------------------
    // 2. Migration from plaintext to SecureStorage
    // -----------------------------------------------------------------------

    #[test]
    fn migrate_plaintext_keys_from_config() {
        let (_tmp, config_path, keys_path) = make_temp_config_dir();
        let ss = SecureStorage::new().unwrap();
        let secure_storage = Some(ss);

        // Write a legacy config.json with plaintext API keys
        let legacy_json = serde_json::json!({
            "anthropic_api_key": "sk-ant-legacy",
            "openai_api_key": "sk-openai-legacy",
            "openrouter_api_key": "",
            "google_api_key": null,
            "ollama_url": "http://localhost:11434",
            "privacy_mode": false,
            "default_model": "claude-sonnet-4-5-20250929",
            "theme": "dark"
        });
        std::fs::write(&config_path, serde_json::to_string_pretty(&legacy_json).unwrap()).unwrap();

        // Load with migration
        let config = ConfigManager::load_with_migration(&config_path, &keys_path, &secure_storage).unwrap();

        // Keys should be populated in memory
        assert_eq!(config.anthropic_api_key.as_deref(), Some("sk-ant-legacy"));
        assert_eq!(config.openai_api_key.as_deref(), Some("sk-openai-legacy"));
        assert!(config.openrouter_api_key.is_none()); // was empty string
        assert!(config.google_api_key.is_none()); // was null

        // config.json should no longer contain any API key fields
        let saved_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(saved_json.get("anthropic_api_key").is_none());
        assert!(saved_json.get("openai_api_key").is_none());
        assert!(saved_json.get("openrouter_api_key").is_none());
        assert!(saved_json.get("google_api_key").is_none());

        // Keys should be stored in keys.enc (encrypted)
        let key_map = load_key_map(&keys_path);
        assert!(key_map.contains_key(KEY_ANTHROPIC));
        assert!(key_map.contains_key(KEY_OPENAI));
        assert!(!key_map.contains_key(KEY_OPENROUTER)); // empty = removed
        assert!(!key_map.contains_key(KEY_GOOGLE)); // null = removed

        // Non-secret fields should be preserved
        assert_eq!(config.theme, "dark");
        assert_eq!(config.privacy_mode, false);
    }

    // -----------------------------------------------------------------------
    // 3. Config save/load without API keys in JSON
    // -----------------------------------------------------------------------

    #[test]
    fn config_json_never_contains_api_keys() {
        let (_tmp, config_path, _keys_path) = make_temp_config_dir();

        let mut config = HiveConfig::default();
        config.anthropic_api_key = Some("should-not-appear".into());
        config.openai_api_key = Some("also-secret".into());
        config.theme = "light".into();

        config.save_to_path(&config_path).unwrap();

        let raw = std::fs::read_to_string(&config_path).unwrap();
        assert!(!raw.contains("should-not-appear"), "API key leaked to config.json");
        assert!(!raw.contains("also-secret"), "API key leaked to config.json");
        assert!(!raw.contains("anthropic_api_key"), "Key field present in JSON");
        assert!(!raw.contains("openai_api_key"), "Key field present in JSON");

        // Non-secret fields should be present
        assert!(raw.contains("light"));
    }

    #[test]
    fn load_config_without_keys_returns_none_for_keys() {
        let (_tmp, config_path, _keys_path) = make_temp_config_dir();

        let config = HiveConfig::default();
        config.save_to_path(&config_path).unwrap();

        let loaded = HiveConfig::load_from_path(&config_path).unwrap();
        assert!(loaded.anthropic_api_key.is_none());
        assert!(loaded.openai_api_key.is_none());
        assert!(loaded.openrouter_api_key.is_none());
        assert!(loaded.google_api_key.is_none());
    }

    // -----------------------------------------------------------------------
    // 4. Missing SecureStorage graceful handling
    // -----------------------------------------------------------------------

    #[test]
    fn populate_keys_without_secure_storage() {
        let (_tmp, _config_path, keys_path) = make_temp_config_dir();
        let mut config = HiveConfig::default();

        // No SecureStorage available
        ConfigManager::populate_keys_from_storage(&mut config, &keys_path, &None);
        assert!(config.anthropic_api_key.is_none());
        assert!(config.openai_api_key.is_none());
    }

    #[test]
    fn populate_keys_with_missing_keys_file() {
        let (_tmp, _config_path, keys_path) = make_temp_config_dir();
        let ss = SecureStorage::new().unwrap();
        let mut config = HiveConfig::default();

        // keys.enc doesn't exist -- should gracefully return empty
        ConfigManager::populate_keys_from_storage(&mut config, &keys_path, &Some(ss));
        assert!(config.anthropic_api_key.is_none());
        assert!(config.openai_api_key.is_none());
    }

    #[test]
    fn populate_keys_with_corrupted_keys_file() {
        let (_tmp, _config_path, keys_path) = make_temp_config_dir();
        let ss = SecureStorage::new().unwrap();
        let mut config = HiveConfig::default();

        // Write garbage to keys.enc
        std::fs::write(&keys_path, "not-valid-json{{{").unwrap();

        // Should gracefully return empty rather than panicking
        ConfigManager::populate_keys_from_storage(&mut config, &keys_path, &Some(ss));
        assert!(config.anthropic_api_key.is_none());
    }

    // -----------------------------------------------------------------------
    // 5. set/get secure key helpers
    // -----------------------------------------------------------------------

    #[test]
    fn set_none_removes_key() {
        let ss = SecureStorage::new().unwrap();
        let mut map = HashMap::new();

        // Set a key
        set_secure_key(&ss, &mut map, KEY_ANTHROPIC, &Some("secret".into())).unwrap();
        assert!(map.contains_key(KEY_ANTHROPIC));

        // Remove it
        set_secure_key(&ss, &mut map, KEY_ANTHROPIC, &None).unwrap();
        assert!(!map.contains_key(KEY_ANTHROPIC));
    }

    #[test]
    fn set_empty_string_removes_key() {
        let ss = SecureStorage::new().unwrap();
        let mut map = HashMap::new();

        set_secure_key(&ss, &mut map, KEY_OPENAI, &Some("secret".into())).unwrap();
        assert!(map.contains_key(KEY_OPENAI));

        set_secure_key(&ss, &mut map, KEY_OPENAI, &Some("".into())).unwrap();
        assert!(!map.contains_key(KEY_OPENAI));
    }

    #[test]
    fn get_missing_key_returns_none() {
        let ss = SecureStorage::new().unwrap();
        let map = HashMap::new();
        assert!(get_secure_key(&ss, &map, KEY_ANTHROPIC).is_none());
    }

    // -----------------------------------------------------------------------
    // 6. migrate_plaintext_keys helper
    // -----------------------------------------------------------------------

    #[test]
    fn migrate_strips_key_fields_from_json() {
        let json = r#"{
            "anthropic_api_key": "sk-test",
            "theme": "dark",
            "font_size": 16
        }"#;

        let (legacy, cleaned) = migrate_plaintext_keys(json).unwrap();
        assert_eq!(legacy.anthropic_api_key.as_deref(), Some("sk-test"));

        let parsed: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
        assert!(parsed.get("anthropic_api_key").is_none());
        assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");
        assert_eq!(parsed.get("font_size").unwrap().as_u64().unwrap(), 16);
    }

    #[test]
    fn migrate_no_keys_is_noop() {
        let json = r#"{"theme": "dark", "font_size": 14}"#;
        let (legacy, cleaned) = migrate_plaintext_keys(json).unwrap();
        assert!(!legacy.has_any());
        // cleaned should be the same (not re-formatted) string
        assert_eq!(cleaned, json);
    }

    // -----------------------------------------------------------------------
    // 7. Full integration: load_with_migration populates keys correctly
    // -----------------------------------------------------------------------

    #[test]
    fn full_load_with_no_legacy_keys() {
        let (_tmp, config_path, keys_path) = make_temp_config_dir();
        let ss = SecureStorage::new().unwrap();

        // Write clean config (no API keys)
        let config = HiveConfig::default();
        config.save_to_path(&config_path).unwrap();

        // Pre-populate some encrypted keys
        let mut map = HashMap::new();
        set_secure_key(&ss, &mut map, KEY_ANTHROPIC, &Some("sk-from-storage".into())).unwrap();
        save_key_map(&keys_path, &map).unwrap();

        let secure_storage = Some(SecureStorage::new().unwrap());
        let loaded = ConfigManager::load_with_migration(&config_path, &keys_path, &secure_storage).unwrap();

        // Should pick up the key from SecureStorage
        assert_eq!(loaded.anthropic_api_key.as_deref(), Some("sk-from-storage"));
        assert!(loaded.openai_api_key.is_none());
    }

    #[test]
    fn save_api_keys_persists_correctly() {
        let (_tmp, _config_path, keys_path) = make_temp_config_dir();
        let ss = SecureStorage::new().unwrap();

        let mut config = HiveConfig::default();
        config.anthropic_api_key = Some("sk-save-test".into());
        config.google_api_key = Some("AIza-save".into());

        // Manually persist via the helper
        let mut key_map = HashMap::new();
        set_secure_key(&ss, &mut key_map, KEY_ANTHROPIC, &config.anthropic_api_key).unwrap();
        set_secure_key(&ss, &mut key_map, KEY_OPENAI, &config.openai_api_key).unwrap();
        set_secure_key(&ss, &mut key_map, KEY_OPENROUTER, &config.openrouter_api_key).unwrap();
        set_secure_key(&ss, &mut key_map, KEY_GOOGLE, &config.google_api_key).unwrap();
        save_key_map(&keys_path, &key_map).unwrap();

        // Re-read and verify
        let ss2 = SecureStorage::new().unwrap();
        let loaded_map = load_key_map(&keys_path);
        assert_eq!(get_secure_key(&ss2, &loaded_map, KEY_ANTHROPIC).unwrap(), "sk-save-test");
        assert_eq!(get_secure_key(&ss2, &loaded_map, KEY_GOOGLE).unwrap(), "AIza-save");
        assert!(get_secure_key(&ss2, &loaded_map, KEY_OPENAI).is_none());
        assert!(get_secure_key(&ss2, &loaded_map, KEY_OPENROUTER).is_none());
    }

    // -----------------------------------------------------------------------
    // 8. Default config is unchanged
    // -----------------------------------------------------------------------

    #[test]
    fn default_config_values() {
        let config = HiveConfig::default();
        assert!(config.anthropic_api_key.is_none());
        assert!(config.openai_api_key.is_none());
        assert!(config.openrouter_api_key.is_none());
        assert!(config.google_api_key.is_none());
        assert_eq!(config.ollama_url, "http://localhost:11434");
        assert_eq!(config.theme, "dark");
        assert_eq!(config.font_size, 14);
    }
}
