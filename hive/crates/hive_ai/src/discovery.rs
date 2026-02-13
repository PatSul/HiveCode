//! Local AI server auto-discovery.
//!
//! Probes localhost for running AI servers (Ollama, LM Studio, vLLM, llama.cpp,
//! text-generation-webui) and discovers their available models.
//!
//! **Security**: Only `127.0.0.1`, `localhost`, and `::1` are ever probed.
//! All non-localhost URLs are rejected before any network request is made.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::types::{ModelInfo, ModelTier, ProviderType};

// ---------------------------------------------------------------------------
// Well-known local AI server ports
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum ProbeProtocol {
    /// Ollama REST API (`/api/tags`)
    Ollama,
    /// OpenAI-compatible API (`/v1/models`)
    OpenAICompat,
}

struct WellKnownPort {
    port: u16,
    provider_type: ProviderType,
    name: &'static str,
    protocol: ProbeProtocol,
}

const WELL_KNOWN_PORTS: &[WellKnownPort] = &[
    WellKnownPort {
        port: 11434,
        provider_type: ProviderType::Ollama,
        name: "Ollama",
        protocol: ProbeProtocol::Ollama,
    },
    WellKnownPort {
        port: 1234,
        provider_type: ProviderType::LMStudio,
        name: "LM Studio",
        protocol: ProbeProtocol::OpenAICompat,
    },
    WellKnownPort {
        port: 8000,
        provider_type: ProviderType::GenericLocal,
        name: "vLLM",
        protocol: ProbeProtocol::OpenAICompat,
    },
    WellKnownPort {
        port: 8080,
        provider_type: ProviderType::GenericLocal,
        name: "LocalAI",
        protocol: ProbeProtocol::OpenAICompat,
    },
    WellKnownPort {
        port: 5000,
        provider_type: ProviderType::GenericLocal,
        name: "TextGenWebUI",
        protocol: ProbeProtocol::OpenAICompat,
    },
];

// ---------------------------------------------------------------------------
// API response types (for deserialization only)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelEntry {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIModelsResponse {
    data: Option<Vec<OpenAIModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModelEntry {
    id: String,
}

// ---------------------------------------------------------------------------
// Discovery state
// ---------------------------------------------------------------------------

/// A discovered local AI server with its models.
#[derive(Debug, Clone)]
pub struct DiscoveredProvider {
    pub url: String,
    pub port: u16,
    pub provider_type: ProviderType,
    pub name: String,
    pub online: bool,
    pub models: Vec<ModelInfo>,
}

/// Current discovery state shared across threads.
#[derive(Debug, Clone, Default)]
pub struct DiscoveryState {
    pub providers: Vec<DiscoveredProvider>,
    pub last_scan: Option<std::time::Instant>,
}

impl DiscoveryState {
    /// All models from all online discovered providers.
    pub fn all_models(&self) -> Vec<ModelInfo> {
        self.providers
            .iter()
            .filter(|p| p.online)
            .flat_map(|p| p.models.clone())
            .collect()
    }

    /// Provider online/offline status.
    pub fn provider_status(&self) -> Vec<(ProviderType, bool)> {
        self.providers
            .iter()
            .map(|p| (p.provider_type, p.online))
            .collect()
    }

    /// Whether any local provider is online.
    pub fn any_online(&self) -> bool {
        self.providers.iter().any(|p| p.online)
    }
}

// ---------------------------------------------------------------------------
// LocalDiscovery engine
// ---------------------------------------------------------------------------

/// Probes localhost for running AI servers and discovers their models.
pub struct LocalDiscovery {
    state: Arc<RwLock<DiscoveryState>>,
    config_urls: Vec<(ProviderType, String)>,
    client: reqwest::Client,
}

