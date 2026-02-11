use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

// ── Diagnostics ────────────────────────────────────────────────────

/// Severity level for a code diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A single diagnostic (error, warning, etc.) reported by an IDE or linter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<String>,
}

// ── Symbols ────────────────────────────────────────────────────────

/// The kind of a code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Interface,
    Variable,
    Constant,
    Module,
    Property,
    Enum,
}

/// A code symbol (function, class, variable, etc.) known to the IDE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub line: u32,
    pub column: u32,
    /// Parent class or module that contains this symbol.
    pub container: Option<String>,
}

// ── Location ───────────────────────────────────────────────────────

/// A location within a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
}

// ── Commands ───────────────────────────────────────────────────────

/// An editor command that can be executed against the IDE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditorCommand {
    GoToDefinition {
        file_path: String,
        line: u32,
        column: u32,
    },
    FindReferences {
        symbol_name: String,
    },
    GetDiagnostics {
        file_path: String,
    },
    ListSymbols {
        file_path: String,
    },
    FormatDocument {
        file_path: String,
    },
    RenameSymbol {
        old_name: String,
        new_name: String,
        file_path: String,
    },
}

/// The result of executing an editor command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
}

impl CommandResult {
    /// Create a successful result with optional data payload.
    pub fn ok(data: Option<Value>) -> Self {
        Self {
            success: true,
            data,
            error: None,
        }
    }

    /// Create a failed result with an error message.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

// ── Workspace ──────────────────────────────────────────────────────

/// Information about the current IDE workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub root_path: String,
    pub open_files: Vec<String>,
    pub language: Option<String>,
}

// ── Service ────────────────────────────────────────────────────────

/// In-memory service that tracks IDE state including diagnostics,
/// symbols, open files, and workspace information.
pub struct IdeIntegrationService {
    workspace: Option<WorkspaceInfo>,
    diagnostics: HashMap<String, Vec<Diagnostic>>,
    symbols: Vec<Symbol>,
}

impl IdeIntegrationService {
    /// Create a new service with no workspace or data.
    pub fn new() -> Self {
        debug!("creating new IdeIntegrationService");
        Self {
            workspace: None,
            diagnostics: HashMap::new(),
            symbols: Vec::new(),
        }
    }

    // ── Workspace ──────────────────────────────────────────────────

    /// Set the workspace root path, clearing any previous workspace.
    pub fn set_workspace(&mut self, root_path: impl Into<String>) {
        let root_path = root_path.into();
        debug!(root_path = %root_path, "setting workspace");
        self.workspace = Some(WorkspaceInfo {
            root_path,
            open_files: Vec::new(),
            language: None,
        });
    }

    /// Get a reference to the current workspace, if one has been set.
    pub fn get_workspace(&self) -> Option<&WorkspaceInfo> {
        self.workspace.as_ref()
    }

    /// Add a file to the list of open files in the workspace.
    ///
    /// Returns `false` if no workspace has been set or the file is already open.
    pub fn add_open_file(&mut self, path: impl Into<String>) -> bool {
        let path = path.into();
        if let Some(ws) = &mut self.workspace {
            if ws.open_files.contains(&path) {
                return false;
            }
            debug!(path = %path, "tracking open file");
            ws.open_files.push(path);
            true
        } else {
            false
        }
    }

    /// Remove a file from the list of open files.
    ///
    /// Returns `true` if the file was found and removed.
    pub fn remove_open_file(&mut self, path: &str) -> bool {
        if let Some(ws) = &mut self.workspace {
            let before = ws.open_files.len();
            ws.open_files.retain(|f| f != path);
            let removed = ws.open_files.len() < before;
            if removed {
                debug!(path = %path, "removed open file");
            }
            removed
        } else {
            false
        }
    }

    // ── Diagnostics ────────────────────────────────────────────────

