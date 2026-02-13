//! MCP Server — built-in Model Context Protocol tool server.
//!
//! Exposes a set of local tools (file I/O, shell execution, git, search) via
//! the JSON-RPC 2.0 protocol defined by MCP. Tool handlers delegate to the
//! workspace runtime services: `hive_fs` for file/search/git operations and
//! `hive_terminal` for shell command execution (with SecurityGateway validation).

use crate::mcp_client::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpTool, error_codes};
use hive_fs::{FileService, GitService, SearchOptions, SearchService};
use hive_terminal::CommandExecutor;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::runtime::Handle;

// ---------------------------------------------------------------------------
// Tool handler type
// ---------------------------------------------------------------------------

/// An async-compatible tool handler.
///
/// Takes the `arguments` value from a `tools/call` request and returns either
/// a JSON result or an error string.
pub type ToolHandler =
    Box<dyn Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync>;

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// A built-in MCP server that hosts local workspace tools.
///
/// Holds shared references to backend services so that tool handlers can
/// delegate to real implementations rather than returning stubs.
pub struct McpServer {
    tools: HashMap<String, (McpTool, ToolHandler)>,
}

impl McpServer {
    /// Create a new server backed by the given workspace root.
    ///
    /// The `workspace_root` is used to resolve relative paths for file
    /// operations and as the default working directory for command execution.
    pub fn new(workspace_root: PathBuf) -> Self {
        let mut server = Self {
            tools: HashMap::new(),
        };
        server.register_builtins(workspace_root);
        server
    }

    /// Register a tool with its definition and handler.
    pub fn register(&mut self, tool: McpTool, handler: ToolHandler) {
        self.tools.insert(tool.name.clone(), (tool, handler));
    }

    /// List all available tools.
    pub fn list_tools(&self) -> Vec<&McpTool> {
        let mut tools: Vec<_> = self.tools.values().map(|(def, _)| def).collect();
        tools.sort_by_key(|t| &t.name);
        tools
    }

