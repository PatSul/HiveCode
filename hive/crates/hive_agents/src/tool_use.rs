//! Tool-use parser and executor -- extracts tool calls from AI responses,
//! dispatches them, and formats results for the next turn.

use anyhow::Result;
use hive_core::SecurityGateway;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A tool call extracted from an AI response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// The result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

/// A tool definition that can be advertised to the AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

// ---------------------------------------------------------------------------
// ToolHandler trait
// ---------------------------------------------------------------------------

/// Trait for implementing a tool that can be invoked by an AI agent.
///
/// Each tool declares its name, description, and JSON schema for parameters,
/// then provides an `execute` method that receives the parsed arguments.
pub trait ToolHandler: Send + Sync {
    /// The tool's unique name (must match what the AI calls).
    fn name(&self) -> &str;

    /// Human-readable description of what the tool does.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's expected parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given JSON arguments.
    /// Returns the output string on success, or an error message.
    fn execute(&self, args: serde_json::Value) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Built-in tool handlers
// ---------------------------------------------------------------------------

/// Reads a file's contents given a `path` argument.
pub struct ReadFileTool;

impl ToolHandler for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to read" }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> Result<String, String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: path".to_string())?;

        let path = Path::new(path_str);
        hive_fs::FileService::read_file(path).map_err(|e| format!("{e}"))
    }
}

/// Writes content to a file given `path` and `content` arguments.
pub struct WriteFileTool;

impl ToolHandler for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file at the given path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The file path to write" },
                "content": { "type": "string", "description": "The content to write" }
            },
            "required": ["path", "content"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> Result<String, String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: path".to_string())?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: content".to_string())?;

        let path = Path::new(path_str);
        hive_fs::FileService::write_file(path, content).map_err(|e| format!("{e}"))?;
        Ok(format!(
            "Successfully wrote {} bytes to {path_str}",
            content.len()
        ))
    }
}

/// Lists files and directories at the given `path`.
pub struct ListDirectoryTool;

impl ToolHandler for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories at the given path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The directory path to list" }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> Result<String, String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: path".to_string())?;

        let path = Path::new(path_str);
        let entries = hive_fs::FileService::list_dir(path).map_err(|e| format!("{e}"))?;

        let mut output = String::new();
        for entry in &entries {
            let kind = if entry.is_dir { "dir " } else { "file" };
            output.push_str(&format!("{kind}  {}\n", entry.name));
        }
        if output.is_empty() {
            output.push_str("(empty directory)\n");
        }
        Ok(output)
    }
}

/// Searches for a regex pattern across files in a directory.
pub struct SearchFilesTool;

impl ToolHandler for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for a regex pattern across files in a directory."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "The search pattern (regex)" },
                "path": { "type": "string", "description": "The directory to search in" },
                "max_results": { "type": "integer", "description": "Maximum number of results (default 50)" }
            },
            "required": ["pattern"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> Result<String, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: pattern".to_string())?;
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let path = Path::new(path_str);
        let options = hive_fs::SearchOptions {
            max_results,
            ..Default::default()
        };

        let results =
            hive_fs::SearchService::search(path, pattern, options).map_err(|e| format!("{e}"))?;

        let mut output = String::new();
        for result in &results {
            output.push_str(&format!(
                "{}:{}: {}\n",
                result.path.display(),
                result.line_number,
                result.line_content,
            ));
        }
        if output.is_empty() {
            output.push_str("No matches found.\n");
        }
        Ok(output)
    }
}

/// Executes a shell command through the SecurityGateway.
pub struct ExecuteCommandTool {
    security: SecurityGateway,
}

impl Default for ExecuteCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecuteCommandTool {
    pub fn new() -> Self {
        Self {
            security: SecurityGateway::new(),
        }
    }
}

impl ToolHandler for ExecuteCommandTool {
    fn name(&self) -> &str {
        "execute_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" }
            },
            "required": ["command"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> Result<String, String> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: command".to_string())?;

        self.security.check_command(command)?;

        let output = if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .arg("/c")
                .arg(command)
                .output()
        } else {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
        };

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str("[stderr] ");
                    result.push_str(&stderr);
                }
                if result.is_empty() {
                    result.push_str(&format!("(exit code {})", out.status.code().unwrap_or(-1)));
                }
                Ok(result)
            }
            Err(e) => Err(format!("Failed to execute command: {e}")),
        }
    }
}

/// Shows git status for the repository at a given path.
pub struct GitStatusTool;

