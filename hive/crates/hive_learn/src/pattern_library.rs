use crate::storage::LearningStorage;
use crate::types::*;
use std::sync::Arc;

/// Library of reusable code patterns extracted from high-quality AI responses.
///
/// Only extracts patterns from responses with quality > 0.8 to ensure the
/// library contains proven, working patterns. Uses simple line-based extraction
/// to identify function signatures, struct/class definitions, and other
/// structural code patterns.
pub struct PatternLibrary {
    storage: Arc<LearningStorage>,
}

impl PatternLibrary {
    pub fn new(storage: Arc<LearningStorage>) -> Self {
        Self { storage }
    }

    /// Extract code patterns from a code block.
    ///
    /// Only extracts from code with quality > 0.8. Identifies function signatures,
    /// struct/class definitions, trait/interface declarations, and other structural
    /// code patterns using line-based heuristics.
    ///
    /// Each extracted pattern is persisted to storage and returned.
    pub fn extract_patterns(
        &self,
        code: &str,
        language: &str,
        quality_score: f64,
    ) -> Result<Vec<CodePattern>, String> {
        if quality_score <= 0.8 {
            return Ok(Vec::new());
        }

        let mut patterns = Vec::new();
        let now = chrono::Utc::now().to_rfc3339();

        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some((category, description)) = classify_line(trimmed, language) {
                let pattern = CodePattern {
                    id: 0,
                    pattern: trimmed.to_string(),
                    language: language.to_string(),
                    category,
                    description,
                    quality_score,
                    use_count: 0,
                    created_at: now.clone(),
                };

                let id = self.storage.save_pattern(&pattern)?;

                let mut saved = pattern;
                saved.id = id;
                patterns.push(saved);
            }
        }

        if !patterns.is_empty() {
            self.storage.log_learning(&LearningLogEntry {
                id: 0,
                event_type: "patterns_extracted".into(),
                description: format!(
                    "Extracted {} pattern(s) from {language} code (quality: {quality_score:.2})",
                    patterns.len()
                ),
                details: serde_json::to_string(
                    &patterns.iter().map(|p| &p.pattern).collect::<Vec<_>>(),
                )
                .unwrap_or_default(),
                reversible: false,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })?;
        }

        Ok(patterns)
    }

    /// Search for patterns matching a query string.
    ///
    /// Searches across pattern text, description, and category fields.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<CodePattern>, String> {
        self.storage.search_patterns(query, limit)
    }

    /// Get the most-used patterns, sorted by use_count descending.
    pub fn popular_patterns(&self, limit: usize) -> Result<Vec<CodePattern>, String> {
        self.storage.popular_patterns(limit)
    }

    /// Retrieve the most relevant patterns for a given task.
    ///
    /// Searches by language, then scores by `quality_score * (1 + use_count)`.
    /// Returns up to `limit` patterns sorted by composite relevance score.
    pub fn relevant_for_task(
        &self,
        query: &str,
        language: &str,
        limit: usize,
    ) -> Result<Vec<CodePattern>, String> {
        // First try language-specific patterns matching the query
        let mut candidates = self.storage.search_patterns(query, limit * 3)?;

        // Also include language-specific popular patterns
        let popular = self.storage.popular_patterns(limit * 3)?;
        for p in popular {
            if p.language == language && !candidates.iter().any(|c| c.id == p.id) {
                candidates.push(p);
            }
        }

        // Filter to matching language and score by relevance
        let mut scored: Vec<(f64, CodePattern)> = candidates
            .into_iter()
            .filter(|p| p.language == language || language.is_empty())
            .map(|p| {
                let score = p.quality_score * (1.0 + p.use_count as f64);
                (score, p)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, p)| p).collect())
    }

    /// Increment the use_count for a pattern (called when a pattern is used in context).
    pub fn record_usage(&self, pattern_id: i64) -> Result<(), String> {
        self.storage.increment_pattern_use_count(pattern_id)
    }
}

