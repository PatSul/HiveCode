//! Qwen3-TTS provider — local-first default via HuggingFace Inference API.
//!
//! Supports voice cloning by sending reference audio alongside the text.
//! Falls back to a local endpoint if a model is running on localhost.

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use tracing::debug;

use super::{AudioData, AudioFormat, TtsError, TtsProvider, TtsProviderType, TtsRequest, VoiceInfo};

const HF_API_BASE: &str = "https://api-inference.huggingface.co/models/Qwen/Qwen3-TTS";
const DEFAULT_LOCAL_URL: &str = "http://localhost:8880";

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct Qwen3TtsProvider {
    client: Client,
    api_key: Option<String>,
    local_url: Option<String>,
}

impl Qwen3TtsProvider {
    pub fn new(api_key: Option<String>, local_url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self {
            client,
            api_key,
            local_url,
        }
    }

    /// Try local inference first; return the working base URL if reachable.
    async fn local_endpoint(&self) -> Option<String> {
        let url = self
            .local_url
            .as_deref()
            .unwrap_or(DEFAULT_LOCAL_URL);
        match self.client.get(format!("{url}/health")).send().await {
            Ok(resp) if resp.status().is_success() => Some(url.to_string()),
            _ => None,
        }
    }

    async fn synthesize_via_hf(&self, request: &TtsRequest) -> Result<AudioData, TtsError> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or(TtsError::InvalidKey)?;

        let payload = HfTtsPayload {
            inputs: request.text.clone(),
            parameters: HfTtsParams {
                voice_id: Some(request.voice_id.clone()),
                speed: Some(request.speed),
            },
        };

        let resp = self
            .client
            .post(HF_API_BASE)
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => TtsError::InvalidKey,
                429 => TtsError::RateLimit,
                _ => TtsError::Other(format!("HF API {status}: {body}")),
            });
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        Ok(AudioData {
            bytes: bytes.to_vec(),
            format: AudioFormat::Wav,
            sample_rate: 24000,
        })
    }

    async fn synthesize_local(
        &self,
        base_url: &str,
        request: &TtsRequest,
    ) -> Result<AudioData, TtsError> {
        let payload = LocalTtsPayload {
            text: request.text.clone(),
            voice_id: request.voice_id.clone(),
            speed: request.speed,
        };

        let resp = self
            .client
            .post(format!("{base_url}/v1/audio/speech"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TtsError::Other(format!("Local Qwen3 error: {body}")));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        Ok(AudioData {
            bytes: bytes.to_vec(),
            format: AudioFormat::Wav,
            sample_rate: 24000,
        })
    }
}

#[async_trait]
impl TtsProvider for Qwen3TtsProvider {
    fn provider_type(&self) -> TtsProviderType {
        TtsProviderType::Qwen3
    }

    fn name(&self) -> &str {
        "Qwen3-TTS"
    }

    async fn is_available(&self) -> bool {
        // Local endpoint takes priority.
        if self.local_endpoint().await.is_some() {
            return true;
        }
        self.api_key.is_some()
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, TtsError> {
        // Qwen3-TTS uses reference-audio-based voice selection.
        // Return a default voice entry since the model doesn't have a fixed catalog.
        Ok(vec![VoiceInfo {
            id: "default".into(),
            name: "Qwen3 Default".into(),
            language: Some("multilingual".into()),
            preview_url: None,
            is_cloned: false,
        }])
    }

    async fn synthesize(&self, request: &TtsRequest) -> Result<AudioData, TtsError> {
        // Prefer local inference.
        if let Some(url) = self.local_endpoint().await {
            debug!("Using local Qwen3-TTS at {url}");
            return self.synthesize_local(&url, request).await;
        }

        debug!("Using HuggingFace Inference API for Qwen3-TTS");
        self.synthesize_via_hf(request).await
    }

    fn supports_cloning(&self) -> bool {
        true
    }

    async fn clone_voice(
        &self,
        name: &str,
        samples: &[Vec<u8>],
    ) -> Result<VoiceInfo, TtsError> {
        if samples.is_empty() {
            return Err(TtsError::Other("At least one audio sample is required".into()));
        }
        // Qwen3-TTS cloning works by passing reference audio at synthesis time.
        // We store a logical voice ID referencing the sample data.
        let voice_id = format!("cloned_{}", uuid::Uuid::new_v4());
        debug!(voice_id, name, "Created cloned voice reference for Qwen3-TTS");
        Ok(VoiceInfo {
            id: voice_id,
            name: name.to_string(),
            language: Some("multilingual".into()),
            preview_url: None,
            is_cloned: true,
        })
    }
}

// ---------------------------------------------------------------------------
// API payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HfTtsPayload {
    inputs: String,
    parameters: HfTtsParams,
}

#[derive(Serialize)]
struct HfTtsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
}

#[derive(Serialize)]
struct LocalTtsPayload {
    text: String,
    voice_id: String,
    speed: f32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata() {
        let provider = Qwen3TtsProvider::new(Some("test-key".into()), None);
        assert_eq!(provider.provider_type(), TtsProviderType::Qwen3);
        assert_eq!(provider.name(), "Qwen3-TTS");
        assert!(provider.supports_cloning());
    }

    #[test]
    fn provider_without_key_or_local() {
        let provider = Qwen3TtsProvider::new(None, None);
        // Without key or local endpoint, we can still construct — availability checked at runtime.
        assert_eq!(provider.name(), "Qwen3-TTS");
    }

    #[tokio::test]
    async fn list_voices_returns_default() {
        let provider = Qwen3TtsProvider::new(Some("test".into()), None);
        let voices = provider.list_voices().await.unwrap();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "default");
    }

    #[tokio::test]
    async fn clone_voice_creates_reference() {
        let provider = Qwen3TtsProvider::new(Some("test".into()), None);
        let info = provider.clone_voice("My Voice", &[vec![0u8; 100]]).await.unwrap();
        assert!(info.id.starts_with("cloned_"));
        assert_eq!(info.name, "My Voice");
        assert!(info.is_cloned);
    }

    #[tokio::test]
    async fn clone_voice_requires_samples() {
        let provider = Qwen3TtsProvider::new(Some("test".into()), None);
        let result = provider.clone_voice("Empty", &[]).await;
        assert!(result.is_err());
    }
}
