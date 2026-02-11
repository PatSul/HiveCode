//! MCP Client — JSON-RPC 2.0 client for external Model Context Protocol servers.
//!
//! Implements the client side of MCP: connecting to external tool servers via
//! stdio or SSE transports, discovering their tools, and invoking them.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, warn};

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// How to connect to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum McpTransport {
    /// Communicate over stdin/stdout of a child process.
    Stdio,
    /// Communicate over Server-Sent Events at the given URL.
    Sse { url: String },
}

// ---------------------------------------------------------------------------
// Server configuration
// ---------------------------------------------------------------------------

/// Configuration for connecting to an external MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable server name.
    pub name: String,
    /// Transport mechanism.
    pub transport: McpTransport,
    /// Command to launch (stdio transport only).
    pub command: Option<String>,
    /// Arguments for the launch command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables for the child process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether this server is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Tool definition
// ---------------------------------------------------------------------------

/// A tool advertised by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
    pub id: u64,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>, params: serde_json::Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
            id,
        }
    }
}

/// A JSON-RPC 2.0 notification (request without an id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    /// Create an error response.
    pub fn error(id: u64, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }

    /// Whether this response indicates success.
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Standard JSON-RPC 2.0 error codes.
pub mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

impl JsonRpcError {
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: error_codes::METHOD_NOT_FOUND,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }

    pub fn invalid_params(detail: &str) -> Self {
        Self {
            code: error_codes::INVALID_PARAMS,
            message: format!("Invalid params: {detail}"),
            data: None,
        }
    }

    pub fn internal(detail: &str) -> Self {
        Self {
            code: error_codes::INTERNAL_ERROR,
            message: format!("Internal error: {detail}"),
            data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Raw JSON-RPC message (for parsing incoming lines that may or may not have id)
// ---------------------------------------------------------------------------

/// An incoming JSON-RPC message that could be a response or a notification.
///
/// We parse with `id` as optional to distinguish between the two. If `id` is
/// `None`, it is a notification. If present, it is a response.
#[derive(Debug, Clone, Deserialize)]
pub struct RawJsonRpcMessage {
    pub jsonrpc: String,
    /// Present only on responses.
    pub id: Option<u64>,
    /// Present only on notifications.
    pub method: Option<String>,
    /// Present on success responses.
    pub result: Option<serde_json::Value>,
    /// Present on error responses.
    pub error: Option<JsonRpcError>,
    /// Present on notifications (and requests, which we don't receive as client).
    pub params: Option<serde_json::Value>,
}

impl RawJsonRpcMessage {
    /// Returns `true` if this message is a notification (no `id` field).
    pub fn is_notification(&self) -> bool {
        self.id.is_none() && self.method.is_some()
    }

    /// Try to convert into a `JsonRpcResponse`. Fails if `id` is missing.
    pub fn into_response(self) -> Option<JsonRpcResponse> {
        let id = self.id?;
        Some(JsonRpcResponse {
            jsonrpc: self.jsonrpc,
            result: self.result,
            error: self.error,
            id,
        })
    }

    /// Try to convert into a `JsonRpcNotification`. Fails if `method` is missing.
    pub fn into_notification(self) -> Option<JsonRpcNotification> {
        let method = self.method?;
        Some(JsonRpcNotification {
            jsonrpc: self.jsonrpc,
            method,
            params: self.params.unwrap_or(serde_json::Value::Null),
        })
    }
}

// ---------------------------------------------------------------------------
// StdioTransport
// ---------------------------------------------------------------------------

/// Manages a child process for stdio-based MCP communication.
///
/// Sends newline-delimited JSON-RPC messages on stdin and reads
/// newline-delimited JSON-RPC messages from stdout.
struct StdioTransport {
    /// The child process handle (kept alive for the lifetime of the transport).
    child: Child,
    /// Writer to the child's stdin.
    stdin: tokio::process::ChildStdin,
    /// Buffered reader over the child's stdout.
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl StdioTransport {
    /// Spawn the child process described by `config` and prepare the I/O handles.
    async fn spawn(config: &McpServerConfig) -> anyhow::Result<Self> {
        let command = config
            .command
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Stdio transport requires a 'command' in config"))?;

        debug!(
            server = %config.name,
            command = %command,
            args = ?config.args,
            "Spawning MCP stdio child process"
        );

        let mut cmd = Command::new(command);
        cmd.args(&config.args)
            .envs(&config.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // On Windows, prevent a console window from popping up.
        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server command: {command}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture child stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture child stdout"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Send a JSON-RPC request as a single newline-delimited JSON line.
    async fn send_request(&mut self, request: &JsonRpcRequest) -> anyhow::Result<()> {
        let mut line =
            serde_json::to_string(request).context("Failed to serialize JSON-RPC request")?;
        line.push('\n');

        debug!(id = request.id, method = %request.method, "Sending JSON-RPC request");

        self.stdin
            .write_all(line.as_bytes())
            .await
            .context("Failed to write to child stdin")?;
        self.stdin
            .flush()
            .await
            .context("Failed to flush child stdin")?;

        Ok(())
    }

    /// Send a JSON-RPC notification (no id) as a single newline-delimited JSON line.
    async fn send_notification(
        &mut self,
        notification: &JsonRpcNotification,
    ) -> anyhow::Result<()> {
        let mut line = serde_json::to_string(notification)
            .context("Failed to serialize JSON-RPC notification")?;
        line.push('\n');

        debug!(method = %notification.method, "Sending JSON-RPC notification");

        self.stdin
            .write_all(line.as_bytes())
            .await
            .context("Failed to write notification to child stdin")?;
        self.stdin
            .flush()
            .await
            .context("Failed to flush child stdin")?;

        Ok(())
    }

    /// Read the next line from stdout and parse it as a raw JSON-RPC message.
    ///
    /// Returns `None` if the child closed stdout (EOF).
    async fn read_message(&mut self) -> anyhow::Result<Option<RawJsonRpcMessage>> {
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = self
                .stdout
                .read_line(&mut line)
                .await
                .context("Failed to read from child stdout")?;

            if bytes_read == 0 {
                // EOF — child closed stdout.
                return Ok(None);
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Skip blank lines.
                continue;
            }

            let msg: RawJsonRpcMessage = serde_json::from_str(trimmed).with_context(|| {
                format!("Failed to parse JSON-RPC message from child: {trimmed}")
            })?;

            return Ok(Some(msg));
        }
    }

    /// Read messages until we get a response with the given `id`.
    ///
    /// Any notifications received while waiting are logged and discarded.
    /// Any responses with non-matching IDs produce a warning and are discarded.
    async fn read_response(&mut self, expected_id: u64) -> anyhow::Result<JsonRpcResponse> {
        loop {
            let msg = self
                .read_message()
                .await?
                .ok_or_else(|| anyhow::anyhow!("Child process closed stdout before responding"))?;

            if msg.is_notification() {
                let notification = msg.into_notification().unwrap();
                debug!(
                    method = %notification.method,
                    "Received notification while waiting for response (discarding)"
                );
                continue;
            }

            if let Some(response) = msg.into_response() {
                if response.id == expected_id {
                    return Ok(response);
                }
                warn!(
                    expected_id,
                    actual_id = response.id,
                    "Received response with unexpected id (discarding)"
                );
                continue;
            }

            // Message has neither id nor method — malformed.
            warn!("Received malformed JSON-RPC message (no id and no method)");
        }
    }

    /// Gracefully shut down the child process.
    async fn shutdown(&mut self) -> anyhow::Result<()> {
        debug!("Shutting down stdio transport");

        // Close stdin to signal EOF to the child.
        // Shutdown write half — this signals the child that no more input is coming.
        self.stdin.shutdown().await.ok();

        // Give the child a moment to exit cleanly before we try to kill it.
        match tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(status)) => {
                debug!(?status, "MCP child process exited");
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Error waiting for MCP child process");
            }
            Err(_) => {
                warn!("MCP child process did not exit in time, killing");
                let _ = self.child.kill().await;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MCP Client
// ---------------------------------------------------------------------------

/// Client for communicating with an external MCP server.
///
/// Handles request ID generation, protocol message construction, and
/// communication over a stdio transport (spawned child process).
pub struct McpClient {
    config: McpServerConfig,
    next_id: AtomicU64,
    /// The live transport connection (populated after `connect()`).
    transport: Arc<Mutex<Option<StdioTransport>>>,
    /// Server capabilities returned from the `initialize` handshake.
    server_info: Arc<Mutex<Option<serde_json::Value>>>,
}

impl McpClient {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            next_id: AtomicU64::new(1),
            transport: Arc::new(Mutex::new(None)),
            server_info: Arc::new(Mutex::new(None)),
        }
    }

    /// Access the server configuration.
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Returns the server info received during initialization, if connected.
    pub async fn server_info(&self) -> Option<serde_json::Value> {
        self.server_info.lock().await.clone()
    }

    /// Returns `true` if the transport is currently connected.
    pub async fn is_connected(&self) -> bool {
        self.transport.lock().await.is_some()
    }

    /// Generate the next request ID.
    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Build an initialize request per the MCP protocol.
    pub fn build_initialize_request(&self) -> JsonRpcRequest {
        JsonRpcRequest::new(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "hive-mcp-client",
                    "version": "0.1.0"
                }
            }),
            self.next_request_id(),
        )
    }

    /// Build a tools/list request.
    pub fn build_list_tools_request(&self) -> JsonRpcRequest {
        JsonRpcRequest::new("tools/list", serde_json::json!({}), self.next_request_id())
    }

    /// Build a tools/call request for the named tool with arguments.
    pub fn build_call_tool_request(&self, name: &str, args: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest::new(
            "tools/call",
            serde_json::json!({
                "name": name,
                "arguments": args,
            }),
            self.next_request_id(),
        )
    }

    // -----------------------------------------------------------------------
    // Transport lifecycle
    // -----------------------------------------------------------------------

    /// Establish the transport connection and perform the MCP `initialize` handshake.
    ///
    /// For stdio transports, this spawns the child process, sends `initialize`,
    /// waits for the server's response, and sends the `initialized` notification.
    pub async fn connect(&self) -> anyhow::Result<serde_json::Value> {
        match &self.config.transport {
            McpTransport::Stdio => self.connect_stdio().await,
            McpTransport::Sse { url } => {
                anyhow::bail!(
                    "SSE transport not yet implemented (server '{}', url '{}')",
                    self.config.name,
                    url
                )
            }
        }
    }

    /// Internal: connect via stdio transport.
    async fn connect_stdio(&self) -> anyhow::Result<serde_json::Value> {
        // Prevent double-connect.
        {
            let guard = self.transport.lock().await;
            if guard.is_some() {
                anyhow::bail!("Already connected to server '{}'", self.config.name);
            }
        }

        let mut transport = StdioTransport::spawn(&self.config).await?;

        // Step 1: Send `initialize` request.
        let init_req = self.build_initialize_request();
        let init_id = init_req.id;
        transport.send_request(&init_req).await?;

        // Step 2: Read the initialize response.
        let init_response = transport.read_response(init_id).await.with_context(|| {
            format!(
                "Failed to read initialize response from server '{}'",
                self.config.name
            )
        })?;

        if let Some(err) = &init_response.error {
            anyhow::bail!(
                "Server '{}' returned error on initialize: {} (code {})",
                self.config.name,
                err.message,
                err.code
            );
        }

        let server_info = init_response
            .result
            .clone()
            .unwrap_or(serde_json::Value::Null);

        debug!(
            server = %self.config.name,
            server_info = %server_info,
            "MCP initialize handshake complete"
        );

        // Step 3: Send `initialized` notification to confirm.
        let initialized_notification =
            JsonRpcNotification::new("notifications/initialized", serde_json::json!({}));
        transport
            .send_notification(&initialized_notification)
            .await?;

        // Store transport and server info.
        *self.transport.lock().await = Some(transport);
        *self.server_info.lock().await = Some(server_info.clone());

        Ok(server_info)
    }

    /// Disconnect from the MCP server and shut down the transport.
    pub async fn disconnect(&self) -> anyhow::Result<()> {
        let mut guard = self.transport.lock().await;
        if let Some(mut transport) = guard.take() {
            debug!(server = %self.config.name, "Disconnecting from MCP server");
            transport.shutdown().await?;
        }
        *self.server_info.lock().await = None;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // High-level protocol methods
    // -----------------------------------------------------------------------

    /// Send an initialize request to the server.
    ///
    /// This is equivalent to `connect()` — it establishes the transport and
    /// performs the MCP initialization handshake.
    pub async fn initialize(&self) -> anyhow::Result<serde_json::Value> {
        self.connect().await
    }

    /// Retrieve the list of tools from the server.
    pub async fn list_tools(&self) -> anyhow::Result<Vec<McpTool>> {
        let request = self.build_list_tools_request();
        let response = self.send_request_internal(request).await?;

        Self::parse_list_tools_response(&response)
    }

    /// Call a tool on the server with the given arguments.
    pub async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let request = self.build_call_tool_request(name, args);
        let response = self.send_request_internal(request).await?;

        Self::parse_call_tool_response(&response)
    }

    /// Internal: send a request over the active transport and wait for the response.
    async fn send_request_internal(
        &self,
        request: JsonRpcRequest,
    ) -> anyhow::Result<JsonRpcResponse> {
        let mut guard = self.transport.lock().await;
        let transport = guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "Not connected to server '{}' — call connect() first",
                self.config.name
            )
        })?;

        let expected_id = request.id;
        transport.send_request(&request).await?;
        transport.read_response(expected_id).await
    }

    // -----------------------------------------------------------------------
    // Response parsing (static helpers)
    // -----------------------------------------------------------------------

    /// Parse a tools/list response into `McpTool` values.
    pub fn parse_list_tools_response(response: &JsonRpcResponse) -> anyhow::Result<Vec<McpTool>> {
        if let Some(err) = &response.error {
            anyhow::bail!("tools/list failed ({}): {}", err.code, err.message);
        }

        let result = response
            .result
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing result in tools/list response"))?;

        let tools_value = result
            .get("tools")
            .ok_or_else(|| anyhow::anyhow!("Missing 'tools' array in response"))?;

        let tools: Vec<McpTool> = serde_json::from_value(tools_value.clone())?;
        Ok(tools)
    }

    /// Parse a tools/call response and extract the content.
    pub fn parse_call_tool_response(
        response: &JsonRpcResponse,
    ) -> anyhow::Result<serde_json::Value> {
        if let Some(err) = &response.error {
            anyhow::bail!("Tool call failed ({}): {}", err.code, err.message);
        }

        response
            .result
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Missing result in tools/call response"))
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Best-effort: if we still have a transport, try to kill the child.
        // We can't do async cleanup in Drop, so we just attempt a sync kill.
        if let Ok(mut guard) = self.transport.try_lock() {
            if let Some(ref mut transport) = *guard {
                // Attempt to start a kill. The OS will clean up the zombie.
                let _ = transport.child.start_kill();
                error!(
                    server = %self.config.name,
                    "McpClient dropped without calling disconnect() — child process killed"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> McpServerConfig {
        McpServerConfig {
            name: "test-server".into(),
            transport: McpTransport::Stdio,
            command: Some("mcp-server".into()),
            args: vec!["--port".into(), "3000".into()],
            env: HashMap::new(),
            enabled: true,
        }
    }

    // -- JSON-RPC serialization tests --

    #[test]
    fn jsonrpc_request_serialization() {
        let req = JsonRpcRequest::new("tools/list", serde_json::json!({}), 1);
        let json = serde_json::to_value(&req).unwrap();

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "tools/list");
        assert_eq!(json["id"], 1);
    }

    #[test]
    fn jsonrpc_request_null_params_omitted() {
        let req = JsonRpcRequest::new("initialize", serde_json::Value::Null, 1);
        let json_str = serde_json::to_string(&req).unwrap();

        // Null params should be omitted from serialization.
        assert!(!json_str.contains("\"params\""));
    }

    #[test]
    fn jsonrpc_response_success_serialization() {
        let resp = JsonRpcResponse::success(42, serde_json::json!({"status": "ok"}));
        let json = serde_json::to_value(&resp).unwrap();

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 42);
        assert_eq!(json["result"]["status"], "ok");
        assert!(json.get("error").is_none() || json["error"].is_null());
        assert!(resp.is_success());
    }

    #[test]
    fn jsonrpc_response_error_serialization() {
        let err = JsonRpcError::method_not_found("tools/unknown");
        let resp = JsonRpcResponse::error(7, err);
        let json = serde_json::to_value(&resp).unwrap();

        assert_eq!(json["id"], 7);
        assert_eq!(json["error"]["code"], error_codes::METHOD_NOT_FOUND);
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("tools/unknown")
        );
        assert!(!resp.is_success());
    }

    #[test]
    fn jsonrpc_error_constructors() {
        let not_found = JsonRpcError::method_not_found("foo");
        assert_eq!(not_found.code, error_codes::METHOD_NOT_FOUND);
        assert!(not_found.message.contains("foo"));

        let invalid = JsonRpcError::invalid_params("missing 'name'");
        assert_eq!(invalid.code, error_codes::INVALID_PARAMS);
        assert!(invalid.message.contains("missing 'name'"));

        let internal = JsonRpcError::internal("disk full");
        assert_eq!(internal.code, error_codes::INTERNAL_ERROR);
        assert!(internal.message.contains("disk full"));
    }

    #[test]
    fn jsonrpc_roundtrip_deserialization() {
        let raw = r#"{"jsonrpc":"2.0","result":{"tools":[]},"id":5}"#;
        let resp: JsonRpcResponse = serde_json::from_str(raw).unwrap();

        assert_eq!(resp.id, 5);
        assert!(resp.is_success());
        assert!(resp.error.is_none());
    }

    // -- Client request building tests --

    #[test]
    fn build_initialize_request() {
        let client = McpClient::new(sample_config());
        let req = client.build_initialize_request();

        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "initialize");
        assert_eq!(req.params["protocolVersion"], "2024-11-05");
        assert_eq!(req.params["clientInfo"]["name"], "hive-mcp-client");
        assert_eq!(req.id, 1);
    }

    #[test]
    fn build_list_tools_request() {
        let client = McpClient::new(sample_config());
        let _init = client.build_initialize_request(); // consumes id 1
        let req = client.build_list_tools_request();

        assert_eq!(req.method, "tools/list");
        assert_eq!(req.id, 2);
    }

    #[test]
    fn build_call_tool_request() {
        let client = McpClient::new(sample_config());
        let req = client
            .build_call_tool_request("read_file", serde_json::json!({"path": "/tmp/test.txt"}));

        assert_eq!(req.method, "tools/call");
        assert_eq!(req.params["name"], "read_file");
        assert_eq!(req.params["arguments"]["path"], "/tmp/test.txt");
    }

    #[test]
    fn request_ids_increment() {
        let client = McpClient::new(sample_config());
        let r1 = client.build_initialize_request();
        let r2 = client.build_list_tools_request();
        let r3 = client.build_call_tool_request("test", serde_json::json!({}));

        assert_eq!(r1.id, 1);
        assert_eq!(r2.id, 2);
        assert_eq!(r3.id, 3);
    }

    // -- Response parsing tests --

    #[test]
    fn parse_list_tools_response_success() {
        let resp = JsonRpcResponse::success(
            1,
            serde_json::json!({
                "tools": [
                    {
                        "name": "read_file",
                        "description": "Read a file",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"}
                            }
                        }
                    }
                ]
            }),
        );

        let tools = McpClient::parse_list_tools_response(&resp).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[0].description, "Read a file");
        assert_eq!(tools[0].input_schema["type"], "object");
    }

    #[test]
    fn parse_list_tools_response_missing_result() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: None,
            id: 1,
        };

        let err = McpClient::parse_list_tools_response(&resp).unwrap_err();
        assert!(err.to_string().contains("Missing result"));
    }

    #[test]
    fn parse_call_tool_response_error() {
        let resp = JsonRpcResponse::error(1, JsonRpcError::internal("file not found"));

        let err = McpClient::parse_call_tool_response(&resp).unwrap_err();
        assert!(err.to_string().contains("file not found"));
    }

    // -- Config serialization tests --

    #[test]
    fn server_config_serialization() {
        let config = sample_config();
        let json = serde_json::to_value(&config).unwrap();

        assert_eq!(json["name"], "test-server");
        assert_eq!(json["transport"]["type"], "stdio");
        assert_eq!(json["command"], "mcp-server");
        assert_eq!(json["args"][0], "--port");
        assert!(json["enabled"].as_bool().unwrap());
    }

    #[test]
    fn sse_transport_serialization() {
        let config = McpServerConfig {
            name: "remote-server".into(),
            transport: McpTransport::Sse {
                url: "https://mcp.example.com/sse".into(),
            },
            command: None,
            args: vec![],
            env: HashMap::new(),
            enabled: true,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["transport"]["type"], "sse");
        assert_eq!(json["transport"]["url"], "https://mcp.example.com/sse");
    }

    #[test]
    fn mcp_tool_deserialization() {
        let raw = r#"{
            "name": "search_files",
            "description": "Search for a pattern in files",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string"},
                    "path": {"type": "string"}
                },
                "required": ["pattern"]
            }
        }"#;

        let tool: McpTool = serde_json::from_str(raw).unwrap();
        assert_eq!(tool.name, "search_files");
        assert_eq!(tool.input_schema["required"][0], "pattern");
    }

    // -- Notification tests --

    #[test]
    fn notification_serialization() {
        let notif = JsonRpcNotification::new("notifications/initialized", serde_json::json!({}));
        let json = serde_json::to_value(&notif).unwrap();

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "notifications/initialized");
        // Notifications must not have an id field.
        assert!(json.get("id").is_none());
    }

    #[test]
    fn notification_null_params_omitted() {
        let notif = JsonRpcNotification::new("test/ping", serde_json::Value::Null);
        let json_str = serde_json::to_string(&notif).unwrap();

        assert!(!json_str.contains("\"params\""));
        assert!(!json_str.contains("\"id\""));
    }

    // -- RawJsonRpcMessage tests --

    #[test]
    fn raw_message_notification_parsing() {
        let raw = r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":50}}"#;
        let msg: RawJsonRpcMessage = serde_json::from_str(raw).unwrap();

        assert!(msg.is_notification());
        assert_eq!(msg.method.as_deref(), Some("notifications/progress"));
        assert!(msg.id.is_none());

        let notif = msg.into_notification().unwrap();
        assert_eq!(notif.method, "notifications/progress");
        assert_eq!(notif.params["progress"], 50);
    }

    #[test]
    fn raw_message_response_parsing() {
        let raw = r#"{"jsonrpc":"2.0","id":3,"result":{"ok":true}}"#;
        let msg: RawJsonRpcMessage = serde_json::from_str(raw).unwrap();

        assert!(!msg.is_notification());
        assert_eq!(msg.id, Some(3));

        let resp = msg.into_response().unwrap();
        assert_eq!(resp.id, 3);
        assert!(resp.is_success());
        assert_eq!(resp.result.unwrap()["ok"], true);
    }

    #[test]
    fn raw_message_error_response_parsing() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": 7,
            "error": {
                "code": -32601,
                "message": "Method not found: unknown/method"
            }
        }"#;
        let msg: RawJsonRpcMessage = serde_json::from_str(raw).unwrap();

        assert!(!msg.is_notification());
        let resp = msg.into_response().unwrap();
        assert_eq!(resp.id, 7);
        assert!(!resp.is_success());
        assert_eq!(
            resp.error.as_ref().unwrap().code,
            error_codes::METHOD_NOT_FOUND
        );
        assert!(
            resp.error
                .as_ref()
                .unwrap()
                .message
                .contains("unknown/method")
        );
    }

    #[test]
    fn raw_message_notification_into_response_returns_none() {
        let raw = r#"{"jsonrpc":"2.0","method":"test/event","params":{}}"#;
        let msg: RawJsonRpcMessage = serde_json::from_str(raw).unwrap();

        assert!(msg.is_notification());
        // Trying to convert a notification into a response should return None.
        assert!(msg.into_response().is_none());
    }

    #[test]
    fn raw_message_response_into_notification_returns_none() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let msg: RawJsonRpcMessage = serde_json::from_str(raw).unwrap();

        assert!(!msg.is_notification());
        // A response has no method, so into_notification returns None.
        assert!(msg.into_notification().is_none());
    }

    // -- Malformed / edge-case parsing tests --

    #[test]
    fn parse_response_with_error_data_field() {
        let raw = r#"{
            "jsonrpc": "2.0",
            "id": 10,
            "error": {
                "code": -32603,
                "message": "Internal error",
                "data": {"detail": "stack trace here", "retryable": false}
            }
        }"#;
        let resp: JsonRpcResponse = serde_json::from_str(raw).unwrap();

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert_eq!(err.code, error_codes::INTERNAL_ERROR);
        assert_eq!(err.data.as_ref().unwrap()["retryable"], false);
        assert_eq!(err.data.as_ref().unwrap()["detail"], "stack trace here");
    }

    #[test]
    fn parse_list_tools_response_with_error() {
        let resp = JsonRpcResponse::error(1, JsonRpcError::internal("server overloaded"));

        let err = McpClient::parse_list_tools_response(&resp).unwrap_err();
        assert!(err.to_string().contains("server overloaded"));
    }

    #[test]
    fn parse_call_tool_response_success() {
        let resp = JsonRpcResponse::success(
            5,
            serde_json::json!({
                "content": [
                    {"type": "text", "text": "Hello, world!"}
                ]
            }),
        );

        let result = McpClient::parse_call_tool_response(&resp).unwrap();
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "Hello, world!");
    }

    #[test]
    fn parse_call_tool_response_missing_result() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: None,
            id: 1,
        };

        let err = McpClient::parse_call_tool_response(&resp).unwrap_err();
        assert!(err.to_string().contains("Missing result"));
    }

    #[test]
    fn parse_list_tools_response_missing_tools_array() {
        let resp = JsonRpcResponse::success(1, serde_json::json!({"other": "data"}));

        let err = McpClient::parse_list_tools_response(&resp).unwrap_err();
        assert!(err.to_string().contains("Missing 'tools' array"));
    }

    #[test]
    fn parse_list_tools_response_multiple_tools() {
        let resp = JsonRpcResponse::success(
            1,
            serde_json::json!({
                "tools": [
                    {
                        "name": "read_file",
                        "description": "Read a file from disk",
                        "inputSchema": {"type": "object"}
                    },
                    {
                        "name": "write_file",
                        "description": "Write content to a file",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"},
                                "content": {"type": "string"}
                            },
                            "required": ["path", "content"]
                        }
                    },
                    {
                        "name": "list_dir",
                        "description": "List directory contents",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"}
                            }
                        }
                    }
                ]
            }),
        );

        let tools = McpClient::parse_list_tools_response(&resp).unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[1].name, "write_file");
        assert_eq!(tools[2].name, "list_dir");
        assert_eq!(
            tools[1].input_schema["required"],
            serde_json::json!(["path", "content"])
        );
    }

    // -- Client construction tests --

    #[test]
    fn client_construction_and_config_access() {
        let config = sample_config();
        let client = McpClient::new(config.clone());

        assert_eq!(client.config().name, "test-server");
        assert_eq!(client.config().command.as_deref(), Some("mcp-server"));
        assert_eq!(client.config().args.len(), 2);
        assert!(client.config().enabled);
    }

    #[tokio::test]
    async fn client_not_connected_initially() {
        let client = McpClient::new(sample_config());
        assert!(!client.is_connected().await);
        assert!(client.server_info().await.is_none());
    }

    #[tokio::test]
    async fn list_tools_fails_when_not_connected() {
        let client = McpClient::new(sample_config());
        let result = client.list_tools().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    #[tokio::test]
    async fn call_tool_fails_when_not_connected() {
        let client = McpClient::new(sample_config());
        let result = client.call_tool("read_file", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    #[tokio::test]
    async fn disconnect_when_not_connected_is_noop() {
        let client = McpClient::new(sample_config());
        // Disconnecting when not connected should succeed silently.
        let result = client.disconnect().await;
        assert!(result.is_ok());
    }

    // -- Initialize request format verification --

    #[test]
    fn initialize_request_has_correct_protocol_version() {
        let client = McpClient::new(sample_config());
        let req = client.build_initialize_request();

        // Verify the full structure of the initialize request.
        assert_eq!(req.method, "initialize");
        assert_eq!(req.params["protocolVersion"], "2024-11-05");
        assert!(req.params["capabilities"].is_object());
        assert_eq!(req.params["clientInfo"]["name"], "hive-mcp-client");
        assert_eq!(req.params["clientInfo"]["version"], "0.1.0");
    }

    // -- JSON-RPC newline-delimited format verification --

    #[test]
    fn request_serializes_to_single_line() {
        let req = JsonRpcRequest::new(
            "tools/call",
            serde_json::json!({"name": "test", "arguments": {"key": "value"}}),
            42,
        );
        let serialized = serde_json::to_string(&req).unwrap();

        // Newline-delimited JSON must not contain embedded newlines.
        assert!(
            !serialized.contains('\n'),
            "Serialized request must be a single line for newline-delimited JSON"
        );
    }

    #[test]
    fn notification_serializes_to_single_line() {
        let notif = JsonRpcNotification::new(
            "notifications/initialized",
            serde_json::json!({"info": "test data with\nnewline in value"}),
        );
        // serde_json::to_string escapes embedded newlines, so the output line
        // itself should not contain a raw newline character.
        let serialized = serde_json::to_string(&notif).unwrap();
        assert!(
            !serialized.contains('\n'),
            "Serialized notification must be a single line"
        );
    }

    // -- Config missing command test --

    #[test]
    fn config_without_command_roundtrips() {
        let config = McpServerConfig {
            name: "no-command".into(),
            transport: McpTransport::Stdio,
            command: None,
            args: vec![],
            env: HashMap::new(),
            enabled: true,
        };

        let json_str = serde_json::to_string(&config).unwrap();
        let deserialized: McpServerConfig = serde_json::from_str(&json_str).unwrap();

        assert!(deserialized.command.is_none());
        assert_eq!(deserialized.name, "no-command");
    }

    // -- Config deserialization with defaults --

    #[test]
    fn config_deserialization_applies_defaults() {
        let raw = r#"{
            "name": "minimal",
            "transport": {"type": "stdio"}
        }"#;

        let config: McpServerConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(config.name, "minimal");
        assert!(config.enabled); // default_true
        assert!(config.args.is_empty()); // default empty vec
        assert!(config.env.is_empty()); // default empty map
        assert!(config.command.is_none());
    }
}