impl ToolHandler for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Show the git status (modified, added, deleted, untracked files) for a repository."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the git repository root" }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> Result<String, String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: path".to_string())?;

        let path = Path::new(path_str);
        let git = hive_fs::GitService::open(path).map_err(|e| format!("{e}"))?;
        let statuses = git.status().map_err(|e| format!("{e}"))?;

        if statuses.is_empty() {
            return Ok("Working tree clean.\n".to_string());
        }

        let mut output = String::new();
        for entry in &statuses {
            let status_label = match entry.status {
                hive_fs::FileStatusType::Modified => "modified",
                hive_fs::FileStatusType::Added => "added",
                hive_fs::FileStatusType::Deleted => "deleted",
                hive_fs::FileStatusType::Renamed => "renamed",
                hive_fs::FileStatusType::Untracked => "untracked",
            };
            output.push_str(&format!("{status_label}: {}\n", entry.path.display()));
        }
        Ok(output)
    }
}

/// Shows the git diff for the repository at a given path.
pub struct GitDiffTool;

impl ToolHandler for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show the git diff (working directory changes) for a repository."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the git repository root" }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> Result<String, String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required argument: path".to_string())?;

        let path = Path::new(path_str);
        let git = hive_fs::GitService::open(path).map_err(|e| format!("{e}"))?;
        let diff = git.diff().map_err(|e| format!("{e}"))?;

        if diff.is_empty() {
            return Ok("No changes.\n".to_string());
        }
        Ok(diff)
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Extract tool calls from an Anthropic-style response content array.
///
/// Anthropic responses contain `content` blocks with `type: "tool_use"`.
/// This function filters and parses those blocks.
pub fn extract_tool_calls(content_blocks: &[serde_json::Value]) -> Vec<ToolCall> {
    content_blocks
        .iter()
        .filter_map(|block| {
            let typ = block.get("type")?.as_str()?;
            if typ != "tool_use" {
                return None;
            }
            Some(ToolCall {
                id: block.get("id")?.as_str()?.to_string(),
                name: block.get("name")?.as_str()?.to_string(),
                input: block
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            })
        })
        .collect()
}

/// Extract tool calls from an OpenAI-style response.
///
/// OpenAI uses `tool_calls` array with `function.name` and `function.arguments`.
pub fn extract_openai_tool_calls(tool_calls: &[serde_json::Value]) -> Vec<ToolCall> {
    tool_calls
        .iter()
        .filter_map(|tc| {
            let id = tc.get("id")?.as_str()?.to_string();
            let function = tc.get("function")?;
            let name = function.get("name")?.as_str()?.to_string();
            let args_str = function.get("arguments")?.as_str()?;
            let input: serde_json::Value = serde_json::from_str(args_str).ok()?;
            Some(ToolCall { id, name, input })
        })
        .collect()
}

/// Check if a response contains any tool calls (Anthropic format).
pub fn has_tool_use(content_blocks: &[serde_json::Value]) -> bool {
    content_blocks
        .iter()
        .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
}

/// Parse tool calls from a raw AI response JSON value.
///
/// Detects the format automatically:
/// - Anthropic: looks for `content` array with `type: "tool_use"` blocks
/// - OpenAI/generic: looks for `tool_calls` array with `function` objects
///
/// Returns an empty vec if no tool calls are found.
pub fn parse_tool_calls_from_response(response: &serde_json::Value) -> Vec<ToolCall> {
    // Try Anthropic format: { "content": [ { "type": "tool_use", ... } ] }
    if let Some(content) = response.get("content").and_then(|v| v.as_array()) {
        let calls = extract_tool_calls(content);
        if !calls.is_empty() {
            return calls;
        }
    }

    // Try OpenAI format: { "choices": [{ "message": { "tool_calls": [...] } }] }
    if let Some(choices) = response.get("choices").and_then(|v| v.as_array())
        && let Some(first) = choices.first()
            && let Some(tool_calls) = first
                .get("message")
                .and_then(|m| m.get("tool_calls"))
                .and_then(|tc| tc.as_array())
            {
                let calls = extract_openai_tool_calls(tool_calls);
                if !calls.is_empty() {
                    return calls;
                }
            }

    // Try flat `tool_calls` at top level (generic format)
    if let Some(tool_calls) = response.get("tool_calls").and_then(|v| v.as_array()) {
        let calls = extract_openai_tool_calls(tool_calls);
        if !calls.is_empty() {
            return calls;
        }
    }

    Vec::new()
}

// ---------------------------------------------------------------------------
// Result formatting
// ---------------------------------------------------------------------------

/// Format a tool result for Anthropic's API (as a user message content block).
pub fn format_anthropic_result(result: &ToolResult) -> serde_json::Value {
    serde_json::json!({
        "type": "tool_result",
        "tool_use_id": result.tool_use_id,
        "content": result.content,
        "is_error": result.is_error,
    })
}

/// Format tool results as an OpenAI-style tool message.
pub fn format_openai_result(result: &ToolResult) -> serde_json::Value {
    serde_json::json!({
        "role": "tool",
        "tool_call_id": result.tool_use_id,
        "content": result.content,
    })
}

