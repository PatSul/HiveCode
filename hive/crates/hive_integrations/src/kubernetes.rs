use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

// ── Types ──────────────────────────────────────────────────────────

/// A Kubernetes pod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pod {
    pub name: String,
    pub namespace: String,
    pub status: String,
    pub ready: String,
    pub restarts: u32,
    pub age: String,
    pub node: String,
    pub ip: String,
}

/// A Kubernetes deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub name: String,
    pub namespace: String,
    pub ready_replicas: u32,
    pub desired_replicas: u32,
    pub updated_replicas: u32,
    pub available: bool,
}

/// A Kubernetes service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sService {
    pub name: String,
    pub namespace: String,
    pub service_type: String,
    pub cluster_ip: String,
    pub external_ip: String,
    pub ports: Vec<String>,
}

/// A Kubernetes namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub name: String,
    pub status: String,
    pub age: String,
}

/// A Kubernetes event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sEvent {
    pub kind: String,
    pub reason: String,
    pub message: String,
    pub first_seen: String,
    pub last_seen: String,
    pub count: u32,
}

/// A Kubernetes context from kubeconfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sContext {
    pub name: String,
    pub cluster: String,
    pub user: String,
    pub namespace: String,
    pub current: bool,
}

/// Kubernetes cluster information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub server_url: String,
    pub kubernetes_version: String,
    pub node_count: u32,
}

// ── KubernetesClient ───────────────────────────────────────────────

/// Client for interacting with Kubernetes via the `kubectl` CLI.
///
/// All operations shell out to `kubectl` using `tokio::process::Command`
/// with `-o json` output for structured parsing. Supports multiple
/// contexts and namespaces.
pub struct KubernetesClient {
    /// Kubernetes context to use. If `None`, uses the current context.
    context: Option<String>,
    /// Default namespace. If `None`, uses "default" or the context namespace.
    namespace: Option<String>,
    /// Path to the kubectl executable.
    kubectl_path: String,
}

impl KubernetesClient {
    /// Create a new client using the current kubectl context and default namespace.
    pub fn new() -> Self {
        Self {
            context: None,
            namespace: None,
            kubectl_path: "kubectl".to_string(),
        }
    }

    /// Create a new client with a specific context and namespace.
    pub fn with_config(context: Option<String>, namespace: Option<String>) -> Self {
        Self {
            context,
            namespace,
            kubectl_path: "kubectl".to_string(),
        }
    }

    /// Create a new client with a custom kubectl path.
    pub fn with_kubectl_path(path: impl Into<String>) -> Self {
        Self {
            context: None,
            namespace: None,
            kubectl_path: path.into(),
        }
    }

    /// Return the configured kubectl executable path.
    pub fn kubectl_path(&self) -> &str {
        &self.kubectl_path
    }

    /// Return the configured context, if any.
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    /// Return the configured namespace, if any.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    // ── Pod operations ────────────────────────────────────────────

    /// List pods in a namespace.
    pub async fn list_pods(&self, namespace: Option<&str>) -> Result<Vec<Pod>> {
        let ns = self.resolve_namespace(namespace);
        debug!(namespace = %ns, "listing pods");

        let output = self
            .run_kubectl(&["get", "pods", "-n", &ns, "-o", "json"])
            .await?;

        let val: Value = serde_json::from_str(&output)
            .context("failed to parse kubectl pods output")?;

        let items = val["items"].as_array().cloned().unwrap_or_default();
        let pods = items.iter().map(|item| parse_pod(item)).collect();
        Ok(pods)
    }

    /// Get details of a specific pod.
    pub async fn get_pod(&self, name: &str, namespace: Option<&str>) -> Result<Pod> {
        let ns = self.resolve_namespace(namespace);
        debug!(name = %name, namespace = %ns, "getting pod");

        let output = self
            .run_kubectl(&["get", "pod", name, "-n", &ns, "-o", "json"])
            .await?;

        let val: Value = serde_json::from_str(&output)
            .context("failed to parse kubectl pod output")?;

        Ok(parse_pod(&val))
    }

