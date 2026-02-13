//! ElevenLabs TTS provider — premium cloud synthesis with voice cloning.
//!
//! REST API: `https://api.elevenlabs.io/v1`
//! Auth: `xi-api-key` header.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{
    AudioData, AudioFormat, TtsError, TtsProvider, TtsProviderType, TtsRequest, VoiceInfo,
};

const API_BASE: &str = "https://api.elevenlabs.io/v1";

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct ElevenLabsProvider {
    client: Client,
    api_key: Option<String>,
}

impl ElevenLabsProvider {
    pub fn new(api_key: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self { client, api_key }
    }

    fn auth_header(&self) -> Result<String, TtsError> {
        self.api_key.clone().ok_or(TtsError::InvalidKey)
    }
}

#[async_trait]
impl TtsProvider for ElevenLabsProvider {
    fn provider_type(&self) -> TtsProviderType {
        TtsProviderType::ElevenLabs
    }

    fn name(&self) -> &str {
        "ElevenLabs"
    }

    async fn is_available(&self) -> bool {
        self.api_key.is_some()
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, TtsError> {
        let api_key = self.auth_header()?;

        let resp = self
            .client
            .get(format!("{API_BASE}/voices"))
            .header("xi-api-key", &api_key)
            .send()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 => TtsError::InvalidKey,
                429 => TtsError::RateLimit,
                _ => TtsError::Other(format!("ElevenLabs {status}: {body}")),
            });
        }

        let data: VoicesResponse = resp
            .json()
            .await
            .map_err(|e| TtsError::Other(format!("Failed to parse voices response: {e}")))?;

        Ok(data
            .voices
            .into_iter()
            .map(|v| VoiceInfo {
                id: v.voice_id,
                name: v.name,
                language: v.labels.and_then(|l| l.language),
                preview_url: v.preview_url,
                is_cloned: v.category.as_deref() == Some("cloned"),
            })
            .collect())
    }

    async fn synthesize(&self, request: &TtsRequest) -> Result<AudioData, TtsError> {
        let api_key = self.auth_header()?;

        let output_format = match request.format {
            AudioFormat::Mp3 => "mp3_44100_128",
            AudioFormat::Pcm => "pcm_24000",
            _ => "mp3_44100_128",
        };

        let payload = SynthesisPayload {
            text: &request.text,
            model_id: "eleven_multilingual_v2",
            voice_settings: VoiceSettings {
                stability: 0.5,
                similarity_boost: 0.75,
                speed: request.speed,
            },
        };

        debug!(voice_id = request.voice_id, "ElevenLabs TTS synthesis");

        let resp = self
            .client
            .post(format!(
                "{API_BASE}/text-to-speech/{}?output_format={output_format}",
                request.voice_id
            ))
            .header("xi-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 => TtsError::InvalidKey,
                429 => TtsError::RateLimit,
                _ => TtsError::Other(format!("ElevenLabs synthesis {status}: {body}")),
            });
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        let (format, sample_rate) = match request.format {
            AudioFormat::Pcm => (AudioFormat::Pcm, 24000),
            _ => (AudioFormat::Mp3, 44100),
        };

        Ok(AudioData {
            bytes: bytes.to_vec(),
            format,
            sample_rate,
        })
    }

    fn supports_cloning(&self) -> bool {
        true
    }

    async fn clone_voice(&self, name: &str, samples: &[Vec<u8>]) -> Result<VoiceInfo, TtsError> {
        if samples.is_empty() {
            return Err(TtsError::Other(
                "At least one audio sample is required".into(),
            ));
        }

        let api_key = self.auth_header()?;

        // Build multipart form: name + files[]
        let mut form = reqwest::multipart::Form::new().text("name", name.to_string());

        for (i, sample) in samples.iter().enumerate() {
            let part = reqwest::multipart::Part::bytes(sample.clone())
                .file_name(format!("sample_{i}.wav"))
                .mime_str("audio/wav")
                .map_err(|e| TtsError::Other(e.to_string()))?;
            form = form.part("files", part);
        }

        debug!(
            name,
            sample_count = samples.len(),
            "ElevenLabs voice cloning"
        );

        let resp = self
            .client
            .post(format!("{API_BASE}/voices/add"))
            .header("xi-api-key", &api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 => TtsError::InvalidKey,
                429 => TtsError::RateLimit,
                _ => TtsError::Other(format!("ElevenLabs clone {status}: {body}")),
            });
        }

        let data: AddVoiceResponse = resp
            .json()
            .await
            .map_err(|e| TtsError::Other(format!("Failed to parse clone response: {e}")))?;

        Ok(VoiceInfo {
            id: data.voice_id,
            name: name.to_string(),
            language: None,
            preview_url: None,
            is_cloned: true,
        })
    }
}

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SynthesisPayload<'a> {
    text: &'a str,
    model_id: &'a str,
    voice_settings: VoiceSettings,
}

#[derive(Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
    speed: f32,
}

#[derive(Deserialize)]
struct VoicesResponse {
    voices: Vec<ElevenLabsVoice>,
}

#[derive(Deserialize)]
struct ElevenLabsVoice {
    voice_id: String,
    name: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    preview_url: Option<String>,
    #[serde(default)]
    labels: Option<VoiceLabels>,
}

#[derive(Deserialize)]
struct VoiceLabels {
    #[serde(default)]
    language: Option<String>,
}

#[derive(Deserialize)]
struct AddVoiceResponse {
    voice_id: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata() {
        let p = ElevenLabsProvider::new(Some("test-key".into()));
        assert_eq!(p.provider_type(), TtsProviderType::ElevenLabs);
        assert_eq!(p.name(), "ElevenLabs");
        assert!(p.supports_cloning());
    }

    #[tokio::test]
    async fn is_available_with_key() {
        let p = ElevenLabsProvider::new(Some("key".into()));
        assert!(p.is_available().await);
    }

    #[tokio::test]
    async fn is_available_without_key() {
        let p = ElevenLabsProvider::new(None);
        assert!(!p.is_available().await);
    }

    #[tokio::test]
    async fn clone_voice_requires_samples() {
        let p = ElevenLabsProvider::new(Some("key".into()));
        assert!(p.clone_voice("Empty", &[]).await.is_err());
    }

    #[test]
    fn auth_header_returns_key() {
        let p = ElevenLabsProvider::new(Some("my-key".into()));
        assert_eq!(p.auth_header().unwrap(), "my-key");
    }

    #[test]
    fn auth_header_errors_without_key() {
        let p = ElevenLabsProvider::new(None);
        assert!(p.auth_header().is_err());
    }
}