// ---------------------------------------------------------------------------
// Tool registry
// ---------------------------------------------------------------------------

/// A closure-based handler function for a tool.
pub type ToolHandlerFn = Box<dyn Fn(serde_json::Value) -> Result<String> + Send + Sync>;

/// Wrapper enum so the registry can hold both closure-based and trait-based handlers.
enum HandlerKind {
    Closure(ToolHandlerFn),
    Trait(Box<dyn ToolHandler>),
}

/// Registry of available tools and their handlers.
pub struct ToolRegistry {
    tools: HashMap<String, (ToolDefinition, HandlerKind)>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool with its definition and a closure handler.
    pub fn register<F>(&mut self, definition: ToolDefinition, handler: F)
    where
        F: Fn(serde_json::Value) -> Result<String> + Send + Sync + 'static,
    {
        let name = definition.name.clone();
        self.tools
            .insert(name, (definition, HandlerKind::Closure(Box::new(handler))));
    }

    /// Register a tool that implements the `ToolHandler` trait.
    ///
    /// The definition is derived from the trait methods automatically.
    pub fn register_tool(&mut self, handler: Box<dyn ToolHandler>) {
        let definition = ToolDefinition {
            name: handler.name().to_string(),
            description: handler.description().to_string(),
            input_schema: handler.parameters_schema(),
        };
        let name = definition.name.clone();
        self.tools
            .insert(name, (definition, HandlerKind::Trait(handler)));
    }

    /// Get all tool definitions (for advertising to the AI).
    pub fn definitions(&self) -> Vec<&ToolDefinition> {
        self.tools.values().map(|(def, _)| def).collect()
    }

    /// Execute a tool call and return the result.
    pub fn execute(&self, call: &ToolCall) -> ToolResult {
        match self.tools.get(&call.name) {
            Some((_, handler)) => {
                let result = match handler {
                    HandlerKind::Closure(f) => f(call.input.clone()).map_err(|e| format!("{e}")),
                    HandlerKind::Trait(t) => t.execute(call.input.clone()),
                };
                match result {
                    Ok(content) => ToolResult {
                        tool_use_id: call.id.clone(),
                        content,
                        is_error: false,
                    },
                    Err(e) => ToolResult {
                        tool_use_id: call.id.clone(),
                        content: format!("Error: {e}"),
                        is_error: true,
                    },
                }
            }
            None => ToolResult {
                tool_use_id: call.id.clone(),
                content: format!("Unknown tool: {}", call.name),
                is_error: true,
            },
        }
    }

    /// Execute all tool calls and return results.
    pub fn execute_all(&self, calls: &[ToolCall]) -> Vec<ToolResult> {
        calls.iter().map(|c| self.execute(c)).collect()
    }

    /// Check if a tool is registered.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Built-in tool definitions
// ---------------------------------------------------------------------------

/// Create tool definitions for the built-in tools.
pub fn builtin_tool_definitions() -> Vec<ToolDefinition> {
    let handlers: Vec<Box<dyn ToolHandler>> = vec![
        Box::new(ReadFileTool),
        Box::new(WriteFileTool),
        Box::new(ListDirectoryTool),
        Box::new(SearchFilesTool),
        Box::new(ExecuteCommandTool::new()),
        Box::new(GitStatusTool),
        Box::new(GitDiffTool),
    ];

    handlers
        .iter()
        .map(|h| ToolDefinition {
            name: h.name().to_string(),
            description: h.description().to_string(),
            input_schema: h.parameters_schema(),
        })
        .collect()
}

/// Create a `ToolRegistry` pre-loaded with all built-in tool handlers.
pub fn builtin_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register_tool(Box::new(ReadFileTool));
    registry.register_tool(Box::new(WriteFileTool));
    registry.register_tool(Box::new(ListDirectoryTool));
    registry.register_tool(Box::new(SearchFilesTool));
    registry.register_tool(Box::new(ExecuteCommandTool::new()));
    registry.register_tool(Box::new(GitStatusTool));
    registry.register_tool(Box::new(GitDiffTool));
    registry
}

// ---------------------------------------------------------------------------
// Tool executor
// ---------------------------------------------------------------------------

/// Drives the tool-use execution loop.
///
/// Given a raw AI response, the executor:
/// 1. Parses tool calls from the response (auto-detecting Anthropic or OpenAI format)
/// 2. Executes each tool via the registry
/// 3. Returns formatted results ready to be sent back to the AI
/// 4. Tracks iteration count to enforce `max_iterations`
pub struct ToolExecutor {
    registry: ToolRegistry,
    max_iterations: usize,
    current_iteration: usize,
    total_calls: usize,
}

impl ToolExecutor {
    pub fn new(registry: ToolRegistry, max_iterations: usize) -> Self {
        Self {
            registry,
            max_iterations,
            current_iteration: 0,
            total_calls: 0,
        }
    }