impl LocalDiscovery {
    /// Create a new discovery engine from config URLs.
    ///
    /// Config URLs are probed first; their ports are deduplicated against the
    /// well-known port scan list.
    pub fn new(config_urls: Vec<(ProviderType, String)>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            state: Arc::new(RwLock::new(DiscoveryState::default())),
            config_urls,
            client,
        }
    }

    /// Cloneable handle to the shared state.
    pub fn state(&self) -> Arc<RwLock<DiscoveryState>> {
        Arc::clone(&self.state)
    }

    /// Snapshot of current discovery state.
    pub fn snapshot(&self) -> DiscoveryState {
        self.state.read().clone()
    }

    /// Blocking scan: creates a Tokio runtime internally so this can be called
    /// from any thread (including GPUI's smol-based executor via `std::thread::spawn`).
    pub fn scan_all_blocking(&self) {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                warn!("Failed to create Tokio runtime for discovery: {e}");
                return;
            }
        };
        rt.block_on(self.scan_all());
    }

    /// Run a full scan: probe config URLs + well-known ports concurrently.
    pub async fn scan_all(&self) {
        let mut futures: Vec<
            std::pin::Pin<Box<dyn std::future::Future<Output = Option<DiscoveredProvider>> + Send>>,
        > = Vec::new();
        let mut scanned_ports: HashSet<u16> = HashSet::new();

        // 1. Config URLs first
        for (ptype, url) in &self.config_urls {
            if let Some(port) = extract_port(url) {
                scanned_ports.insert(port);
            }
            let protocol = match ptype {
                ProviderType::Ollama => ProbeProtocol::Ollama,
                _ => ProbeProtocol::OpenAICompat,
            };
            let client = self.client.clone();
            let url = url.clone();
            let ptype = *ptype;
            let name = format!("{ptype}");
            futures.push(Box::pin(async move {
                probe(&client, &url, ptype, &name, protocol).await
            }));
        }

        // 2. Well-known ports (skip already-covered by config URLs)
        for wk in WELL_KNOWN_PORTS {
            if scanned_ports.contains(&wk.port) {
                continue;
            }
            let client = self.client.clone();
            let url = format!("http://127.0.0.1:{}", wk.port);
            let ptype = wk.provider_type;
            let name = wk.name.to_string();
            let protocol = wk.protocol;
            futures.push(Box::pin(async move {
                probe(&client, &url, ptype, &name, protocol).await
            }));
        }

        // Run all probes concurrently
        let results = futures::future::join_all(futures).await;

        let providers: Vec<DiscoveredProvider> = results.into_iter().flatten().collect();
        let online_count = providers.iter().filter(|p| p.online).count();
        let model_count: usize = providers.iter().map(|p| p.models.len()).sum();

        info!(
            "Discovery scan complete: {} providers ({} online), {} models",
            providers.len(),
            online_count,
            model_count
        );

        // Update shared state
        let mut state = self.state.write();
        state.providers = providers;
        state.last_scan = Some(std::time::Instant::now());
    }
}

// ---------------------------------------------------------------------------
// Probing
// ---------------------------------------------------------------------------

async fn probe(
    client: &reqwest::Client,
    url: &str,
    provider_type: ProviderType,
    name: &str,
    protocol: ProbeProtocol,
) -> Option<DiscoveredProvider> {
    // Security: only probe localhost
    if !is_localhost_url(url) {
        warn!("Refusing to probe non-localhost URL: {}", url);
        return None;
    }

    let port = extract_port(url).unwrap_or(0);

    match protocol {
        ProbeProtocol::Ollama => probe_ollama(client, url, provider_type, name, port).await,
        ProbeProtocol::OpenAICompat => {
            probe_openai_compat(client, url, provider_type, name, port).await
        }
    }
}