    /// Report a diagnostic for a file.
    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        debug!(
            file = %diagnostic.file_path,
            line = diagnostic.line,
            severity = ?diagnostic.severity,
            "adding diagnostic"
        );
        self.diagnostics
            .entry(diagnostic.file_path.clone())
            .or_default()
            .push(diagnostic);
    }

    /// Get all diagnostics for a specific file.
    pub fn get_diagnostics(&self, file_path: &str) -> Vec<&Diagnostic> {
        self.diagnostics
            .get(file_path)
            .map(|diags| diags.iter().collect())
            .unwrap_or_default()
    }

    /// Clear all diagnostics for a specific file.
    ///
    /// Returns the number of diagnostics removed.
    pub fn clear_diagnostics(&mut self, file_path: &str) -> usize {
        let count = self
            .diagnostics
            .get(file_path)
            .map(|d| d.len())
            .unwrap_or(0);
        self.diagnostics.remove(file_path);
        if count > 0 {
            debug!(file = %file_path, count = count, "cleared diagnostics");
        }
        count
    }

    /// Return all diagnostics across every file.
    pub fn all_diagnostics(&self) -> Vec<&Diagnostic> {
        self.diagnostics.values().flat_map(|v| v.iter()).collect()
    }

    /// Total number of diagnostics across all files.
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.values().map(|v| v.len()).sum()
    }

    /// Count of diagnostics with `Error` severity.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .values()
            .flat_map(|v| v.iter())
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count()
    }

    // ── Symbols ────────────────────────────────────────────────────

    /// Register a symbol.
    pub fn add_symbol(&mut self, symbol: Symbol) {
        debug!(
            name = %symbol.name,
            kind = ?symbol.kind,
            file = %symbol.file_path,
            "adding symbol"
        );
        self.symbols.push(symbol);
    }

    /// Search for symbols whose name contains the given substring (case-insensitive).
    pub fn find_symbols(&self, name: &str) -> Vec<&Symbol> {
        let lower = name.to_lowercase();
        self.symbols
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&lower))
            .collect()
    }

    /// List all symbols defined in a specific file.
    pub fn symbols_in_file(&self, file_path: &str) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| s.file_path == file_path)
            .collect()
    }

    /// Total number of registered symbols.
    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }

    // ── Command execution ──────────────────────────────────────────

    /// Execute an editor command and return a result.
    ///
    /// Commands are handled in-memory against the service's current state.
    pub fn execute_command(&self, command: EditorCommand) -> CommandResult {
        debug!(command = ?command, "executing editor command");

        match command {
            EditorCommand::GoToDefinition {
                file_path,
                line,
                column,
            } => {
                let location = Location {
                    file_path,
                    line,
                    column,
                };
                CommandResult::ok(Some(serde_json::to_value(location).unwrap()))
            }

            EditorCommand::FindReferences { symbol_name } => {
                let matches = self.find_symbols(&symbol_name);
                let locations: Vec<Location> = matches
                    .iter()
                    .map(|s| Location {
                        file_path: s.file_path.clone(),
                        line: s.line,
                        column: s.column,
                    })
                    .collect();
                CommandResult::ok(Some(serde_json::to_value(locations).unwrap()))
            }

            EditorCommand::GetDiagnostics { file_path } => {
                let diags = self.get_diagnostics(&file_path);
                if diags.is_empty() {
                    CommandResult::ok(Some(serde_json::json!([])))
                } else {
                    let values: Vec<Value> = diags
                        .iter()
                        .map(|d| serde_json::to_value(d).unwrap())
                        .collect();
                    CommandResult::ok(Some(Value::Array(values)))
                }
            }

            EditorCommand::ListSymbols { file_path } => {
                let syms = self.symbols_in_file(&file_path);
                let values: Vec<Value> = syms
                    .iter()
                    .map(|s| serde_json::to_value(s).unwrap())
                    .collect();
                CommandResult::ok(Some(Value::Array(values)))
            }

            EditorCommand::FormatDocument { file_path } => {
                // In-memory service acknowledges the request.
                CommandResult::ok(Some(
                    serde_json::json!({ "formatted": true, "file": file_path }),
                ))
            }

            EditorCommand::RenameSymbol {
                old_name,
                new_name,
                file_path,
            } => {
                let matching: Vec<&Symbol> = self
                    .symbols
                    .iter()
                    .filter(|s| s.name == old_name && s.file_path == file_path)
                    .collect();

                if matching.is_empty() {
                    CommandResult::err(format!(
                        "symbol '{old_name}' not found in '{file_path}'"
                    ))
                } else {
                    CommandResult::ok(Some(serde_json::json!({
                        "renamed": true,
                        "old_name": old_name,
                        "new_name": new_name,
                        "occurrences": matching.len(),
                    })))
                }
            }
        }
    }
}