    /// Handle a JSON-RPC request and return a response.
    pub fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => JsonRpcResponse::success(
                request.id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "hive-mcp-server",
                        "version": "0.1.0"
                    }
                }),
            ),

            "tools/list" => {
                let tools: Vec<serde_json::Value> = self
                    .list_tools()
                    .iter()
                    .map(|t| serde_json::to_value(t).unwrap_or(json!(null)))
                    .collect();

                JsonRpcResponse::success(request.id, json!({ "tools": tools }))
            }

            "tools/call" => self.handle_tool_call(request),

            _ => {
                JsonRpcResponse::error(request.id, JsonRpcError::method_not_found(&request.method))
            }
        }
    }

    /// Dispatch a tools/call request to the appropriate handler.
    fn handle_tool_call(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let name = match request.params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return JsonRpcResponse::error(
                    request.id,
                    JsonRpcError::invalid_params("missing 'name' in tools/call"),
                );
            }
        };

        let args = request
            .params
            .get("arguments")
            .cloned()
            .unwrap_or(json!({}));

        match self.tools.get(name) {
            Some((_, handler)) => match handler(args) {
                Ok(result) => {
                    // MCP content text must be a string. If the handler returned
                    // a JSON string, unwrap it; otherwise serialize the value.
                    let text = match result {
                        serde_json::Value::String(s) => s,
                        other => serde_json::to_string(&other).unwrap_or_default(),
                    };
                    JsonRpcResponse::success(
                        request.id,
                        json!({
                            "content": [{ "type": "text", "text": text }]
                        }),
                    )
                }
                Err(msg) => JsonRpcResponse::error(
                    request.id,
                    JsonRpcError {
                        code: error_codes::INTERNAL_ERROR,
                        message: msg,
                        data: None,
                    },
                ),
            },
            None => JsonRpcResponse::error(
                request.id,
                JsonRpcError {
                    code: error_codes::METHOD_NOT_FOUND,
                    message: format!("Unknown tool: {name}"),
                    data: None,
                },
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Built-in tool registration
    // -----------------------------------------------------------------------

    fn register_builtins(&mut self, workspace_root: PathBuf) {
        let root = Arc::new(workspace_root);

        // -- read_file -------------------------------------------------------
        {
            let root = Arc::clone(&root);
            self.register(
                McpTool {
                    name: "read_file".into(),
                    description: "Read the contents of a file at the given path.".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Absolute or relative file path" }
                        },
                        "required": ["path"]
                    }),
                },
                Box::new(move |args| {
                    let path_str = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing required argument 'path'")?;
                    let path = resolve_path(&root, path_str);
                    let content = FileService::read_file(&path)
                        .map_err(|e| format!("Failed to read file: {e}"))?;
                    Ok(json!(content))
                }),
            );
        }

        // -- write_file ------------------------------------------------------
        {
            let root = Arc::clone(&root);
            self.register(
                McpTool {
                    name: "write_file".into(),
                    description: "Write content to a file at the given path.".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Absolute or relative file path" },
                            "content": { "type": "string", "description": "Content to write" }
                        },
                        "required": ["path", "content"]
                    }),
                },
                Box::new(move |args| {
                    let path_str = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing required argument 'path'")?;
                    let content = args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing required argument 'content'")?;
                    let path = resolve_path(&root, path_str);
                    FileService::write_file(&path, content)
                        .map_err(|e| format!("Failed to write file: {e}"))?;
                    Ok(json!(format!(
                        "Wrote {} bytes to {}",
                        content.len(),
                        path.display()
                    )))
                }),
            );
        }

        // -- execute_command -------------------------------------------------
        {
            let root = Arc::clone(&root);
            self.register(
                McpTool {
                    name: "execute_command".into(),
                    description: "Execute a shell command (validated by SecurityGateway).".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "command": { "type": "string", "description": "Shell command to execute" },
                            "cwd": { "type": "string", "description": "Working directory (optional)" }
                        },
                        "required": ["command"]
                    }),
                },
                Box::new(move |args| {
                    let command = args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing required argument 'command'")?;
                    let cwd_str = args.get("cwd").and_then(|v| v.as_str());
                    let working_dir = match cwd_str {
                        Some(dir) => resolve_path(&root, dir),
                        None => root.as_ref().clone(),
                    };
                    let executor = CommandExecutor::new(working_dir)
                        .map_err(|e| format!("Invalid working directory: {e}"))?;

                    // Execute the command on the tokio runtime.
                    let output = execute_command_blocking(&executor, command)?;
                    Ok(json!({
                        "stdout": output.stdout,
                        "stderr": output.stderr,
                        "exit_code": output.exit_code,
                        "duration_ms": output.duration.as_millis() as u64
                    }))
                }),
            );
        }

        // -- search_files ----------------------------------------------------
        {
            let root = Arc::clone(&root);
            self.register(
                McpTool {
                    name: "search_files".into(),
                    description: "Search for a regex pattern across files in a directory.".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "pattern": { "type": "string", "description": "Regex pattern to search for" },
                            "path": { "type": "string", "description": "Directory to search in" },
                            "file_pattern": { "type": "string", "description": "Glob pattern to filter files (e.g. *.rs)" },
                            "max_results": { "type": "integer", "description": "Maximum number of results (default 100)" }
                        },
                        "required": ["pattern"]
                    }),
                },
                Box::new(move |args| {
                    let pattern = args
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing required argument 'pattern'")?;
                    let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let search_root = resolve_path(&root, path_str);

                    let file_pattern = args
                        .get("file_pattern")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let max_results = args
                        .get("max_results")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(100) as usize;

                    let options = SearchOptions {
                        file_pattern,
                        max_results,
                        ..Default::default()
                    };

                    let results = SearchService::search(&search_root, pattern, options)
                        .map_err(|e| format!("Search failed: {e}"))?;

                    let matches: Vec<serde_json::Value> = results
                        .iter()
                        .map(|r| {
                            json!({
                                "path": r.path.display().to_string(),
                                "line": r.line_number,
                                "content": r.line_content,
                                "match_start": r.match_start,
                                "match_end": r.match_end,
                            })
                        })
                        .collect();

                    Ok(json!({
                        "total": matches.len(),
                        "matches": matches
                    }))
                }),
            );
        }

        // -- list_files ------------------------------------------------------
        {
            let root = Arc::clone(&root);
            self.register(
                McpTool {
                    name: "list_files".into(),
                    description: "List files and directories at the given path.".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Directory path to list" }
                        },
                        "required": ["path"]
                    }),
                },
                Box::new(move |args| {
                    let path_str = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing required argument 'path'")?;
                    let path = resolve_path(&root, path_str);
                    let entries = FileService::list_dir(&path)
                        .map_err(|e| format!("Failed to list directory: {e}"))?;

                    let items: Vec<serde_json::Value> = entries
                        .iter()
                        .map(|e| {
                            json!({
                                "name": e.name,
                                "path": e.path.display().to_string(),
                                "is_dir": e.is_dir,
                                "size": e.size,
                            })
                        })
                        .collect();

                    Ok(json!({
                        "total": items.len(),
                        "entries": items
                    }))
                }),
            );
        }

        // -- git_status ------------------------------------------------------
        {
            let root = Arc::clone(&root);
            self.register(
                McpTool {
                    name: "git_status".into(),
                    description: "Return git status of the working directory.".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "cwd": { "type": "string", "description": "Repository working directory (optional)" }
                        }
                    }),
                },
                Box::new(move |args| {
                    let cwd_str = args.get("cwd").and_then(|v| v.as_str());
                    let repo_path = match cwd_str {
                        Some(dir) => resolve_path(&root, dir),
                        None => root.as_ref().clone(),
                    };

                    let git = GitService::open(&repo_path)
                        .map_err(|e| format!("Failed to open git repo: {e}"))?;
                    let statuses = git
                        .status()
                        .map_err(|e| format!("Failed to get git status: {e}"))?;

                    let branch = git.current_branch().unwrap_or_else(|_| "unknown".into());

                    let files: Vec<serde_json::Value> = statuses
                        .iter()
                        .map(|s| {
                            json!({
                                "path": s.path.display().to_string(),
                                "status": format!("{:?}", s.status),
                            })
                        })
                        .collect();

                    Ok(json!({
                        "branch": branch,
                        "total": files.len(),
                        "files": files
                    }))
                }),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve a path string relative to the workspace root, or as absolute.
