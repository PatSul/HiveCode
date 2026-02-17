use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

// ── Types ──────────────────────────────────────────────────────────

/// A Docker container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub ports: Vec<PortMapping>,
    pub created_at: String,
}

/// A port mapping between host and container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

/// A Docker image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerImage {
    pub id: String,
    pub tags: Vec<String>,
    pub size_bytes: u64,
    pub created_at: String,
}

/// Request to run a new Docker container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunContainerRequest {
    pub image: String,
    pub name: Option<String>,
    pub ports: Vec<(u16, u16)>,
    pub env_vars: Vec<(String, String)>,
    pub volumes: Vec<(String, String)>,
    pub network: Option<String>,
    pub command: Option<Vec<String>>,
}

/// A Docker network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

/// A Docker volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
}

/// Docker system information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerInfo {
    pub containers_running: u64,
    pub containers_stopped: u64,
    pub images_count: u64,
    pub server_version: String,
}

// ── DockerClient ───────────────────────────────────────────────────

/// Client for interacting with Docker via the `docker` CLI.
///
/// All operations shell out to the `docker` command using
/// `tokio::process::Command`. This is more portable than using the
/// Docker Engine API directly and works on all platforms where Docker
/// Desktop or the Docker CLI is installed.
pub struct DockerClient {
    /// Path to the docker executable.
    docker_path: String,
}

impl DockerClient {
    /// Create a new client using the default `docker` command.
    pub fn new() -> Self {
        Self {
            docker_path: "docker".to_string(),
        }
    }

    /// Create a new client with a custom path to the docker executable.
    pub fn with_docker_path(path: impl Into<String>) -> Self {
        Self {
            docker_path: path.into(),
        }
    }

    /// Return the configured docker executable path.
    pub fn docker_path(&self) -> &str {
        &self.docker_path
    }

    // ── Container operations ──────────────────────────────────────

