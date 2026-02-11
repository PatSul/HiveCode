use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Status of a Docker container in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Removed,
}

/// Resource limits applied to a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in megabytes.
    pub memory_mb: Option<u64>,
    /// Number of CPU cores (fractional allowed).
    pub cpu_cores: Option<f64>,
    /// Maximum disk space in megabytes.
    pub disk_mb: Option<u64>,
    /// Maximum execution timeout in seconds.
    pub timeout_secs: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_mb: None,
            cpu_cores: None,
            disk_mb: None,
            timeout_secs: None,
        }
    }
}

/// A bind-mount volume specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Path on the host filesystem.
    pub host_path: String,
    /// Path inside the container.
    pub container_path: String,
    /// Whether the mount is read-only.
    pub read_only: bool,
}

/// Configuration used to create a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Docker image name (e.g. `"ubuntu:22.04"`).
    pub image: String,
    /// Optional human-readable container name.
    pub name: Option<String>,
    /// Environment variables passed to the container.
    pub env_vars: HashMap<String, String>,
    /// Volume mounts.
    pub volumes: Vec<VolumeMount>,
    /// Resource limits.
    pub resource_limits: ResourceLimits,
    /// Working directory inside the container.
    pub working_dir: Option<String>,
    /// Whether networking is enabled (default `false`).
    pub network_enabled: bool,
}

impl ContainerConfig {
    /// Create a minimal configuration for the given image.
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            name: None,
            env_vars: HashMap::new(),
            volumes: Vec::new(),
            resource_limits: ResourceLimits::default(),
            working_dir: None,
            network_enabled: false,
        }
    }
}

/// A managed container instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    /// Unique identifier for this container.
    pub id: String,
    /// The configuration used to create this container.
    pub config: ContainerConfig,
    /// Current lifecycle status.
    pub status: ContainerStatus,
    /// When the container was created.
    pub created_at: DateTime<Utc>,
    /// When the container was last started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the container was last stopped.
    pub stopped_at: Option<DateTime<Utc>>,
}

/// The result of executing a command inside a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// DockerSandbox
// ---------------------------------------------------------------------------

/// In-memory Docker sandbox manager.
///
/// Manages container lifecycle state for feature parity with the Electron
/// `docker-sandbox.ts` module. Actual Docker daemon interaction is not
/// performed; instead the manager tracks containers in memory and returns
/// simulated execution results.
#[derive(Debug)]
pub struct DockerSandbox {
    containers: HashMap<String, Container>,
}

impl DockerSandbox {
    /// Create a new, empty sandbox manager.
    pub fn new() -> Self {
        debug!("creating new DockerSandbox");
        Self {
            containers: HashMap::new(),
        }
    }

    /// Create a container from the given configuration.
    ///
    /// Returns the unique container id.
    pub fn create_container(&mut self, config: ContainerConfig) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        debug!(id = %id, image = %config.image, "creating container");

        let container = Container {
            id: id.clone(),
            config,
            status: ContainerStatus::Created,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
        };