fn resolve_path(root: &Path, path_str: &str) -> PathBuf {
    let path = Path::new(path_str);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Execute a command via `CommandExecutor` by blocking on the tokio runtime.
///
/// This bridges the sync tool handler signature with the async executor.
fn execute_command_blocking(
    executor: &CommandExecutor,
    command: &str,
) -> Result<hive_terminal::CommandOutput, String> {
    // Try to get a handle to the current tokio runtime.
    match Handle::try_current() {
        Ok(handle) => {
            // We're inside a tokio runtime but in a sync context.
            // Use block_in_place to avoid blocking the executor thread.
            let cmd = command.to_string();
            std::thread::scope(|s| {
                s.spawn(|| {
                    handle.block_on(async {
                        executor
                            .execute(&cmd)
                            .await
                            .map_err(|e| format!("Command execution failed: {e}"))
                    })
                })
                .join()
                .map_err(|_| "Command execution thread panicked".to_string())?
            })
        }
        Err(_) => {
            // No tokio runtime — create a temporary one for the call.
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("Failed to create runtime: {e}"))?;
            rt.block_on(executor.execute(command))
                .map_err(|e| format!("Command execution failed: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_client::JsonRpcRequest;
    use std::fs;
    use tempfile::TempDir;

    fn make_request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest::new(method, params, 1)
    }

    fn setup_workspace() -> (TempDir, McpServer) {
        let dir = TempDir::new().unwrap();
        let server = McpServer::new(dir.path().to_path_buf());
        (dir, server)
    }

    // -- Tool listing tests --

    #[test]
    fn list_tools_returns_all_builtins() {
        let (_dir, server) = setup_workspace();
        let tools = server.list_tools();

        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"execute_command"));
        assert!(names.contains(&"search_files"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"git_status"));
    }

    #[test]
    fn list_tools_sorted_by_name() {
        let (_dir, server) = setup_workspace();
        let tools = server.list_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn list_tools_via_jsonrpc() {
        let (_dir, server) = setup_workspace();
        let req = make_request("tools/list", json!({}));
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);
    }

    // -- Initialize tests --

    #[test]
    fn initialize_response() {
        let (_dir, server) = setup_workspace();
        let req = make_request("initialize", json!({}));
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "hive-mcp-server");
    }

    // -- read_file tests --

    #[test]
    fn read_file_success() {
        let (dir, server) = setup_workspace();
        let file = dir.path().join("hello.txt");
        fs::write(&file, "hello world").unwrap();

        let req = make_request(
            "tools/call",
            json!({
                "name": "read_file",
                "arguments": { "path": file.to_str().unwrap() }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let content = &resp.result.unwrap()["content"][0]["text"];
        assert_eq!(content.as_str().unwrap(), "hello world");
    }

    #[test]
    fn read_file_relative_path() {
        let (dir, server) = setup_workspace();
        fs::write(dir.path().join("relative.txt"), "relative content").unwrap();

        let req = make_request(
            "tools/call",
            json!({
                "name": "read_file",
                "arguments": { "path": "relative.txt" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let content = &resp.result.unwrap()["content"][0]["text"];
        assert_eq!(content.as_str().unwrap(), "relative content");
    }

    #[test]
    fn read_file_not_found_returns_error() {
        let (_dir, server) = setup_workspace();

        let req = make_request(
            "tools/call",
            json!({
                "name": "read_file",
                "arguments": { "path": "nonexistent.txt" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(err.message.contains("Failed to read file"));
    }

    #[test]
    fn read_file_missing_path_arg() {
        let (_dir, server) = setup_workspace();

        let req = make_request(
            "tools/call",
            json!({
                "name": "read_file",
                "arguments": {}
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(err.message.contains("path"));
    }

    // -- write_file tests --

    #[test]
    fn write_file_creates_and_writes() {
        let (dir, server) = setup_workspace();
        let file = dir.path().join("output.txt");

        let req = make_request(
            "tools/call",
            json!({
                "name": "write_file",
                "arguments": {
                    "path": file.to_str().unwrap(),
                    "content": "written content"
                }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let text = resp.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.contains("15 bytes"));
        assert!(file.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "written content");
    }

    #[test]
    fn write_file_relative_path() {
        let (dir, server) = setup_workspace();

        let req = make_request(
            "tools/call",
            json!({
                "name": "write_file",
                "arguments": {
                    "path": "rel_write.txt",
                    "content": "relative write"
                }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let file = dir.path().join("rel_write.txt");
        assert!(file.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "relative write");
    }

    #[test]
    fn write_file_missing_content_arg() {
        let (_dir, server) = setup_workspace();

        let req = make_request(
            "tools/call",
            json!({
                "name": "write_file",
                "arguments": { "path": "test.txt" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(err.message.contains("content"));
    }

    // -- list_files tests --

    #[test]
    fn list_files_returns_entries() {
        let (dir, server) = setup_workspace();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        let req = make_request(
            "tools/call",
            json!({
                "name": "list_files",
                "arguments": { "path": dir.path().to_str().unwrap() }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert_eq!(parsed["total"], 3);
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 3);
        // Entries are sorted by name
        assert_eq!(entries[0]["name"], "a.txt");
        assert_eq!(entries[0]["is_dir"], false);
        assert_eq!(entries[2]["name"], "sub");
        assert_eq!(entries[2]["is_dir"], true);
    }

    #[test]
    fn list_files_relative_path() {
        let (dir, server) = setup_workspace();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("file.txt"), "content").unwrap();

        let req = make_request(
            "tools/call",
            json!({
                "name": "list_files",
                "arguments": { "path": "subdir" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert_eq!(parsed["total"], 1);
    }

    #[test]
    fn list_files_nonexistent_returns_error() {
        let (_dir, server) = setup_workspace();

        let req = make_request(
            "tools/call",
            json!({
                "name": "list_files",
                "arguments": { "path": "no_such_dir" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(err.message.contains("Failed to list directory"));
    }

    // -- search_files tests --

    #[test]
    fn search_files_finds_matches() {
        let (dir, server) = setup_workspace();
        fs::write(
            dir.path().join("code.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("lib.rs"),
            "fn helper() {\n    println!(\"world\");\n}\n",
        )
        .unwrap();

        let req = make_request(
            "tools/call",
            json!({
                "name": "search_files",
                "arguments": {
                    "pattern": "println",
                    "path": dir.path().to_str().unwrap()
                }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert_eq!(parsed["total"], 2);
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches[0]["content"].as_str().unwrap().contains("println"));
    }

    #[test]
    fn search_files_default_path() {
        let (dir, server) = setup_workspace();
        fs::write(dir.path().join("data.txt"), "TODO: fix this").unwrap();

        let req = make_request(
            "tools/call",
            json!({
                "name": "search_files",
                "arguments": { "pattern": "TODO" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert!(parsed["total"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn search_files_invalid_regex_returns_error() {
        let (_dir, server) = setup_workspace();

        let req = make_request(
            "tools/call",
            json!({
                "name": "search_files",
                "arguments": { "pattern": "[invalid" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(err.message.contains("Search failed"));
    }

    #[test]
    fn search_files_with_max_results() {
        let (dir, server) = setup_workspace();
        // Create a file with multiple matches
        let content = "line1: match\nline2: match\nline3: match\nline4: match\n";
        fs::write(dir.path().join("multi.txt"), content).unwrap();

        let req = make_request(
            "tools/call",
            json!({
                "name": "search_files",
                "arguments": {
                    "pattern": "match",
                    "max_results": 2
                }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert_eq!(parsed["total"], 2);
    }

    // -- git_status tests --

    #[test]
    fn git_status_in_repo() {
        let dir = TempDir::new().unwrap();
        // Initialize a git repo in the temp dir
        let _git = GitService::init(dir.path()).unwrap();

        // Create an untracked file
        fs::write(dir.path().join("new.txt"), "new file").unwrap();

        let server = McpServer::new(dir.path().to_path_buf());
        let req = make_request(
            "tools/call",
            json!({
                "name": "git_status",
                "arguments": {}
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert!(parsed["total"].as_u64().unwrap() >= 1);
        let files = parsed["files"].as_array().unwrap();
        assert!(files.iter().any(|f| {
            f["path"].as_str().unwrap().contains("new.txt")
                && f["status"].as_str().unwrap() == "Untracked"
        }));
    }

    #[test]
    fn git_status_not_a_repo_returns_error() {
        let dir = TempDir::new().unwrap();
        let server = McpServer::new(dir.path().to_path_buf());

        let req = make_request(
            "tools/call",
            json!({
                "name": "git_status",
                "arguments": {}
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(err.message.contains("Failed to open git repo"));
    }

    // -- execute_command tests --

    #[tokio::test]
    async fn execute_command_echo() {
        let dir = TempDir::new().unwrap();
        let server = McpServer::new(dir.path().to_path_buf());

        let req = make_request(
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": { "command": "echo hello_mcp" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert!(parsed["stdout"].as_str().unwrap().contains("hello_mcp"));
        assert_eq!(parsed["exit_code"], 0);
    }

    #[tokio::test]
    async fn execute_command_blocked_by_security() {
        let dir = TempDir::new().unwrap();
        let server = McpServer::new(dir.path().to_path_buf());

        let req = make_request(
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": { "command": "rm -rf /" }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(
            err.message.contains("Blocked") || err.message.contains("failed"),
            "Expected security rejection, got: {}",
            err.message
        );
    }

    #[tokio::test]
    async fn execute_command_with_cwd() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("workdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("marker.txt"), "exists").unwrap();

        let server = McpServer::new(dir.path().to_path_buf());

        let cmd = if cfg!(target_os = "windows") {
            "dir /b"
        } else {
            "ls"
        };

        let req = make_request(
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "command": cmd,
                    "cwd": sub.to_str().unwrap()
                }
            }),
        );
        let resp = server.handle_request(&req);

        assert!(resp.is_success());
        let result = resp.result.unwrap();
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert!(parsed["stdout"].as_str().unwrap().contains("marker.txt"));
    }

    // -- Error handling tests --

    #[test]
    fn unknown_method_returns_error() {
        let (_dir, server) = setup_workspace();
        let req = make_request("unknown/method", json!({}));
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert_eq!(err.code, error_codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("unknown/method"));
    }

    #[test]
    fn unknown_tool_returns_error() {
        let (_dir, server) = setup_workspace();
        let req = make_request(
            "tools/call",
            json!({
                "name": "nonexistent_tool",
                "arguments": {}
            }),
        );
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert!(err.message.contains("nonexistent_tool"));
    }

    #[test]
    fn missing_tool_name_returns_invalid_params() {
        let (_dir, server) = setup_workspace();
        let req = make_request("tools/call", json!({}));
        let resp = server.handle_request(&req);

        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
        assert!(err.message.contains("name"));
    }

    // -- Custom tool registration test --

    #[test]
    fn register_custom_tool() {
        let (_dir, mut server) = setup_workspace();
        let initial_count = server.list_tools().len();

        server.register(
            McpTool {
                name: "custom_tool".into(),
                description: "A custom test tool".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string" }
                    }
                }),
            },
            Box::new(|args| {
                let input = args
                    .get("input")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                Ok(json!(format!("Custom: {input}")))
            }),
        );

        assert_eq!(server.list_tools().len(), initial_count + 1);

        let req = make_request(
            "tools/call",
            json!({
                "name": "custom_tool",
                "arguments": { "input": "hello" }
            }),
        );
        let resp = server.handle_request(&req);
        assert!(resp.is_success());
    }

    // -- resolve_path tests --

    #[test]
    fn resolve_absolute_path_unchanged() {
        let root = PathBuf::from("/workspace");
        let result = resolve_path(&root, "/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn resolve_relative_path_joins_root() {
        let root = PathBuf::from("/workspace");
        let result = resolve_path(&root, "relative/path");
        assert_eq!(result, PathBuf::from("/workspace/relative/path"));
    }

    #[test]
    fn resolve_dot_path() {
        let root = PathBuf::from("/workspace");
        let result = resolve_path(&root, ".");
        assert_eq!(result, PathBuf::from("/workspace/."));
    }
}
