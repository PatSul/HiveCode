//! Telnyx NaturalHD TTS provider — cloud natural voices.
//!
//! Built-in natural voices; no voice cloning.

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use tracing::debug;

use super::{
    AudioData, AudioFormat, TtsError, TtsProvider, TtsProviderType, TtsRequest, VoiceInfo,
};

const API_BASE: &str = "https://api.telnyx.com/v2/ai/generate/audio";

/// Built-in Telnyx NaturalHD voices.
const VOICES: &[(&str, &str)] = &[
    ("walnut", "Walnut"),
    ("cedar", "Cedar"),
    ("maple", "Maple"),
    ("birch", "Birch"),
    ("oak", "Oak"),
    ("pine", "Pine"),
];

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct TelnyxTtsProvider {
    client: Client,
    api_key: Option<String>,
}

impl TelnyxTtsProvider {
    pub fn new(api_key: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self { client, api_key }
    }
}

#[async_trait]
impl TtsProvider for TelnyxTtsProvider {
    fn provider_type(&self) -> TtsProviderType {
        TtsProviderType::Telnyx
    }

    fn name(&self) -> &str {
        "Telnyx NaturalHD"
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

        let payload = TelnyxPayload {
            text: &request.text,
            voice: &request.voice_id,
            speed: request.speed,
        };

        debug!(voice = request.voice_id, "Telnyx NaturalHD TTS synthesis");

        let resp = self
            .client
            .post(API_BASE)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
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
                _ => TtsError::Other(format!("Telnyx TTS {status}: {body}")),
            });
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::Network(e.to_string()))?;

        Ok(AudioData {
            bytes: bytes.to_vec(),
            format: AudioFormat::Mp3,
            sample_rate: 44100,
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
struct TelnyxPayload<'a> {
    text: &'a str,
    voice: &'a str,
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
        let p = TelnyxTtsProvider::new(Some("key".into()));
        assert_eq!(p.provider_type(), TtsProviderType::Telnyx);
        assert_eq!(p.name(), "Telnyx NaturalHD");
        assert!(!p.supports_cloning());
    }

    #[tokio::test]
    async fn list_voices_returns_all() {
        let p = TelnyxTtsProvider::new(Some("key".into()));
        let voices = p.list_voices().await.unwrap();
        assert_eq!(voices.len(), 6);
        assert!(voices.iter().any(|v| v.id == "walnut"));
    }

    #[tokio::test]
    async fn is_available_with_key() {
        let p = TelnyxTtsProvider::new(Some("key".into()));
        assert!(p.is_available().await);
    }

    #[tokio::test]
    async fn is_available_without_key() {
        let p = TelnyxTtsProvider::new(None);
        assert!(!p.is_available().await);
    }
}
