use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Client for AWS services via the `aws` CLI.
///
/// All methods shell out to the `aws` command-line tool with `--output json`
/// and parse the resulting JSON. This avoids a heavyweight SDK dependency while
/// still providing typed access to the most common AWS resources.
pub struct AwsClient {
    /// Optional AWS CLI profile name (`--profile`).
    pub profile: Option<String>,
    /// Optional AWS region override (`--region`).
    pub region: Option<String>,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ec2Instance {
    pub id: String,
    pub name: Option<String>,
    pub instance_type: String,
    pub state: String,
    pub public_ip: Option<String>,
    pub private_ip: Option<String>,
    pub launch_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Bucket {
    pub name: String,
    pub creation_date: String,
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Object {
    pub key: String,
    pub size_bytes: u64,
    pub last_modified: String,
    pub storage_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaFunction {
    pub name: String,
    pub runtime: Option<String>,
    pub handler: Option<String>,
    pub memory_mb: u64,
    pub timeout_sec: u64,
    pub last_modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdsInstance {
    pub id: String,
    pub engine: String,
    pub engine_version: String,
    pub instance_class: String,
    pub status: String,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcsService {
    pub name: String,
    pub status: String,
    pub desired_count: u64,
    pub running_count: u64,
    pub task_definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub timestamp: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamoTable {
    pub name: String,
    pub status: String,
    pub item_count: u64,
    pub size_bytes: u64,
    pub key_schema: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsAccountInfo {
    pub account_id: String,
    pub arn: String,
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub total_usd: f64,
    pub services: Vec<(String, f64)>,
    pub period: String,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl AwsClient {
    /// Create a new client. Both `profile` and `region` are optional.
    pub fn new(profile: Option<String>, region: Option<String>) -> Self {
        Self { profile, region }
    }

    // -- EC2 ----------------------------------------------------------------

    /// List EC2 instances in the configured region.
    pub async fn list_ec2_instances(&self) -> Result<Vec<Ec2Instance>> {
        debug!("listing EC2 instances");
        let output = self
            .run(&[
                "ec2",
                "describe-instances",
                "--query",
                "Reservations[].Instances[]",
            ])
            .await?;

        let raw: Vec<serde_json::Value> =
            serde_json::from_str(&output).context("failed to parse EC2 instances JSON")?;

        let mut instances = Vec::with_capacity(raw.len());
        for item in &raw {
            let name = item
                .get("Tags")
                .and_then(|tags| tags.as_array())
                .and_then(|tags| {
                    tags.iter().find(|t| t.get("Key").and_then(|k| k.as_str()) == Some("Name"))
                })
                .and_then(|t| t.get("Value"))
                .and_then(|v| v.as_str())
                .map(String::from);

            instances.push(Ec2Instance {
                id: json_str(item, "InstanceId"),
                name,
                instance_type: json_str(item, "InstanceType"),
                state: item
                    .get("State")
                    .and_then(|s| s.get("Name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                public_ip: item
                    .get("PublicIpAddress")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                private_ip: item
                    .get("PrivateIpAddress")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                launch_time: json_str(item, "LaunchTime"),
            });
        }

        Ok(instances)
    }

    // -- S3 -----------------------------------------------------------------

    /// List all S3 buckets in the account.
    pub async fn list_s3_buckets(&self) -> Result<Vec<S3Bucket>> {
        debug!("listing S3 buckets");
        let output = self.run(&["s3api", "list-buckets"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse S3 buckets JSON")?;

        let buckets = parsed
            .get("Buckets")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        let result = buckets
            .iter()
            .map(|b| S3Bucket {
                name: json_str(b, "Name"),
                creation_date: json_str(b, "CreationDate"),
                region: None, // bucket-level region requires a separate call
            })
            .collect();

        Ok(result)
    }

    /// List objects in an S3 bucket with an optional key prefix.
    pub async fn list_s3_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<S3Object>> {
        debug!(bucket = %bucket, prefix = ?prefix, "listing S3 objects");

        let mut args = vec![
            "s3api".to_string(),
            "list-objects-v2".to_string(),
            "--bucket".to_string(),
            bucket.to_string(),
        ];
        if let Some(pfx) = prefix {
            args.push("--prefix".to_string());
            args.push(pfx.to_string());
        }

        let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.run(&str_args).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse S3 objects JSON")?;

        let contents = parsed
            .get("Contents")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        let result = contents
            .iter()
            .map(|obj| S3Object {
                key: json_str(obj, "Key"),
                size_bytes: obj
                    .get("Size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                last_modified: json_str(obj, "LastModified"),
                storage_class: obj
                    .get("StorageClass")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
            .collect();

        Ok(result)
    }

    // -- Lambda -------------------------------------------------------------

    /// List all Lambda functions.
    pub async fn list_lambda_functions(&self) -> Result<Vec<LambdaFunction>> {
        debug!("listing Lambda functions");
        let output = self.run(&["lambda", "list-functions"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse Lambda functions JSON")?;

        let functions = parsed
            .get("Functions")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();

        let result = functions
            .iter()
            .map(|f| LambdaFunction {
                name: json_str(f, "FunctionName"),
                runtime: f.get("Runtime").and_then(|v| v.as_str()).map(String::from),
                handler: f.get("Handler").and_then(|v| v.as_str()).map(String::from),
                memory_mb: f.get("MemorySize").and_then(|v| v.as_u64()).unwrap_or(0),
                timeout_sec: f.get("Timeout").and_then(|v| v.as_u64()).unwrap_or(0),
                last_modified: json_str(f, "LastModified"),
            })
            .collect();

        Ok(result)
    }

    /// Invoke a Lambda function synchronously with an optional JSON payload.
    pub async fn invoke_lambda(
        &self,
        function_name: &str,
        payload: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        debug!(function = %function_name, "invoking Lambda function");

        let payload_str = match payload {
            Some(p) => serde_json::to_string(p).context("failed to serialise Lambda payload")?,
            None => "{}".to_string(),
        };

        // The Lambda invoke command writes its response to a file. We use
        // /dev/stdout so we can capture it directly.
        let args = vec![
            "lambda".to_string(),
            "invoke".to_string(),
            "--function-name".to_string(),
            function_name.to_string(),
            "--payload".to_string(),
            payload_str,
            "--cli-binary-format".to_string(),
            "raw-in-base64-out".to_string(),
            "/dev/stdout".to_string(),
        ];

        // Suppress the status output that `aws lambda invoke` writes to
        // stderr alongside the response file.
        let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.run(&str_args).await?;

        let value: serde_json::Value = serde_json::from_str(&output)
            .context("failed to parse Lambda invocation response")?;

        Ok(value)
    }

    // -- RDS ----------------------------------------------------------------

    /// List RDS database instances.
    pub async fn list_rds_instances(&self) -> Result<Vec<RdsInstance>> {
        debug!("listing RDS instances");
        let output = self.run(&["rds", "describe-db-instances"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse RDS instances JSON")?;

        let instances = parsed
            .get("DBInstances")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();

        let result = instances
            .iter()
            .map(|db| RdsInstance {
                id: json_str(db, "DBInstanceIdentifier"),
                engine: json_str(db, "Engine"),
                engine_version: json_str(db, "EngineVersion"),
                instance_class: json_str(db, "DBInstanceClass"),
                status: json_str(db, "DBInstanceStatus"),
                endpoint: db
                    .get("Endpoint")
                    .and_then(|e| e.get("Address"))
                    .and_then(|a| a.as_str())
                    .map(String::from),
            })
            .collect();

        Ok(result)
    }

    // -- ECS ----------------------------------------------------------------

    /// List ECS cluster ARNs.
    pub async fn list_ecs_clusters(&self) -> Result<Vec<String>> {
        debug!("listing ECS clusters");
        let output = self.run(&["ecs", "list-clusters"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse ECS clusters JSON")?;

        let arns = parsed
            .get("clusterArns")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        Ok(arns)
    }

    /// List ECS services in a cluster.
    pub async fn list_ecs_services(&self, cluster: &str) -> Result<Vec<EcsService>> {
        debug!(cluster = %cluster, "listing ECS services");

        // First, get the service ARNs.
        let list_output = self
            .run(&["ecs", "list-services", "--cluster", cluster])
            .await?;
        let list_parsed: serde_json::Value =
            serde_json::from_str(&list_output).context("failed to parse ECS service list JSON")?;

        let arns: Vec<String> = list_parsed
            .get("serviceArns")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if arns.is_empty() {
            return Ok(Vec::new());
        }

        // Now describe them for full details.
        let mut args = vec![
            "ecs".to_string(),
            "describe-services".to_string(),
            "--cluster".to_string(),
            cluster.to_string(),
            "--services".to_string(),
        ];
        args.extend(arns);

        let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let desc_output = self.run(&str_args).await?;
        let desc_parsed: serde_json::Value = serde_json::from_str(&desc_output)
            .context("failed to parse ECS service descriptions JSON")?;

        let services = desc_parsed
            .get("services")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();

        let result = services
            .iter()
            .map(|svc| EcsService {
                name: json_str(svc, "serviceName"),
                status: json_str(svc, "status"),
                desired_count: svc
                    .get("desiredCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                running_count: svc
                    .get("runningCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                task_definition: json_str(svc, "taskDefinition"),
            })
            .collect();

        Ok(result)
    }

    // -- CloudWatch ---------------------------------------------------------

    /// Retrieve recent log events from a CloudWatch log group.
    pub async fn get_cloudwatch_logs(
        &self,
        log_group: &str,
        limit: u32,
    ) -> Result<Vec<LogEvent>> {
        debug!(log_group = %log_group, limit = limit, "fetching CloudWatch logs");

        let limit_str = limit.to_string();
        let output = self
            .run(&[
                "logs",
                "filter-log-events",
                "--log-group-name",
                log_group,
                "--limit",
                &limit_str,
                "--query",
                "events[]",
            ])
            .await?;

        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse CloudWatch logs JSON")?;

        let events = parsed
            .get("events")
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_else(|| {
                // The --query may return a bare array.
                parsed.as_array().cloned().unwrap_or_default()
            });

        let result = events
            .iter()
            .map(|evt| LogEvent {
                timestamp: evt
                    .get("timestamp")
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
                message: json_str(evt, "message"),
            })
            .collect();

        Ok(result)
    }

    // -- DynamoDB -----------------------------------------------------------

    /// List DynamoDB table names.
    pub async fn list_dynamodb_tables(&self) -> Result<Vec<String>> {
        debug!("listing DynamoDB tables");
        let output = self.run(&["dynamodb", "list-tables"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse DynamoDB tables JSON")?;

        let names = parsed
            .get("TableNames")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        Ok(names)
    }

    /// Describe a single DynamoDB table.
    pub async fn describe_dynamodb_table(&self, name: &str) -> Result<DynamoTable> {
        debug!(table = %name, "describing DynamoDB table");
        let output = self
            .run(&["dynamodb", "describe-table", "--table-name", name])
            .await?;

        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse DynamoDB table JSON")?;

        let table = parsed
            .get("Table")
            .context("missing 'Table' key in describe-table response")?;

        let key_schema = table
            .get("KeySchema")
            .and_then(|k| k.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let attr = entry.get("AttributeName")?.as_str()?;
                        let key_type = entry.get("KeyType")?.as_str()?;
                        Some(format!("{attr} ({key_type})"))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(DynamoTable {
            name: json_str(table, "TableName"),
            status: json_str(table, "TableStatus"),
            item_count: table
                .get("ItemCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            size_bytes: table
                .get("TableSizeBytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            key_schema,
        })
    }

    // -- STS / Account ------------------------------------------------------

    /// Get information about the currently authenticated AWS identity.
    pub async fn get_account_info(&self) -> Result<AwsAccountInfo> {
        debug!("getting AWS account info via STS");
        let output = self.run(&["sts", "get-caller-identity"]).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse STS identity JSON")?;

        Ok(AwsAccountInfo {
            account_id: json_str(&parsed, "Account"),
            arn: json_str(&parsed, "Arn"),
            region: self.region.clone(),
        })
    }

    // -- Cost Explorer ------------------------------------------------------

    /// Get a cost estimate for the last `days` days via AWS Cost Explorer.
    pub async fn get_cost_estimate(&self, days: u32) -> Result<CostEstimate> {
        debug!(days = days, "fetching AWS cost estimate");

        let end = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let start = (chrono::Utc::now() - chrono::Duration::days(i64::from(days)))
            .format("%Y-%m-%d")
            .to_string();

        let period = format!("{start} to {end}");

        let output = self
            .run(&[
                "ce",
                "get-cost-and-usage",
                "--time-period",
                &format!("Start={start},End={end}"),
                "--granularity",
                "MONTHLY",
                "--metrics",
                "UnblendedCost",
                "--group-by",
                "Type=DIMENSION,Key=SERVICE",
            ])
            .await?;

        let parsed: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse Cost Explorer JSON")?;

        let mut total_usd = 0.0;
        let mut services: Vec<(String, f64)> = Vec::new();

        if let Some(results) = parsed.get("ResultsByTime").and_then(|r| r.as_array()) {
            for result in results {
                if let Some(groups) = result.get("Groups").and_then(|g| g.as_array()) {
                    for group in groups {
                        let service_name = group
                            .get("Keys")
                            .and_then(|k| k.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown")
                            .to_string();

                        let amount = group
                            .get("Metrics")
                            .and_then(|m| m.get("UnblendedCost"))
                            .and_then(|c| c.get("Amount"))
                            .and_then(|a| a.as_str())
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);

                        total_usd += amount;

                        // Merge into existing entry or create a new one.
                        if let Some(entry) = services.iter_mut().find(|(n, _)| n == &service_name) {
                            entry.1 += amount;
                        } else {
                            services.push((service_name, amount));
                        }
                    }
                }
            }
        }

        // Sort by cost descending.
        services.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(CostEstimate {
            total_usd,
            services,
            period,
        })
    }

    // -- Internal helpers ---------------------------------------------------

    /// Build and execute an `aws` CLI command, returning stdout on success.
    async fn run(&self, args: &[&str]) -> Result<String> {
        let mut cmd = tokio::process::Command::new("aws");
        cmd.args(args);
        cmd.arg("--output").arg("json");

        if let Some(ref profile) = self.profile {
            cmd.arg("--profile").arg(profile);
        }
        if let Some(ref region) = self.region {
            cmd.arg("--region").arg(region);
        }

        debug!(command = ?cmd, "executing AWS CLI command");

        let output = cmd
            .output()
            .await
            .context("failed to execute `aws` CLI — is it installed and on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "aws {} failed (exit {}): {}",
                args.first().unwrap_or(&""),
                output.status,
                stderr.trim()
            );
        }

        let stdout =
            String::from_utf8(output.stdout).context("aws CLI produced non-UTF-8 output")?;

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

    fn make_client() -> AwsClient {
        AwsClient::new(Some("test-profile".into()), Some("us-east-1".into()))
    }

    #[test]
    fn test_client_stores_profile_and_region() {
        let client = make_client();
        assert_eq!(client.profile.as_deref(), Some("test-profile"));
        assert_eq!(client.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn test_client_none_defaults() {
        let client = AwsClient::new(None, None);
        assert!(client.profile.is_none());
        assert!(client.region.is_none());
    }

    #[test]
    fn test_ec2_instance_deserialise() {
        let json = serde_json::json!({
            "id": "i-1234567890abcdef0",
            "name": "web-server",
            "instance_type": "t3.micro",
            "state": "running",
            "public_ip": "54.123.45.67",
            "private_ip": "10.0.1.5",
            "launch_time": "2024-01-15T08:30:00Z"
        });
        let instance: Ec2Instance = serde_json::from_value(json).unwrap();
        assert_eq!(instance.id, "i-1234567890abcdef0");
        assert_eq!(instance.state, "running");
    }

    #[test]
    fn test_s3_bucket_deserialise() {
        let json = serde_json::json!({
            "name": "my-bucket",
            "creation_date": "2024-01-01T00:00:00Z",
            "region": "us-west-2"
        });
        let bucket: S3Bucket = serde_json::from_value(json).unwrap();
        assert_eq!(bucket.name, "my-bucket");
    }

    #[test]
    fn test_s3_object_deserialise() {
        let json = serde_json::json!({
            "key": "docs/readme.md",
            "size_bytes": 4096,
            "last_modified": "2024-06-01T12:00:00Z",
            "storage_class": "STANDARD"
        });
        let obj: S3Object = serde_json::from_value(json).unwrap();
        assert_eq!(obj.key, "docs/readme.md");
        assert_eq!(obj.size_bytes, 4096);
    }

    #[test]
    fn test_lambda_function_deserialise() {
        let json = serde_json::json!({
            "name": "process-orders",
            "runtime": "nodejs20.x",
            "handler": "index.handler",
            "memory_mb": 256,
            "timeout_sec": 30,
            "last_modified": "2024-03-15T10:00:00Z"
        });
        let func: LambdaFunction = serde_json::from_value(json).unwrap();
        assert_eq!(func.name, "process-orders");
        assert_eq!(func.memory_mb, 256);
    }

    #[test]
    fn test_rds_instance_deserialise() {
        let json = serde_json::json!({
            "id": "mydb",
            "engine": "postgres",
            "engine_version": "15.4",
            "instance_class": "db.t3.micro",
            "status": "available",
            "endpoint": "mydb.abc123.us-east-1.rds.amazonaws.com"
        });
        let db: RdsInstance = serde_json::from_value(json).unwrap();
        assert_eq!(db.engine, "postgres");
    }

    #[test]
    fn test_ecs_service_deserialise() {
        let json = serde_json::json!({
            "name": "web",
            "status": "ACTIVE",
            "desired_count": 3,
            "running_count": 3,
            "task_definition": "arn:aws:ecs:us-east-1:123:task-definition/web:5"
        });
        let svc: EcsService = serde_json::from_value(json).unwrap();
        assert_eq!(svc.desired_count, 3);
        assert_eq!(svc.running_count, 3);
    }

    #[test]
    fn test_dynamo_table_deserialise() {
        let json = serde_json::json!({
            "name": "users",
            "status": "ACTIVE",
            "item_count": 1500,
            "size_bytes": 524288,
            "key_schema": ["id (HASH)"]
        });
        let table: DynamoTable = serde_json::from_value(json).unwrap();
        assert_eq!(table.name, "users");
        assert_eq!(table.item_count, 1500);
    }

    #[test]
    fn test_account_info_deserialise() {
        let json = serde_json::json!({
            "account_id": "123456789012",
            "arn": "arn:aws:iam::123456789012:root",
            "region": "us-east-1"
        });
        let info: AwsAccountInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.account_id, "123456789012");
    }

    #[test]
    fn test_cost_estimate_deserialise() {
        let json = serde_json::json!({
            "total_usd": 142.57,
            "services": [["EC2", 80.0], ["S3", 62.57]],
            "period": "2024-01-01 to 2024-01-31"
        });
        let cost: CostEstimate = serde_json::from_value(json).unwrap();
        assert_eq!(cost.total_usd, 142.57);
        assert_eq!(cost.services.len(), 2);
    }

    #[test]
    fn test_json_str_helper() {
        let val = serde_json::json!({ "name": "hello", "count": 42 });
        assert_eq!(json_str(&val, "name"), "hello");
        assert_eq!(json_str(&val, "missing"), "");
    }

    #[test]
    fn test_log_event_deserialise() {
        let json = serde_json::json!({
            "timestamp": "1700000000000",
            "message": "INFO: request processed"
        });
        let evt: LogEvent = serde_json::from_value(json).unwrap();
        assert_eq!(evt.message, "INFO: request processed");
    }
}