        self.containers.insert(id.clone(), container);
        Ok(id)
    }

    /// Start a container that is in the `Created` or `Stopped` state.
    pub fn start_container(&mut self, id: &str) -> Result<()> {
        let container = self
            .containers
            .get_mut(id)
            .context(format!("Container not found: {id}"))?;

        match container.status {
            ContainerStatus::Created | ContainerStatus::Stopped => {
                debug!(id = %id, "starting container");
                container.status = ContainerStatus::Running;
                container.started_at = Some(Utc::now());
                Ok(())
            }
            ContainerStatus::Running => {
                bail!("Container {id} is already running");
            }
            ContainerStatus::Paused => {
                bail!("Container {id} is paused; unpause it first");
            }
            ContainerStatus::Removed => {
                bail!("Container {id} has been removed");
            }
        }
    }

    /// Stop a running or paused container.
    pub fn stop_container(&mut self, id: &str) -> Result<()> {
        let container = self
            .containers
            .get_mut(id)
            .context(format!("Container not found: {id}"))?;

        match container.status {
            ContainerStatus::Running | ContainerStatus::Paused => {
                debug!(id = %id, "stopping container");
                container.status = ContainerStatus::Stopped;
                container.stopped_at = Some(Utc::now());
                Ok(())
            }
            ContainerStatus::Stopped => {
                bail!("Container {id} is already stopped");
            }
            ContainerStatus::Created => {
                bail!("Container {id} was never started");
            }
            ContainerStatus::Removed => {
                bail!("Container {id} has been removed");
            }
        }
    }

    /// Pause a running container.
    pub fn pause_container(&mut self, id: &str) -> Result<()> {
        let container = self
            .containers
            .get_mut(id)
            .context(format!("Container not found: {id}"))?;

        match container.status {
            ContainerStatus::Running => {
                debug!(id = %id, "pausing container");
                container.status = ContainerStatus::Paused;
                Ok(())
            }
            ContainerStatus::Paused => {
                bail!("Container {id} is already paused");
            }
            _ => {
                bail!("Container {id} is not running (status: {:?})", container.status);
            }
        }
    }

    /// Unpause (resume) a paused container.
    pub fn unpause_container(&mut self, id: &str) -> Result<()> {
        let container = self
            .containers
            .get_mut(id)
            .context(format!("Container not found: {id}"))?;

        match container.status {
            ContainerStatus::Paused => {
                debug!(id = %id, "unpausing container");
                container.status = ContainerStatus::Running;
                Ok(())
            }
            ContainerStatus::Running => {
                bail!("Container {id} is not paused");
            }
            _ => {
                bail!("Container {id} is not paused (status: {:?})", container.status);
            }
        }
    }

    /// Remove a container that is in the `Stopped` or `Created` state.
    ///
    /// Running or paused containers must be stopped first.
    pub fn remove_container(&mut self, id: &str) -> Result<()> {
        let container = self
            .containers
            .get(id)
            .context(format!("Container not found: {id}"))?;

        match container.status {
            ContainerStatus::Stopped | ContainerStatus::Created => {
                debug!(id = %id, "removing container");
                self.containers.remove(id);
                Ok(())
            }
            ContainerStatus::Running | ContainerStatus::Paused => {
                bail!("Container {id} must be stopped before removal (status: {:?})", container.status);
            }
            ContainerStatus::Removed => {
                bail!("Container {id} has already been removed");
            }
        }
    }

    /// Execute a command inside a running container.
    ///
    /// Since no real Docker daemon is available, this returns a simulated
    /// [`ExecResult`] with exit code 0, the command echoed to stdout, and
    /// empty stderr.
    pub fn exec_in_container(&self, id: &str, command: &str) -> Result<ExecResult> {
        let container = self
            .containers
            .get(id)
            .context(format!("Container not found: {id}"))?;

        if container.status != ContainerStatus::Running {
            bail!(
                "Container {id} is not running (status: {:?}); cannot exec",
                container.status
            );
        }

        debug!(id = %id, command = %command, "executing command in container (simulated)");

        Ok(ExecResult {
            exit_code: 0,
            stdout: format!("simulated output of: {command}"),
            stderr: String::new(),
            duration_ms: 1,
        })
    }

    /// Look up a container by id.
    pub fn get_container(&self, id: &str) -> Option<&Container> {
        self.containers.get(id)
    }

    /// Return a list of all tracked containers.
    pub fn list_containers(&self) -> Vec<&Container> {
        self.containers.values().collect()
    }

    /// Count the number of containers currently in the `Running` state.
    pub fn running_count(&self) -> usize {
        self.containers
            .values()
            .filter(|c| c.status == ContainerStatus::Running)
            .count()
    }

    /// Total number of tracked containers (all statuses).
    pub fn total_count(&self) -> usize {
        self.containers.len()
    }

    /// Remove all containers in the `Stopped` state.
    ///
    /// Returns the number of containers removed.
    pub fn cleanup_stopped(&mut self) -> usize {
        let stopped_ids: Vec<String> = self
            .containers
            .iter()
            .filter(|(_, c)| c.status == ContainerStatus::Stopped)
            .map(|(id, _)| id.clone())
            .collect();

        let count = stopped_ids.len();
        for id in &stopped_ids {
            debug!(id = %id, "cleaning up stopped container");
            self.containers.remove(id);
        }
        count
    }

    /// Check whether the Docker CLI is available on the host.
    ///
    /// Attempts to run `docker --version`. Returns `true` if the command
    /// succeeds (exit code 0), `false` otherwise.
    pub fn is_docker_available(&self) -> bool {
        let result = std::process::Command::new("docker")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        match result {
            Ok(status) => status.success(),
            Err(_) => false,
        }
    }
}

