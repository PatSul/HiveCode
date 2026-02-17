use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Client for Microsoft Azure services via the `az` CLI.
///
/// All methods shell out to the `az` command-line tool with `--output json`
/// and parse the resulting JSON. This avoids pulling in the Azure SDK while
/// providing typed access to the most common Azure resources.
pub struct AzureClient {
    /// Optional subscription ID override (`--subscription`).
    pub subscription: Option<String>,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureVm {
    pub name: String,
    pub resource_group: String,
    pub location: String,
    pub vm_size: String,
    pub status: String,
    pub public_ip: Option<String>,
    pub private_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAccount {
    pub name: String,
    pub resource_group: String,
    pub location: String,
    pub kind: String,
    pub sku: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppService {
    pub name: String,
    pub resource_group: String,
    pub default_hostname: String,
    pub state: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureFunction {
    pub name: String,
    pub resource_group: String,
    pub runtime: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AksCluster {
    pub name: String,
    pub resource_group: String,
    pub location: String,
    pub kubernetes_version: String,
    pub node_count: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureSqlServer {
    pub name: String,
    pub resource_group: String,
    pub location: String,
    pub version: String,
    pub state: String,
    pub fqdn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceGroup {
    pub name: String,
    pub location: String,
    pub provisioning_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureSubscriptionInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub tenant_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub timestamp: String,
    pub operation: String,
    pub status: String,
    pub caller: String,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl AzureClient {
    /// Create a new client with an optional subscription ID.
    pub fn new(subscription: Option<String>) -> Self {
        Self { subscription }
    }

    // -- Virtual Machines ---------------------------------------------------

    /// List all VMs in the subscription (with instance view for power state).
    pub async fn list_vms(&self) -> Result<Vec<AzureVm>> {
        debug!("listing Azure VMs");
        let output = self
            .run(&["vm", "list", "-d"]) // -d includes instance view / power state
            .await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse Azure VMs JSON")?;

        let vms = raw
            .iter()
            .map(|vm| {
                let status = vm
                    .get("powerState")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let public_ip = vm
                    .get("publicIps")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from);

                let private_ip = vm
                    .get("privateIps")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from);

                AzureVm {
                    name: json_str(vm, "name"),
                    resource_group: json_str(vm, "resourceGroup"),
                    location: json_str(vm, "location"),
                    vm_size: vm
                        .get("hardwareProfile")
                        .and_then(|hp| hp.get("vmSize"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    status,
                    public_ip,
                    private_ip,
                }
            })
            .collect();

        Ok(vms)
    }

    // -- Storage Accounts ---------------------------------------------------

    /// List all storage accounts in the subscription.
    pub async fn list_storage_accounts(&self) -> Result<Vec<StorageAccount>> {
        debug!("listing Azure storage accounts");
        let output = self.run(&["storage", "account", "list"]).await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse storage accounts JSON")?;

        let accounts = raw
            .iter()
            .map(|sa| {
                let sku = sa
                    .get("sku")
                    .and_then(|s| s.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                StorageAccount {
                    name: json_str(sa, "name"),
                    resource_group: json_str(sa, "resourceGroup"),
                    location: json_str(sa, "location"),
                    kind: json_str(sa, "kind"),
                    sku,
                }
            })
            .collect();

        Ok(accounts)
    }

    // -- App Service --------------------------------------------------------

    /// List all App Service web apps.
    pub async fn list_app_services(&self) -> Result<Vec<AppService>> {
        debug!("listing Azure App Services");
        let output = self.run(&["webapp", "list"]).await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse App Service JSON")?;

        let apps = raw
            .iter()
            .map(|app| AppService {
                name: json_str(app, "name"),
                resource_group: json_str(app, "resourceGroup"),
                default_hostname: json_str(app, "defaultHostName"),
                state: json_str(app, "state"),
                kind: json_str(app, "kind"),
            })
            .collect();

        Ok(apps)
    }

    // -- Azure Functions ----------------------------------------------------

    /// List all Azure Function apps.
    pub async fn list_functions(&self) -> Result<Vec<AzureFunction>> {
        debug!("listing Azure Function apps");
        let output = self.run(&["functionapp", "list"]).await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse Azure Functions JSON")?;

        let functions = raw
            .iter()
            .map(|f| {
                let runtime = f
                    .get("siteConfig")
                    .and_then(|sc| {
                        sc.get("linuxFxVersion")
                            .or_else(|| sc.get("netFrameworkVersion"))
                    })
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                AzureFunction {
                    name: json_str(f, "name"),
                    resource_group: json_str(f, "resourceGroup"),
                    runtime,
                    state: json_str(f, "state"),
                }
            })
            .collect();

        Ok(functions)
    }

    // -- AKS ----------------------------------------------------------------

    /// List AKS (Azure Kubernetes Service) clusters.
    pub async fn list_aks_clusters(&self) -> Result<Vec<AksCluster>> {
        debug!("listing AKS clusters");
        let output = self.run(&["aks", "list"]).await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse AKS clusters JSON")?;

        let clusters = raw
            .iter()
            .map(|c| {
                let node_count = c
                    .get("agentPoolProfiles")
                    .and_then(|profiles| profiles.as_array())
                    .map(|pools| {
                        pools
                            .iter()
                            .filter_map(|p| p.get("count").and_then(|v| v.as_u64()))
                            .sum::<u64>() as u32
                    })
                    .unwrap_or(0);

                let status = c
                    .get("provisioningState")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        c.get("powerState")
                            .and_then(|ps| ps.get("code"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("")
                    .to_string();

                AksCluster {
                    name: json_str(c, "name"),
                    resource_group: json_str(c, "resourceGroup"),
                    location: json_str(c, "location"),
                    kubernetes_version: json_str(c, "kubernetesVersion"),
                    node_count,
                    status,
                }
            })
            .collect();

        Ok(clusters)
    }

    // -- Azure SQL ----------------------------------------------------------

    /// List Azure SQL servers.
    pub async fn list_sql_servers(&self) -> Result<Vec<AzureSqlServer>> {
        debug!("listing Azure SQL servers");
        let output = self.run(&["sql", "server", "list"]).await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse Azure SQL servers JSON")?;

        let servers = raw
            .iter()
            .map(|srv| AzureSqlServer {
                name: json_str(srv, "name"),
                resource_group: json_str(srv, "resourceGroup"),
                location: json_str(srv, "location"),
                version: json_str(srv, "version"),
                state: json_str(srv, "state"),
                fqdn: json_str(srv, "fullyQualifiedDomainName"),
            })
            .collect();

        Ok(servers)
    }

    // -- Resource Groups ----------------------------------------------------

    /// List all resource groups.
    pub async fn list_resource_groups(&self) -> Result<Vec<ResourceGroup>> {
        debug!("listing Azure resource groups");
        let output = self.run(&["group", "list"]).await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse resource groups JSON")?;

        let groups = raw
            .iter()
            .map(|rg| {
                let provisioning_state = rg
                    .get("properties")
                    .and_then(|p| p.get("provisioningState"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                ResourceGroup {
                    name: json_str(rg, "name"),
                    location: json_str(rg, "location"),
                    provisioning_state,
                }
            })
            .collect();

        Ok(groups)
    }

    // -- Subscription Info --------------------------------------------------

    /// Get information about the current or configured subscription.
    pub async fn get_subscription_info(&self) -> Result<AzureSubscriptionInfo> {
        debug!("getting Azure subscription info");

        let args = match &self.subscription {
            Some(sub) => vec!["account", "show", "--subscription", sub.as_str()],
            None => vec!["account", "show"],
        };

        let str_args: Vec<&str> = args.iter().map(|s| *s).collect();
        let output = self.run(&str_args).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse subscription info JSON")?;

        Ok(AzureSubscriptionInfo {
            id: json_str(&parsed, "id"),
            name: json_str(&parsed, "name"),
            state: json_str(&parsed, "state"),
            tenant_id: json_str(&parsed, "tenantId"),
        })
    }

    // -- Activity Log -------------------------------------------------------

    /// Get activity log entries for a resource group over the last `days` days.
    pub async fn get_activity_log(
        &self,
        resource_group: &str,
        days: u32,
    ) -> Result<Vec<ActivityLogEntry>> {
        debug!(
            resource_group = %resource_group,
            days = days,
            "fetching Azure activity log"
        );

        let start_time = (chrono::Utc::now() - chrono::Duration::days(i64::from(days)))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let output = self
            .run(&[
                "monitor",
                "activity-log",
                "list",
                "--resource-group",
                resource_group,
                "--start-time",
                &start_time,
            ])
            .await?;

        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse activity log JSON")?;

        let entries = raw
            .iter()
            .map(|entry| {
                let operation = entry
                    .get("operationName")
                    .and_then(|op| op.get("localizedValue").or_else(|| op.get("value")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let status = entry
                    .get("status")
                    .and_then(|s| s.get("localizedValue").or_else(|| s.get("value")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                ActivityLogEntry {
                    timestamp: json_str(entry, "eventTimestamp"),
                    operation,
                    status,
                    caller: json_str(entry, "caller"),
                }
            })
            .collect();

        Ok(entries)
    }

    // -- Internal helpers ---------------------------------------------------

    /// Build and execute an `az` CLI command, returning stdout on success.
    async fn run(&self, args: &[&str]) -> Result<String> {
        let mut cmd = tokio::process::Command::new("az");
        cmd.args(args);
        cmd.arg("--output").arg("json");

        if let Some(ref subscription) = self.subscription {
            cmd.arg("--subscription").arg(subscription);
        }

        debug!(command = ?cmd, "executing az CLI command");

        let output = cmd
            .output()
            .await
            .context("failed to execute `az` CLI — is it installed and on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "az {} failed (exit {}): {}",
                args.first().unwrap_or(&""),
                output.status,
                stderr.trim()
            );
        }

        let stdout =
            String::from_utf8(output.stdout).context("az CLI produced non-UTF-8 output")?;

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

    fn make_client() -> AzureClient {
        AzureClient::new(Some("00000000-0000-0000-0000-000000000001".into()))
    }

    #[test]
    fn test_client_stores_subscription() {
        let client = make_client();
        assert_eq!(
            client.subscription.as_deref(),
            Some("00000000-0000-0000-0000-000000000001")
        );
    }

    #[test]
    fn test_client_none_subscription() {
        let client = AzureClient::new(None);
        assert!(client.subscription.is_none());
    }

    #[test]
    fn test_azure_vm_deserialise() {
        let json = serde_json::json!({
            "name": "web-vm-1",
            "resource_group": "prod-rg",
            "location": "eastus",
            "vm_size": "Standard_D2s_v3",
            "status": "VM running",
            "public_ip": "20.120.0.1",
            "private_ip": "10.0.0.4"
        });
        let vm: AzureVm = serde_json::from_value(json).unwrap();
        assert_eq!(vm.name, "web-vm-1");
        assert_eq!(vm.vm_size, "Standard_D2s_v3");
    }

    #[test]
    fn test_storage_account_deserialise() {
        let json = serde_json::json!({
            "name": "prodstorage01",
            "resource_group": "prod-rg",
            "location": "eastus",
            "kind": "StorageV2",
            "sku": "Standard_LRS"
        });
        let sa: StorageAccount = serde_json::from_value(json).unwrap();
        assert_eq!(sa.name, "prodstorage01");
        assert_eq!(sa.kind, "StorageV2");
    }

    #[test]
    fn test_app_service_deserialise() {
        let json = serde_json::json!({
            "name": "my-webapp",
            "resource_group": "prod-rg",
            "default_hostname": "my-webapp.azurewebsites.net",
            "state": "Running",
            "kind": "app,linux"
        });
        let app: AppService = serde_json::from_value(json).unwrap();
        assert_eq!(app.name, "my-webapp");
        assert_eq!(app.state, "Running");
    }

    #[test]
    fn test_azure_function_deserialise() {
        let json = serde_json::json!({
            "name": "order-processor",
            "resource_group": "prod-rg",
            "runtime": "DOTNET|8.0",
            "state": "Running"
        });
        let func: AzureFunction = serde_json::from_value(json).unwrap();
        assert_eq!(func.name, "order-processor");
    }

    #[test]
    fn test_aks_cluster_deserialise() {
        let json = serde_json::json!({
            "name": "prod-aks",
            "resource_group": "prod-rg",
            "location": "eastus",
            "kubernetes_version": "1.29.2",
            "node_count": 6,
            "status": "Succeeded"
        });
        let cluster: AksCluster = serde_json::from_value(json).unwrap();
        assert_eq!(cluster.name, "prod-aks");
        assert_eq!(cluster.node_count, 6);
    }

    #[test]
    fn test_azure_sql_server_deserialise() {
        let json = serde_json::json!({
            "name": "prod-sql-server",
            "resource_group": "prod-rg",
            "location": "eastus",
            "version": "12.0",
            "state": "Ready",
            "fqdn": "prod-sql-server.database.windows.net"
        });
        let srv: AzureSqlServer = serde_json::from_value(json).unwrap();
        assert_eq!(srv.fqdn, "prod-sql-server.database.windows.net");
    }

    #[test]
    fn test_resource_group_deserialise() {
        let json = serde_json::json!({
            "name": "prod-rg",
            "location": "eastus",
            "provisioning_state": "Succeeded"
        });
        let rg: ResourceGroup = serde_json::from_value(json).unwrap();
        assert_eq!(rg.name, "prod-rg");
        assert_eq!(rg.provisioning_state, "Succeeded");
    }

    #[test]
    fn test_subscription_info_deserialise() {
        let json = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Production",
            "state": "Enabled",
            "tenant_id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        });
        let info: AzureSubscriptionInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.name, "Production");
        assert_eq!(info.state, "Enabled");
    }

    #[test]
    fn test_activity_log_entry_deserialise() {
        let json = serde_json::json!({
            "timestamp": "2024-06-15T10:00:00Z",
            "operation": "Microsoft.Compute/virtualMachines/start/action",
            "status": "Succeeded",
            "caller": "user@example.com"
        });
        let entry: ActivityLogEntry = serde_json::from_value(json).unwrap();
        assert_eq!(entry.status, "Succeeded");
        assert_eq!(entry.caller, "user@example.com");
    }

    #[test]
    fn test_json_str_helper() {
        let val = serde_json::json!({ "name": "test", "count": 42 });
        assert_eq!(json_str(&val, "name"), "test");
        assert_eq!(json_str(&val, "missing"), "");
    }
}
