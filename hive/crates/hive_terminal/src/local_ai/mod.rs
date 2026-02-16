pub mod detection;

use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, warn};

const DETECT_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Information about a detected local AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProviderInfo {
    pub name: String,
    pub base_url: String,
    pub port: u16,
    pub available: bool,
    pub models: Vec<String>,
}

/// Progress update emitted while pulling an Ollama model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullProgress {
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
}

/// Metadata about an Ollama model returned by the tags/show endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal probe definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ProviderProbe {
    name: &'static str,
    base_url: &'static str,
    test_path: &'static str,
    port: u16,
    model_parser: ModelParser,
}

#[derive(Debug, Clone)]
enum ModelParser {
    /// Ollama `/api/tags` format: `{ "models": [{ "name": "..." }] }`
    OllamaTags,
    /// OpenAI-compatible `/v1/models` format: `{ "data": [{ "id": "..." }] }`
    OpenAIModels,
    /// Health-check only -- just verify the endpoint returns 200.
    HealthOnly,
}

// ---------------------------------------------------------------------------
// LocalAiDetector
// ---------------------------------------------------------------------------

/// Probes well-known localhost ports for running local AI servers.
pub struct LocalAiDetector {
    client: Client,
    probes: Vec<ProviderProbe>,
}

impl LocalAiDetector {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(DETECT_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        let probes = vec![
            ProviderProbe {
                name: "Ollama",
                base_url: "http://localhost:11434",
                test_path: "/api/tags",
                port: 11434,
                model_parser: ModelParser::OllamaTags,
            },
            ProviderProbe {
                name: "LM Studio",
                base_url: "http://localhost:1234",
                test_path: "/v1/models",
                port: 1234,
                model_parser: ModelParser::OpenAIModels,
            },
            ProviderProbe {
                name: "vLLM",
                base_url: "http://localhost:8000",
                test_path: "/v1/models",
                port: 8000,
                model_parser: ModelParser::OpenAIModels,
            },
            ProviderProbe {
                name: "LocalAI",
                base_url: "http://localhost:8080",
                test_path: "/v1/models",
                port: 8080,
                model_parser: ModelParser::OpenAIModels,
            },
            ProviderProbe {
                name: "llama.cpp",
                base_url: "http://localhost:8081",
                test_path: "/health",
                port: 8081,
                model_parser: ModelParser::HealthOnly,
            },
            ProviderProbe {
                name: "text-generation-webui",
                base_url: "http://localhost:5000",
                test_path: "/api/v1/model",
                port: 5000,
                model_parser: ModelParser::HealthOnly,
            },
            ProviderProbe {
                name: "Custom Local",
                base_url: "http://localhost:8090",
                test_path: "/v1/models",
                port: 8090,
                model_parser: ModelParser::OpenAIModels,
            },
        ];

        Self { client, probes }
    }