    /// Get logs from a pod.
    pub async fn get_pod_logs(
        &self,
        name: &str,
        namespace: Option<&str>,
        tail_lines: Option<u32>,
    ) -> Result<String> {
        let ns = self.resolve_namespace(namespace);
        let tail = tail_lines
            .map(|n| n.to_string())
            .unwrap_or_else(|| "100".to_string());

        debug!(name = %name, namespace = %ns, tail = %tail, "getting pod logs");

        self.run_kubectl(&["logs", name, "-n", &ns, "--tail", &tail])
            .await
    }

    // ── Deployment operations ─────────────────────────────────────

    /// List deployments in a namespace.
    pub async fn list_deployments(&self, namespace: Option<&str>) -> Result<Vec<Deployment>> {
        let ns = self.resolve_namespace(namespace);
        debug!(namespace = %ns, "listing deployments");

        let output = self
            .run_kubectl(&["get", "deployments", "-n", &ns, "-o", "json"])
            .await?;

        let val: Value = serde_json::from_str(&output)
            .context("failed to parse kubectl deployments output")?;

        let items = val["items"].as_array().cloned().unwrap_or_default();
        let deployments = items.iter().map(|item| parse_deployment(item)).collect();
        Ok(deployments)
    }

    /// Scale a deployment to a specific number of replicas.
    pub async fn scale_deployment(
        &self,
        name: &str,
        namespace: Option<&str>,
        replicas: u32,
    ) -> Result<()> {
        let ns = self.resolve_namespace(namespace);
        let replicas_arg = format!("--replicas={}", replicas);
        debug!(name = %name, namespace = %ns, replicas = replicas, "scaling deployment");

        self.run_kubectl(&["scale", "deployment", name, "-n", &ns, &replicas_arg])
            .await?;
        Ok(())
    }

    // ── Service operations ────────────────────────────────────────

    /// List services in a namespace.
    pub async fn list_services(&self, namespace: Option<&str>) -> Result<Vec<K8sService>> {
        let ns = self.resolve_namespace(namespace);
        debug!(namespace = %ns, "listing services");

        let output = self
            .run_kubectl(&["get", "services", "-n", &ns, "-o", "json"])
            .await?;

        let val: Value = serde_json::from_str(&output)
            .context("failed to parse kubectl services output")?;

        let items = val["items"].as_array().cloned().unwrap_or_default();
        let services = items.iter().map(|item| parse_service(item)).collect();
        Ok(services)
    }

    // ── Namespace operations ──────────────────────────────────────

    /// List all namespaces.
    pub async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        debug!("listing namespaces");

        let output = self
            .run_kubectl(&["get", "namespaces", "-o", "json"])
            .await?;

        let val: Value = serde_json::from_str(&output)
            .context("failed to parse kubectl namespaces output")?;

