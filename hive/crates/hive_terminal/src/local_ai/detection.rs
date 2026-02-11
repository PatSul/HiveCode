use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

const DEFAULT_ENDPOINTS: &[(LocalProviderKind, &str)] = &[
    (LocalProviderKind::Ollama, "http://localhost:11434"),
    (LocalProviderKind::LMStudio, "http://localhost:1234"),
    (LocalProviderKind::VLLM, "http://localhost:8000"),
    (LocalProviderKind::LocalAI, "http://localhost:8080"),
    (LocalProviderKind::LlamaCpp, "http://localhost:8081"),
    (LocalProviderKind::TextGenWebUI, "http://localhost:5000"),
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The kind of local AI provider detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalProviderKind {
    Ollama,
    LMStudio,
    VLLM,
    LocalAI,
    LlamaCpp,
    TextGenWebUI,
    Custom,
}

impl LocalProviderKind {
    /// Human-readable display name for the provider.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::LMStudio => "LM Studio",
            Self::VLLM => "vLLM",
            Self::LocalAI => "LocalAI",
            Self::LlamaCpp => "llama.cpp",
            Self::TextGenWebUI => "text-generation-webui",
            Self::Custom => "Custom",
        }
    }

    /// The path to probe for this provider's health or model list.
    fn probe_path(self) -> &'static str {
        match self {
            Self::Ollama => "/api/tags",
            Self::LMStudio | Self::VLLM | Self::LocalAI | Self::Custom => "/v1/models",
            Self::LlamaCpp => "/health",
            Self::TextGenWebUI => "/api/v1/model",
        }
    }
}

impl std::fmt::Display for LocalProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Connection status of a detected provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderStatus {
    Online,
    Offline,
    Error(String),
}

impl ProviderStatus {
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online)
    }
}

/// Metadata about a single model available from a local provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModel {
    pub name: String,
    pub size: Option<u64>,
    pub modified: Option<String>,
}

/// A detected local AI provider with its connection status and available models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedProvider {
    pub kind: LocalProviderKind,
    pub endpoint: String,
    pub status: ProviderStatus,
    pub models: Vec<LocalModel>,
    #[serde(skip)]
    pub last_check: Option<Instant>,
}

impl DetectedProvider {
    /// Create an offline result for a provider that could not be reached.
    fn offline(kind: LocalProviderKind, endpoint: &str) -> Self {
        Self {
            kind,
            endpoint: endpoint.to_string(),
            status: ProviderStatus::Offline,
            models: Vec::new(),
            last_check: Some(Instant::now()),
        }
    }

    /// Create an error result for a provider that responded unexpectedly.
    fn error(kind: LocalProviderKind, endpoint: &str, message: String) -> Self {
        Self {
            kind,
            endpoint: endpoint.to_string(),
            status: ProviderStatus::Error(message),
            models: Vec::new(),
            last_check: Some(Instant::now()),
        }
    }

    /// The number of models available from this provider.
    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Model names as a collected vector of string references.
    pub fn model_names(&self) -> Vec<&str> {
        self.models.iter().map(|m| m.name.as_str()).collect()
    }
}

// ---------------------------------------------------------------------------
// LocalAiDetector
// ---------------------------------------------------------------------------

/// Probes well-known localhost ports for running local AI servers and returns
/// rich [`DetectedProvider`] results with model metadata.
pub struct LocalAiDetector {
    client: Client,
}

impl LocalAiDetector {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .expect("failed to create HTTP client for local AI detection");