    /// Probe all known local AI providers in parallel.
    ///
    /// Every probe has a 2-second timeout so the entire scan completes in ~2 s
    /// regardless of how many providers are checked.
    pub async fn detect_all(&self) -> Vec<LocalProviderInfo> {
        let futures: Vec<_> = self
            .probes
            .iter()
            .map(|probe| self.probe_provider(probe))
            .collect();

        join_all(futures).await
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    async fn probe_provider(&self, probe: &ProviderProbe) -> LocalProviderInfo {
        let url = format!("{}{}", probe.base_url, probe.test_path);
        debug!(provider = probe.name, %url, "probing local AI provider");

        let result = self.client.get(&url).send().await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                let models = match probe.model_parser {
                    ModelParser::OllamaTags => match resp.text().await {
                        Ok(body) => self.parse_ollama_tags(&body),
                        Err(_) => Vec::new(),
                    },
                    ModelParser::OpenAIModels => match resp.text().await {
                        Ok(body) => self.parse_openai_models(&body),
                        Err(_) => Vec::new(),
                    },
                    ModelParser::HealthOnly => Vec::new(),
                };

                debug!(
                    provider = probe.name,
                    model_count = models.len(),
                    "provider detected"
                );

                LocalProviderInfo {
                    name: probe.name.to_string(),
                    base_url: probe.base_url.to_string(),
                    port: probe.port,
                    available: true,
                    models,
                }
            }
            Ok(resp) => {
                debug!(
                    provider = probe.name,
                    status = %resp.status(),
                    "provider responded with non-success status"
                );
                LocalProviderInfo {
                    name: probe.name.to_string(),
                    base_url: probe.base_url.to_string(),
                    port: probe.port,
                    available: false,
                    models: Vec::new(),
                }
            }
            Err(e) => {
                debug!(
                    provider = probe.name,
                    error = %e,
                    "provider unreachable"
                );
                LocalProviderInfo {
                    name: probe.name.to_string(),
                    base_url: probe.base_url.to_string(),
                    port: probe.port,
                    available: false,
                    models: Vec::new(),
                }
            }
        }
    }

    /// Parse Ollama `/api/tags` response: `{ "models": [{ "name": "llama3.2:latest" }] }`
    fn parse_ollama_tags(&self, body: &str) -> Vec<String> {
        #[derive(Deserialize)]
        struct TagsResponse {
            models: Option<Vec<TagModel>>,
        }
        #[derive(Deserialize)]
        struct TagModel {
            name: Option<String>,
        }

        match serde_json::from_str::<TagsResponse>(body) {
            Ok(resp) => resp
                .models
                .unwrap_or_default()
                .into_iter()
                .filter_map(|m| m.name)
                .collect(),
            Err(e) => {
                warn!(error = %e, "failed to parse Ollama tags response");
                Vec::new()
            }
        }
    }

    /// Parse OpenAI-compatible `/v1/models` response: `{ "data": [{ "id": "model-name" }] }`
    fn parse_openai_models(&self, body: &str) -> Vec<String> {
        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Option<Vec<ModelEntry>>,
        }
        #[derive(Deserialize)]
        struct ModelEntry {
            id: Option<String>,
        }

        match serde_json::from_str::<ModelsResponse>(body) {
            Ok(resp) => resp
                .data
                .unwrap_or_default()
                .into_iter()
                .filter_map(|m| m.id)
                .collect(),
            Err(e) => {
                warn!(error = %e, "failed to parse OpenAI-compatible models response");
                Vec::new()
            }
        }
    }
}

impl Default for LocalAiDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// OllamaManager
// ---------------------------------------------------------------------------

/// Ollama-specific management operations (pull, delete, show, list).
pub struct OllamaManager {
    client: Client,
    base_url: String,
}

impl OllamaManager {
    pub fn new(base_url: Option<String>) -> Self {
        let base_url = base_url.unwrap_or_else(|| "http://localhost:11434".to_string());
        let client = Client::builder()
            .timeout(Duration::from_secs(300)) // pulls can take a long time
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    /// Pull a model with progress reporting via an `mpsc` channel.
    ///
    /// Sends NDJSON progress lines as [`PullProgress`] values until the pull
    /// finishes or an error occurs.
    pub async fn pull_model(
        &self,
        model: &str,
        tx: mpsc::Sender<PullProgress>,
    ) -> Result<(), String> {
        let url = format!("{}/api/pull", self.base_url);

        let body = serde_json::json!({
            "name": model,
            "stream": true,
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama pull request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Ollama pull failed with status {}",
                response.status()
            ));
        }

        // Stream NDJSON lines from the response body.
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Error reading pull stream: {e}"))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines.
            while let Some(newline_pos) = buffer.find('\n') {
                let line: String = buffer.drain(..=newline_pos).collect();
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                if let Ok(progress) = serde_json::from_str::<PullProgress>(line) {
                    // Best-effort send; if the receiver dropped, we still finish the pull.
                    let _ = tx.send(progress).await;
                }
            }
        }

        // Process any remaining data in the buffer.
        let remaining = buffer.trim();
        if !remaining.is_empty()
            && let Ok(progress) = serde_json::from_str::<PullProgress>(remaining) {
                let _ = tx.send(progress).await;
            }

        // Signal completion.
        let _ = tx
            .send(PullProgress {
                status: "success".to_string(),
                completed: None,
                total: None,
            })
            .await;