        let items = val["items"].as_array().cloned().unwrap_or_default();
        let namespaces = items.iter().map(|item| parse_namespace(item)).collect();
        Ok(namespaces)
    }

    // ── Resource management ───────────────────────────────────────

    /// Apply a Kubernetes manifest (YAML content).
    pub async fn apply_manifest(&self, yaml_content: &str) -> Result<String> {
        debug!("applying kubernetes manifest");

        let mut child = tokio::process::Command::new(&self.kubectl_path)
            .args(self.base_args())
            .args(["apply", "-f", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("failed to spawn kubectl apply")?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(yaml_content.as_bytes())
                .await
                .context("failed to write manifest to kubectl stdin")?;
            // Drop stdin to close the pipe and signal EOF.
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .context("failed to wait for kubectl apply")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("kubectl apply failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Delete a Kubernetes resource by kind and name.
    pub async fn delete_resource(
        &self,
        kind: &str,
        name: &str,
        namespace: Option<&str>,
    ) -> Result<String> {
        let ns = self.resolve_namespace(namespace);
        debug!(kind = %kind, name = %name, namespace = %ns, "deleting resource");

        self.run_kubectl(&["delete", kind, name, "-n", &ns]).await
    }

    // ── Events ────────────────────────────────────────────────────

    /// Get events in a namespace.
    pub async fn get_events(&self, namespace: Option<&str>) -> Result<Vec<K8sEvent>> {
        let ns = self.resolve_namespace(namespace);
        debug!(namespace = %ns, "getting events");

        let output = self
            .run_kubectl(&["get", "events", "-n", &ns, "-o", "json"])
            .await?;

        let val: Value = serde_json::from_str(&output)
            .context("failed to parse kubectl events output")?;

        let items = val["items"].as_array().cloned().unwrap_or_default();
        let events = items.iter().map(|item| parse_event(item)).collect();
        Ok(events)
    }

    // ── Context management ────────────────────────────────────────

    /// List all available kubectl contexts.
    pub async fn list_contexts(&self) -> Result<Vec<K8sContext>> {
        debug!("listing kubectl contexts");

        let output = self
            .run_kubectl(&["config", "get-contexts", "-o", "name"])
            .await?;

        let current_ctx = self.get_current_context().await.unwrap_or_default();

        // Get detailed context info.
        let config_output = self
            .run_kubectl(&["config", "view", "-o", "json"])
            .await?;

        let config: Value = serde_json::from_str(&config_output)
            .context("failed to parse kubectl config")?;

        let mut contexts = Vec::new();
        if let Some(ctx_list) = config["contexts"].as_array() {
            for ctx in ctx_list {
                let name = ctx["name"].as_str().unwrap_or("").to_string();
                let context_data = &ctx["context"];
                contexts.push(K8sContext {
                    current: name == current_ctx,
                    cluster: context_data["cluster"].as_str().unwrap_or("").to_string(),
                    user: context_data["user"].as_str().unwrap_or("").to_string(),
                    namespace: context_data["namespace"]
                        .as_str()
                        .unwrap_or("default")
                        .to_string(),
                    name,
                });
            }
        }

        // Drop the unused `output` variable's context names silently.
        let _ = output;

        Ok(contexts)
    }

    /// Switch to a different kubectl context.
    pub async fn use_context(&mut self, name: &str) -> Result<()> {
        debug!(name = %name, "switching kubectl context");
        self.run_kubectl(&["config", "use-context", name]).await?;
        self.context = Some(name.to_string());
        Ok(())
    }

    /// Get cluster information.
    pub async fn get_cluster_info(&self) -> Result<ClusterInfo> {
        debug!("getting cluster info");

        // Get server URL and version from kubectl.
        let version_output = self.run_kubectl(&["version", "-o", "json"]).await?;
        let version_val: Value = serde_json::from_str(&version_output)
            .context("failed to parse kubectl version output")?;

        let server_version = format!(
            "v{}.{}",
            version_val["serverVersion"]["major"]
                .as_str()
                .unwrap_or("?"),
            version_val["serverVersion"]["minor"]
                .as_str()
                .unwrap_or("?")
        );

        // Get node count.
        let nodes_output = self
            .run_kubectl(&["get", "nodes", "-o", "json"])
            .await?;
        let nodes_val: Value = serde_json::from_str(&nodes_output)
            .context("failed to parse kubectl nodes output")?;
        let node_count = nodes_val["items"]
            .as_array()
            .map(|a| a.len() as u32)
            .unwrap_or(0);

        // Get server URL from cluster info.
        let config_output = self
            .run_kubectl(&["config", "view", "-o", "json"])
            .await?;
        let config_val: Value = serde_json::from_str(&config_output)
            .context("failed to parse kubectl config")?;

        let server_url = config_val["clusters"]
            .as_array()
            .and_then(|clusters| clusters.first())
            .and_then(|c| c["cluster"]["server"].as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ClusterInfo {
            server_url,
            kubernetes_version: server_version,
            node_count,
        })
    }

    // ── Internal helpers ──────────────────────────────────────────

    /// Get the current kubectl context name.
    async fn get_current_context(&self) -> Result<String> {
        let output = self
            .run_kubectl(&["config", "current-context"])
            .await?;
        Ok(output.trim().to_string())
    }

    /// Resolve the namespace to use: explicit parameter > configured > "default".
    fn resolve_namespace(&self, namespace: Option<&str>) -> String {
        namespace
            .map(|s| s.to_string())
            .or_else(|| self.namespace.clone())
            .unwrap_or_else(|| "default".to_string())
    }

    /// Build base arguments for kubectl (context flag, etc.).
    fn base_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(ref ctx) = self.context {
            args.push("--context".to_string());
            args.push(ctx.clone());
        }
        args
    }

    /// Execute a kubectl CLI command and return its stdout.
    async fn run_kubectl(&self, args: &[&str]) -> Result<String> {
        let mut all_args = self.base_args();
        all_args.extend(args.iter().map(|s| s.to_string()));

        let all_args_refs: Vec<&str> = all_args.iter().map(|s| s.as_str()).collect();

        let output = tokio::process::Command::new(&self.kubectl_path)
            .args(&all_args_refs)
            .output()
            .await
            .with_context(|| {
                format!(
                    "failed to execute: {} {}",
                    self.kubectl_path,
                    all_args.join(" ")
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "kubectl command failed ({}): {}",
                output.status,
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }
}

impl Default for KubernetesClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── JSON parsing helpers ───────────────────────────────────────────

/// Parse a pod from kubectl JSON output.
fn parse_pod(val: &Value) -> Pod {
    let metadata = &val["metadata"];
    let status = &val["status"];
    let spec = &val["spec"];

    let name = metadata["name"].as_str().unwrap_or("").to_string();
    let namespace = metadata["namespace"].as_str().unwrap_or("default").to_string();

    let phase = status["phase"].as_str().unwrap_or("Unknown").to_string();

    // Compute ready count like "1/1".
    let container_statuses = status["containerStatuses"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let total = container_statuses.len();
    let ready_count = container_statuses
        .iter()
        .filter(|cs| cs["ready"].as_bool().unwrap_or(false))
        .count();
    let ready = format!("{}/{}", ready_count, total);

    let restarts: u32 = container_statuses
        .iter()
        .map(|cs| cs["restartCount"].as_u64().unwrap_or(0) as u32)
        .sum();

    let age = compute_age(metadata["creationTimestamp"].as_str().unwrap_or(""));

    let node = spec["nodeName"].as_str().unwrap_or("").to_string();
    let ip = status["podIP"].as_str().unwrap_or("").to_string();

    Pod {
        name,
        namespace,
        status: phase,
        ready,
        restarts,
        age,
        node,
        ip,
    }
}

/// Parse a deployment from kubectl JSON output.
fn parse_deployment(val: &Value) -> Deployment {
    let metadata = &val["metadata"];
    let status = &val["status"];
    let spec = &val["spec"];

    let name = metadata["name"].as_str().unwrap_or("").to_string();
    let namespace = metadata["namespace"].as_str().unwrap_or("default").to_string();

    let desired = spec["replicas"].as_u64().unwrap_or(0) as u32;
    let ready = status["readyReplicas"].as_u64().unwrap_or(0) as u32;
    let updated = status["updatedReplicas"].as_u64().unwrap_or(0) as u32;
    let available_replicas = status["availableReplicas"].as_u64().unwrap_or(0) as u32;

    Deployment {
        name,
        namespace,
        ready_replicas: ready,
        desired_replicas: desired,
        updated_replicas: updated,
        available: available_replicas >= desired && desired > 0,
    }
}

/// Parse a service from kubectl JSON output.
fn parse_service(val: &Value) -> K8sService {
    let metadata = &val["metadata"];
    let spec = &val["spec"];

    let name = metadata["name"].as_str().unwrap_or("").to_string();
    let namespace = metadata["namespace"].as_str().unwrap_or("default").to_string();
    let service_type = spec["type"].as_str().unwrap_or("ClusterIP").to_string();
    let cluster_ip = spec["clusterIP"].as_str().unwrap_or("None").to_string();

    // External IP from status.
    let external_ip = val["status"]["loadBalancer"]["ingress"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|ing| {
            ing["ip"]
                .as_str()
                .or_else(|| ing["hostname"].as_str())
        })
        .unwrap_or("<none>")
        .to_string();

    let ports: Vec<String> = spec["ports"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|p| {
            let port = p["port"].as_u64().unwrap_or(0);
            let target = p["targetPort"].as_u64().unwrap_or(0);
            let protocol = p["protocol"].as_str().unwrap_or("TCP");
            let node_port = p["nodePort"].as_u64();
            if let Some(np) = node_port {
                format!("{}:{}/{}", port, np, protocol)
            } else {
                format!("{}:{}/{}", port, target, protocol)
            }
        })
        .collect();

    K8sService {
        name,
        namespace,
        service_type,
        cluster_ip,
        external_ip,
        ports,
    }
}

/// Parse a namespace from kubectl JSON output.
fn parse_namespace(val: &Value) -> Namespace {
    let metadata = &val["metadata"];
    let status = &val["status"];

    Namespace {
        name: metadata["name"].as_str().unwrap_or("").to_string(),
        status: status["phase"].as_str().unwrap_or("Active").to_string(),
        age: compute_age(metadata["creationTimestamp"].as_str().unwrap_or("")),
    }
}

/// Parse an event from kubectl JSON output.
fn parse_event(val: &Value) -> K8sEvent {
    K8sEvent {
        kind: val["involvedObject"]["kind"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        reason: val["reason"].as_str().unwrap_or("").to_string(),
        message: val["message"].as_str().unwrap_or("").to_string(),
        first_seen: val["firstTimestamp"]
            .as_str()
            .or_else(|| val["metadata"]["creationTimestamp"].as_str())
            .unwrap_or("")
            .to_string(),
        last_seen: val["lastTimestamp"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        count: val["count"].as_u64().unwrap_or(1) as u32,
    }
}

/// Compute a human-readable age string from an ISO 8601 timestamp.
fn compute_age(timestamp: &str) -> String {
    if timestamp.is_empty() {
        return "unknown".to_string();
    }

    let Ok(created) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_string();
    };

    let duration = chrono::Utc::now().signed_duration_since(created);
    let days = duration.num_days();
    let hours = duration.num_hours();
    let minutes = duration.num_minutes();

    if days > 0 {
        format!("{}d", days)
    } else if hours > 0 {
        format!("{}h", hours)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        "just now".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Client construction ───────────────────────────────────────

    #[test]
    fn test_default_client() {
        let client = KubernetesClient::new();
        assert_eq!(client.kubectl_path(), "kubectl");
        assert!(client.context().is_none());
        assert!(client.namespace().is_none());
    }

    #[test]
    fn test_client_with_config() {
        let client = KubernetesClient::with_config(
            Some("my-cluster".to_string()),
            Some("production".to_string()),
        );
        assert_eq!(client.context(), Some("my-cluster"));
        assert_eq!(client.namespace(), Some("production"));
    }

    #[test]
    fn test_client_with_kubectl_path() {
        let client = KubernetesClient::with_kubectl_path("/usr/local/bin/kubectl");
        assert_eq!(client.kubectl_path(), "/usr/local/bin/kubectl");
    }

    #[test]
    fn test_default_trait() {
        let client = KubernetesClient::default();
        assert_eq!(client.kubectl_path(), "kubectl");
    }

    // ── Namespace resolution ──────────────────────────────────────

    #[test]
    fn test_resolve_namespace_explicit() {
        let client = KubernetesClient::new();
        assert_eq!(client.resolve_namespace(Some("prod")), "prod");
    }

    #[test]
    fn test_resolve_namespace_configured() {
        let client = KubernetesClient::with_config(None, Some("staging".to_string()));
        assert_eq!(client.resolve_namespace(None), "staging");
    }

    #[test]
    fn test_resolve_namespace_default() {
        let client = KubernetesClient::new();
        assert_eq!(client.resolve_namespace(None), "default");
    }

    #[test]
    fn test_resolve_namespace_explicit_overrides_configured() {
        let client = KubernetesClient::with_config(None, Some("staging".to_string()));
        assert_eq!(client.resolve_namespace(Some("prod")), "prod");
    }

    // ── Base args ─────────────────────────────────────────────────

    #[test]
    fn test_base_args_no_context() {
        let client = KubernetesClient::new();
        assert!(client.base_args().is_empty());
    }

    #[test]
    fn test_base_args_with_context() {
        let client = KubernetesClient::with_config(Some("my-ctx".to_string()), None);
        let args = client.base_args();
        assert_eq!(args, vec!["--context", "my-ctx"]);
    }

    // ── Pod parsing ───────────────────────────────────────────────

    #[test]
    fn test_parse_pod() {
        let val = serde_json::json!({
            "metadata": {
                "name": "nginx-abc123",
                "namespace": "default",
                "creationTimestamp": "2024-01-15T10:30:00Z"
            },
            "spec": {
                "nodeName": "node-1"
            },
            "status": {
                "phase": "Running",
                "podIP": "10.0.1.5",
                "containerStatuses": [
                    {
                        "ready": true,
                        "restartCount": 2
                    }
                ]
            }
        });

        let pod = parse_pod(&val);
        assert_eq!(pod.name, "nginx-abc123");
        assert_eq!(pod.namespace, "default");
        assert_eq!(pod.status, "Running");
        assert_eq!(pod.ready, "1/1");
        assert_eq!(pod.restarts, 2);
        assert_eq!(pod.node, "node-1");
        assert_eq!(pod.ip, "10.0.1.5");
    }

    #[test]
    fn test_parse_pod_multi_container() {
        let val = serde_json::json!({
            "metadata": {
                "name": "multi-pod",
                "namespace": "apps",
                "creationTimestamp": ""
            },
            "spec": { "nodeName": "node-2" },
            "status": {
                "phase": "Running",
                "podIP": "10.0.2.3",
                "containerStatuses": [
                    { "ready": true, "restartCount": 0 },
                    { "ready": false, "restartCount": 3 }
                ]
            }
        });

        let pod = parse_pod(&val);
        assert_eq!(pod.ready, "1/2");
        assert_eq!(pod.restarts, 3);
    }

    #[test]
    fn test_parse_pod_no_containers() {
        let val = serde_json::json!({
            "metadata": {
                "name": "pending-pod",
                "namespace": "default",
                "creationTimestamp": ""
            },
            "spec": {},
            "status": {
                "phase": "Pending"
            }
        });

        let pod = parse_pod(&val);
        assert_eq!(pod.status, "Pending");
        assert_eq!(pod.ready, "0/0");
        assert_eq!(pod.restarts, 0);
    }

    // ── Deployment parsing ────────────────────────────────────────

    #[test]
    fn test_parse_deployment() {
        let val = serde_json::json!({
            "metadata": {
                "name": "web-app",
                "namespace": "production"
            },
            "spec": {
                "replicas": 3
            },
            "status": {
                "readyReplicas": 3,
                "updatedReplicas": 3,
                "availableReplicas": 3
            }
        });

        let dep = parse_deployment(&val);
        assert_eq!(dep.name, "web-app");
        assert_eq!(dep.namespace, "production");
        assert_eq!(dep.desired_replicas, 3);
        assert_eq!(dep.ready_replicas, 3);
        assert_eq!(dep.updated_replicas, 3);
        assert!(dep.available);
    }

    #[test]
    fn test_parse_deployment_not_ready() {
        let val = serde_json::json!({
            "metadata": {
                "name": "api",
                "namespace": "default"
            },
            "spec": { "replicas": 5 },
            "status": {
                "readyReplicas": 2,
                "updatedReplicas": 3,
                "availableReplicas": 2
            }
        });

        let dep = parse_deployment(&val);
        assert_eq!(dep.desired_replicas, 5);
        assert_eq!(dep.ready_replicas, 2);
        assert!(!dep.available);
    }

    #[test]
    fn test_parse_deployment_zero_replicas() {
        let val = serde_json::json!({
            "metadata": {
                "name": "scaled-down",
                "namespace": "default"
            },
            "spec": { "replicas": 0 },
            "status": {}
        });

        let dep = parse_deployment(&val);
        assert_eq!(dep.desired_replicas, 0);
        assert!(!dep.available); // 0 desired means not "available" by convention.
    }

    // ── Service parsing ───────────────────────────────────────────

    #[test]
    fn test_parse_service_clusterip() {
        let val = serde_json::json!({
            "metadata": {
                "name": "backend",
                "namespace": "default"
            },
            "spec": {
                "type": "ClusterIP",
                "clusterIP": "10.96.0.15",
                "ports": [
                    { "port": 80, "targetPort": 8080, "protocol": "TCP" }
                ]
            },
            "status": {}
        });

        let svc = parse_service(&val);
        assert_eq!(svc.name, "backend");
        assert_eq!(svc.service_type, "ClusterIP");
        assert_eq!(svc.cluster_ip, "10.96.0.15");
        assert_eq!(svc.external_ip, "<none>");
        assert_eq!(svc.ports, vec!["80:8080/TCP"]);
    }

    #[test]
    fn test_parse_service_loadbalancer() {
        let val = serde_json::json!({
            "metadata": {
                "name": "frontend",
                "namespace": "default"
            },
            "spec": {
                "type": "LoadBalancer",
                "clusterIP": "10.96.0.20",
                "ports": [
                    { "port": 443, "targetPort": 8443, "protocol": "TCP", "nodePort": 30443 }
                ]
            },
            "status": {
                "loadBalancer": {
                    "ingress": [
                        { "ip": "203.0.113.50" }
                    ]
                }
            }
        });

        let svc = parse_service(&val);
        assert_eq!(svc.service_type, "LoadBalancer");
        assert_eq!(svc.external_ip, "203.0.113.50");
        assert_eq!(svc.ports, vec!["443:30443/TCP"]);
    }

    // ── Namespace parsing ─────────────────────────────────────────

    #[test]
    fn test_parse_namespace() {
        let val = serde_json::json!({
            "metadata": {
                "name": "production",
                "creationTimestamp": "2024-01-01T00:00:00Z"
            },
            "status": {
                "phase": "Active"
            }
        });

        let ns = parse_namespace(&val);
        assert_eq!(ns.name, "production");
        assert_eq!(ns.status, "Active");
        assert!(!ns.age.is_empty());
    }

    // ── Event parsing ─────────────────────────────────────────────

    #[test]
    fn test_parse_event() {
        let val = serde_json::json!({
            "involvedObject": { "kind": "Pod" },
            "reason": "Pulled",
            "message": "Successfully pulled image nginx:latest",
            "firstTimestamp": "2024-01-15T10:30:00Z",
            "lastTimestamp": "2024-01-15T10:30:05Z",
            "count": 1
        });

        let event = parse_event(&val);
        assert_eq!(event.kind, "Pod");
        assert_eq!(event.reason, "Pulled");
        assert!(event.message.contains("nginx:latest"));
        assert_eq!(event.count, 1);
    }

    #[test]
    fn test_parse_event_repeated() {
        let val = serde_json::json!({
            "involvedObject": { "kind": "Deployment" },
            "reason": "ScalingReplicaSet",
            "message": "Scaled up replica set",
            "firstTimestamp": "2024-01-15T08:00:00Z",
            "lastTimestamp": "2024-01-15T12:00:00Z",
            "count": 5
        });

        let event = parse_event(&val);
        assert_eq!(event.count, 5);
        assert_eq!(event.kind, "Deployment");
    }

    // ── Age computation ───────────────────────────────────────────

    #[test]
    fn test_compute_age_empty() {
        assert_eq!(compute_age(""), "unknown");
    }

    #[test]
    fn test_compute_age_invalid() {
        assert_eq!(compute_age("not-a-date"), "not-a-date");
    }

    #[test]
    fn test_compute_age_recent() {
        // A timestamp from "now" should give a short age.
        let now = chrono::Utc::now().to_rfc3339();
        let age = compute_age(&now);
        // Should be "just now" or a very small number of minutes.
        assert!(age == "just now" || age.ends_with('m') || age.ends_with('h'));
    }

    // ── Type serialization ────────────────────────────────────────

    #[test]
    fn test_pod_serialization() {
        let pod = Pod {
            name: "web-abc".to_string(),
            namespace: "default".to_string(),
            status: "Running".to_string(),
            ready: "1/1".to_string(),
            restarts: 0,
            age: "5d".to_string(),
            node: "node-1".to_string(),
            ip: "10.0.0.5".to_string(),
        };

        let json = serde_json::to_string(&pod).unwrap();
        let restored: Pod = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "web-abc");
        assert_eq!(restored.status, "Running");
    }

    #[test]
    fn test_deployment_serialization() {
        let dep = Deployment {
            name: "api".to_string(),
            namespace: "prod".to_string(),
            ready_replicas: 3,
            desired_replicas: 3,
            updated_replicas: 3,
            available: true,
        };

        let json = serde_json::to_string(&dep).unwrap();
        let restored: Deployment = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "api");
        assert!(restored.available);
    }

    #[test]
    fn test_k8s_service_serialization() {
        let svc = K8sService {
            name: "frontend".to_string(),
            namespace: "default".to_string(),
            service_type: "LoadBalancer".to_string(),
            cluster_ip: "10.96.0.1".to_string(),
            external_ip: "203.0.113.10".to_string(),
            ports: vec!["80:30080/TCP".to_string()],
        };

        let json = serde_json::to_string(&svc).unwrap();
        let restored: K8sService = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.service_type, "LoadBalancer");
        assert_eq!(restored.external_ip, "203.0.113.10");
    }

    #[test]
    fn test_k8s_context_serialization() {
        let ctx = K8sContext {
            name: "prod-cluster".to_string(),
            cluster: "prod".to_string(),
            user: "admin".to_string(),
            namespace: "default".to_string(),
            current: true,
        };

        let json = serde_json::to_string(&ctx).unwrap();
        let restored: K8sContext = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "prod-cluster");
        assert!(restored.current);
    }

    #[test]
    fn test_cluster_info_serialization() {
        let info = ClusterInfo {
            server_url: "https://k8s.example.com:6443".to_string(),
            kubernetes_version: "v1.28.4".to_string(),
            node_count: 5,
        };

        let json = serde_json::to_string(&info).unwrap();
        let restored: ClusterInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.server_url, "https://k8s.example.com:6443");
        assert_eq!(restored.node_count, 5);
    }

    #[test]
    fn test_namespace_serialization() {
        let ns = Namespace {
            name: "kube-system".to_string(),
            status: "Active".to_string(),
            age: "30d".to_string(),
        };

        let json = serde_json::to_string(&ns).unwrap();
        let restored: Namespace = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "kube-system");
    }

    #[test]
    fn test_k8s_event_serialization() {
        let event = K8sEvent {
            kind: "Pod".to_string(),
            reason: "Scheduled".to_string(),
            message: "Successfully assigned pod".to_string(),
            first_seen: "2024-01-15T10:00:00Z".to_string(),
            last_seen: "2024-01-15T10:00:00Z".to_string(),
            count: 1,
        };

        let json = serde_json::to_string(&event).unwrap();
        let restored: K8sEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.reason, "Scheduled");
        assert_eq!(restored.count, 1);
    }
}