    /// Process a raw AI response JSON value.
    ///
    /// Returns `Some(results)` if tool calls were found and executed,
    /// or `None` if there were no tool calls (meaning the AI is done)
    /// or the iteration limit has been reached.
    pub fn process_response(&mut self, response: &serde_json::Value) -> Option<Vec<ToolResult>> {
        if self.current_iteration >= self.max_iterations {
            debug!(
                iteration = self.current_iteration,
                max = self.max_iterations,
                "Tool executor reached max iterations"
            );
            return None;
        }

        let calls = parse_tool_calls_from_response(response);
        if calls.is_empty() {
            return None;
        }

        self.current_iteration += 1;
        self.total_calls += calls.len();

        debug!(
            iteration = self.current_iteration,
            num_calls = calls.len(),
            "Executing tool calls"
        );

        let results = self.registry.execute_all(&calls);
        Some(results)
    }

    /// Format results for Anthropic's API (array of tool_result content blocks).
    pub fn format_results_anthropic(results: &[ToolResult]) -> Vec<serde_json::Value> {
        results.iter().map(format_anthropic_result).collect()
    }

    /// Format results for OpenAI's API (array of tool messages).
    pub fn format_results_openai(results: &[ToolResult]) -> Vec<serde_json::Value> {
        results.iter().map(format_openai_result).collect()
    }

    /// Whether the executor has reached its iteration limit.
    pub fn is_exhausted(&self) -> bool {
        self.current_iteration >= self.max_iterations
    }

    /// The number of iterations completed so far.
    pub fn iterations(&self) -> usize {
        self.current_iteration
    }

    /// The total number of individual tool calls executed.
    pub fn total_calls(&self) -> usize {
        self.total_calls
    }

    /// Reset the executor state for a new conversation.
    pub fn reset(&mut self) {
        self.current_iteration = 0;
        self.total_calls = 0;
    }
}

// ---------------------------------------------------------------------------
// Loop controller (legacy, preserved for backward compatibility)
// ---------------------------------------------------------------------------

/// Manages the tool-use loop: send -> parse -> execute -> send results -> repeat.
///
/// This is a state machine that tracks the current phase of a tool-use
/// conversation. The actual AI calls are done externally; this struct
/// handles bookkeeping and result formatting.
pub struct ToolUseLoop {
    pub registry: ToolRegistry,
    pub max_rounds: usize,
    pub current_round: usize,
    pub total_tool_calls: usize,
}

impl ToolUseLoop {
    pub fn new(registry: ToolRegistry, max_rounds: usize) -> Self {
        Self {
            registry,
            max_rounds,
            current_round: 0,
            total_tool_calls: 0,
        }
    }

    /// Process a response: extract tool calls, execute them, return results.
    /// Returns `None` if no tool calls found (conversation is done).
    pub fn process_response(
        &mut self,
        content_blocks: &[serde_json::Value],
    ) -> Option<Vec<ToolResult>> {
        if self.current_round >= self.max_rounds {
            return None;
        }

        let calls = extract_tool_calls(content_blocks);
        if calls.is_empty() {
            return None;
        }

        self.current_round += 1;
        self.total_tool_calls += calls.len();

        let results = self.registry.execute_all(&calls);
        Some(results)
    }

    /// Check if the loop has reached its maximum rounds.
    pub fn is_exhausted(&self) -> bool {
        self.current_round >= self.max_rounds
    }