async fn probe_ollama(
    client: &reqwest::Client,
    base_url: &str,
    provider_type: ProviderType,
    name: &str,
    port: u16,
) -> Option<DiscoveredProvider> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            debug!("{name} at {base_url} returned {}", r.status());
            return Some(DiscoveredProvider {
                url: base_url.to_string(),
                port,
                provider_type,
                name: name.to_string(),
                online: false,
                models: vec![],
            });
        }
        Err(_) => return None, // Not running
    };

    let data: OllamaTagsResponse = match resp.json().await {
        Ok(d) => d,
        Err(e) => {
            debug!("Failed to parse {name} response: {e}");
            return Some(DiscoveredProvider {
                url: base_url.to_string(),
                port,
                provider_type,
                name: name.to_string(),
                online: true,
                models: vec![],
            });
        }
    };

    let models = data
        .models
        .unwrap_or_default()
        .into_iter()
        .map(|m| ModelInfo {
            id: m.name.clone(),
            name: m.name,
            provider: "ollama".into(),
            provider_type,
            tier: ModelTier::Free,
            context_window: 8192,
            input_price_per_mtok: 0.0,
            output_price_per_mtok: 0.0,
            capabilities: Default::default(),
        })
        .collect();

    Some(DiscoveredProvider {
        url: base_url.to_string(),
        port,
        provider_type,
        name: name.to_string(),
        online: true,
        models,
    })
}

async fn probe_openai_compat(
    client: &reqwest::Client,
    base_url: &str,
    provider_type: ProviderType,
    name: &str,
    port: u16,
) -> Option<DiscoveredProvider> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            debug!("{name} at {base_url} returned {}", r.status());
            return Some(DiscoveredProvider {
                url: base_url.to_string(),
                port,
                provider_type,
                name: name.to_string(),
                online: false,
                models: vec![],
            });
        }
        Err(_) => return None,
    };

    let data: OpenAIModelsResponse = match resp.json().await {
        Ok(d) => d,
        Err(e) => {
            debug!("Failed to parse {name} models response: {e}");
            return Some(DiscoveredProvider {
                url: base_url.to_string(),
                port,
                provider_type,
                name: name.to_string(),
                online: true,
                models: vec![],
            });
        }
    };

    let provider_str = provider_type.to_string();
    let models = data
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id,
            provider: provider_str.clone(),
            provider_type,
            tier: ModelTier::Free,
            context_window: 8192,
            input_price_per_mtok: 0.0,
            output_price_per_mtok: 0.0,
            capabilities: Default::default(),
        })
        .collect();

    Some(DiscoveredProvider {
        url: base_url.to_string(),
        port,
        provider_type,
        name: name.to_string(),
        online: true,
        models,
    })
}

// ---------------------------------------------------------------------------
// Security validation
// ---------------------------------------------------------------------------

/// Check if a URL points to localhost. Rejects all non-local hosts.
fn is_localhost_url(url: &str) -> bool {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);

    let host_port = without_scheme.split('/').next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        // IPv6: [::1]:port
        host_port
            .split(']')
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
    } else {
        host_port.split(':').next().unwrap_or("")
    };

    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Extract port from a URL string.
