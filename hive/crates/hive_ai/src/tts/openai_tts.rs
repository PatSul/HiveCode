//! OpenAI TTS provider — cloud synthesis via `/v1/audio/speech`.
//!
//! 6 built-in voices: alloy, echo, fable, onyx, nova, shimmer.
//! Models: `tts-1` (fast) and `tts-1-hd` (quality).
//! Reuses the existing `openai_api_key` from config.
//! No voice cloning support.

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use tracing::debug;

use super::{
    AudioData, AudioFormat, TtsError, TtsProvider, TtsProviderType, TtsRequest, VoiceInfo,
};

const API_URL: &str = "https://api.openai.com/v1/audio/speech";

/// Available OpenAI TTS voices.
const VOICES: &[(&str, &str)] = &[
    ("alloy", "Alloy"),
    ("echo", "Echo"),
    ("fable", "Fable"),
    ("onyx", "Onyx"),
    ("nova", "Nova"),
    ("shimmer", "Shimmer"),
];

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct OpenAiTtsProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl OpenAiTtsProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_model(api_key, "tts-1".into())
    }

    pub fn with_model(api_key: Option<String>, model: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            client,
            api_key,
            model,
        }
    }

    fn format_to_openai(format: AudioFormat) -> &'static str {
        match format {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Opus => "opus",
            AudioFormat::Aac => "aac",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Pcm => "pcm",
        }
    }
}

#[async_trait]
impl TtsProvider for OpenAiTtsProvider {
    fn provider_type(&self) -> TtsProviderType {
        TtsProviderType::OpenAi
    }

    fn name(&self) -> &str {
        "OpenAI TTS"
    }

    async fn is_available(&self) -> bool {
        self.api_key.is_some()
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, TtsError> {
        Ok(VOICES
            .iter()
            .map(|(id, name)| VoiceInfo {
                id: id.to_string(),
                name: name.to_string(),
                language: Some("en".into()),
                preview_url: None,
                is_cloned: false,
            })
            .collect())
    }

    async fn synthesize(&self, request: &TtsRequest) -> Result<AudioData, TtsError> {
        let api_key = self.api_key.as_ref().ok_or(TtsError::InvalidKey)?;

        let payload = OpenAiTtsPayload {
            model: &self.model,
            input: &request.text,
            voice: &request.voice_id,
            response_format: Self::format_to_openai(request.format),
            speed: request.speed,
        };

        debug!(
            model = self.model,
            voice = request.voice_id,
            "OpenAI TTS synthesis"
        );

        let resp = self
            .client
            .post(API_URL)
            .header("Authorization", format!("Bearer {api_key}"))
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
                _ => TtsError::Other(format!("OpenAI TTS {status}: {body}")),
            });
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        let sample_rate = match request.format {
            AudioFormat::Pcm => 24000,
            _ => 44100,
        };

        Ok(AudioData {
            bytes: bytes.to_vec(),
            format: request.format,
            sample_rate,
        })
    }

    fn supports_cloning(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// API payload
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OpenAiTtsPayload<'a> {
    model: &'a str,
    input: &'a str,
    voice: &'a str,
    response_format: &'a str,
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
        let p = OpenAiTtsProvider::new(Some("sk-test".into()));
        assert_eq!(p.provider_type(), TtsProviderType::OpenAi);
        assert_eq!(p.name(), "OpenAI TTS");
        assert!(!p.supports_cloning());
    }

    #[tokio::test]
    async fn list_voices_returns_six() {
        let p = OpenAiTtsProvider::new(Some("sk-test".into()));
        let voices = p.list_voices().await.unwrap();
        assert_eq!(voices.len(), 6);
        assert!(voices.iter().any(|v| v.id == "alloy"));
        assert!(voices.iter().any(|v| v.id == "shimmer"));
    }

    #[tokio::test]
    async fn is_available_with_key() {
        let p = OpenAiTtsProvider::new(Some("sk-test".into()));
        assert!(p.is_available().await);
    }

    #[tokio::test]
    async fn is_available_without_key() {
        let p = OpenAiTtsProvider::new(None);
        assert!(!p.is_available().await);
    }

    #[test]
    fn format_mapping() {
        assert_eq!(OpenAiTtsProvider::format_to_openai(AudioFormat::Mp3), "mp3");
        assert_eq!(OpenAiTtsProvider::format_to_openai(AudioFormat::Wav), "wav");
        assert_eq!(
            OpenAiTtsProvider::format_to_openai(AudioFormat::Opus),
            "opus"
        );
    }

    #[test]
    fn default_model_is_tts1() {
        let p = OpenAiTtsProvider::new(None);
        assert_eq!(p.model, "tts-1");
    }

    #[test]
    fn custom_model() {
        let p = OpenAiTtsProvider::with_model(None, "tts-1-hd".into());
        assert_eq!(p.model, "tts-1-hd");
    }
}