        Self { client }
    }

    /// Probe all known default endpoints in parallel.
    ///
    /// Each probe has a 2-second timeout, so the entire scan completes in
    /// roughly 2 seconds regardless of how many providers are checked.
    pub async fn detect_all(&mut self) -> Vec<DetectedProvider> {
        let futures: Vec<_> = DEFAULT_ENDPOINTS
            .iter()
            .map(|(kind, url)| self.probe_endpoint(*kind, url))
            .collect();

        join_all(futures).await
    }

    /// Probe a single endpoint for a specific provider kind.
    pub async fn probe_endpoint(&self, kind: LocalProviderKind, url: &str) -> DetectedProvider {
        match kind {
            LocalProviderKind::Ollama => self.detect_ollama(url).await,
            LocalProviderKind::LMStudio => self.detect_lmstudio(url).await,
            LocalProviderKind::LlamaCpp => self.detect_health_only(url, kind).await,
            LocalProviderKind::TextGenWebUI => self.detect_health_only(url, kind).await,
            _ => self.detect_openai_compatible(url, kind).await,
        }
    }

    /// Detect Ollama by hitting `/api/tags` and parsing the model list.
    pub async fn detect_ollama(&self, url: &str) -> DetectedProvider {
        let probe_url = format!("{}/api/tags", url);
        debug!(provider = "Ollama", %probe_url, "probing");

        let response = match self.client.get(&probe_url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                debug!(provider = "Ollama", error = %e, "unreachable");
                return if is_connection_error(&e) {
                    DetectedProvider::offline(LocalProviderKind::Ollama, url)
                } else {
                    DetectedProvider::error(LocalProviderKind::Ollama, url, format!("{e}"))
                };
            }
        };

        if !response.status().is_success() {
            return DetectedProvider::error(
                LocalProviderKind::Ollama,
                url,
                format!("HTTP {}", response.status()),
            );
        }

        let body = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                return DetectedProvider::error(
                    LocalProviderKind::Ollama,
                    url,
                    format!("failed to read response body: {e}"),
                );
            }
        };

        let models = parse_ollama_tags(&body);

        debug!(provider = "Ollama", model_count = models.len(), "detected");

        DetectedProvider {
            kind: LocalProviderKind::Ollama,
            endpoint: url.to_string(),
            status: ProviderStatus::Online,
            models,
            last_check: Some(Instant::now()),
        }
    }

    /// Detect LM Studio by hitting `/v1/models` (OpenAI-compatible format).
    pub async fn detect_lmstudio(&self, url: &str) -> DetectedProvider {
        self.detect_openai_compatible(url, LocalProviderKind::LMStudio)
            .await
    }

    /// Detect any OpenAI-compatible local provider (vLLM, LocalAI, Custom)
    /// by hitting `/v1/models`.
    pub async fn detect_openai_compatible(
        &self,
        url: &str,
        kind: LocalProviderKind,
    ) -> DetectedProvider {
        let probe_url = format!("{}{}", url, kind.probe_path());
        let name = kind.display_name();
        debug!(provider = name, %probe_url, "probing");

        let response = match self.client.get(&probe_url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                debug!(provider = name, error = %e, "unreachable");
                return if is_connection_error(&e) {
                    DetectedProvider::offline(kind, url)
                } else {
                    DetectedProvider::error(kind, url, format!("{e}"))
                };
            }
        };

        if !response.status().is_success() {
            return DetectedProvider::error(kind, url, format!("HTTP {}", response.status()));
        }

        let body = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                return DetectedProvider::error(
                    kind,
                    url,
                    format!("failed to read response body: {e}"),
                );
            }
        };

        let models = parse_openai_models(&body);

        debug!(provider = name, model_count = models.len(), "detected");

        DetectedProvider {
            kind,
            endpoint: url.to_string(),
            status: ProviderStatus::Online,
            models,
            last_check: Some(Instant::now()),
        }
    }

    /// Health-check-only probe for providers that don't expose a model list
    /// (e.g. llama.cpp `/health`, text-generation-webui `/api/v1/model`).
    async fn detect_health_only(&self, url: &str, kind: LocalProviderKind) -> DetectedProvider {
        let probe_url = format!("{}{}", url, kind.probe_path());
        let name = kind.display_name();
        debug!(provider = name, %probe_url, "probing (health-only)");

        let response = match self.client.get(&probe_url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                debug!(provider = name, error = %e, "unreachable");
                return if is_connection_error(&e) {
                    DetectedProvider::offline(kind, url)
                } else {
                    DetectedProvider::error(kind, url, format!("{e}"))
                };
            }
        };

        if response.status().is_success() {
            debug!(provider = name, "detected (health-only)");
            DetectedProvider {
                kind,
                endpoint: url.to_string(),
                status: ProviderStatus::Online,
                models: Vec::new(),
                last_check: Some(Instant::now()),
            }
        } else {
            DetectedProvider::error(kind, url, format!("HTTP {}", response.status()))
        }
    }
}