    /// List all containers. If `all` is true, include stopped containers.
    pub async fn list_containers(&self, all: bool) -> Result<Vec<Container>> {
        let mut args = vec!["ps", "--format", "{{json .}}", "--no-trunc"];
        if all {
            args.push("-a");
        }

        debug!(all = all, "listing docker containers");
        let output = self.run_docker(&args).await?;

        let mut containers = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(val) => containers.push(parse_container_ps(&val)),
                Err(e) => {
                    tracing::warn!(line = %line, error = %e, "failed to parse container JSON line");
                }
            }
        }

        Ok(containers)
    }

    /// Get details of a specific container by ID or name.
    pub async fn get_container(&self, id: &str) -> Result<Container> {
        debug!(id = %id, "inspecting docker container");
        let output = self.run_docker(&["inspect", id, "--format", "{{json .}}"]).await?;

        let val: Value = serde_json::from_str(output.trim())
            .context("failed to parse docker inspect output")?;

        Ok(parse_container_inspect(&val))
    }

    /// Start a stopped container.
    pub async fn start_container(&self, id: &str) -> Result<()> {
        debug!(id = %id, "starting docker container");
        self.run_docker(&["start", id]).await?;
        Ok(())
    }

    /// Stop a running container.
    pub async fn stop_container(&self, id: &str) -> Result<()> {
        debug!(id = %id, "stopping docker container");
        self.run_docker(&["stop", id]).await?;
        Ok(())
    }

    /// Restart a container.
    pub async fn restart_container(&self, id: &str) -> Result<()> {
        debug!(id = %id, "restarting docker container");
        self.run_docker(&["restart", id]).await?;
        Ok(())
    }

    /// Get container logs, optionally tailing the last N lines.
    pub async fn container_logs(&self, id: &str, tail_lines: Option<u32>) -> Result<String> {
        let tail = tail_lines
            .map(|n| n.to_string())
            .unwrap_or_else(|| "100".to_string());
        debug!(id = %id, tail = %tail, "fetching docker container logs");
        self.run_docker(&["logs", "--tail", &tail, id]).await
    }

    /// Run a new container from the given request configuration.
    pub async fn run_container(&self, request: &RunContainerRequest) -> Result<Container> {
        let mut args = vec!["run".to_string(), "-d".to_string()];

        if let Some(ref name) = request.name {
            args.push("--name".to_string());
            args.push(name.clone());
        }

        for (host, container) in &request.ports {
            args.push("-p".to_string());
            args.push(format!("{}:{}", host, container));
        }

        for (key, val) in &request.env_vars {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, val));
        }

        for (host_path, container_path) in &request.volumes {
            args.push("-v".to_string());
            args.push(format!("{}:{}", host_path, container_path));
        }

        if let Some(ref network) = request.network {
            args.push("--network".to_string());
            args.push(network.clone());
        }

        args.push(request.image.clone());

        if let Some(ref cmd) = request.command {
            args.extend(cmd.clone());
        }

        debug!(image = %request.image, "running docker container");
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.run_docker(&args_refs).await?;

        let container_id = output.trim().to_string();
        self.get_container(&container_id).await
    }

    // ── Image operations ──────────────────────────────────────────

    /// List all Docker images.
    pub async fn list_images(&self) -> Result<Vec<DockerImage>> {
        debug!("listing docker images");
        let output = self
            .run_docker(&["images", "--format", "{{json .}}", "--no-trunc"])
            .await?;

        let mut images = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(val) => images.push(parse_image(&val)),
                Err(e) => {
                    tracing::warn!(line = %line, error = %e, "failed to parse image JSON line");
                }
            }
        }

        Ok(images)
    }

    /// Build a Docker image from a Dockerfile.
    pub async fn build_image(&self, dockerfile_path: &str, tag: &str) -> Result<String> {
        debug!(path = %dockerfile_path, tag = %tag, "building docker image");
        self.run_docker(&["build", "-t", tag, "-f", dockerfile_path, "."]).await
    }

    // ── Network operations ────────────────────────────────────────

    /// List all Docker networks.
    pub async fn list_networks(&self) -> Result<Vec<Network>> {
        debug!("listing docker networks");
        let output = self
            .run_docker(&["network", "ls", "--format", "{{json .}}", "--no-trunc"])
            .await?;

        let mut networks = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(val) => {
                    networks.push(Network {
                        id: val["ID"].as_str().unwrap_or("").to_string(),
                        name: val["Name"].as_str().unwrap_or("").to_string(),
                        driver: val["Driver"].as_str().unwrap_or("").to_string(),
                        scope: val["Scope"].as_str().unwrap_or("").to_string(),
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse network JSON line");
                }
            }
        }

        Ok(networks)
    }

    // ── Volume operations ─────────────────────────────────────────

    /// List all Docker volumes.
    pub async fn list_volumes(&self) -> Result<Vec<Volume>> {
        debug!("listing docker volumes");
        let output = self
            .run_docker(&["volume", "ls", "--format", "{{json .}}"])
            .await?;

        let mut volumes = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(val) => {
                    volumes.push(Volume {
                        name: val["Name"].as_str().unwrap_or("").to_string(),
                        driver: val["Driver"].as_str().unwrap_or("").to_string(),
                        mountpoint: val["Mountpoint"].as_str().unwrap_or("").to_string(),
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse volume JSON line");
                }
            }
        }

        Ok(volumes)
    }

    // ── Docker Compose ────────────────────────────────────────────

    /// Start services defined in a Docker Compose file.
    pub async fn compose_up(&self, compose_file: &str) -> Result<String> {
        debug!(file = %compose_file, "docker compose up");
        self.run_docker(&["compose", "-f", compose_file, "up", "-d"]).await
    }

    /// Stop and remove services defined in a Docker Compose file.
    pub async fn compose_down(&self, compose_file: &str) -> Result<String> {
        debug!(file = %compose_file, "docker compose down");
        self.run_docker(&["compose", "-f", compose_file, "down"]).await
    }

    // ── System information ────────────────────────────────────────

    /// Get Docker system information.
    pub async fn get_system_info(&self) -> Result<DockerInfo> {
        debug!("getting docker system info");
        let output = self.run_docker(&["info", "--format", "{{json .}}"]).await?;

        let val: Value = serde_json::from_str(output.trim())
            .context("failed to parse docker info output")?;

        Ok(DockerInfo {
            containers_running: val["ContainersRunning"].as_u64().unwrap_or(0),
            containers_stopped: val["ContainersStopped"].as_u64().unwrap_or(0),
            images_count: val["Images"].as_u64().unwrap_or(0),
            server_version: val["ServerVersion"].as_str().unwrap_or("unknown").to_string(),
        })
    }

    // ── Internal helpers ──────────────────────────────────────────

    /// Execute a docker CLI command and return its stdout.
    async fn run_docker(&self, args: &[&str]) -> Result<String> {
        let output = tokio::process::Command::new(&self.docker_path)
            .args(args)
            .output()
            .await
            .with_context(|| {
                format!(
                    "failed to execute: {} {}",
                    self.docker_path,
                    args.join(" ")
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "docker command failed ({}): {}",
                output.status,
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }
}

impl Default for DockerClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── JSON parsing helpers ───────────────────────────────────────────

/// Parse a container from `docker ps --format '{{json .}}'` output.
fn parse_container_ps(val: &Value) -> Container {
    let ports_str = val["Ports"].as_str().unwrap_or("");
    let ports = parse_port_mappings(ports_str);

    Container {
        id: val["ID"].as_str().unwrap_or("").to_string(),
        name: val["Names"].as_str().unwrap_or("").to_string(),
        image: val["Image"].as_str().unwrap_or("").to_string(),
        status: val["Status"].as_str().unwrap_or("").to_string(),
        state: val["State"].as_str().unwrap_or("").to_string(),
        ports,
        created_at: val["CreatedAt"].as_str().unwrap_or("").to_string(),
    }
}

/// Parse a container from `docker inspect` output.
fn parse_container_inspect(val: &Value) -> Container {
    let name = val["Name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('/')
        .to_string();

    let image = val["Config"]["Image"].as_str().unwrap_or("").to_string();
    let state = val["State"]["Status"].as_str().unwrap_or("").to_string();
    let created = val["Created"].as_str().unwrap_or("").to_string();
    let id = val["Id"].as_str().unwrap_or("").to_string();

    // Parse port bindings from inspect output.
    let mut ports = Vec::new();
    if let Some(port_bindings) = val["NetworkSettings"]["Ports"].as_object() {
        for (container_port_proto, bindings) in port_bindings {
            let (container_port, protocol) = parse_port_protocol(container_port_proto);
            if let Some(bindings) = bindings.as_array() {
                for binding in bindings {
                    let host_port = binding["HostPort"]
                        .as_str()
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(0);
                    ports.push(PortMapping {
                        host_port,
                        container_port,
                        protocol: protocol.clone(),
                    });
                }
            }
        }
    }

    let status = match state.as_str() {
        "running" => {
            let started = val["State"]["StartedAt"].as_str().unwrap_or("");
            format!("Up since {}", started)
        }
        other => other.to_string(),
    };

    Container {
        id,
        name,
        image,
        status,
        state,
        ports,
        created_at: created,
    }
}

/// Parse port mappings from the docker ps Ports column.
///
/// Example: "0.0.0.0:8080->80/tcp, 0.0.0.0:8443->443/tcp"
fn parse_port_mappings(ports_str: &str) -> Vec<PortMapping> {
    let mut mappings = Vec::new();

    for segment in ports_str.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        // Pattern: "host_ip:host_port->container_port/protocol"
        if let Some(arrow_pos) = segment.find("->") {
            let host_part = &segment[..arrow_pos];
            let container_part = &segment[arrow_pos + 2..];

            let host_port = host_part
                .rsplit(':')
                .next()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(0);

            let (container_port, protocol) = parse_port_protocol(container_part);

            mappings.push(PortMapping {
                host_port,
                container_port,
                protocol,
            });
        }
    }

    mappings
}

/// Parse "80/tcp" into (80, "tcp").
fn parse_port_protocol(s: &str) -> (u16, String) {
    let parts: Vec<&str> = s.split('/').collect();
    let port = parts
        .first()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(0);
    let protocol = parts.get(1).unwrap_or(&"tcp").to_string();
    (port, protocol)
}

/// Parse an image from `docker images --format '{{json .}}'` output.
fn parse_image(val: &Value) -> DockerImage {
    let repo = val["Repository"].as_str().unwrap_or("<none>");
    let tag = val["Tag"].as_str().unwrap_or("<none>");
    let full_tag = if repo == "<none>" {
        vec![]
    } else if tag == "<none>" {
        vec![repo.to_string()]
    } else {
        vec![format!("{}:{}", repo, tag)]
    };

    let size_str = val["Size"].as_str().unwrap_or("0");
    let size_bytes = parse_human_size(size_str);

    DockerImage {
        id: val["ID"].as_str().unwrap_or("").to_string(),
        tags: full_tag,
        size_bytes,
        created_at: val["CreatedAt"]
            .as_str()
            .or_else(|| val["CreatedSince"].as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// Parse a human-readable size like "150MB" or "1.2GB" into bytes.
fn parse_human_size(s: &str) -> u64 {
    let s = s.trim();
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("GB") {
        (n, 1_000_000_000u64)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1_000_000u64)
    } else if let Some(n) = s.strip_suffix("KB") {
        (n, 1_000u64)
    } else if let Some(n) = s.strip_suffix("kB") {
        (n, 1_000u64)
    } else if let Some(n) = s.strip_suffix('B') {
        (n, 1u64)
    } else {
        (s, 1u64)
    };

    num_str
        .trim()
        .parse::<f64>()
        .map(|n| (n * multiplier as f64) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Client construction ───────────────────────────────────────

    #[test]
    fn test_default_docker_path() {
        let client = DockerClient::new();
        assert_eq!(client.docker_path(), "docker");
    }

    #[test]
    fn test_custom_docker_path() {
        let client = DockerClient::with_docker_path("/usr/local/bin/docker");
        assert_eq!(client.docker_path(), "/usr/local/bin/docker");
    }

    #[test]
    fn test_default_trait() {
        let client = DockerClient::default();
        assert_eq!(client.docker_path(), "docker");
    }

    // ── Port parsing ──────────────────────────────────────────────

    #[test]
    fn test_parse_port_mappings_single() {
        let ports = parse_port_mappings("0.0.0.0:8080->80/tcp");
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].host_port, 8080);
        assert_eq!(ports[0].container_port, 80);
        assert_eq!(ports[0].protocol, "tcp");
    }

    #[test]
    fn test_parse_port_mappings_multiple() {
        let ports = parse_port_mappings("0.0.0.0:8080->80/tcp, 0.0.0.0:8443->443/tcp");
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].host_port, 8080);
        assert_eq!(ports[0].container_port, 80);
        assert_eq!(ports[1].host_port, 8443);
        assert_eq!(ports[1].container_port, 443);
    }

    #[test]
    fn test_parse_port_mappings_empty() {
        let ports = parse_port_mappings("");
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_port_protocol() {
        let (port, proto) = parse_port_protocol("80/tcp");
        assert_eq!(port, 80);
        assert_eq!(proto, "tcp");

        let (port, proto) = parse_port_protocol("53/udp");
        assert_eq!(port, 53);
        assert_eq!(proto, "udp");
    }

    // ── Size parsing ──────────────────────────────────────────────

    #[test]
    fn test_parse_human_size_mb() {
        assert_eq!(parse_human_size("150MB"), 150_000_000);
    }

    #[test]
    fn test_parse_human_size_gb() {
        assert_eq!(parse_human_size("1.5GB"), 1_500_000_000);
    }

    #[test]
    fn test_parse_human_size_kb() {
        assert_eq!(parse_human_size("500KB"), 500_000);
        assert_eq!(parse_human_size("500kB"), 500_000);
    }

    #[test]
    fn test_parse_human_size_bytes() {
        assert_eq!(parse_human_size("1024B"), 1024);
    }

    #[test]
    fn test_parse_human_size_plain_number() {
        assert_eq!(parse_human_size("12345"), 12345);
    }

    #[test]
    fn test_parse_human_size_invalid() {
        assert_eq!(parse_human_size("not_a_number"), 0);
    }

    // ── Container PS parsing ──────────────────────────────────────

    #[test]
    fn test_parse_container_ps() {
        let val = serde_json::json!({
            "ID": "abc123def456",
            "Names": "my-web-app",
            "Image": "nginx:latest",
            "Status": "Up 3 hours",
            "State": "running",
            "Ports": "0.0.0.0:8080->80/tcp",
            "CreatedAt": "2024-01-15 10:30:00"
        });

        let container = parse_container_ps(&val);
        assert_eq!(container.id, "abc123def456");
        assert_eq!(container.name, "my-web-app");
        assert_eq!(container.image, "nginx:latest");
        assert_eq!(container.state, "running");
        assert_eq!(container.ports.len(), 1);
        assert_eq!(container.ports[0].host_port, 8080);
        assert_eq!(container.ports[0].container_port, 80);
    }

    #[test]
    fn test_parse_container_ps_no_ports() {
        let val = serde_json::json!({
            "ID": "xyz789",
            "Names": "worker",
            "Image": "python:3.11",
            "Status": "Exited (0) 1 hour ago",
            "State": "exited",
            "Ports": "",
            "CreatedAt": "2024-01-15 09:00:00"
        });

        let container = parse_container_ps(&val);
        assert_eq!(container.state, "exited");
        assert!(container.ports.is_empty());
    }

    // ── Container inspect parsing ─────────────────────────────────

    #[test]
    fn test_parse_container_inspect() {
        let val = serde_json::json!({
            "Id": "abc123def456789",
            "Name": "/my-app",
            "Config": {
                "Image": "myapp:v2"
            },
            "State": {
                "Status": "running",
                "StartedAt": "2024-01-15T10:30:00Z"
            },
            "Created": "2024-01-15T10:29:00Z",
            "NetworkSettings": {
                "Ports": {
                    "80/tcp": [
                        { "HostIp": "0.0.0.0", "HostPort": "8080" }
                    ]
                }
            }
        });

        let container = parse_container_inspect(&val);
        assert_eq!(container.id, "abc123def456789");
        assert_eq!(container.name, "my-app");
        assert_eq!(container.image, "myapp:v2");
        assert_eq!(container.state, "running");
        assert_eq!(container.ports.len(), 1);
        assert_eq!(container.ports[0].host_port, 8080);
        assert_eq!(container.ports[0].container_port, 80);
        assert!(container.status.contains("Up since"));
    }

    #[test]
    fn test_parse_container_inspect_stopped() {
        let val = serde_json::json!({
            "Id": "stopped123",
            "Name": "/stopped-app",
            "Config": { "Image": "app:v1" },
            "State": { "Status": "exited" },
            "Created": "2024-01-10T08:00:00Z",
            "NetworkSettings": { "Ports": {} }
        });

        let container = parse_container_inspect(&val);
        assert_eq!(container.state, "exited");
        assert_eq!(container.status, "exited");
        assert!(container.ports.is_empty());
    }

    // ── Image parsing ─────────────────────────────────────────────

    #[test]
    fn test_parse_image() {
        let val = serde_json::json!({
            "ID": "sha256:abc123",
            "Repository": "nginx",
            "Tag": "latest",
            "Size": "150MB",
            "CreatedAt": "2024-01-15 10:00:00"
        });

        let image = parse_image(&val);
        assert_eq!(image.id, "sha256:abc123");
        assert_eq!(image.tags, vec!["nginx:latest"]);
        assert_eq!(image.size_bytes, 150_000_000);
    }

    #[test]
    fn test_parse_image_no_tag() {
        let val = serde_json::json!({
            "ID": "sha256:def456",
            "Repository": "myapp",
            "Tag": "<none>",
            "Size": "200MB",
            "CreatedSince": "2 weeks ago"
        });

        let image = parse_image(&val);
        assert_eq!(image.tags, vec!["myapp"]);
    }

    #[test]
    fn test_parse_image_none_repo() {
        let val = serde_json::json!({
            "ID": "sha256:orphan",
            "Repository": "<none>",
            "Tag": "<none>",
            "Size": "50MB"
        });

        let image = parse_image(&val);
        assert!(image.tags.is_empty());
    }

    // ── Type serialization ────────────────────────────────────────

    #[test]
    fn test_container_serialization() {
        let container = Container {
            id: "abc123".to_string(),
            name: "web".to_string(),
            image: "nginx:latest".to_string(),
            status: "Up 2 hours".to_string(),
            state: "running".to_string(),
            ports: vec![PortMapping {
                host_port: 8080,
                container_port: 80,
                protocol: "tcp".to_string(),
            }],
            created_at: "2024-01-15".to_string(),
        };

        let json = serde_json::to_string(&container).unwrap();
        let restored: Container = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, "abc123");
        assert_eq!(restored.name, "web");
        assert_eq!(restored.ports.len(), 1);
    }

    #[test]
    fn test_docker_info_serialization() {
        let info = DockerInfo {
            containers_running: 5,
            containers_stopped: 3,
            images_count: 12,
            server_version: "24.0.7".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let restored: DockerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.containers_running, 5);
        assert_eq!(restored.server_version, "24.0.7");
    }

    #[test]
    fn test_run_container_request_serialization() {
        let request = RunContainerRequest {
            image: "nginx:latest".to_string(),
            name: Some("web".to_string()),
            ports: vec![(8080, 80)],
            env_vars: vec![("ENV".to_string(), "prod".to_string())],
            volumes: vec![("/data".to_string(), "/app/data".to_string())],
            network: Some("my-net".to_string()),
            command: Some(vec!["nginx".to_string(), "-g".to_string(), "daemon off;".to_string()]),
        };

        let json = serde_json::to_string(&request).unwrap();
        let restored: RunContainerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.image, "nginx:latest");
        assert_eq!(restored.name, Some("web".to_string()));
        assert_eq!(restored.ports.len(), 1);
        assert_eq!(restored.env_vars.len(), 1);
    }

    #[test]
    fn test_network_serialization() {
        let net = Network {
            id: "net123".to_string(),
            name: "bridge".to_string(),
            driver: "bridge".to_string(),
            scope: "local".to_string(),
        };

        let json = serde_json::to_string(&net).unwrap();
        let restored: Network = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "bridge");
        assert_eq!(restored.driver, "bridge");
    }

    #[test]
    fn test_volume_serialization() {
        let vol = Volume {
            name: "my-data".to_string(),
            driver: "local".to_string(),
            mountpoint: "/var/lib/docker/volumes/my-data/_data".to_string(),
        };

        let json = serde_json::to_string(&vol).unwrap();
        let restored: Volume = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "my-data");
        assert_eq!(restored.driver, "local");
    }
}