impl Default for DockerSandbox {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction --------------------------------------------------------

    #[test]
    fn new_creates_empty_sandbox() {
        let sandbox = DockerSandbox::new();
        assert_eq!(sandbox.total_count(), 0);
        assert_eq!(sandbox.running_count(), 0);
    }

    #[test]
    fn default_creates_empty_sandbox() {
        let sandbox = DockerSandbox::default();
        assert_eq!(sandbox.total_count(), 0);
    }

    // -- Container creation --------------------------------------------------

    #[test]
    fn create_container_returns_unique_ids() {
        let mut sandbox = DockerSandbox::new();
        let config1 = ContainerConfig::new("ubuntu:22.04");
        let config2 = ContainerConfig::new("alpine:latest");

        let id1 = sandbox.create_container(config1).unwrap();
        let id2 = sandbox.create_container(config2).unwrap();

        assert_ne!(id1, id2);
        assert_eq!(sandbox.total_count(), 2);
    }

    #[test]
    fn created_container_has_correct_initial_state() {
        let mut sandbox = DockerSandbox::new();
        let config = ContainerConfig::new("ubuntu:22.04");
        let id = sandbox.create_container(config).unwrap();

        let container = sandbox.get_container(&id).unwrap();
        assert_eq!(container.status, ContainerStatus::Created);
        assert_eq!(container.config.image, "ubuntu:22.04");
        assert!(container.started_at.is_none());
        assert!(container.stopped_at.is_none());
    }

    #[test]
    fn create_container_preserves_config() {
        let mut sandbox = DockerSandbox::new();
        let mut config = ContainerConfig::new("node:18");
        config.name = Some("my-node-app".into());
        config.working_dir = Some("/app".into());
        config.network_enabled = true;
        config.env_vars.insert("NODE_ENV".into(), "production".into());
        config.volumes.push(VolumeMount {
            host_path: "/data".into(),
            container_path: "/mnt/data".into(),
            read_only: true,
        });
        config.resource_limits.memory_mb = Some(512);
        config.resource_limits.cpu_cores = Some(2.0);

        let id = sandbox.create_container(config).unwrap();
        let c = sandbox.get_container(&id).unwrap();

        assert_eq!(c.config.name.as_deref(), Some("my-node-app"));
        assert_eq!(c.config.working_dir.as_deref(), Some("/app"));
        assert!(c.config.network_enabled);
        assert_eq!(c.config.env_vars.get("NODE_ENV").unwrap(), "production");
        assert_eq!(c.config.volumes.len(), 1);
        assert!(c.config.volumes[0].read_only);
        assert_eq!(c.config.resource_limits.memory_mb, Some(512));
        assert_eq!(c.config.resource_limits.cpu_cores, Some(2.0));
    }

    // -- Lifecycle transitions -----------------------------------------------

    #[test]
    fn start_transitions_created_to_running() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();

        sandbox.start_container(&id).unwrap();