        Ok(())
    }

    /// Delete a model from Ollama.
    pub async fn delete_model(&self, model: &str) -> Result<(), String> {
        let url = format!("{}/api/delete", self.base_url);

        let body = serde_json::json!({ "name": model });

        let response = self
            .client
            .delete(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama delete request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Ollama delete failed with status {}",
                response.status()
            ))
        }
    }

    /// Show model info (parameters, size, template, etc.).
    pub async fn show_model(&self, model: &str) -> Result<OllamaModelInfo, String> {
        let url = format!("{}/api/show", self.base_url);

        let body = serde_json::json!({ "name": model });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama show request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Ollama show failed with status {}",
                response.status()
            ));
        }

        // The show endpoint returns a rich object; we extract what we need.
        #[derive(Deserialize)]
        struct ShowResponse {
            #[serde(default)]
            modified_at: Option<String>,
            #[serde(default)]
            size: Option<u64>,
        }

        let data: ShowResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama show response: {e}"))?;

        Ok(OllamaModelInfo {
            name: model.to_string(),
            size: data.size,
            modified_at: data.modified_at,
        })
    }

    /// List all models available in Ollama.
    pub async fn list_models(&self) -> Result<Vec<OllamaModelInfo>, String> {
        let url = format!("{}/api/tags", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Ollama list request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Ollama list failed with status {}",
                response.status()
            ));
        }

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Option<Vec<TagModel>>,
        }
        #[derive(Deserialize)]
        struct TagModel {
            name: Option<String>,
            size: Option<u64>,
            modified_at: Option<String>,
        }

        let data: TagsResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama tags response: {e}"))?;

        let models = data
            .models
            .unwrap_or_default()
            .into_iter()
            .filter_map(|m| {
                Some(OllamaModelInfo {
                    name: m.name?,
                    size: m.size,
                    modified_at: m.modified_at,
                })
            })
            .collect();

        Ok(models)
    }
}

impl Default for OllamaManager {
    fn default() -> Self {
        Self::new(None)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ollama_tags_valid() {
        let detector = LocalAiDetector::new();
        let body = r#"{"models":[{"name":"llama3.2:latest"},{"name":"mistral:7b"}]}"#;
        let models = detector.parse_ollama_tags(body);
        assert_eq!(models, vec!["llama3.2:latest", "mistral:7b"]);
    }

    #[test]
    fn parse_ollama_tags_empty() {
        let detector = LocalAiDetector::new();
        let body = r#"{"models":[]}"#;
        let models = detector.parse_ollama_tags(body);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_ollama_tags_missing_field() {
        let detector = LocalAiDetector::new();
        let body = r#"{}"#;
        let models = detector.parse_ollama_tags(body);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_ollama_tags_invalid_json() {
        let detector = LocalAiDetector::new();
        let models = detector.parse_ollama_tags("not json at all");
        assert!(models.is_empty());
    }

    #[test]
    fn parse_openai_models_valid() {
        let detector = LocalAiDetector::new();
        let body = r#"{"data":[{"id":"gpt-4"},{"id":"llama-3"}]}"#;
        let models = detector.parse_openai_models(body);
        assert_eq!(models, vec!["gpt-4", "llama-3"]);
    }

    #[test]
    fn parse_openai_models_empty() {
        let detector = LocalAiDetector::new();
        let body = r#"{"data":[]}"#;
        let models = detector.parse_openai_models(body);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_openai_models_missing_field() {
        let detector = LocalAiDetector::new();
        let body = r#"{}"#;
        let models = detector.parse_openai_models(body);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_openai_models_invalid_json() {
        let detector = LocalAiDetector::new();
        let models = detector.parse_openai_models("<<<");
        assert!(models.is_empty());
    }

    #[test]
    fn detector_default_probes() {
        let detector = LocalAiDetector::new();
        assert_eq!(detector.probes.len(), 7);
        assert_eq!(detector.probes[0].name, "Ollama");
        assert_eq!(detector.probes[0].port, 11434);
        assert_eq!(detector.probes[1].name, "LM Studio");
        assert_eq!(detector.probes[1].port, 1234);
    }

    #[test]
    fn ollama_manager_default_url() {
        let mgr = OllamaManager::new(None);
        assert_eq!(mgr.base_url, "http://localhost:11434");
    }

    #[test]
    fn ollama_manager_custom_url() {
        let mgr = OllamaManager::new(Some("http://192.168.1.100:11434".to_string()));
        assert_eq!(mgr.base_url, "http://192.168.1.100:11434");
    }
}
