//! Integration tool definitions for the MCP server.
//!
//! Defines MCP tools that bridge to the integration hubs (messaging,
//! project management, knowledge bases, databases, Docker, Kubernetes,
//! cloud providers, etc.).
//!
//! Tool handlers start as stubs and are replaced with real implementations
//! via [`wire_integration_handlers`] once the integration services are
//! initialized as GPUI globals.

use crate::mcp_client::McpTool;
use crate::mcp_server::ToolHandler;
use serde_json::json;
use std::sync::Arc;
use tokio::runtime::Handle;

/// Return all integration tool definitions with default (stub) handlers.
///
/// These stubs are used at startup before integration services are ready.
/// Call [`wire_integration_handlers`] afterwards to replace them with real
/// implementations.
pub fn integration_tools() -> Vec<(McpTool, ToolHandler)> {
    vec![
        // --- Messaging ---
        (send_message_tool(), stub("Connect a messaging platform in Settings to send messages")),
        // --- Project Management ---
        (create_issue_tool(), stub("Connect your project management platform in Settings to create issues")),
        (list_issues_tool(), stub("Connect your project management platform in Settings to list issues")),
        // --- Knowledge Base ---
        (search_knowledge_tool(), stub("Connect a knowledge base in Settings to search content")),
        // --- Database ---
        (query_database_tool(), Box::new(|args| {
            let query = args["query"].as_str().unwrap_or("");
            if !query.trim_start().to_lowercase().starts_with("select") {
                return Err("Only SELECT queries are allowed for safety".into());
            }
            Ok(json!({
                "connection": args["connection"].as_str().unwrap_or("default"),
                "query": query,
                "rows": [],
                "note": "Connect a database in Settings to run real queries"
            }))
        })),
        (describe_schema_tool(), stub("Connect a database in Settings to see real schema")),
        // --- Docker ---
        (docker_list_tool(), stub("Docker integration active — ensure Docker daemon is running")),
        (docker_logs_tool(), stub("Docker integration active — ensure Docker daemon is running")),
        // --- Kubernetes ---
        (k8s_pods_tool(), stub("Ensure kubeconfig is configured to see real pods")),
        // --- Cloud ---
        (cloud_resources_tool(), stub("Configure cloud credentials in Settings to see real resources")),
        // --- Browser ---
        (browse_url_tool(), stub("Browser automation available — content extraction pending connection")),
        // --- Docs Search ---
        (search_docs_tool(), stub("Run /index-docs to build the documentation index first")),
        // --- Deploy ---
        (deploy_trigger_tool(), stub("Configure deployment workflows in Settings")),
    ]
}