impl Default for LocalAiDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Response parsers
// ---------------------------------------------------------------------------

/// Parse Ollama `/api/tags` JSON into a vec of [`LocalModel`].
///
/// Expected format: `{ "models": [{ "name": "...", "size": N, "modified_at": "..." }] }`
fn parse_ollama_tags(body: &str) -> Vec<LocalModel> {
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

    match serde_json::from_str::<TagsResponse>(body) {
        Ok(resp) => resp
            .models
            .unwrap_or_default()
            .into_iter()
            .filter_map(|m| {
                Some(LocalModel {
                    name: m.name?,
                    size: m.size,
                    modified: m.modified_at,
                })
            })
            .collect(),
        Err(e) => {
            warn!(error = %e, "failed to parse Ollama tags response");
            Vec::new()
        }
    }
}

/// Parse OpenAI-compatible `/v1/models` JSON into a vec of [`LocalModel`].
///
/// Expected format: `{ "data": [{ "id": "model-name" }] }`
fn parse_openai_models(body: &str) -> Vec<LocalModel> {
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
            .filter_map(|m| {
                Some(LocalModel {
                    name: m.id?,
                    size: None,
                    modified: None,
                })
            })
            .collect(),
        Err(e) => {
            warn!(error = %e, "failed to parse OpenAI-compatible models response");
            Vec::new()
        }
    }
}

