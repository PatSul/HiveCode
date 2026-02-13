//! TTS service orchestrator — provider routing, caching, and playback.
//!
//! Holds all configured TTS providers, routes requests based on user config,
//! caches recently synthesised clips in `~/.hive/tts_cache/`, and manages an
//! audio playback queue.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use super::elevenlabs::ElevenLabsProvider;
use super::f5::F5TtsProvider;
use super::openai_tts::OpenAiTtsProvider;
use super::qwen3::Qwen3TtsProvider;
use super::telnyx::TelnyxTtsProvider;
use super::{AudioData, TtsError, TtsProvider, TtsProviderType, TtsRequest, VoiceInfo};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration snapshot used to build the TTS service.
#[derive(Debug, Clone)]
pub struct TtsServiceConfig {
    pub default_provider: TtsProviderType,
    pub default_voice_id: Option<String>,
    pub speed: f32,
    pub enabled: bool,
    pub auto_speak: bool,

    // API keys
    pub openai_api_key: Option<String>,
    pub huggingface_api_key: Option<String>,
    pub elevenlabs_api_key: Option<String>,
    pub telnyx_api_key: Option<String>,
}

impl Default for TtsServiceConfig {
    fn default() -> Self {
        Self {
            default_provider: TtsProviderType::Qwen3,
            default_voice_id: None,
            speed: 1.0,
            enabled: false,
            auto_speak: false,
            openai_api_key: None,
            huggingface_api_key: None,
            elevenlabs_api_key: None,
            telnyx_api_key: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

pub struct TtsService {
    providers: HashMap<TtsProviderType, Arc<dyn TtsProvider>>,
    config: RwLock<TtsServiceConfig>,
    cache_dir: PathBuf,
}

impl TtsService {
    /// Build from configuration, creating all configured providers.
    pub fn new(config: TtsServiceConfig) -> Self {
        let mut providers: HashMap<TtsProviderType, Arc<dyn TtsProvider>> = HashMap::new();

        // Always register local-first providers.
        providers.insert(
            TtsProviderType::Qwen3,
            Arc::new(Qwen3TtsProvider::new(
                config.huggingface_api_key.clone(),
                None,
            )),
        );
        providers.insert(
            TtsProviderType::F5Tts,
            Arc::new(F5TtsProvider::new(config.huggingface_api_key.clone(), None)),
        );

        // Cloud providers — register if API key is available.
        if config.openai_api_key.is_some() {
            providers.insert(
                TtsProviderType::OpenAi,
                Arc::new(OpenAiTtsProvider::new(config.openai_api_key.clone())),
            );
        }
        if config.elevenlabs_api_key.is_some() {
            providers.insert(
                TtsProviderType::ElevenLabs,
                Arc::new(ElevenLabsProvider::new(config.elevenlabs_api_key.clone())),
            );
        }
        if config.telnyx_api_key.is_some() {
            providers.insert(
                TtsProviderType::Telnyx,
                Arc::new(TelnyxTtsProvider::new(config.telnyx_api_key.clone())),
            );
        }

        let cache_dir = hive_core::config::HiveConfig::base_dir()
            .map(|d| d.join("tts_cache"))
            .unwrap_or_else(|_| PathBuf::from("tts_cache"));

        // Ensure cache dir exists (best-effort).
        let _ = std::fs::create_dir_all(&cache_dir);

        info!(
            provider_count = providers.len(),
            default = config.default_provider.as_str(),
            "TTS service initialized"
        );

        Self {
            providers,
            config: RwLock::new(config),
            cache_dir,
        }
    }

    /// Whether the TTS system is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.read().enabled
    }

    /// Whether auto-speak is turned on.
    pub fn auto_speak(&self) -> bool {
        self.config.read().auto_speak
    }

    /// Get the configured default provider type.
    pub fn default_provider(&self) -> TtsProviderType {
        self.config.read().default_provider
    }

    /// Update the configuration at runtime.
    pub fn update_config(&self, f: impl FnOnce(&mut TtsServiceConfig)) {
        let mut cfg = self.config.write();
        f(&mut cfg);
    }

    /// List available providers (those that are registered).
    pub fn available_providers(&self) -> Vec<TtsProviderType> {
        self.providers.keys().copied().collect()
    }

    /// Get a provider by type.
    pub fn provider(&self, ty: TtsProviderType) -> Option<Arc<dyn TtsProvider>> {
        self.providers.get(&ty).cloned()
    }

    /// List voices for the given provider type.
    pub async fn list_voices(
        &self,
        provider_type: TtsProviderType,
    ) -> Result<Vec<VoiceInfo>, TtsError> {
        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| TtsError::Unavailable(format!("{:?} not configured", provider_type)))?;
        provider.list_voices().await
    }

    /// Synthesize speech using the default provider and voice.
    pub async fn speak(&self, text: &str) -> Result<AudioData, TtsError> {
        let cfg = self.config.read().clone();
        if !cfg.enabled {
            return Err(TtsError::Other("TTS is disabled".into()));
        }

        let voice_id = cfg
            .default_voice_id
            .clone()
            .unwrap_or_else(|| "default".into());

        let request = TtsRequest::new(text, voice_id).with_speed(cfg.speed);
        self.synthesize(cfg.default_provider, &request).await
    }

    /// Synthesize speech with a specific provider.
    pub async fn synthesize(
        &self,
        provider_type: TtsProviderType,
        request: &TtsRequest,
    ) -> Result<AudioData, TtsError> {
        // Check cache first.
        let cache_key = self.cache_key(provider_type, request);
        if let Some(cached) = self.load_cached(&cache_key) {
            debug!("TTS cache hit for {cache_key}");
            return Ok(cached);
        }

        let provider = self
            .providers
            .get(&provider_type)
            .ok_or_else(|| TtsError::Unavailable(format!("{:?} not configured", provider_type)))?;

        let audio = provider.synthesize(request).await?;

        // Cache the result (best-effort).
        self.save_cached(&cache_key, &audio);

        Ok(audio)
    }

    // -----------------------------------------------------------------------
    // Cache helpers
    // -----------------------------------------------------------------------

    fn cache_key(&self, provider: TtsProviderType, request: &TtsRequest) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        provider.as_str().hash(&mut hasher);
        request.text.hash(&mut hasher);
        request.voice_id.hash(&mut hasher);
        // Quantise speed to avoid floating-point variance in cache keys.
        ((request.speed * 100.0) as u32).hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn cache_path(&self, key: &str, format: &super::AudioFormat) -> PathBuf {
        self.cache_dir.join(format!("{key}.{}", format.extension()))
    }

    fn load_cached(&self, key: &str) -> Option<AudioData> {
        // Try common formats.
        for fmt in [
            super::AudioFormat::Mp3,
            super::AudioFormat::Wav,
            super::AudioFormat::Opus,
        ] {
            let path = self.cache_path(key, &fmt);
            if let Ok(bytes) = std::fs::read(&path) {
                return Some(AudioData {
                    bytes,
                    format: fmt,
                    sample_rate: 44100,
                });
            }
        }
        None
    }

    fn save_cached(&self, key: &str, audio: &AudioData) {
        let path = self.cache_path(key, &audio.format);
        if let Err(e) = std::fs::write(&path, &audio.bytes) {
            warn!("Failed to cache TTS audio at {}: {e}", path.display());
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> TtsServiceConfig {
        TtsServiceConfig {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn service_creates_local_providers() {
        let svc = TtsService::new(test_config());
        assert!(svc.provider(TtsProviderType::Qwen3).is_some());
        assert!(svc.provider(TtsProviderType::F5Tts).is_some());
    }

    #[test]
    fn service_skips_cloud_without_keys() {
        let svc = TtsService::new(test_config());
        assert!(svc.provider(TtsProviderType::OpenAi).is_none());
        assert!(svc.provider(TtsProviderType::ElevenLabs).is_none());
        assert!(svc.provider(TtsProviderType::Telnyx).is_none());
    }

    #[test]
    fn service_registers_cloud_with_keys() {
        let config = TtsServiceConfig {
            openai_api_key: Some("sk-test".into()),
            elevenlabs_api_key: Some("el-test".into()),
            telnyx_api_key: Some("tx-test".into()),
            ..test_config()
        };
        let svc = TtsService::new(config);
        assert!(svc.provider(TtsProviderType::OpenAi).is_some());
        assert!(svc.provider(TtsProviderType::ElevenLabs).is_some());
        assert!(svc.provider(TtsProviderType::Telnyx).is_some());
    }

    #[test]
    fn default_provider_is_qwen3() {
        let svc = TtsService::new(TtsServiceConfig::default());
        assert_eq!(svc.default_provider(), TtsProviderType::Qwen3);
    }

    #[test]
    fn update_config_at_runtime() {
        let svc = TtsService::new(test_config());
        assert!(svc.is_enabled());

        svc.update_config(|c| c.enabled = false);
        assert!(!svc.is_enabled());
    }

    #[test]
    fn cache_key_deterministic() {
        let svc = TtsService::new(test_config());
        let req = TtsRequest::new("hello", "voice-1");
        let k1 = svc.cache_key(TtsProviderType::OpenAi, &req);
        let k2 = svc.cache_key(TtsProviderType::OpenAi, &req);
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_differs_by_provider() {
        let svc = TtsService::new(test_config());
        let req = TtsRequest::new("hello", "voice-1");
        let k1 = svc.cache_key(TtsProviderType::OpenAi, &req);
        let k2 = svc.cache_key(TtsProviderType::Qwen3, &req);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_differs_by_text() {
        let svc = TtsService::new(test_config());
        let r1 = TtsRequest::new("hello", "v1");
        let r2 = TtsRequest::new("world", "v1");
        let k1 = svc.cache_key(TtsProviderType::Qwen3, &r1);
        let k2 = svc.cache_key(TtsProviderType::Qwen3, &r2);
        assert_ne!(k1, k2);
    }

    #[tokio::test]
    async fn speak_returns_error_when_disabled() {
        let config = TtsServiceConfig {
            enabled: false,
            ..Default::default()
        };
        let svc = TtsService::new(config);
        let result = svc.speak("hello").await;
        assert!(result.is_err());
    }

    #[test]
    fn available_providers_includes_local() {
        let svc = TtsService::new(test_config());
        let providers = svc.available_providers();
        assert!(providers.contains(&TtsProviderType::Qwen3));
        assert!(providers.contains(&TtsProviderType::F5Tts));
    }
}
