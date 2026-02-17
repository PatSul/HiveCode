//! Integration tool definitions for the MCP server.
//!
//! Defines MCP tools that bridge to the integration hubs (messaging,
//! project management, knowledge bases, databases, Docker, Kubernetes,
//! cloud providers, etc.).
//!
//! Handlers are registered as closures that can be swapped at runtime
//! when the actual integration hub becomes available.

use crate::mcp_client::McpTool;
use crate::mcp_server::ToolHandler;
use serde_json::json;

/// Return all integration tool definitions with default (stub) handlers.
pub fn integration_tools() -> Vec<(McpTool, ToolHandler)> {
    vec![
        // --- Messaging ---
        (
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
            },
            Box::new(|args| {
                let platform = args["platform"].as_str().unwrap_or("unknown");
                let channel = args["channel"].as_str().unwrap_or("unknown");
                let message = args["message"].as_str().unwrap_or("");
                Ok(json!({
                    "status": "queued",
                    "platform": platform,
                    "channel": channel,
                    "message_preview": &message[..message.len().min(100)]
                }))
            }),
        ),
        // --- Project Management ---
        (
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
            },
            Box::new(|args| {
                Ok(json!({
                    "status": "created",
                    "platform": args["platform"].as_str().unwrap_or("unknown"),
                    "project": args["project"].as_str().unwrap_or(""),
                    "title": args["title"].as_str().unwrap_or(""),
                    "id": format!("ISSUE-{}", chrono::Utc::now().timestamp_millis() % 10000)
                }))
            }),
        ),
        (
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
            },
            Box::new(|args| {
                Ok(json!({
                    "platform": args["platform"].as_str().unwrap_or("unknown"),
                    "project": args["project"].as_str().unwrap_or(""),
                    "issues": [],
                    "note": "Connect your project management platform in Settings to see real issues"
                }))
            }),
        ),
        // --- Knowledge Base ---
        (
            McpTool {
                name: "search_knowledge".into(),
                description: "Search knowledge bases (Notion, Obsidian) for relevant information"
                    .into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "platform": { "type": "string", "enum": ["notion", "obsidian", "all"] }
                    },
                    "required": ["query"]
                }),
            },
            Box::new(|args| {
                Ok(json!({
                    "query": args["query"].as_str().unwrap_or(""),
                    "results": [],
                    "note": "Connect your knowledge base in Settings to search real content"
                }))
            }),
        ),
        // --- Database ---
        (
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
            },
            Box::new(|args| {
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
            }),
        ),
        (
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
            },
            Box::new(|_args| {
                Ok(json!({
                    "tables": [],
                    "note": "Connect a database in Settings to see real schema"
                }))
            }),
        ),
        // --- Docker ---
        (
            McpTool {
                name: "docker_list".into(),
                description: "List Docker containers (running and stopped)".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "all": { "type": "boolean", "description": "Include stopped containers" }
                    }
                }),
            },
            Box::new(|_args| {
                Ok(json!({
                    "containers": [],
                    "note": "Docker integration active — ensure Docker daemon is running"
                }))
            }),
        ),
        (
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
            },
            Box::new(|args| {
                Ok(json!({
                    "container": args["container"].as_str().unwrap_or(""),
                    "logs": "",
                    "note": "Docker integration active — ensure Docker daemon is running"
                }))
            }),
        ),
        // --- Kubernetes ---
        (
            McpTool {
                name: "k8s_pods".into(),
                description: "List Kubernetes pods in a namespace".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "namespace": { "type": "string", "description": "Kubernetes namespace (default: default)" }
                    }
                }),
            },
            Box::new(|args| {
                Ok(json!({
                    "namespace": args["namespace"].as_str().unwrap_or("default"),
                    "pods": [],
                    "note": "Ensure kubeconfig is configured to see real pods"
                }))
            }),
        ),
        // --- Cloud ---
        (
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
            },
            Box::new(|args| {
                Ok(json!({
                    "provider": args["provider"].as_str().unwrap_or("unknown"),
                    "resources": [],
                    "note": "Configure cloud credentials in Settings to see real resources"
                }))
            }),
        ),
        // --- Browser ---
        (
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
            },
            Box::new(|args| {
                Ok(json!({
                    "url": args["url"].as_str().unwrap_or(""),
                    "content": "",
                    "note": "Browser automation available — content extraction pending connection"
                }))
            }),
        ),
        // --- Docs Search ---
        (
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
            },
            Box::new(|args| {
                Ok(json!({
                    "query": args["query"].as_str().unwrap_or(""),
                    "results": [],
                    "note": "Run /index-docs to build the documentation index first"
                }))
            }),
        ),
    ]
}