/// Classify a line of code and return (category, description) if it matches
/// a known pattern type.
fn classify_line(trimmed: &str, language: &str) -> Option<(String, String)> {
    match language {
        "rust" => classify_rust_line(trimmed),
        "python" => classify_python_line(trimmed),
        "javascript" | "typescript" => classify_js_line(trimmed),
        "go" => classify_go_line(trimmed),
        "java" | "kotlin" | "scala" => classify_java_line(trimmed),
        "c" | "cpp" | "c++" => classify_c_line(trimmed),
        _ => classify_generic_line(trimmed),
    }
}

fn classify_rust_line(line: &str) -> Option<(String, String)> {
    if line.starts_with("pub fn ") || line.starts_with("fn ") {
        let name = extract_fn_name(line, "fn ");
        return Some(("function".to_string(), format!("Rust function: {name}")));
    }
    if line.starts_with("pub async fn ") || line.starts_with("async fn ") {
        let name = extract_fn_name(line, "fn ");
        return Some((
            "async_function".to_string(),
            format!("Rust async function: {name}"),
        ));
    }
    if line.starts_with("pub struct ") || line.starts_with("struct ") {
        let name = extract_type_name(line, "struct ");
        return Some(("struct".to_string(), format!("Rust struct: {name}")));
    }
    if line.starts_with("pub enum ") || line.starts_with("enum ") {
        let name = extract_type_name(line, "enum ");
        return Some(("enum".to_string(), format!("Rust enum: {name}")));
    }
    if line.starts_with("pub trait ") || line.starts_with("trait ") {
        let name = extract_type_name(line, "trait ");
        return Some(("trait".to_string(), format!("Rust trait: {name}")));
    }
    if line.starts_with("impl ") {
        return Some((
            "impl".to_string(),
            format!("Rust impl block: {}", &line[..line.len().min(60)]),
        ));
    }
    if line.starts_with("pub type ") || line.starts_with("type ") {
        let name = extract_type_name(line, "type ");
        return Some(("type_alias".to_string(), format!("Rust type alias: {name}")));
    }
    None
}

fn classify_python_line(line: &str) -> Option<(String, String)> {
    if line.starts_with("def ") {
        let name = extract_fn_name(line, "def ");
        return Some(("function".to_string(), format!("Python function: {name}")));
    }
    if line.starts_with("async def ") {
        let name = extract_fn_name(line, "async def ");
        return Some((
            "async_function".to_string(),
            format!("Python async function: {name}"),
        ));
    }
    if line.starts_with("class ") {
        let name = extract_type_name(line, "class ");
        return Some(("class".to_string(), format!("Python class: {name}")));
    }
    None
}

fn classify_js_line(line: &str) -> Option<(String, String)> {
    if line.starts_with("function ") {
        let name = extract_fn_name(line, "function ");
        return Some(("function".to_string(), format!("JS function: {name}")));
    }
    if line.starts_with("async function ") {
        let name = extract_fn_name(line, "async function ");
        return Some((
            "async_function".to_string(),
            format!("JS async function: {name}"),
        ));
    }
    if line.starts_with("export function ") {
        let name = extract_fn_name(line, "export function ");
        return Some((
            "function".to_string(),
            format!("JS exported function: {name}"),
        ));
    }
    if line.starts_with("export default function ") {
        let name = extract_fn_name(line, "export default function ");
        return Some((
            "function".to_string(),
            format!("JS default exported function: {name}"),
        ));
    }
    if line.starts_with("class ") || line.starts_with("export class ") {
        let keyword = if line.starts_with("export class ") {
            "export class "
        } else {
            "class "
        };
        let name = extract_type_name(line, keyword);
        return Some(("class".to_string(), format!("JS class: {name}")));
    }
    if line.starts_with("interface ") || line.starts_with("export interface ") {
        let keyword = if line.starts_with("export interface ") {
            "export interface "
        } else {
            "interface "
        };
        let name = extract_type_name(line, keyword);
        return Some(("interface".to_string(), format!("TS interface: {name}")));
    }
    None
}

