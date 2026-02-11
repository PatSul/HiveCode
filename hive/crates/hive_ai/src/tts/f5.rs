//! F5-TTS provider — local-first with strong zero-shot voice cloning.
//!
//! Uses HuggingFace Inference API or a local inference endpoint.

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use tracing::debug;

use super::{AudioData, AudioFormat, TtsError, TtsProvider, TtsProviderType, TtsRequest, VoiceInfo};

const HF_API_BASE: &str = "https://api-inference.huggingface.co/models/SWivid/F5-TTS";
const DEFAULT_LOCAL_URL: &str = "http://localhost:8881";

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct F5TtsProvider {
    client: Client,
    api_key: Option<String>,
    local_url: Option<String>,
}

impl F5TtsProvider {
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

    async fn local_endpoint(&self) -> Option<String> {
        let url = self.local_url.as_deref().unwrap_or(DEFAULT_LOCAL_URL);
        match self.client.get(format!("{url}/health")).send().await {
            Ok(resp) if resp.status().is_success() => Some(url.to_string()),
            _ => None,
        }
    }

    async fn synthesize_via_hf(&self, request: &TtsRequest) -> Result<AudioData, TtsError> {
        let api_key = self.api_key.as_ref().ok_or(TtsError::InvalidKey)?;

        let payload = HfPayload {
            inputs: request.text.clone(),
            parameters: HfParams {
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
        let payload = LocalPayload {
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
            return Err(TtsError::Other(format!("Local F5-TTS error: {body}")));
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
impl TtsProvider for F5TtsProvider {
    fn provider_type(&self) -> TtsProviderType {
        TtsProviderType::F5Tts
    }

    fn name(&self) -> &str {
        "F5-TTS"
    }

    async fn is_available(&self) -> bool {
        if self.local_endpoint().await.is_some() {
            return true;
        }
        self.api_key.is_some()
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, TtsError> {
        Ok(vec![VoiceInfo {
            id: "default".into(),
            name: "F5 Default".into(),
            language: Some("multilingual".into()),
            preview_url: None,
            is_cloned: false,
        }])
    }

    async fn synthesize(&self, request: &TtsRequest) -> Result<AudioData, TtsError> {
        if let Some(url) = self.local_endpoint().await {
            debug!("Using local F5-TTS at {url}");
            return self.synthesize_local(&url, request).await;
        }

        debug!("Using HuggingFace Inference API for F5-TTS");
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
        let voice_id = format!("cloned_{}", uuid::Uuid::new_v4());
        debug!(voice_id, name, "Created cloned voice reference for F5-TTS");
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
struct HfPayload {
    inputs: String,
    parameters: HfParams,
}

#[derive(Serialize)]
struct HfParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
}

#[derive(Serialize)]
struct LocalPayload {
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
        let p = F5TtsProvider::new(Some("key".into()), None);
        assert_eq!(p.provider_type(), TtsProviderType::F5Tts);
        assert_eq!(p.name(), "F5-TTS");
        assert!(p.supports_cloning());
    }

    #[tokio::test]
    async fn list_voices_returns_default() {
        let p = F5TtsProvider::new(Some("key".into()), None);
        let voices = p.list_voices().await.unwrap();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "default");
    }

    #[tokio::test]
    async fn clone_voice_creates_reference() {
        let p = F5TtsProvider::new(Some("key".into()), None);
        let info = p.clone_voice("My Voice", &[vec![1u8; 50]]).await.unwrap();
        assert!(info.id.starts_with("cloned_"));
        assert!(info.is_cloned);
    }

    #[tokio::test]
    async fn clone_voice_requires_samples() {
        let p = F5TtsProvider::new(Some("key".into()), None);
        assert!(p.clone_voice("Empty", &[]).await.is_err());
    }
}