    /// Reset the loop for a new conversation.
    pub fn reset(&mut self) {
        self.current_round = 0;
        self.total_tool_calls = 0;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Test helper: a simple trait-based tool for testing ------------------

    struct EchoTool;

    impl ToolHandler for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the input text back."
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                },
                "required": ["text"]
            })
        }
        fn execute(&self, args: serde_json::Value) -> Result<String, String> {
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("(empty)");
            Ok(format!("Echo: {text}"))
        }
    }

    struct FailTool;

    impl ToolHandler for FailTool {
        fn name(&self) -> &str {
            "fail"
        }
        fn description(&self) -> &str {
            "Always fails."
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn execute(&self, _args: serde_json::Value) -> Result<String, String> {
            Err("intentional failure".to_string())
        }
    }

    // -- Sample data --------------------------------------------------------

    fn sample_anthropic_blocks() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "type": "text",
                "text": "Let me read that file for you."
            }),
            serde_json::json!({
                "type": "tool_use",
                "id": "toolu_01A",
                "name": "read_file",
                "input": { "path": "/tmp/test.txt" }
            }),
        ]
    }

    fn sample_openai_tool_calls() -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "id": "call_abc123",
            "type": "function",
            "function": {
                "name": "read_file",
                "arguments": "{\"path\":\"/tmp/test.txt\"}"
            }
        })]
    }

    fn sample_anthropic_response() -> serde_json::Value {
        serde_json::json!({
            "content": [
                { "type": "text", "text": "I will read the file." },
                {
                    "type": "tool_use",
                    "id": "toolu_01B",
                    "name": "echo",
                    "input": { "text": "hello" }
                }
            ],
            "stop_reason": "tool_use"
        })
    }

    fn sample_openai_response() -> serde_json::Value {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_xyz789",
                        "type": "function",
                        "function": {
                            "name": "echo",
                            "arguments": "{\"text\":\"world\"}"
                        }
                    }]
                }
            }]
        })
    }

    fn sample_flat_tool_calls_response() -> serde_json::Value {
        serde_json::json!({
            "tool_calls": [{
                "id": "call_flat_1",
                "type": "function",
                "function": {
                    "name": "echo",
                    "arguments": "{\"text\":\"flat\"}"
                }
            }]
        })
    }

    // -- Parsing tests ------------------------------------------------------

    #[test]
    fn test_extract_anthropic_tool_calls() {
        let blocks = sample_anthropic_blocks();
        let calls = extract_tool_calls(&blocks);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "toolu_01A");
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].input["path"], "/tmp/test.txt");
    }

    #[test]
    fn test_extract_openai_calls() {
        let tc = sample_openai_tool_calls();
        let calls = extract_openai_tool_calls(&tc);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc123");
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].input["path"], "/tmp/test.txt");
    }

    #[test]
    fn test_has_tool_use_detection() {
        let blocks = sample_anthropic_blocks();
        assert!(has_tool_use(&blocks));

        let text_only = vec![serde_json::json!({"type": "text", "text": "hello"})];
        assert!(!has_tool_use(&text_only));
    }

    #[test]
    fn test_parse_tool_calls_from_anthropic_response() {
        let response = sample_anthropic_response();
        let calls = parse_tool_calls_from_response(&response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "toolu_01B");
        assert_eq!(calls[0].name, "echo");
        assert_eq!(calls[0].input["text"], "hello");
    }

    #[test]
    fn test_parse_tool_calls_from_openai_response() {
        let response = sample_openai_response();
        let calls = parse_tool_calls_from_response(&response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_xyz789");
        assert_eq!(calls[0].name, "echo");
        assert_eq!(calls[0].input["text"], "world");
    }

    #[test]
    fn test_parse_tool_calls_from_flat_response() {
        let response = sample_flat_tool_calls_response();
        let calls = parse_tool_calls_from_response(&response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_flat_1");
        assert_eq!(calls[0].name, "echo");
        assert_eq!(calls[0].input["text"], "flat");
    }

    #[test]
    fn test_parse_tool_calls_from_empty_response() {
        let response = serde_json::json!({"content": "just text, no tools"});
        let calls = parse_tool_calls_from_response(&response);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_tool_calls_text_only_blocks() {
        let response = serde_json::json!({
            "content": [
                { "type": "text", "text": "I have no tools to call." }
            ]
        });
        let calls = parse_tool_calls_from_response(&response);
        assert!(calls.is_empty());
    }

    // -- Result formatting tests --------------------------------------------

    #[test]
    fn test_format_anthropic_result_json() {
        let result = ToolResult {
            tool_use_id: "toolu_01A".into(),
            content: "file contents here".into(),
            is_error: false,
        };
        let json = format_anthropic_result(&result);
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_use_id"], "toolu_01A");
        assert_eq!(json["content"], "file contents here");
        assert_eq!(json["is_error"], false);
    }

    #[test]
    fn test_format_openai_result_json() {
        let result = ToolResult {
            tool_use_id: "call_abc123".into(),
            content: "file contents here".into(),
            is_error: false,
        };
        let json = format_openai_result(&result);
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_abc123");
        assert_eq!(json["content"], "file contents here");
    }

    #[test]
    fn test_format_error_result() {
        let result = ToolResult {
            tool_use_id: "t_err".into(),
            content: "Error: something went wrong".into(),
            is_error: true,
        };
        let json = format_anthropic_result(&result);
        assert_eq!(json["is_error"], true);
        assert!(json["content"].as_str().unwrap().contains("Error"));
    }

    // -- Registry tests (closure-based) -------------------------------------

    #[test]
    fn test_registry_register_and_execute_closure() {
        let mut registry = ToolRegistry::new();
        registry.register(
            ToolDefinition {
                name: "echo".into(),
                description: "Echo input".into(),
                input_schema: serde_json::json!({}),
            },
            |input| {
                let text = input
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no text");
                Ok(format!("Echo: {text}"))
            },
        );

        assert_eq!(registry.len(), 1);
        assert!(registry.has_tool("echo"));
        assert!(!registry.has_tool("nonexistent"));

        let call = ToolCall {
            id: "t1".into(),
            name: "echo".into(),
            input: serde_json::json!({"text": "hello"}),
        };
        let result = registry.execute(&call);
        assert!(!result.is_error);
        assert_eq!(result.content, "Echo: hello");
    }

    #[test]
    fn test_registry_unknown_tool() {
        let registry = ToolRegistry::new();
        let call = ToolCall {
            id: "t1".into(),
            name: "missing".into(),
            input: serde_json::json!({}),
        };
        let result = registry.execute(&call);
        assert!(result.is_error);
        assert!(result.content.contains("Unknown tool"));
    }

    #[test]
    fn test_registry_handler_error_closure() {
        let mut registry = ToolRegistry::new();
        registry.register(
            ToolDefinition {
                name: "fail".into(),
                description: "Always fails".into(),
                input_schema: serde_json::json!({}),
            },
            |_| anyhow::bail!("intentional failure"),
        );

        let call = ToolCall {
            id: "t1".into(),
            name: "fail".into(),
            input: serde_json::json!({}),
        };
        let result = registry.execute(&call);
        assert!(result.is_error);
        assert!(result.content.contains("intentional failure"));
    }

    #[test]
    fn test_execute_all_multiple() {
        let mut registry = ToolRegistry::new();
        registry.register(
            ToolDefinition {
                name: "add".into(),
                description: "Add two numbers".into(),
                input_schema: serde_json::json!({}),
            },
            |input| {
                let a = input.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
                let b = input.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
                Ok(format!("{}", a + b))
            },
        );

        let calls = vec![
            ToolCall {
                id: "t1".into(),
                name: "add".into(),
                input: serde_json::json!({"a": 1, "b": 2}),
            },
            ToolCall {
                id: "t2".into(),
                name: "add".into(),
                input: serde_json::json!({"a": 10, "b": 20}),
            },
        ];

        let results = registry.execute_all(&calls);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].content, "3");
        assert_eq!(results[1].content, "30");
    }

    // -- Registry tests (trait-based) ---------------------------------------

    #[test]
    fn test_registry_register_trait_handler() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));

        assert_eq!(registry.len(), 1);
        assert!(registry.has_tool("echo"));

        let call = ToolCall {
            id: "t1".into(),
            name: "echo".into(),
            input: serde_json::json!({"text": "trait-based"}),
        };
        let result = registry.execute(&call);
        assert!(!result.is_error);
        assert_eq!(result.content, "Echo: trait-based");
    }

    #[test]
    fn test_registry_trait_handler_error() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(FailTool));

        let call = ToolCall {
            id: "t1".into(),
            name: "fail".into(),
            input: serde_json::json!({}),
        };
        let result = registry.execute(&call);
        assert!(result.is_error);
        assert!(result.content.contains("intentional failure"));
    }

    #[test]
    fn test_registry_trait_generates_definition() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));

        let defs = registry.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo");
        assert_eq!(defs[0].description, "Echoes the input text back.");
        assert!(defs[0].input_schema["properties"]["text"].is_object());
    }

    #[test]
    fn test_registry_mixed_closure_and_trait() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));
        registry.register(
            ToolDefinition {
                name: "add".into(),
                description: "Add".into(),
                input_schema: serde_json::json!({}),
            },
            |input| {
                let a = input.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
                let b = input.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
                Ok(format!("{}", a + b))
            },
        );

        assert_eq!(registry.len(), 2);

        let echo_result = registry.execute(&ToolCall {
            id: "t1".into(),
            name: "echo".into(),
            input: serde_json::json!({"text": "hi"}),
        });
        assert_eq!(echo_result.content, "Echo: hi");

        let add_result = registry.execute(&ToolCall {
            id: "t2".into(),
            name: "add".into(),
            input: serde_json::json!({"a": 5, "b": 7}),
        });
        assert_eq!(add_result.content, "12");
    }

    // -- Built-in definitions tests -----------------------------------------

    #[test]
    fn test_builtin_definitions_count() {
        let defs = builtin_tool_definitions();
        assert_eq!(defs.len(), 7);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"execute_command"));
        assert!(names.contains(&"search_files"));
        assert!(names.contains(&"list_directory"));
        assert!(names.contains(&"git_status"));
        assert!(names.contains(&"git_diff"));
    }

    #[test]
    fn test_builtin_registry_has_all_tools() {
        let registry = builtin_registry();
        assert_eq!(registry.len(), 7);
        assert!(registry.has_tool("read_file"));
        assert!(registry.has_tool("write_file"));
        assert!(registry.has_tool("list_directory"));
        assert!(registry.has_tool("search_files"));
        assert!(registry.has_tool("execute_command"));
        assert!(registry.has_tool("git_status"));
        assert!(registry.has_tool("git_diff"));
    }

    // -- ToolExecutor tests -------------------------------------------------

    #[test]
    fn test_executor_processes_anthropic_response() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));

        let mut executor = ToolExecutor::new(registry, 10);
        let response = sample_anthropic_response();
        let results = executor.process_response(&response);

        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Echo: hello");
        assert!(!results[0].is_error);
        assert_eq!(executor.iterations(), 1);
        assert_eq!(executor.total_calls(), 1);
    }

    #[test]
    fn test_executor_processes_openai_response() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));

        let mut executor = ToolExecutor::new(registry, 10);
        let response = sample_openai_response();
        let results = executor.process_response(&response);

        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Echo: world");
        assert_eq!(executor.iterations(), 1);
    }

    #[test]
    fn test_executor_returns_none_for_no_tools() {
        let registry = ToolRegistry::new();
        let mut executor = ToolExecutor::new(registry, 10);
        let response = serde_json::json!({
            "content": [{ "type": "text", "text": "All done." }]
        });
        let results = executor.process_response(&response);
        assert!(results.is_none());
        assert_eq!(executor.iterations(), 0);
    }

    #[test]
    fn test_executor_respects_max_iterations() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));

        let mut executor = ToolExecutor::new(registry, 2);

        let response = sample_anthropic_response();

        // First iteration
        let r1 = executor.process_response(&response);
        assert!(r1.is_some());
        assert_eq!(executor.iterations(), 1);

        // Second iteration
        let r2 = executor.process_response(&response);
        assert!(r2.is_some());
        assert_eq!(executor.iterations(), 2);
        assert!(executor.is_exhausted());

        // Third iteration should be blocked
        let r3 = executor.process_response(&response);
        assert!(r3.is_none());
        assert_eq!(executor.iterations(), 2);
    }

    #[test]
    fn test_executor_reset() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));

        let mut executor = ToolExecutor::new(registry, 1);
        let response = sample_anthropic_response();

        executor.process_response(&response);
        assert!(executor.is_exhausted());

        executor.reset();
        assert!(!executor.is_exhausted());
        assert_eq!(executor.iterations(), 0);
        assert_eq!(executor.total_calls(), 0);

        // Can process again after reset
        let results = executor.process_response(&response);
        assert!(results.is_some());
    }

    #[test]
    fn test_executor_format_results_anthropic() {
        let results = vec![
            ToolResult {
                tool_use_id: "t1".into(),
                content: "ok".into(),
                is_error: false,
            },
            ToolResult {
                tool_use_id: "t2".into(),
                content: "Error: bad".into(),
                is_error: true,
            },
        ];
        let formatted = ToolExecutor::format_results_anthropic(&results);
        assert_eq!(formatted.len(), 2);
        assert_eq!(formatted[0]["type"], "tool_result");
        assert_eq!(formatted[0]["tool_use_id"], "t1");
        assert_eq!(formatted[1]["is_error"], true);
    }

    #[test]
    fn test_executor_format_results_openai() {
        let results = vec![ToolResult {
            tool_use_id: "call_1".into(),
            content: "result".into(),
            is_error: false,
        }];
        let formatted = ToolExecutor::format_results_openai(&results);
        assert_eq!(formatted.len(), 1);
        assert_eq!(formatted[0]["role"], "tool");
        assert_eq!(formatted[0]["tool_call_id"], "call_1");
        assert_eq!(formatted[0]["content"], "result");
    }

    #[test]
    fn test_executor_handles_unknown_tool_gracefully() {
        let registry = ToolRegistry::new(); // empty, no tools registered
        let mut executor = ToolExecutor::new(registry, 10);

        let response = sample_anthropic_response();
        let results = executor.process_response(&response);

        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
        assert!(results[0].content.contains("Unknown tool: echo"));
    }

    #[test]
    fn test_executor_multiple_tool_calls_in_one_response() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Box::new(EchoTool));

        let mut executor = ToolExecutor::new(registry, 10);

        let response = serde_json::json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "t1",
                    "name": "echo",
                    "input": { "text": "first" }
                },
                {
                    "type": "tool_use",
                    "id": "t2",
                    "name": "echo",
                    "input": { "text": "second" }
                }
            ]
        });

        let results = executor.process_response(&response);
        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].content, "Echo: first");
        assert_eq!(results[0].tool_use_id, "t1");
        assert_eq!(results[1].content, "Echo: second");
        assert_eq!(results[1].tool_use_id, "t2");
        assert_eq!(executor.total_calls(), 2);
        assert_eq!(executor.iterations(), 1);
    }

    // -- Legacy ToolUseLoop tests -------------------------------------------

    #[test]
    fn test_tool_use_loop_basic() {
        let mut registry = ToolRegistry::new();
        registry.register(
            ToolDefinition {
                name: "read_file".into(),
                description: "Read file".into(),
                input_schema: serde_json::json!({}),
            },
            |_| Ok("file contents".into()),
        );

        let mut tool_loop = ToolUseLoop::new(registry, 5);
        assert!(!tool_loop.is_exhausted());
        assert_eq!(tool_loop.current_round, 0);

        let blocks = sample_anthropic_blocks();
        let results = tool_loop.process_response(&blocks);
        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "file contents");
        assert_eq!(tool_loop.current_round, 1);
        assert_eq!(tool_loop.total_tool_calls, 1);
    }

    #[test]
    fn test_tool_use_loop_no_tools_returns_none() {
        let registry = ToolRegistry::new();
        let mut tool_loop = ToolUseLoop::new(registry, 5);

        let text_only = vec![serde_json::json!({"type": "text", "text": "done"})];
        let results = tool_loop.process_response(&text_only);
        assert!(results.is_none());
        assert_eq!(tool_loop.current_round, 0);
    }

    #[test]
    fn test_tool_use_loop_exhaustion() {
        let registry = ToolRegistry::new();
        let mut tool_loop = ToolUseLoop::new(registry, 0);
        assert!(tool_loop.is_exhausted());

        let blocks = sample_anthropic_blocks();
        let results = tool_loop.process_response(&blocks);
        assert!(results.is_none());
    }

    #[test]
    fn test_tool_use_loop_reset() {
        let mut registry = ToolRegistry::new();
        registry.register(
            ToolDefinition {
                name: "read_file".into(),
                description: "Read file".into(),
                input_schema: serde_json::json!({}),
            },
            |_| Ok("ok".into()),
        );

        let mut tool_loop = ToolUseLoop::new(registry, 5);
        let blocks = sample_anthropic_blocks();
        tool_loop.process_response(&blocks);
        assert_eq!(tool_loop.current_round, 1);

        tool_loop.reset();
        assert_eq!(tool_loop.current_round, 0);
        assert_eq!(tool_loop.total_tool_calls, 0);
        assert!(!tool_loop.is_exhausted());
    }

    // -- Serialization tests ------------------------------------------------

    #[test]
    fn test_tool_definition_serialization() {
        let def = ToolDefinition {
            name: "test_tool".into(),
            description: "A test tool".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "arg": {"type": "string"}
                }
            }),
        };
        let json = serde_json::to_string(&def).unwrap();
        let parsed: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test_tool");
        assert_eq!(parsed.description, "A test tool");
    }

    #[test]
    fn test_tool_call_serialization() {
        let call = ToolCall {
            id: "tc_123".into(),
            name: "read_file".into(),
            input: serde_json::json!({"path": "/tmp/test.txt"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "tc_123");
        assert_eq!(parsed.name, "read_file");
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult {
            tool_use_id: "tc_123".into(),
            content: "success".into(),
            is_error: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_use_id, "tc_123");
        assert!(!parsed.is_error);
    }

    // -- Built-in tool handler unit tests (no real FS/shell calls) ----------

    #[test]
    fn test_read_file_tool_missing_path_arg() {
        let tool = ReadFileTool;
        let result = tool.execute(serde_json::json!({}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: path")
        );
    }

    #[test]
    fn test_write_file_tool_missing_args() {
        let tool = WriteFileTool;
        let result = tool.execute(serde_json::json!({}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: path")
        );

        let result = tool.execute(serde_json::json!({"path": "/tmp/x.txt"}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: content")
        );
    }

    #[test]
    fn test_list_directory_tool_missing_path() {
        let tool = ListDirectoryTool;
        let result = tool.execute(serde_json::json!({}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: path")
        );
    }

    #[test]
    fn test_search_files_tool_missing_pattern() {
        let tool = SearchFilesTool;
        let result = tool.execute(serde_json::json!({}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: pattern")
        );
    }

    #[test]
    fn test_execute_command_tool_missing_command() {
        let tool = ExecuteCommandTool::new();
        let result = tool.execute(serde_json::json!({}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: command")
        );
    }

    #[test]
    fn test_execute_command_tool_blocks_dangerous_command() {
        let tool = ExecuteCommandTool::new();
        let result = tool.execute(serde_json::json!({"command": "rm -rf /"}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Blocked"));
    }

    #[test]
    fn test_git_status_tool_missing_path() {
        let tool = GitStatusTool;
        let result = tool.execute(serde_json::json!({}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: path")
        );
    }

    #[test]
    fn test_git_diff_tool_missing_path() {
        let tool = GitDiffTool;
        let result = tool.execute(serde_json::json!({}));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Missing required argument: path")
        );
    }

    #[test]
    fn test_tool_handler_trait_name_and_schema() {
        let tool = ReadFileTool;
        assert_eq!(tool.name(), "read_file");
        assert_eq!(
            tool.description(),
            "Read the contents of a file at the given path."
        );
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["path"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("path")));
    }

    #[test]
    fn test_execute_command_tool_schema_has_command_field() {
        let tool = ExecuteCommandTool::new();
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["command"].is_object());
    }
}
