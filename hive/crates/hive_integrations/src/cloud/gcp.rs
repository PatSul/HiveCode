use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Client for Google Cloud Platform services via the `gcloud` CLI.
///
/// All methods shell out to `gcloud` with `--format=json` and parse the
/// resulting JSON. This avoids pulling in the full GCP SDK while still
/// providing typed access to the most common GCP resources.
pub struct GcpClient {
    /// Optional GCP project ID override (`--project`).
    pub project: Option<String>,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeInstance {
    pub name: String,
    pub zone: String,
    pub machine_type: String,
    pub status: String,
    pub external_ip: Option<String>,
    pub internal_ip: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcsBucket {
    pub name: String,
    pub location: String,
    pub storage_class: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudFunction {
    pub name: String,
    pub runtime: String,
    pub status: String,
    pub entry_point: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudRunService {
    pub name: String,
    pub region: String,
    pub url: Option<String>,
    pub latest_revision: Option<String>,
    pub traffic_percent: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GkeCluster {
    pub name: String,
    pub zone: String,
    pub status: String,
    pub node_count: u32,
    pub kubernetes_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSqlInstance {
    pub name: String,
    pub database_version: String,
    pub tier: String,
    pub state: String,
    pub ip_address: Option<String>,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcpProjectInfo {
    pub project_id: String,
    pub name: String,
    pub project_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub severity: String,
    pub message: String,
    pub resource_type: String,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl GcpClient {
    /// Create a new client with an optional project override.
    pub fn new(project: Option<String>) -> Self {
        Self { project }
    }

    // -- Compute Engine -----------------------------------------------------

    /// List Compute Engine instances across all zones.
    pub async fn list_compute_instances(&self) -> Result<Vec<ComputeInstance>> {
        debug!("listing Compute Engine instances");
        let output = self.run_gcloud(&["compute", "instances", "list"]).await?;
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse Compute instances JSON")?;

        let instances = raw
            .iter()
            .map(|item| {
                let zone_full = json_str(item, "zone");
                let zone = zone_full
                    .rsplit('/')
                    .next()
                    .unwrap_or(&zone_full)
                    .to_string();

                let machine_type_full = json_str(item, "machineType");
                let machine_type = machine_type_full
                    .rsplit('/')
                    .next()
                    .unwrap_or(&machine_type_full)
                    .to_string();

                let external_ip = item
                    .get("networkInterfaces")
                    .and_then(|ni| ni.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|ni| ni.get("accessConfigs"))
                    .and_then(|ac| ac.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|ac| ac.get("natIP"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let internal_ip = item
                    .get("networkInterfaces")
                    .and_then(|ni| ni.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|ni| ni.get("networkIP"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                ComputeInstance {
                    name: json_str(item, "name"),
                    zone,
                    machine_type,
                    status: json_str(item, "status"),
                    external_ip,
                    internal_ip,
                    created_at: json_str(item, "creationTimestamp"),
                }
            })
            .collect();

        Ok(instances)
    }

    // -- Cloud Storage ------------------------------------------------------

    /// List GCS buckets in the project.
    pub async fn list_gcs_buckets(&self) -> Result<Vec<GcsBucket>> {
        debug!("listing GCS buckets");
        // gsutil ls -p <project> --json is not directly available; use the
        // gcloud storage buckets list command instead.
        let output = self
            .run_gcloud(&["storage", "buckets", "list"])
            .await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse GCS buckets JSON")?;

        let buckets = raw
            .iter()
            .map(|b| {
                let name = b
                    .get("name")
                    .or_else(|| b.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                GcsBucket {
                    name,
                    location: json_str(b, "location"),
                    storage_class: json_str(b, "storageClass"),
                    created_at: json_str(b, "timeCreated"),
                }
            })
            .collect();

        Ok(buckets)
    }

    // -- Cloud Functions ----------------------------------------------------

    /// List Cloud Functions (2nd gen / v2).
    pub async fn list_cloud_functions(&self) -> Result<Vec<CloudFunction>> {
        debug!("listing Cloud Functions");
        let output = self
            .run_gcloud(&["functions", "list"])
            .await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse Cloud Functions JSON")?;

        let functions = raw
            .iter()
            .map(|f| {
                let name_full = json_str(f, "name");
                let name = name_full
                    .rsplit('/')
                    .next()
                    .unwrap_or(&name_full)
                    .to_string();

                let region = name_full
                    .split('/')
                    .nth(3)
                    .unwrap_or("")
                    .to_string();

                CloudFunction {
                    name,
                    runtime: json_str(f, "runtime"),
                    status: json_str(f, "status"),
                    entry_point: json_str(f, "entryPoint"),
                    region,
                }
            })
            .collect();

        Ok(functions)
    }

    // -- Cloud Run ----------------------------------------------------------

    /// List Cloud Run services across all regions.
    pub async fn list_cloud_run_services(&self) -> Result<Vec<CloudRunService>> {
        debug!("listing Cloud Run services");
        let output = self
            .run_gcloud(&["run", "services", "list"])
            .await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse Cloud Run services JSON")?;

        let services = raw
            .iter()
            .map(|svc| {
                let metadata = svc.get("metadata");
                let status_obj = svc.get("status");

                let name = metadata
                    .and_then(|m| m.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let namespace = metadata
                    .and_then(|m| m.get("namespace"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let labels = metadata.and_then(|m| m.get("labels"));
                let region = labels
                    .and_then(|l| l.get("cloud.googleapis.com/location"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&namespace)
                    .to_string();

                let url = status_obj
                    .and_then(|s| s.get("url"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let latest_revision = status_obj
                    .and_then(|s| s.get("latestReadyRevisionName"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let traffic_percent = status_obj
                    .and_then(|s| s.get("traffic"))
                    .and_then(|t| t.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|t| t.get("percent"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);

                CloudRunService {
                    name,
                    region,
                    url,
                    latest_revision,
                    traffic_percent,
                }
            })
            .collect();

        Ok(services)
    }

    // -- GKE ----------------------------------------------------------------

    /// List GKE clusters.
    pub async fn list_gke_clusters(&self) -> Result<Vec<GkeCluster>> {
        debug!("listing GKE clusters");
        let output = self
            .run_gcloud(&["container", "clusters", "list"])
            .await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse GKE clusters JSON")?;

        let clusters = raw
            .iter()
            .map(|c| GkeCluster {
                name: json_str(c, "name"),
                zone: json_str(c, "zone"),
                status: json_str(c, "status"),
                node_count: c
                    .get("currentNodeCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                kubernetes_version: json_str(c, "currentMasterVersion"),
            })
            .collect();

        Ok(clusters)
    }

    // -- Cloud SQL ----------------------------------------------------------

    /// List Cloud SQL instances.
    pub async fn list_sql_instances(&self) -> Result<Vec<CloudSqlInstance>> {
        debug!("listing Cloud SQL instances");
        let output = self.run_gcloud(&["sql", "instances", "list"]).await?;
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse Cloud SQL instances JSON")?;

        let instances = raw
            .iter()
            .map(|inst| {
                let ip_address = inst
                    .get("ipAddresses")
                    .and_then(|ips| ips.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|ip| ip.get("ipAddress"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                CloudSqlInstance {
                    name: json_str(inst, "name"),
                    database_version: json_str(inst, "databaseVersion"),
                    tier: inst
                        .get("settings")
                        .and_then(|s| s.get("tier"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    state: json_str(inst, "state"),
                    ip_address,
                    region: json_str(inst, "region"),
                }
            })
            .collect();

        Ok(instances)
    }

    // -- Project Info -------------------------------------------------------

    /// Get project metadata for the configured project.
    pub async fn get_project_info(&self) -> Result<GcpProjectInfo> {
        debug!("getting GCP project info");
        let project = self
            .project
            .as_deref()
            .context("no project configured — set GcpClient.project or use gcloud config")?;

        let output = self
            .run_gcloud(&["projects", "describe", project])
            .await?;

        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse GCP project info JSON")?;

        Ok(GcpProjectInfo {
            project_id: json_str(&parsed, "projectId"),
            name: json_str(&parsed, "name"),
            project_number: json_str(&parsed, "projectNumber"),
        })
    }

    // -- Firestore ----------------------------------------------------------

    /// List Firestore collection IDs in the default database.
    pub async fn list_firestore_collections(&self) -> Result<Vec<String>> {
        debug!("listing Firestore collections");
        let output = self
            .run_gcloud(&["firestore", "indexes", "fields", "list"])
            .await?;

        // The gcloud Firestore CLI doesn't have a direct "list collections"
        // command; we parse index field listings and extract unique
        // collection group names. If the output is empty we return an empty
        // vec rather than erroring.
        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let raw: Vec<serde_json::Value> = serde_json::from_str(&output)
            .context("failed to parse Firestore collections JSON")?;

        let mut collections: Vec<String> = raw
            .iter()
            .filter_map(|entry| {
                let name = entry.get("name")?.as_str()?;
                // Format: projects/{p}/databases/{d}/collectionGroups/{collection}/fields/{f}
                let parts: Vec<&str> = name.split('/').collect();
                parts
                    .iter()
                    .position(|&p| p == "collectionGroups")
                    .and_then(|i| parts.get(i + 1))
                    .map(|s| s.to_string())
            })
            .collect();

        collections.sort();
        collections.dedup();

        Ok(collections)
    }

    // -- Logging ------------------------------------------------------------

    /// Read log entries with an optional filter expression.
    pub async fn get_logs(&self, filter: Option<&str>, limit: u32) -> Result<Vec<LogEntry>> {
        debug!(filter = ?filter, limit = limit, "fetching GCP logs");

        let limit_str = limit.to_string();
        let mut args = vec!["logging", "read", "--limit", &limit_str];

        // If a filter is provided, pass it as the positional argument.
        let filter_owned: String;
        if let Some(f) = filter {
            filter_owned = f.to_string();
            args.push(&filter_owned);
        }

        let output = self.run_gcloud(&args).await?;

        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse GCP log entries JSON")?;

        let entries = raw
            .iter()
            .map(|entry| {
                let message = entry
                    .get("textPayload")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        entry
                            .get("jsonPayload")
                            .map(|v| v.to_string())
                            .as_deref()
                            // We need the owned string to live long enough,
                            // so we handle this separately below.
                            .and_then(|_| None)
                    })
                    .unwrap_or("")
                    .to_string();

                // If textPayload was empty, try jsonPayload as a string.
                let message = if message.is_empty() {
                    entry
                        .get("jsonPayload")
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                } else {
                    message
                };

                let resource_type = entry
                    .get("resource")
                    .and_then(|r| r.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                LogEntry {
                    timestamp: json_str(entry, "timestamp"),
                    severity: json_str(entry, "severity"),
                    message,
                    resource_type,
                }
            })
            .collect();

        Ok(entries)
    }

    // -- Internal helpers ---------------------------------------------------

    /// Build and execute a `gcloud` CLI command, returning stdout on success.
    async fn run_gcloud(&self, args: &[&str]) -> Result<String> {
        let mut cmd = tokio::process::Command::new("gcloud");
        cmd.args(args);
        cmd.arg("--format=json");

        if let Some(ref project) = self.project {
            cmd.arg("--project").arg(project);
        }

        debug!(command = ?cmd, "executing gcloud CLI command");

        let output = cmd
            .output()
            .await
            .context("failed to execute `gcloud` CLI — is it installed and on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "gcloud {} failed (exit {}): {}",
                args.first().unwrap_or(&""),
                output.status,
                stderr.trim()
            );
        }

        let stdout =
            String::from_utf8(output.stdout).context("gcloud CLI produced non-UTF-8 output")?;

        Ok(stdout)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a string from a JSON object, returning an empty string if absent.
fn json_str(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client() -> GcpClient {
        GcpClient::new(Some("my-project-123".into()))
    }

    #[test]
    fn test_client_stores_project() {
        let client = make_client();
        assert_eq!(client.project.as_deref(), Some("my-project-123"));
    }

    #[test]
    fn test_client_none_project() {
        let client = GcpClient::new(None);
        assert!(client.project.is_none());
    }

    #[test]
    fn test_compute_instance_deserialise() {
        let json = serde_json::json!({
            "name": "web-server-1",
            "zone": "us-central1-a",
            "machine_type": "e2-medium",
            "status": "RUNNING",
            "external_ip": "35.192.0.1",
            "internal_ip": "10.128.0.2",
            "created_at": "2024-01-10T08:00:00Z"
        });
        let inst: ComputeInstance = serde_json::from_value(json).unwrap();
        assert_eq!(inst.name, "web-server-1");
        assert_eq!(inst.status, "RUNNING");
    }

    #[test]
    fn test_gcs_bucket_deserialise() {
        let json = serde_json::json!({
            "name": "my-data-bucket",
            "location": "US",
            "storage_class": "STANDARD",
            "created_at": "2024-02-01T00:00:00Z"
        });
        let bucket: GcsBucket = serde_json::from_value(json).unwrap();
        assert_eq!(bucket.name, "my-data-bucket");
        assert_eq!(bucket.storage_class, "STANDARD");
    }

    #[test]
    fn test_cloud_function_deserialise() {
        let json = serde_json::json!({
            "name": "process-events",
            "runtime": "python312",
            "status": "ACTIVE",
            "entry_point": "handle_event",
            "region": "us-central1"
        });
        let func: CloudFunction = serde_json::from_value(json).unwrap();
        assert_eq!(func.name, "process-events");
        assert_eq!(func.runtime, "python312");
    }

    #[test]
    fn test_cloud_run_service_deserialise() {
        let json = serde_json::json!({
            "name": "api-service",
            "region": "us-central1",
            "url": "https://api-service-abc123.run.app",
            "latest_revision": "api-service-00005-abc",
            "traffic_percent": 100
        });
        let svc: CloudRunService = serde_json::from_value(json).unwrap();
        assert_eq!(svc.name, "api-service");
        assert!(svc.url.is_some());
    }

    #[test]
    fn test_gke_cluster_deserialise() {
        let json = serde_json::json!({
            "name": "prod-cluster",
            "zone": "us-central1-a",
            "status": "RUNNING",
            "node_count": 5,
            "kubernetes_version": "1.29.1-gke.1"
        });
        let cluster: GkeCluster = serde_json::from_value(json).unwrap();
        assert_eq!(cluster.name, "prod-cluster");
        assert_eq!(cluster.node_count, 5);
    }

    #[test]
    fn test_cloud_sql_instance_deserialise() {
        let json = serde_json::json!({
            "name": "prod-db",
            "database_version": "POSTGRES_15",
            "tier": "db-custom-4-16384",
            "state": "RUNNABLE",
            "ip_address": "34.72.100.50",
            "region": "us-central1"
        });
        let inst: CloudSqlInstance = serde_json::from_value(json).unwrap();
        assert_eq!(inst.database_version, "POSTGRES_15");
    }

    #[test]
    fn test_project_info_deserialise() {
        let json = serde_json::json!({
            "project_id": "my-project-123",
            "name": "My Project",
            "project_number": "987654321"
        });
        let info: GcpProjectInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.project_id, "my-project-123");
        assert_eq!(info.project_number, "987654321");
    }

    #[test]
    fn test_log_entry_deserialise() {
        let json = serde_json::json!({
            "timestamp": "2024-06-15T10:30:00Z",
            "severity": "ERROR",
            "message": "connection refused",
            "resource_type": "cloud_run_revision"
        });
        let entry: LogEntry = serde_json::from_value(json).unwrap();
        assert_eq!(entry.severity, "ERROR");
        assert_eq!(entry.resource_type, "cloud_run_revision");
    }

    #[test]
    fn test_json_str_helper() {
        let val = serde_json::json!({ "name": "test", "num": 42 });
        assert_eq!(json_str(&val, "name"), "test");
        assert_eq!(json_str(&val, "missing"), "");
    }
}