/// Check whether a reqwest error is a connection-level failure (refused,
/// timeout, DNS) as opposed to a protocol-level error.
fn is_connection_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- LocalProviderKind ---------------------------------------------------

    #[test]
    fn provider_kind_display_names() {
        assert_eq!(LocalProviderKind::Ollama.display_name(), "Ollama");
        assert_eq!(LocalProviderKind::LMStudio.display_name(), "LM Studio");
        assert_eq!(LocalProviderKind::VLLM.display_name(), "vLLM");
        assert_eq!(LocalProviderKind::LocalAI.display_name(), "LocalAI");
        assert_eq!(LocalProviderKind::LlamaCpp.display_name(), "llama.cpp");
        assert_eq!(
            LocalProviderKind::TextGenWebUI.display_name(),
            "text-generation-webui"
        );
        assert_eq!(LocalProviderKind::Custom.display_name(), "Custom");
    }

    #[test]
    fn provider_kind_display_trait() {
        let kind = LocalProviderKind::Ollama;
        assert_eq!(format!("{kind}"), "Ollama");
    }

    #[test]
    fn provider_kind_probe_paths() {
        assert_eq!(LocalProviderKind::Ollama.probe_path(), "/api/tags");
        assert_eq!(LocalProviderKind::LMStudio.probe_path(), "/v1/models");
        assert_eq!(LocalProviderKind::VLLM.probe_path(), "/v1/models");
        assert_eq!(LocalProviderKind::LocalAI.probe_path(), "/v1/models");
        assert_eq!(LocalProviderKind::LlamaCpp.probe_path(), "/health");
        assert_eq!(
            LocalProviderKind::TextGenWebUI.probe_path(),
            "/api/v1/model"
        );
        assert_eq!(LocalProviderKind::Custom.probe_path(), "/v1/models");
    }

    #[test]
    fn provider_kind_equality_and_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(LocalProviderKind::Ollama);
        set.insert(LocalProviderKind::Ollama);
        set.insert(LocalProviderKind::VLLM);
        assert_eq!(set.len(), 2);
    }

    // -- ProviderStatus ------------------------------------------------------

    #[test]
    fn provider_status_is_online() {
        assert!(ProviderStatus::Online.is_online());
        assert!(!ProviderStatus::Offline.is_online());
        assert!(!ProviderStatus::Error("oops".to_string()).is_online());
    }

    // -- DetectedProvider constructors ---------------------------------------

    #[test]
    fn detected_provider_offline() {
        let p = DetectedProvider::offline(LocalProviderKind::Ollama, "http://localhost:11434");
        assert_eq!(p.kind, LocalProviderKind::Ollama);
        assert_eq!(p.endpoint, "http://localhost:11434");
        assert!(matches!(p.status, ProviderStatus::Offline));
        assert!(p.models.is_empty());
        assert!(p.last_check.is_some());
    }

    #[test]
    fn detected_provider_error() {
        let p = DetectedProvider::error(
            LocalProviderKind::VLLM,
            "http://localhost:8000",
            "HTTP 500".to_string(),
        );
        assert_eq!(p.kind, LocalProviderKind::VLLM);
        assert!(matches!(p.status, ProviderStatus::Error(ref msg) if msg == "HTTP 500"));
        assert!(p.models.is_empty());
    }

    #[test]
    fn detected_provider_model_count_and_names() {
        let p = DetectedProvider {
            kind: LocalProviderKind::Ollama,
            endpoint: "http://localhost:11434".to_string(),
            status: ProviderStatus::Online,
            models: vec![
                LocalModel {
                    name: "llama3.2:latest".to_string(),
                    size: Some(4_000_000_000),
                    modified: Some("2025-01-15".to_string()),
                },
                LocalModel {
                    name: "mistral:7b".to_string(),
                    size: None,
                    modified: None,
                },
            ],
            last_check: Some(Instant::now()),
        };

        assert_eq!(p.model_count(), 2);
        assert_eq!(p.model_names(), vec!["llama3.2:latest", "mistral:7b"]);
    }

    // -- parse_ollama_tags ---------------------------------------------------

    #[test]
    fn parse_ollama_tags_valid() {
        let body = r#"{
            "models": [
                {"name": "llama3.2:latest", "size": 4000000000, "modified_at": "2025-01-15"},
                {"name": "mistral:7b", "size": 7000000000}
            ]
        }"#;
        let models = parse_ollama_tags(body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama3.2:latest");
        assert_eq!(models[0].size, Some(4_000_000_000));
        assert_eq!(models[0].modified.as_deref(), Some("2025-01-15"));
        assert_eq!(models[1].name, "mistral:7b");
        assert!(models[1].modified.is_none());
    }

    #[test]
    fn parse_ollama_tags_empty_list() {
        let models = parse_ollama_tags(r#"{"models":[]}"#);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_ollama_tags_missing_models_field() {
        let models = parse_ollama_tags(r#"{}"#);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_ollama_tags_skips_entries_without_name() {
        let body = r#"{"models": [{"size": 123}, {"name": "good"}]}"#;
        let models = parse_ollama_tags(body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "good");
    }

    #[test]
    fn parse_ollama_tags_invalid_json() {
        let models = parse_ollama_tags("not json at all");
        assert!(models.is_empty());
    }

    // -- parse_openai_models -------------------------------------------------

    #[test]
    fn parse_openai_models_valid() {
        let body = r#"{"data": [{"id": "gpt-4"}, {"id": "llama-3"}]}"#;
        let models = parse_openai_models(body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "gpt-4");
        assert_eq!(models[1].name, "llama-3");
        assert!(models[0].size.is_none());
        assert!(models[0].modified.is_none());
    }

    #[test]
    fn parse_openai_models_empty_list() {
        let models = parse_openai_models(r#"{"data":[]}"#);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_openai_models_missing_data_field() {
        let models = parse_openai_models(r#"{}"#);
        assert!(models.is_empty());
    }

    #[test]
    fn parse_openai_models_skips_entries_without_id() {
        let body = r#"{"data": [{}, {"id": "valid-model"}]}"#;
        let models = parse_openai_models(body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "valid-model");
    }

    #[test]
    fn parse_openai_models_invalid_json() {
        let models = parse_openai_models("<<<");
        assert!(models.is_empty());
    }

    // -- DEFAULT_ENDPOINTS ---------------------------------------------------

    #[test]
    fn default_endpoints_cover_all_non_custom_kinds() {
        assert_eq!(DEFAULT_ENDPOINTS.len(), 6);
        assert_eq!(DEFAULT_ENDPOINTS[0].0, LocalProviderKind::Ollama);
        assert_eq!(DEFAULT_ENDPOINTS[0].1, "http://localhost:11434");
        assert_eq!(DEFAULT_ENDPOINTS[1].0, LocalProviderKind::LMStudio);
        assert_eq!(DEFAULT_ENDPOINTS[1].1, "http://localhost:1234");
        assert_eq!(DEFAULT_ENDPOINTS[2].0, LocalProviderKind::VLLM);
        assert_eq!(DEFAULT_ENDPOINTS[2].1, "http://localhost:8000");
        assert_eq!(DEFAULT_ENDPOINTS[3].0, LocalProviderKind::LocalAI);
        assert_eq!(DEFAULT_ENDPOINTS[3].1, "http://localhost:8080");
        assert_eq!(DEFAULT_ENDPOINTS[4].0, LocalProviderKind::LlamaCpp);
        assert_eq!(DEFAULT_ENDPOINTS[4].1, "http://localhost:8081");
        assert_eq!(DEFAULT_ENDPOINTS[5].0, LocalProviderKind::TextGenWebUI);
        assert_eq!(DEFAULT_ENDPOINTS[5].1, "http://localhost:5000");
    }

    // -- LocalAiDetector construction ----------------------------------------

    #[test]
    fn detector_construction() {
        let detector = LocalAiDetector::new();
        // Just verify it doesn't panic — the client is opaque.
        drop(detector);
    }

    #[test]
    fn detector_default_trait() {
        let detector = LocalAiDetector::default();
        drop(detector);
    }

    // -- LocalModel ----------------------------------------------------------

    #[test]
    fn local_model_serde_roundtrip() {
        let model = LocalModel {
            name: "llama3.2:latest".to_string(),
            size: Some(4_000_000_000),
            modified: Some("2025-01-15T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&model).unwrap();
        let deserialized: LocalModel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, model.name);
        assert_eq!(deserialized.size, model.size);
        assert_eq!(deserialized.modified, model.modified);
    }

    #[test]
    fn local_model_optional_fields() {
        let model = LocalModel {
            name: "tiny-model".to_string(),
            size: None,
            modified: None,
        };
        assert!(model.size.is_none());
        assert!(model.modified.is_none());
    }

    // -- DetectedProvider serde ----------------------------------------------

    #[test]
    fn detected_provider_serde_skips_instant() {
        let provider = DetectedProvider {
            kind: LocalProviderKind::Ollama,
            endpoint: "http://localhost:11434".to_string(),
            status: ProviderStatus::Online,
            models: vec![LocalModel {
                name: "test".to_string(),
                size: None,
                modified: None,
            }],
            last_check: Some(Instant::now()),
        };

        let json = serde_json::to_string(&provider).unwrap();
        assert!(!json.contains("last_check"));

        let deserialized: DetectedProvider = serde_json::from_str(&json).unwrap();
        assert!(deserialized.last_check.is_none());
        assert_eq!(deserialized.kind, LocalProviderKind::Ollama);
        assert_eq!(deserialized.models.len(), 1);
    }

    // -- ProviderKind serde --------------------------------------------------

    #[test]
    fn provider_kind_serde_roundtrip() {
        let kinds = [
            LocalProviderKind::Ollama,
            LocalProviderKind::LMStudio,
            LocalProviderKind::VLLM,
            LocalProviderKind::LocalAI,
            LocalProviderKind::LlamaCpp,
            LocalProviderKind::TextGenWebUI,
            LocalProviderKind::Custom,
        ];
        for kind in &kinds {
            let json = serde_json::to_string(kind).unwrap();
            let deserialized: LocalProviderKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*kind, deserialized);
        }
    }
}