impl Default for IdeIntegrationService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_diagnostic(file: &str, severity: DiagnosticSeverity) -> Diagnostic {
        Diagnostic {
            file_path: file.to_string(),
            line: 10,
            column: 5,
            severity,
            message: format!("{severity:?} in {file}"),
            source: Some("rustc".to_string()),
        }
    }

    fn sample_symbol(name: &str, file: &str, kind: SymbolKind) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file_path: file.to_string(),
            line: 1,
            column: 0,
            container: None,
        }
    }

    // ── Service creation ───────────────────────────────────────────

    #[test]
    fn test_new_service_is_empty() {
        let svc = IdeIntegrationService::new();
        assert!(svc.get_workspace().is_none());
        assert_eq!(svc.diagnostic_count(), 0);
        assert_eq!(svc.symbol_count(), 0);
        assert_eq!(svc.error_count(), 0);
    }

    // ── Workspace ──────────────────────────────────────────────────

    #[test]
    fn test_set_and_get_workspace() {
        let mut svc = IdeIntegrationService::new();
        svc.set_workspace("/home/user/project");

        let ws = svc.get_workspace().unwrap();
        assert_eq!(ws.root_path, "/home/user/project");
        assert!(ws.open_files.is_empty());
        assert!(ws.language.is_none());
    }

    #[test]
    fn test_add_and_remove_open_files() {
        let mut svc = IdeIntegrationService::new();
        svc.set_workspace("/project");

        assert!(svc.add_open_file("src/main.rs"));
        assert!(svc.add_open_file("src/lib.rs"));
        // Duplicate is rejected.
        assert!(!svc.add_open_file("src/main.rs"));

        let ws = svc.get_workspace().unwrap();
        assert_eq!(ws.open_files.len(), 2);

        assert!(svc.remove_open_file("src/main.rs"));
        assert!(!svc.remove_open_file("src/main.rs")); // already removed

        let ws = svc.get_workspace().unwrap();
        assert_eq!(ws.open_files.len(), 1);
        assert_eq!(ws.open_files[0], "src/lib.rs");
    }

    #[test]
    fn test_open_file_without_workspace_returns_false() {
        let mut svc = IdeIntegrationService::new();
        assert!(!svc.add_open_file("orphan.rs"));
        assert!(!svc.remove_open_file("orphan.rs"));
    }

    // ── Diagnostics ────────────────────────────────────────────────

    #[test]
    fn test_add_and_get_diagnostics() {
        let mut svc = IdeIntegrationService::new();
        svc.add_diagnostic(sample_diagnostic("main.rs", DiagnosticSeverity::Error));
        svc.add_diagnostic(sample_diagnostic("main.rs", DiagnosticSeverity::Warning));
        svc.add_diagnostic(sample_diagnostic("lib.rs", DiagnosticSeverity::Info));

        assert_eq!(svc.get_diagnostics("main.rs").len(), 2);
        assert_eq!(svc.get_diagnostics("lib.rs").len(), 1);
        assert_eq!(svc.get_diagnostics("unknown.rs").len(), 0);
    }

    #[test]
    fn test_clear_diagnostics() {
        let mut svc = IdeIntegrationService::new();
        svc.add_diagnostic(sample_diagnostic("main.rs", DiagnosticSeverity::Error));
        svc.add_diagnostic(sample_diagnostic("main.rs", DiagnosticSeverity::Warning));

        let removed = svc.clear_diagnostics("main.rs");
        assert_eq!(removed, 2);
        assert_eq!(svc.get_diagnostics("main.rs").len(), 0);

        // Clearing again removes nothing.
        assert_eq!(svc.clear_diagnostics("main.rs"), 0);
    }

    #[test]
    fn test_all_diagnostics_and_counts() {
        let mut svc = IdeIntegrationService::new();
        svc.add_diagnostic(sample_diagnostic("a.rs", DiagnosticSeverity::Error));
        svc.add_diagnostic(sample_diagnostic("a.rs", DiagnosticSeverity::Error));
        svc.add_diagnostic(sample_diagnostic("b.rs", DiagnosticSeverity::Warning));
        svc.add_diagnostic(sample_diagnostic("c.rs", DiagnosticSeverity::Hint));

        assert_eq!(svc.diagnostic_count(), 4);
        assert_eq!(svc.error_count(), 2);
        assert_eq!(svc.all_diagnostics().len(), 4);
    }

    // ── Symbols ────────────────────────────────────────────────────

    #[test]
    fn test_add_and_find_symbols() {
        let mut svc = IdeIntegrationService::new();
        svc.add_symbol(sample_symbol("process_data", "main.rs", SymbolKind::Function));
        svc.add_symbol(sample_symbol("DataProcessor", "lib.rs", SymbolKind::Class));
        svc.add_symbol(sample_symbol("process_item", "lib.rs", SymbolKind::Method));

        // Substring match, case-insensitive.
        let results = svc.find_symbols("process");
        assert_eq!(results.len(), 3); // process_data, DataProcessor (contains "process"), process_item

        let results = svc.find_symbols("DATA");
        assert_eq!(results.len(), 2); // process_data, DataProcessor

        let results = svc.find_symbols("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_symbols_in_file() {
        let mut svc = IdeIntegrationService::new();
        svc.add_symbol(sample_symbol("foo", "a.rs", SymbolKind::Function));
        svc.add_symbol(sample_symbol("bar", "a.rs", SymbolKind::Variable));
        svc.add_symbol(sample_symbol("baz", "b.rs", SymbolKind::Constant));

        assert_eq!(svc.symbols_in_file("a.rs").len(), 2);
        assert_eq!(svc.symbols_in_file("b.rs").len(), 1);
        assert_eq!(svc.symbols_in_file("c.rs").len(), 0);
        assert_eq!(svc.symbol_count(), 3);
    }

    // ── Command execution ──────────────────────────────────────────

    #[test]
    fn test_execute_go_to_definition() {
        let svc = IdeIntegrationService::new();
        let result = svc.execute_command(EditorCommand::GoToDefinition {
            file_path: "main.rs".into(),
            line: 42,
            column: 8,
        });

        assert!(result.success);
        let data = result.data.unwrap();
        assert_eq!(data["file_path"], "main.rs");
        assert_eq!(data["line"], 42);
        assert_eq!(data["column"], 8);
    }

    #[test]
    fn test_execute_find_references() {
        let mut svc = IdeIntegrationService::new();
        svc.add_symbol(sample_symbol("render", "ui.rs", SymbolKind::Function));
        svc.add_symbol(sample_symbol("render_frame", "gfx.rs", SymbolKind::Function));

        let result = svc.execute_command(EditorCommand::FindReferences {
            symbol_name: "render".into(),
        });

        assert!(result.success);
        let arr = result.data.unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_execute_get_diagnostics() {
        let mut svc = IdeIntegrationService::new();
        svc.add_diagnostic(sample_diagnostic("main.rs", DiagnosticSeverity::Error));

        let result = svc.execute_command(EditorCommand::GetDiagnostics {
            file_path: "main.rs".into(),
        });

        assert!(result.success);
        let arr = result.data.unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_execute_format_document() {
        let svc = IdeIntegrationService::new();
        let result = svc.execute_command(EditorCommand::FormatDocument {
            file_path: "lib.rs".into(),
        });

        assert!(result.success);
        let data = result.data.unwrap();
        assert_eq!(data["formatted"], true);
        assert_eq!(data["file"], "lib.rs");
    }

    #[test]
    fn test_execute_rename_symbol_success() {
        let mut svc = IdeIntegrationService::new();
        svc.add_symbol(sample_symbol("old_fn", "main.rs", SymbolKind::Function));

        let result = svc.execute_command(EditorCommand::RenameSymbol {
            old_name: "old_fn".into(),
            new_name: "new_fn".into(),
            file_path: "main.rs".into(),
        });

        assert!(result.success);
        let data = result.data.unwrap();
        assert_eq!(data["renamed"], true);
        assert_eq!(data["old_name"], "old_fn");
        assert_eq!(data["new_name"], "new_fn");
    }

    #[test]
    fn test_execute_rename_symbol_not_found() {
        let svc = IdeIntegrationService::new();
        let result = svc.execute_command(EditorCommand::RenameSymbol {
            old_name: "missing".into(),
            new_name: "renamed".into(),
            file_path: "main.rs".into(),
        });

        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("missing"));
    }

    // ── Serialization ──────────────────────────────────────────────

    #[test]
    fn test_diagnostic_serialization_roundtrip() {
        let diag = sample_diagnostic("test.rs", DiagnosticSeverity::Warning);
        let json = serde_json::to_string(&diag).unwrap();
        let restored: Diagnostic = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.file_path, diag.file_path);
        assert_eq!(restored.severity, diag.severity);
        assert_eq!(restored.message, diag.message);
        assert_eq!(restored.source, diag.source);
    }

    #[test]
    fn test_command_result_helpers() {
        let ok = CommandResult::ok(Some(serde_json::json!({"key": "value"})));
        assert!(ok.success);
        assert!(ok.data.is_some());
        assert!(ok.error.is_none());

        let err = CommandResult::err("something went wrong");
        assert!(!err.success);
        assert!(err.data.is_none());
        assert_eq!(err.error.unwrap(), "something went wrong");
    }

    #[test]
    fn test_workspace_info_serialization() {
        let ws = WorkspaceInfo {
            root_path: "/project".into(),
            open_files: vec!["main.rs".into(), "lib.rs".into()],
            language: Some("rust".into()),
        };
        let json = serde_json::to_string(&ws).unwrap();
        let restored: WorkspaceInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.root_path, ws.root_path);
        assert_eq!(restored.open_files, ws.open_files);
        assert_eq!(restored.language, ws.language);
    }

    #[test]
    fn test_default_trait_implementation() {
        let svc = IdeIntegrationService::default();
        assert!(svc.get_workspace().is_none());
        assert_eq!(svc.diagnostic_count(), 0);
        assert_eq!(svc.symbol_count(), 0);
    }

    #[test]
    fn test_symbol_with_container() {
        let mut svc = IdeIntegrationService::new();
        let sym = Symbol {
            name: "render".to_string(),
            kind: SymbolKind::Method,
            file_path: "widget.rs".to_string(),
            line: 25,
            column: 4,
            container: Some("Widget".to_string()),
        };
        svc.add_symbol(sym);

        let found = svc.find_symbols("render");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].container.as_deref(), Some("Widget"));
    }

    #[test]
    fn test_execute_list_symbols() {
        let mut svc = IdeIntegrationService::new();
        svc.add_symbol(sample_symbol("alpha", "mod.rs", SymbolKind::Function));
        svc.add_symbol(sample_symbol("beta", "mod.rs", SymbolKind::Variable));
        svc.add_symbol(sample_symbol("gamma", "other.rs", SymbolKind::Class));

        let result = svc.execute_command(EditorCommand::ListSymbols {
            file_path: "mod.rs".into(),
        });

        assert!(result.success);
        let arr = result.data.unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 2);
    }
}