/// Replace stub handlers with real implementations backed by service `Arc`s.
///
/// Each handler bridges the sync MCP tool handler signature to the async
/// service methods using a blocking spawn on the tokio runtime.
pub fn wire_integration_handlers(services: IntegrationServices) -> Vec<(McpTool, ToolHandler)> {
    let mut tools = Vec::new();

    // --- Messaging ---
    {
        let svc = Arc::clone(&services.messaging);
        tools.push((send_message_tool(), Box::new(move |args: serde_json::Value| {
            let platform = args["platform"].as_str().unwrap_or("slack").to_string();
            let channel = args["channel"].as_str().unwrap_or("").to_string();
            let message = args["message"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let plat = parse_messaging_platform(&platform);
                match svc.send_message(plat, &channel, &message).await {
                    Ok(sent) => Ok(json!({
                        "status": "sent",
                        "platform": platform,
                        "channel": channel,
                        "timestamp": sent.timestamp
                    })),
                    Err(e) => Err(format!("Failed to send message: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Project Management ---
    {
        let svc = Arc::clone(&services.project_management);
        tools.push((create_issue_tool(), Box::new(move |args: serde_json::Value| {
            let platform = args["platform"].as_str().unwrap_or("jira").to_string();
            let project = args["project"].as_str().unwrap_or("").to_string();
            let title = args["title"].as_str().unwrap_or("").to_string();
            let description = args["description"].as_str().unwrap_or("").to_string();
            let priority = args["priority"].as_str().unwrap_or("medium").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let plat = parse_pm_platform(&platform);
                let request = hive_integrations::project_management::CreateIssueRequest {
                    project_id: project,
                    title,
                    description: if description.is_empty() { None } else { Some(description) },
                    priority: Some(parse_priority(&priority)),
                    assignee: None,
                    labels: vec![],
                };
                match svc.create_issue(plat, &request).await {
                    Ok(issue) => Ok(json!({
                        "status": "created",
                        "platform": platform,
                        "id": issue.id,
                        "key": issue.key,
                        "title": issue.title,
                        "url": issue.url
                    })),
                    Err(e) => Err(format!("Failed to create issue: {e}")),
                }
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.project_management);
        tools.push((list_issues_tool(), Box::new(move |args: serde_json::Value| {
            let platform = args["platform"].as_str().unwrap_or("jira").to_string();
            let project = args["project"].as_str().unwrap_or("").to_string();
            let status_filter = args["status"].as_str().unwrap_or("all").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let plat = parse_pm_platform(&platform);
                let filters = hive_integrations::project_management::IssueFilters {
                    status: parse_issue_status_filter(&status_filter),
                    ..Default::default()
                };
                match svc.list_issues(plat, &project, &filters).await {
                    Ok(issues) => {
                        let items: Vec<serde_json::Value> = issues.iter().map(|i| json!({
                            "id": i.id,
                            "key": i.key,
                            "title": i.title,
                            "status": format!("{:?}", i.status),
                            "priority": format!("{:?}", i.priority),
                            "url": i.url
                        })).collect();
                        Ok(json!({
                            "platform": platform,
                            "project": project,
                            "count": items.len(),
                            "issues": items
                        }))
                    }
                    Err(e) => Err(format!("Failed to list issues: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Knowledge Base ---
    {
        let svc = Arc::clone(&services.knowledge);
        tools.push((search_knowledge_tool(), Box::new(move |args: serde_json::Value| {
            let query = args["query"].as_str().unwrap_or("").to_string();
            let platform = args["platform"].as_str().unwrap_or("all").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let results = if platform == "all" {
                    svc.search_all(&query, 20).await
                } else {
                    let plat = parse_kb_platform(&platform);
                    svc.search(plat, &query, 20).await.unwrap_or_default()
                };
                let items: Vec<serde_json::Value> = results.iter().map(|r| json!({
                    "title": r.title,
                    "snippet": r.snippet,
                    "url": r.url,
                    "score": r.relevance_score
                })).collect();
                Ok(json!({
                    "query": query,
                    "count": items.len(),
                    "results": items
                }))
            })
        }) as ToolHandler));
    }

    // --- Database ---
    {
        let svc = Arc::clone(&services.database);
        tools.push((query_database_tool(), Box::new(move |args: serde_json::Value| {
            let connection = args["connection"].as_str().unwrap_or("default").to_string();
            let query = args["query"].as_str().unwrap_or("").to_string();
            if !query.trim_start().to_lowercase().starts_with("select") {
                return Err("Only SELECT queries are allowed for safety".into());
            }
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let provider = svc.get_provider(&connection)
                    .ok_or_else(|| format!("No database connection named '{connection}'. Use Settings to configure one."))?;
                match provider.execute_query(&query).await {
                    Ok(result) => Ok(json!({
                        "connection": connection,
                        "query": query,
                        "columns": result.columns,
                        "rows": result.rows,
                        "rows_affected": result.rows_affected,
                        "execution_time_ms": result.execution_time_ms
                    })),
                    Err(e) => Err(format!("Query failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.database);
        tools.push((describe_schema_tool(), Box::new(move |args: serde_json::Value| {
            let connection = args["connection"].as_str().unwrap_or("default").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let provider = svc.get_provider(&connection)
                    .ok_or_else(|| format!("No database connection named '{connection}'. Use Settings to configure one."))?;
                match provider.list_tables("public").await {
                    Ok(tables) => {
                        let items: Vec<serde_json::Value> = tables.iter().map(|t| json!({
                            "name": t.name,
                            "schema": t.schema,
                            "row_count_estimate": t.row_count_estimate,
                            "size_bytes": t.size_bytes
                        })).collect();
                        Ok(json!({
                            "connection": connection,
                            "table_count": items.len(),
                            "tables": items
                        }))
                    }
                    Err(e) => Err(format!("Schema introspection failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Docker ---
    {
        let svc = Arc::clone(&services.docker);
        tools.push((docker_list_tool(), Box::new(move |args: serde_json::Value| {
            let all = args["all"].as_bool().unwrap_or(false);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.list_containers(all).await {
                    Ok(containers) => {
                        let items: Vec<serde_json::Value> = containers.iter().map(|c| json!({
                            "id": c.id,
                            "name": c.name,
                            "image": c.image,
                            "status": c.status,
                            "state": c.state,
                            "ports": c.ports
                        })).collect();
                        Ok(json!({
                            "count": items.len(),
                            "containers": items
                        }))
                    }
                    Err(e) => Err(format!("Docker list failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.docker);
        tools.push((docker_logs_tool(), Box::new(move |args: serde_json::Value| {
            let container = args["container"].as_str().unwrap_or("").to_string();
            let tail = args["tail"].as_u64().map(|n| n as u32).unwrap_or(100);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.container_logs(&container, Some(tail)).await {
                    Ok(logs) => Ok(json!({
                        "container": container,
                        "logs": logs
                    })),
                    Err(e) => Err(format!("Docker logs failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Kubernetes ---
    {
        let svc = Arc::clone(&services.kubernetes);
        tools.push((k8s_pods_tool(), Box::new(move |args: serde_json::Value| {
            let namespace = args["namespace"].as_str().map(String::from);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.list_pods(namespace.as_deref()).await {
                    Ok(pods) => {
                        let items: Vec<serde_json::Value> = pods.iter().map(|p| json!({
                            "name": p.name,
                            "namespace": p.namespace,
                            "status": p.status,
                            "node": p.node,
                            "restarts": p.restarts,
                            "age": p.age
                        })).collect();
                        Ok(json!({
                            "namespace": namespace.as_deref().unwrap_or("default"),
                            "count": items.len(),
                            "pods": items
                        }))
                    }
                    Err(e) => Err(format!("Kubernetes list pods failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Cloud ---
    {
        let aws = Arc::clone(&services.aws);
        let azure = Arc::clone(&services.azure);
        let gcp = Arc::clone(&services.gcp);
        tools.push((cloud_resources_tool(), Box::new(move |args: serde_json::Value| {
            let provider = args["provider"].as_str().unwrap_or("aws").to_string();
            let resource_type = args["resource_type"].as_str().unwrap_or("").to_string();
            let aws = Arc::clone(&aws);
            let azure = Arc::clone(&azure);
            let gcp = Arc::clone(&gcp);
            block_on_async(async move {
                match provider.as_str() {
                    "aws" => list_aws_resources(&aws, &resource_type).await,
                    "azure" => list_azure_resources(&azure, &resource_type).await,
                    "gcp" => list_gcp_resources(&gcp, &resource_type).await,
                    _ => Err(format!("Unknown cloud provider: {provider}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Browser ---
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browse_url_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.get_page_content(&url).await {
                    Ok(content) => Ok(json!({
                        "url": url,
                        "title": content.title,
                        "content": content.text_content,
                        "links": content.links
                    })),
                    Err(e) => Err(format!("Browse failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Docs Search ---
    {
        let svc = Arc::clone(&services.docs_indexer);
        tools.push((search_docs_tool(), Box::new(move |args: serde_json::Value| {
            let query = args["query"].as_str().unwrap_or("").to_string();
            let max_results = args["max_results"].as_u64().unwrap_or(10) as usize;
            let svc = Arc::clone(&svc);
            // search is sync
            let results = svc.search("default", &query, max_results);
            let items: Vec<serde_json::Value> = results.iter().map(|r| json!({
                "title": r.title,
                "url": r.page_url,
                "snippet": r.snippet,
                "score": r.relevance_score
            })).collect();
            Ok(json!({
                "query": query,
                "count": items.len(),
                "results": items
            }))
        }) as ToolHandler));
    }

    // --- Deploy (dispatches via deploy scripts, Makefile, or GitHub Actions) ---
    tools.push((deploy_trigger_tool(), Box::new(|args: serde_json::Value| {
        let environment = args["environment"].as_str().unwrap_or("staging").to_string();
        let branch = args["branch"].as_str().unwrap_or("main").to_string();

        // Try common deployment dispatch mechanisms in order of preference:
        // 1. deploy.sh in current directory
        // 2. Makefile "deploy" target
        // 3. GitHub Actions via `gh workflow run`
        let deploy_result = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "if [ -f deploy.sh ]; then \
                    DEPLOY_ENV='{env}' DEPLOY_BRANCH='{branch}' bash deploy.sh 2>&1; \
                elif [ -f Makefile ] && grep -q '^deploy:' Makefile 2>/dev/null; then \
                    DEPLOY_ENV='{env}' DEPLOY_BRANCH='{branch}' make deploy 2>&1; \
                elif command -v gh >/dev/null 2>&1; then \
                    gh workflow run deploy.yml -f environment='{env}' -f branch='{branch}' 2>&1; \
                else \
                    echo '__NO_DEPLOY_MECHANISM__'; \
                fi",
                env = environment,
                branch = branch,
            ))
            .output();

        match deploy_result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

                if stdout.contains("__NO_DEPLOY_MECHANISM__") {
                    Ok(json!({
                        "status": "no_deploy_mechanism",
                        "environment": environment,
                        "branch": branch,
                        "note": "No deploy.sh, Makefile deploy target, or gh CLI found. Add a deploy.sh to your project root or configure a GitHub Actions deploy workflow."
                    }))
                } else if output.status.success() {
                    Ok(json!({
                        "status": "triggered",
                        "environment": environment,
                        "branch": branch,
                        "output": if stdout.len() > 500 { stdout[..500].to_string() } else { stdout },
                        "note": "Deployment initiated successfully."
                    }))
                } else {
                    Ok(json!({
                        "status": "failed",
                        "environment": environment,
                        "branch": branch,
                        "error": if stderr.is_empty() { stdout } else { stderr },
                        "exit_code": output.status.code()
                    }))
                }
            }
            Err(e) => Err(format!("Failed to execute deploy command: {e}")),
        }
    }) as ToolHandler));

    tools
}

/// Services needed by integration tool handlers.
pub struct IntegrationServices {
    pub messaging: Arc<hive_integrations::messaging::MessagingHub>,
    pub project_management: Arc<hive_integrations::project_management::ProjectManagementHub>,
    pub knowledge: Arc<hive_integrations::knowledge::KnowledgeHub>,
    pub database: Arc<hive_integrations::database::DatabaseHub>,
    pub docker: Arc<hive_integrations::docker::DockerClient>,
    pub kubernetes: Arc<hive_integrations::kubernetes::KubernetesClient>,
    pub browser: Arc<hive_integrations::browser::BrowserAutomation>,
    pub aws: Arc<hive_integrations::cloud::AwsClient>,
    pub azure: Arc<hive_integrations::cloud::AzureClient>,
    pub gcp: Arc<hive_integrations::cloud::GcpClient>,
    pub docs_indexer: Arc<hive_integrations::docs_indexer::DocsIndexer>,
}

// ---------------------------------------------------------------------------
// Tool definitions (shared between stubs and wired handlers)
// ---------------------------------------------------------------------------

fn send_message_tool() -> McpTool {
    McpTool {
        name: "send_message".into(),
        description: "Send a message via Slack, Discord, or Teams".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string", "enum": ["slack", "discord", "teams"] },
                "channel": { "type": "string", "description": "Channel name or ID" },
                "message": { "type": "string", "description": "Message content" }
            },
            "required": ["platform", "channel", "message"]
        }),
    }
}

fn create_issue_tool() -> McpTool {
    McpTool {
        name: "create_issue".into(),
        description: "Create an issue/ticket in Jira, Linear, or Asana".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string", "enum": ["jira", "linear", "asana"] },
                "project": { "type": "string" },
                "title": { "type": "string" },
                "description": { "type": "string" },
                "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] }
            },
            "required": ["platform", "project", "title"]
        }),
    }
}

fn list_issues_tool() -> McpTool {
    McpTool {
        name: "list_issues".into(),
        description: "List issues/tickets from Jira, Linear, or Asana".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string", "enum": ["jira", "linear", "asana"] },
                "project": { "type": "string" },
                "status": { "type": "string", "enum": ["open", "in_progress", "done", "all"] }
            },
            "required": ["platform", "project"]
        }),
    }
}

fn search_knowledge_tool() -> McpTool {
    McpTool {
        name: "search_knowledge".into(),
        description: "Search knowledge bases (Notion, Obsidian) for relevant information".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "platform": { "type": "string", "enum": ["notion", "obsidian", "all"] }
            },
            "required": ["query"]
        }),
    }
}

fn query_database_tool() -> McpTool {
    McpTool {
        name: "query_database".into(),
        description: "Run a read-only SQL query against a connected database".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "connection": { "type": "string", "description": "Database connection name" },
                "query": { "type": "string", "description": "SQL query (SELECT only)" }
            },
            "required": ["connection", "query"]
        }),
    }
}

fn describe_schema_tool() -> McpTool {
    McpTool {
        name: "describe_schema".into(),
        description: "Get the schema (tables, columns) of a connected database".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "connection": { "type": "string", "description": "Database connection name" }
            },
            "required": ["connection"]
        }),
    }
}

fn docker_list_tool() -> McpTool {
    McpTool {
        name: "docker_list".into(),
        description: "List Docker containers (running and stopped)".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "all": { "type": "boolean", "description": "Include stopped containers" }
            }
        }),
    }
}

fn docker_logs_tool() -> McpTool {
    McpTool {
        name: "docker_logs".into(),
        description: "Fetch logs from a Docker container".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "container": { "type": "string", "description": "Container name or ID" },
                "tail": { "type": "integer", "description": "Number of lines from the end (default 100)" }
            },
            "required": ["container"]
        }),
    }
}

fn k8s_pods_tool() -> McpTool {
    McpTool {
        name: "k8s_pods".into(),
        description: "List Kubernetes pods in a namespace".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "namespace": { "type": "string", "description": "Kubernetes namespace (default: default)" }
            }
        }),
    }
}

fn cloud_resources_tool() -> McpTool {
    McpTool {
        name: "cloud_resources".into(),
        description: "List cloud resources from AWS, Azure, or GCP".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "provider": { "type": "string", "enum": ["aws", "azure", "gcp"] },
                "resource_type": { "type": "string", "description": "Resource type (e.g. ec2, storage, functions)" }
            },
            "required": ["provider"]
        }),
    }
}

fn browse_url_tool() -> McpTool {
    McpTool {
        name: "browse_url".into(),
        description: "Fetch and extract content from a URL".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to browse" },
                "selector": { "type": "string", "description": "Optional CSS selector to extract specific content" }
            },
            "required": ["url"]
        }),
    }
}

fn search_docs_tool() -> McpTool {
    McpTool {
        name: "search_docs".into(),
        description: "Search indexed documentation for the current project".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "max_results": { "type": "integer", "description": "Maximum results to return" }
            },
            "required": ["query"]
        }),
    }
}

fn deploy_trigger_tool() -> McpTool {
    McpTool {
        name: "deploy_trigger".into(),
        description: "Trigger a deployment workflow to a target environment".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "environment": { "type": "string", "enum": ["staging", "production", "development"] },
                "branch": { "type": "string", "description": "Branch or tag to deploy (default: main)" }
            },
            "required": ["environment"]
        }),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a stub handler that returns a note.
fn stub(note: &'static str) -> ToolHandler {
    Box::new(move |_args| {
        Ok(json!({ "note": note }))
    })
}

/// Bridge sync handler to async by running on the tokio runtime.
fn block_on_async<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
    T: Send + 'static,
{
    if Handle::try_current().is_ok() {
        // Already inside a tokio runtime — spawn on a separate thread
        // to avoid blocking the runtime.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("Failed to create runtime: {e}"))?;
            rt.block_on(future)
        })
        .join()
        .map_err(|_| "Async task panicked".to_string())?
    } else {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create runtime: {e}"))?;
        rt.block_on(future)
    }
}

fn parse_messaging_platform(s: &str) -> hive_integrations::messaging::Platform {
    match s {
        "discord" => hive_integrations::messaging::Platform::Discord,
        "teams" => hive_integrations::messaging::Platform::Teams,
        _ => hive_integrations::messaging::Platform::Slack,
    }
}

fn parse_pm_platform(s: &str) -> hive_integrations::project_management::PMPlatform {
    match s {
        "linear" => hive_integrations::project_management::PMPlatform::Linear,
        "asana" => hive_integrations::project_management::PMPlatform::Asana,
        _ => hive_integrations::project_management::PMPlatform::Jira,
    }
}

fn parse_priority(s: &str) -> hive_integrations::project_management::IssuePriority {
    match s {
        "low" => hive_integrations::project_management::IssuePriority::Low,
        "high" => hive_integrations::project_management::IssuePriority::High,
        "critical" => hive_integrations::project_management::IssuePriority::Critical,
        _ => hive_integrations::project_management::IssuePriority::Medium,
    }
}

fn parse_issue_status_filter(s: &str) -> Option<hive_integrations::project_management::IssueStatus> {
    match s {
        "open" => Some(hive_integrations::project_management::IssueStatus::Todo),
        "in_progress" => Some(hive_integrations::project_management::IssueStatus::InProgress),
        "done" => Some(hive_integrations::project_management::IssueStatus::Done),
        _ => None, // "all"
    }
}

fn parse_kb_platform(s: &str) -> hive_integrations::knowledge::KBPlatform {
    match s {
        "obsidian" => hive_integrations::knowledge::KBPlatform::Obsidian,
        _ => hive_integrations::knowledge::KBPlatform::Notion,
    }
}

async fn list_aws_resources(
    aws: &hive_integrations::cloud::AwsClient,
    resource_type: &str,
) -> Result<serde_json::Value, String> {
    match resource_type {
        "s3" | "storage" => {
            let buckets = aws.list_s3_buckets().await.map_err(|e| format!("AWS error: {e}"))?;
            let items: Vec<serde_json::Value> = buckets.iter().map(|b| json!({
                "name": b.name, "region": b.region, "created": b.creation_date
            })).collect();
            Ok(json!({ "provider": "aws", "resource_type": "s3", "count": items.len(), "resources": items }))
        }
        "lambda" | "functions" => {
            let fns = aws.list_lambda_functions().await.map_err(|e| format!("AWS error: {e}"))?;
            let items: Vec<serde_json::Value> = fns.iter().map(|f| json!({
                "name": f.name, "runtime": f.runtime, "memory_mb": f.memory_mb
            })).collect();
            Ok(json!({ "provider": "aws", "resource_type": "lambda", "count": items.len(), "resources": items }))
        }
        _ => {
            // Default: list EC2 instances
            let instances = aws.list_ec2_instances().await.map_err(|e| format!("AWS error: {e}"))?;
            let items: Vec<serde_json::Value> = instances.iter().map(|i| json!({
                "id": i.id, "name": i.name, "state": i.state, "instance_type": i.instance_type
            })).collect();
            Ok(json!({ "provider": "aws", "resource_type": "ec2", "count": items.len(), "resources": items }))
        }
    }
}

async fn list_azure_resources(
    azure: &hive_integrations::cloud::AzureClient,
    resource_type: &str,
) -> Result<serde_json::Value, String> {
    match resource_type {
        "storage" => {
            let accounts = azure.list_storage_accounts().await.map_err(|e| format!("Azure error: {e}"))?;
            let items: Vec<serde_json::Value> = accounts.iter().map(|a| json!({
                "name": a.name, "kind": a.kind, "location": a.location
            })).collect();
            Ok(json!({ "provider": "azure", "resource_type": "storage", "count": items.len(), "resources": items }))
        }
        "functions" => {
            let fns = azure.list_functions().await.map_err(|e| format!("Azure error: {e}"))?;
            let items: Vec<serde_json::Value> = fns.iter().map(|f| json!({
                "name": f.name, "runtime": f.runtime, "state": f.state
            })).collect();
            Ok(json!({ "provider": "azure", "resource_type": "functions", "count": items.len(), "resources": items }))
        }
        _ => {
            let vms = azure.list_vms().await.map_err(|e| format!("Azure error: {e}"))?;
            let items: Vec<serde_json::Value> = vms.iter().map(|v| json!({
                "name": v.name, "size": v.vm_size, "status": v.status, "location": v.location
            })).collect();
            Ok(json!({ "provider": "azure", "resource_type": "vms", "count": items.len(), "resources": items }))
        }
    }
}

async fn list_gcp_resources(
    gcp: &hive_integrations::cloud::GcpClient,
    resource_type: &str,
) -> Result<serde_json::Value, String> {
    match resource_type {
        "storage" => {
            let buckets = gcp.list_gcs_buckets().await.map_err(|e| format!("GCP error: {e}"))?;
            let items: Vec<serde_json::Value> = buckets.iter().map(|b| json!({
                "name": b.name, "location": b.location, "storage_class": b.storage_class
            })).collect();
            Ok(json!({ "provider": "gcp", "resource_type": "storage", "count": items.len(), "resources": items }))
        }
        "functions" => {
            let fns = gcp.list_cloud_functions().await.map_err(|e| format!("GCP error: {e}"))?;
            let items: Vec<serde_json::Value> = fns.iter().map(|f| json!({
                "name": f.name, "runtime": f.runtime, "status": f.status
            })).collect();
            Ok(json!({ "provider": "gcp", "resource_type": "functions", "count": items.len(), "resources": items }))
        }
        _ => {
            let instances = gcp.list_compute_instances().await.map_err(|e| format!("GCP error: {e}"))?;
            let items: Vec<serde_json::Value> = instances.iter().map(|i| json!({
                "name": i.name, "machine_type": i.machine_type, "status": i.status, "zone": i.zone
            })).collect();
            Ok(json!({ "provider": "gcp", "resource_type": "compute", "count": items.len(), "resources": items }))
        }
    }
}