        let c = sandbox.get_container(&id).unwrap();
        assert_eq!(c.status, ContainerStatus::Running);
        assert!(c.started_at.is_some());
    }

    #[test]
    fn stop_transitions_running_to_stopped() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();

        sandbox.stop_container(&id).unwrap();

        let c = sandbox.get_container(&id).unwrap();
        assert_eq!(c.status, ContainerStatus::Stopped);
        assert!(c.stopped_at.is_some());
    }

    #[test]
    fn pause_transitions_running_to_paused() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();

        sandbox.pause_container(&id).unwrap();

        let c = sandbox.get_container(&id).unwrap();
        assert_eq!(c.status, ContainerStatus::Paused);
    }

    #[test]
    fn unpause_transitions_paused_to_running() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();
        sandbox.pause_container(&id).unwrap();

        sandbox.unpause_container(&id).unwrap();

        let c = sandbox.get_container(&id).unwrap();
        assert_eq!(c.status, ContainerStatus::Running);
    }

    #[test]
    fn restart_stopped_container() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();
        sandbox.stop_container(&id).unwrap();

        sandbox.start_container(&id).unwrap();

        let c = sandbox.get_container(&id).unwrap();
        assert_eq!(c.status, ContainerStatus::Running);
    }

    // -- Invalid transitions -------------------------------------------------

    #[test]
    fn start_already_running_fails() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();

        let result = sandbox.start_container(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already running"));
    }

    #[test]
    fn stop_created_container_fails() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();

        let result = sandbox.stop_container(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("never started"));
    }

    #[test]
    fn pause_non_running_container_fails() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();

        let result = sandbox.pause_container(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not running"));
    }

    #[test]
    fn unpause_running_container_fails() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();

        let result = sandbox.unpause_container(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not paused"));
    }

    #[test]
    fn remove_running_container_fails() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();

        let result = sandbox.remove_container(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be stopped"));
    }

    // -- Remove and exec ----------------------------------------------------

    #[test]
    fn remove_stopped_container_succeeds() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();
        sandbox.stop_container(&id).unwrap();

        sandbox.remove_container(&id).unwrap();

        assert!(sandbox.get_container(&id).is_none());
        assert_eq!(sandbox.total_count(), 0);
    }

    #[test]
    fn remove_created_container_succeeds() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();

        sandbox.remove_container(&id).unwrap();
        assert_eq!(sandbox.total_count(), 0);
    }

    #[test]
    fn exec_in_running_container_succeeds() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();

        let result = sandbox.exec_in_container(&id, "echo hello").unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("echo hello"));
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn exec_in_stopped_container_fails() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();

        let result = sandbox.exec_in_container(&id, "echo hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not running"));
    }

    // -- Listing and counting -----------------------------------------------

    #[test]
    fn list_containers_returns_all() {
        let mut sandbox = DockerSandbox::new();
        sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.create_container(ContainerConfig::new("alpine:latest")).unwrap();
        sandbox.create_container(ContainerConfig::new("node:18")).unwrap();

        let containers = sandbox.list_containers();
        assert_eq!(containers.len(), 3);
    }

    #[test]
    fn running_count_reflects_state() {
        let mut sandbox = DockerSandbox::new();
        let id1 = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        let id2 = sandbox.create_container(ContainerConfig::new("alpine:latest")).unwrap();
        let _id3 = sandbox.create_container(ContainerConfig::new("node:18")).unwrap();

        assert_eq!(sandbox.running_count(), 0);

        sandbox.start_container(&id1).unwrap();
        assert_eq!(sandbox.running_count(), 1);

        sandbox.start_container(&id2).unwrap();
        assert_eq!(sandbox.running_count(), 2);

        sandbox.stop_container(&id1).unwrap();
        assert_eq!(sandbox.running_count(), 1);
    }

    // -- Cleanup stopped ----------------------------------------------------

    #[test]
    fn cleanup_stopped_removes_only_stopped() {
        let mut sandbox = DockerSandbox::new();

        // Container 1: running (should survive cleanup)
        let id1 = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id1).unwrap();

        // Container 2: stopped (should be removed)
        let id2 = sandbox.create_container(ContainerConfig::new("alpine:latest")).unwrap();
        sandbox.start_container(&id2).unwrap();
        sandbox.stop_container(&id2).unwrap();

        // Container 3: created (should survive cleanup)
        let _id3 = sandbox.create_container(ContainerConfig::new("node:18")).unwrap();

        assert_eq!(sandbox.total_count(), 3);

        let removed = sandbox.cleanup_stopped();
        assert_eq!(removed, 1);
        assert_eq!(sandbox.total_count(), 2);
        assert!(sandbox.get_container(&id1).is_some());
        assert!(sandbox.get_container(&id2).is_none());
    }

    #[test]
    fn cleanup_stopped_returns_zero_when_none_stopped() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();

        let removed = sandbox.cleanup_stopped();
        assert_eq!(removed, 0);
    }

    // -- Not found errors ---------------------------------------------------

    #[test]
    fn operations_on_nonexistent_container_fail() {
        let mut sandbox = DockerSandbox::new();
        let fake_id = "nonexistent-id";

        assert!(sandbox.start_container(fake_id).is_err());
        assert!(sandbox.stop_container(fake_id).is_err());
        assert!(sandbox.pause_container(fake_id).is_err());
        assert!(sandbox.unpause_container(fake_id).is_err());
        assert!(sandbox.remove_container(fake_id).is_err());
        assert!(sandbox.exec_in_container(fake_id, "echo").is_err());
        assert!(sandbox.get_container(fake_id).is_none());
    }

    // -- Docker availability -------------------------------------------------

    #[test]
    fn is_docker_available_returns_bool() {
        let sandbox = DockerSandbox::new();
        // We just verify the method doesn't panic and returns a bool.
        let _available: bool = sandbox.is_docker_available();
    }

    // -- Serialization -------------------------------------------------------

    #[test]
    fn container_status_serialization_roundtrip() {
        let statuses = vec![
            ContainerStatus::Created,
            ContainerStatus::Running,
            ContainerStatus::Paused,
            ContainerStatus::Stopped,
            ContainerStatus::Removed,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).expect("serialize");
            let restored: ContainerStatus =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&restored, status);
        }
    }

    #[test]
    fn exec_result_serialization_roundtrip() {
        let result = ExecResult {
            exit_code: 0,
            stdout: "hello".into(),
            stderr: String::new(),
            duration_ms: 42,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let restored: ExecResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.exit_code, result.exit_code);
        assert_eq!(restored.stdout, result.stdout);
        assert_eq!(restored.stderr, result.stderr);
        assert_eq!(restored.duration_ms, result.duration_ms);
    }

    #[test]
    fn container_config_new_defaults() {
        let config = ContainerConfig::new("python:3.11");
        assert_eq!(config.image, "python:3.11");
        assert!(config.name.is_none());
        assert!(config.env_vars.is_empty());
        assert!(config.volumes.is_empty());
        assert!(!config.network_enabled);
        assert!(config.working_dir.is_none());
        assert!(config.resource_limits.memory_mb.is_none());
        assert!(config.resource_limits.cpu_cores.is_none());
        assert!(config.resource_limits.disk_mb.is_none());
        assert!(config.resource_limits.timeout_secs.is_none());
    }

    // -- Stop paused container -----------------------------------------------

    #[test]
    fn stop_paused_container_succeeds() {
        let mut sandbox = DockerSandbox::new();
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        sandbox.start_container(&id).unwrap();
        sandbox.pause_container(&id).unwrap();

        sandbox.stop_container(&id).unwrap();

        let c = sandbox.get_container(&id).unwrap();
        assert_eq!(c.status, ContainerStatus::Stopped);
    }

    // -- Full lifecycle -------------------------------------------------------

    #[test]
    fn full_lifecycle() {
        let mut sandbox = DockerSandbox::new();

        // Create
        let id = sandbox.create_container(ContainerConfig::new("ubuntu:22.04")).unwrap();
        assert_eq!(sandbox.get_container(&id).unwrap().status, ContainerStatus::Created);

        // Start
        sandbox.start_container(&id).unwrap();
        assert_eq!(sandbox.get_container(&id).unwrap().status, ContainerStatus::Running);
        assert_eq!(sandbox.running_count(), 1);

        // Exec
        let result = sandbox.exec_in_container(&id, "ls -la").unwrap();
        assert_eq!(result.exit_code, 0);

        // Pause
        sandbox.pause_container(&id).unwrap();
        assert_eq!(sandbox.get_container(&id).unwrap().status, ContainerStatus::Paused);
        assert_eq!(sandbox.running_count(), 0);

        // Unpause
        sandbox.unpause_container(&id).unwrap();
        assert_eq!(sandbox.get_container(&id).unwrap().status, ContainerStatus::Running);
        assert_eq!(sandbox.running_count(), 1);

        // Stop
        sandbox.stop_container(&id).unwrap();
        assert_eq!(sandbox.get_container(&id).unwrap().status, ContainerStatus::Stopped);
        assert_eq!(sandbox.running_count(), 0);

        // Restart
        sandbox.start_container(&id).unwrap();
        assert_eq!(sandbox.running_count(), 1);
        sandbox.stop_container(&id).unwrap();

        // Remove
        sandbox.remove_container(&id).unwrap();
        assert_eq!(sandbox.total_count(), 0);
        assert!(sandbox.get_container(&id).is_none());
    }
}