fn classify_go_line(line: &str) -> Option<(String, String)> {
    if line.starts_with("func ") {
        let name = extract_fn_name(line, "func ");
        return Some(("function".to_string(), format!("Go function: {name}")));
    }
    if line.starts_with("type ") && line.contains("struct") {
        let name = extract_type_name(line, "type ");
        return Some(("struct".to_string(), format!("Go struct: {name}")));
    }
    if line.starts_with("type ") && line.contains("interface") {
        let name = extract_type_name(line, "type ");
        return Some(("interface".to_string(), format!("Go interface: {name}")));
    }
    None
}

fn classify_java_line(line: &str) -> Option<(String, String)> {
    // Simplified: check for common Java/Kotlin patterns
    if line.contains("class ") && (line.starts_with("public ") || line.starts_with("class ")) {
        let keyword_pos = line.find("class ")?;
        let after = &line[keyword_pos + 6..];
        let name = after
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("unknown");
        return Some(("class".to_string(), format!("Java class: {name}")));
    }
    if line.contains("interface ") {
        let keyword_pos = line.find("interface ")?;
        let after = &line[keyword_pos + 10..];
        let name = after
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("unknown");
        return Some(("interface".to_string(), format!("Java interface: {name}")));
    }
    None
}

fn classify_c_line(line: &str) -> Option<(String, String)> {
    if line.starts_with("struct ") {
        let name = extract_type_name(line, "struct ");
        return Some(("struct".to_string(), format!("C struct: {name}")));
    }
    if line.starts_with("typedef ") {
        return Some((
            "typedef".to_string(),
            format!("C typedef: {}", &line[..line.len().min(60)]),
        ));
    }
    None
}

fn classify_generic_line(line: &str) -> Option<(String, String)> {
    if line.starts_with("fn ") || line.starts_with("pub fn ") {
        let name = extract_fn_name(line, "fn ");
        return Some(("function".to_string(), format!("Function: {name}")));
    }
    if line.starts_with("def ") {
        let name = extract_fn_name(line, "def ");
        return Some(("function".to_string(), format!("Function: {name}")));
    }
    if line.starts_with("function ") {
        let name = extract_fn_name(line, "function ");
        return Some(("function".to_string(), format!("Function: {name}")));
    }
    if line.starts_with("class ") {
        let name = extract_type_name(line, "class ");
        return Some(("class".to_string(), format!("Class: {name}")));
    }
    if line.starts_with("struct ") {
        let name = extract_type_name(line, "struct ");
        return Some(("struct".to_string(), format!("Struct: {name}")));
    }
    None
}

/// Extract the function name after the given keyword prefix.
fn extract_fn_name(line: &str, after_keyword: &str) -> String {
    let start = match line.find(after_keyword) {
        Some(pos) => pos + after_keyword.len(),
        None => return "unknown".to_string(),
    };
    let rest = &line[start..];
    rest.split(['(', '<', ' ', ':'])
        .next()
        .unwrap_or("unknown")
        .to_string()
}