fn extract_port(url: &str) -> Option<u16> {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);

    let host_port = without_scheme.split('/').next().unwrap_or("");

    if host_port.starts_with('[') {
        // IPv6: [::1]:port
        host_port
            .rsplit_once("]:")
            .and_then(|(_, p)| p.parse().ok())
    } else {
        host_port.rsplit_once(':').and_then(|(_, p)| p.parse().ok())
    }
    .or_else(|| {
        if url.starts_with("https://") {
            Some(443)
        } else {
            Some(80)
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_localhost_only_validation() {
        assert!(is_localhost_url("http://localhost:11434"));
        assert!(is_localhost_url("http://127.0.0.1:1234"));
        assert!(is_localhost_url("http://localhost"));
        assert!(is_localhost_url("https://localhost:8080"));
        assert!(is_localhost_url("http://[::1]:8080"));

        assert!(!is_localhost_url("http://192.168.1.100:11434"));
        assert!(!is_localhost_url("http://example.com:11434"));
        assert!(!is_localhost_url("http://10.0.0.1:1234"));
        assert!(!is_localhost_url("not-a-url"));
    }

    #[test]
    fn test_extract_port() {
        assert_eq!(extract_port("http://localhost:11434"), Some(11434));
        assert_eq!(extract_port("http://127.0.0.1:1234"), Some(1234));
        assert_eq!(extract_port("http://localhost"), Some(80));
        assert_eq!(extract_port("https://localhost"), Some(443));
        assert_eq!(extract_port("http://[::1]:8080"), Some(8080));
    }

    #[test]
    fn test_duplicate_port_dedup() {
        let config_urls = vec![(ProviderType::Ollama, "http://localhost:11434".to_string())];
        let discovery = LocalDiscovery::new(config_urls);
        assert_eq!(discovery.config_urls.len(), 1);
        assert_eq!(discovery.config_urls[0].1, "http://localhost:11434");
    }

    #[tokio::test]
    async fn test_scan_completes_gracefully() {
        // Probe a port that is almost certainly not running any AI server.
        // We only care that scan_all completes without panicking and produces
        // valid state. Real servers may be running on well-known ports.
        let discovery = LocalDiscovery::new(vec![(
            ProviderType::Ollama,
            "http://127.0.0.1:19999".to_string(),
        )]);
        discovery.scan_all().await;
        let state = discovery.snapshot();
        assert!(state.last_scan.is_some());
        // Our explicit port 19999 should not be online.
        let explicit = state.providers.iter().find(|p| p.port == 19999);
        assert!(
            explicit.is_none() || !explicit.unwrap().online,
            "Port 19999 should not have a server"
        );
    }

    #[tokio::test]
    async fn test_discovered_models_aggregation() {
        let state = DiscoveryState {
            providers: vec![
                DiscoveredProvider {
                    url: "http://127.0.0.1:11434".into(),
                    port: 11434,
                    provider_type: ProviderType::Ollama,
                    name: "Ollama".into(),
                    online: true,
                    models: vec![ModelInfo {
                        id: "llama3:latest".into(),
                        name: "llama3:latest".into(),
                        provider: "ollama".into(),
                        provider_type: ProviderType::Ollama,
                        tier: ModelTier::Free,
                        context_window: 8192,
                        input_price_per_mtok: 0.0,
                        output_price_per_mtok: 0.0,
                        capabilities: Default::default(),
                    }],
                },
                DiscoveredProvider {
                    url: "http://127.0.0.1:1234".into(),
                    port: 1234,
                    provider_type: ProviderType::LMStudio,
                    name: "LM Studio".into(),
                    online: true,
                    models: vec![ModelInfo {
                        id: "qwen2.5-coder-7b".into(),
                        name: "qwen2.5-coder-7b".into(),
                        provider: "lmstudio".into(),
                        provider_type: ProviderType::LMStudio,
                        tier: ModelTier::Free,
                        context_window: 8192,
                        input_price_per_mtok: 0.0,
                        output_price_per_mtok: 0.0,
                        capabilities: Default::default(),
                    }],
                },
                DiscoveredProvider {
                    url: "http://127.0.0.1:9999".into(),
                    port: 9999,
                    provider_type: ProviderType::GenericLocal,
                    name: "Offline".into(),
                    online: false,
                    models: vec![],
                },
            ],
            last_scan: Some(std::time::Instant::now()),
        };

        let models = state.all_models();
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "llama3:latest"));
        assert!(models.iter().any(|m| m.id == "qwen2.5-coder-7b"));

        // Offline provider should not contribute models
        assert!(state.any_online());
        let status = state.provider_status();
        assert_eq!(status.len(), 3);
    }

    #[tokio::test]
    async fn test_non_localhost_rejected() {
        let result = probe(
            &reqwest::Client::new(),
            "http://192.168.1.100:11434",
            ProviderType::Ollama,
            "Remote",
            ProbeProtocol::Ollama,
        )
        .await;
        assert!(result.is_none());
    }
}