/// Extract a type name after the given keyword prefix.
fn extract_type_name(line: &str, keyword: &str) -> String {
    let start = match line.find(keyword) {
        Some(pos) => pos + keyword.len(),
        None => return "unknown".to_string(),
    };
    let rest = &line[start..];
    rest.split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_library() -> PatternLibrary {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        PatternLibrary::new(storage)
    }

    fn make_library_with_storage() -> (PatternLibrary, Arc<LearningStorage>) {
        let storage = Arc::new(LearningStorage::in_memory().unwrap());
        let lib = PatternLibrary::new(Arc::clone(&storage));
        (lib, storage)
    }

    // ── extract_patterns tests ───────────────────────────────────────

    #[test]
    fn test_extract_patterns_skips_low_quality() {
        let lib = make_library();
        let code = "pub fn hello() -> String { \"hello\".to_string() }";
        let patterns = lib.extract_patterns(code, "rust", 0.5).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_extract_patterns_at_threshold_is_skipped() {
        let lib = make_library();
        let code = "pub fn hello() -> String { \"hello\".to_string() }";
        let patterns = lib.extract_patterns(code, "rust", 0.8).unwrap();
        // quality must be > 0.8, not >= 0.8
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_extract_rust_function() {
        let lib = make_library();
        let code = "pub fn process_data(input: &str) -> Result<(), Error> {\n    Ok(())\n}";
        let patterns = lib.extract_patterns(code, "rust", 0.9).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].category, "function");
        assert!(patterns[0].description.contains("process_data"));
        assert_eq!(patterns[0].language, "rust");
    }

    #[test]
    fn test_extract_rust_struct() {
        let lib = make_library();
        let code = "pub struct Config {\n    pub name: String,\n}";
        let patterns = lib.extract_patterns(code, "rust", 0.9).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].category, "struct");
        assert!(patterns[0].description.contains("Config"));
    }

    #[test]
    fn test_extract_rust_enum() {
        let lib = make_library();
        let code = "pub enum Status {\n    Active,\n    Inactive,\n}";
        let patterns = lib.extract_patterns(code, "rust", 0.9).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].category, "enum");
        assert!(patterns[0].description.contains("Status"));
    }

    #[test]
    fn test_extract_rust_trait() {
        let lib = make_library();
        let code = "pub trait Handler {\n    fn handle(&self);\n}";
        let patterns = lib.extract_patterns(code, "rust", 0.9).unwrap();
        // Should find trait and fn
        assert!(patterns.len() >= 1);
        assert!(patterns.iter().any(|p| p.category == "trait"));
    }

    #[test]
    fn test_extract_rust_impl() {
        let lib = make_library();
        let code = "impl Config {\n    pub fn new() -> Self { Self {} }\n}";
        let patterns = lib.extract_patterns(code, "rust", 0.9).unwrap();
        assert!(patterns.iter().any(|p| p.category == "impl"));
        assert!(patterns.iter().any(|p| p.category == "function"));
    }

    #[test]
    fn test_extract_python_function() {
        let lib = make_library();
        let code = "def process_data(input_str):\n    return input_str.upper()";
        let patterns = lib.extract_patterns(code, "python", 0.9).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].category, "function");
        assert!(patterns[0].description.contains("process_data"));
    }

    #[test]
    fn test_extract_python_class() {
        let lib = make_library();
        let code = "class DataProcessor:\n    def __init__(self):\n        pass";
        let patterns = lib.extract_patterns(code, "python", 0.9).unwrap();
        assert!(patterns.iter().any(|p| p.category == "class"));
        assert!(
            patterns
                .iter()
                .any(|p| p.description.contains("DataProcessor"))
        );
    }

    #[test]
    fn test_extract_js_function() {
        let lib = make_library();
        let code = "function fetchData(url) {\n    return fetch(url);\n}";
        let patterns = lib.extract_patterns(code, "javascript", 0.9).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].category, "function");
        assert!(patterns[0].description.contains("fetchData"));
    }

    #[test]
    fn test_extract_ts_interface() {
        let lib = make_library();
        let code = "export interface Config {\n    name: string;\n}";
        let patterns = lib.extract_patterns(code, "typescript", 0.9).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].category, "interface");
        assert!(patterns[0].description.contains("Config"));
    }

    #[test]
    fn test_extract_go_function() {
        let lib = make_library();
        let code = "func ProcessData(input string) error {\n    return nil\n}";
        let patterns = lib.extract_patterns(code, "go", 0.9).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].category, "function");
        assert!(patterns[0].description.contains("ProcessData"));
    }

    #[test]
    fn test_extract_multiple_patterns() {
        let lib = make_library();
        let code = "\
pub struct App {
    config: Config,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run(&self) -> Result<(), Error> {
        Ok(())
    }
}";
        let patterns = lib.extract_patterns(code, "rust", 0.9).unwrap();
        // Should find: struct App, impl App, fn new, fn run
        assert!(patterns.len() >= 3);
    }

    #[test]
    fn test_extract_empty_code() {
        let lib = make_library();
        let patterns = lib.extract_patterns("", "rust", 0.9).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_extract_no_recognizable_patterns() {
        let lib = make_library();
        let code = "// just a comment\nlet x = 42;\nlet y = x + 1;";
        let patterns = lib.extract_patterns(code, "rust", 0.9).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_extract_patterns_persisted() {
        let (lib, storage) = make_library_with_storage();
        let code = "pub fn hello() -> String { \"hi\".into() }";
        lib.extract_patterns(code, "rust", 0.9).unwrap();

        let found = storage.search_patterns("hello", 10).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_extract_patterns_logs() {
        let (lib, storage) = make_library_with_storage();
        let code = "pub fn hello() -> String { \"hi\".into() }";
        lib.extract_patterns(code, "rust", 0.9).unwrap();

        let log = storage.get_learning_log(10).unwrap();
        assert!(log.iter().any(|e| e.event_type == "patterns_extracted"));
    }

    // ── search tests ─────────────────────────────────────────────────

    #[test]
    fn test_search_finds_patterns() {
        let lib = make_library();
        let code = "pub fn process_data(input: &str) -> String { input.to_string() }";
        lib.extract_patterns(code, "rust", 0.9).unwrap();

        let results = lib.search("process", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_empty_result() {
        let lib = make_library();
        let results = lib.search("nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_by_category() {
        let lib = make_library();
        let code = "pub struct Config {}\npub fn new() -> Config { Config {} }";
        lib.extract_patterns(code, "rust", 0.9).unwrap();

        let results = lib.search("struct", 10).unwrap();
        assert!(!results.is_empty());
    }

    // ── popular_patterns tests ───────────────────────────────────────

    #[test]
    fn test_popular_patterns_empty() {
        let lib = make_library();
        let results = lib.popular_patterns(10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_popular_patterns_order() {
        let (lib, storage) = make_library_with_storage();

        // Insert patterns with different use counts directly
        let p1 = CodePattern {
            id: 0,
            pattern: "fn alpha()".into(),
            language: "rust".into(),
            category: "function".into(),
            description: "Alpha function".into(),
            quality_score: 0.9,
            use_count: 5,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let p2 = CodePattern {
            id: 0,
            pattern: "fn beta()".into(),
            language: "rust".into(),
            category: "function".into(),
            description: "Beta function".into(),
            quality_score: 0.85,
            use_count: 20,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        storage.save_pattern(&p1).unwrap();
        storage.save_pattern(&p2).unwrap();

        let popular = lib.popular_patterns(10).unwrap();
        assert_eq!(popular.len(), 2);
        // beta (use_count=20) should be first
        assert!(popular[0].pattern.contains("beta"));
        assert!(popular[1].pattern.contains("alpha"));
    }

    // ── helper function tests ────────────────────────────────────────

    #[test]
    fn test_extract_fn_name() {
        assert_eq!(extract_fn_name("fn hello()", "fn "), "hello");
        assert_eq!(
            extract_fn_name("pub fn process_data(input: &str)", "fn "),
            "process_data"
        );
        assert_eq!(extract_fn_name("fn generic<T>(x: T)", "fn "), "generic");
    }

    #[test]
    fn test_extract_type_name() {
        assert_eq!(extract_type_name("struct Config {", "struct "), "Config");
        assert_eq!(
            extract_type_name("pub struct MyStruct {", "struct "),
            "MyStruct"
        );
        assert_eq!(extract_type_name("enum Status {", "enum "), "Status");
    }

    #[test]
    fn test_classify_unknown_language() {
        // Unknown language falls through to generic classifier
        let result = classify_line("fn hello()", "brainfuck");
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "function");
    }
}
