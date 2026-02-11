//! Smart Context Curation Engine.
//!
//! Filters thousands of potential context sources (files, symbols, docs,
//! git history) down to the most relevant subset that fits within a token
//! budget. Uses TF-IDF scoring with heuristic boosts for filename matches,
//! symbol names, recency, and test files.

use chrono::{DateTime, Utc};
use hive_core::context::estimate_tokens;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tracing::debug;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Common English stopwords filtered during keyword extraction.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from",
    "had", "has", "have", "he", "her", "his", "how", "i", "if", "in", "into",
    "is", "it", "its", "let", "my", "no", "not", "of", "on", "or", "our",
    "she", "so", "than", "that", "the", "their", "them", "then", "there",
    "these", "they", "this", "to", "us", "was", "we", "were", "what", "when",
    "where", "which", "who", "will", "with", "you", "your",
];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The kind of context source (file, symbol, documentation, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    File,
    Symbol,
    Documentation,
    GitHistory,
    Dependency,
    Config,
    Test,
}

/// A single context source with its content and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSource {
    pub path: String,
    pub content: String,
    pub source_type: SourceType,
    pub last_modified: DateTime<Utc>,
}

/// Relevance score for a context source, with reasons explaining the score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceScore {
    pub source_idx: usize,
    pub score: f64,
    pub reasons: Vec<String>,
}

/// Token and source budget constraints for context curation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    pub max_tokens: usize,
    pub max_sources: usize,
    /// Tokens reserved for the prompt/response (subtracted from max_tokens).
    pub reserved_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            max_sources: 50,
            reserved_tokens: 0,
        }
    }
}

/// The result of context curation: selected sources with scores and stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratedContext {
    pub sources: Vec<ContextSource>,
    pub scores: Vec<RelevanceScore>,
    pub total_tokens: usize,
    pub original_count: usize,
    pub selected_count: usize,
}

/// Aggregate statistics about the sources in a `ContextEngine`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStats {
    pub total_sources: usize,
    pub total_tokens_approx: usize,
    pub by_type: HashMap<SourceType, usize>,
}

// ---------------------------------------------------------------------------
// Keyword / tokenization helpers
// ---------------------------------------------------------------------------

/// Tokenize text into lowercase word tokens, splitting on non-alphanumeric
/// characters (underscore preserved).
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Return whether `word` is a stopword.
fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word)
}

// ---------------------------------------------------------------------------
// ContextEngine
// ---------------------------------------------------------------------------

/// Smart context curation engine.
///
/// Collects context sources, scores them against a query using TF-IDF with
/// heuristic boosts, and greedily packs the highest-scoring sources into a
/// token budget.
pub struct ContextEngine {
    sources: Vec<ContextSource>,
    /// Cached IDF values keyed by term. Invalidated when sources change.
    idf_cache: HashMap<String, f64>,
}

impl ContextEngine {
    /// Create a new, empty context engine.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            idf_cache: HashMap::new(),
        }
    }

    /// Add a pre-built context source.
    pub fn add_source(&mut self, source: ContextSource) {
        self.sources.push(source);
        self.idf_cache.clear();
    }

    /// Convenience: add a file source with the current timestamp.
    pub fn add_file(&mut self, path: &str, content: &str) {
        self.add_source(ContextSource {
            path: path.to_string(),
            content: content.to_string(),
            source_type: SourceType::File,
            last_modified: Utc::now(),
        });
    }

    /// Convenience: add a symbol source (e.g. a function/struct body).
    pub fn add_symbol(&mut self, name: &str, body: &str) {
        self.add_source(ContextSource {
            path: name.to_string(),
            content: body.to_string(),
            source_type: SourceType::Symbol,
            last_modified: Utc::now(),
        });
    }

    /// Recursively walk `dir_path`, read text files, and add them as
    /// `SourceType::File` sources. Returns the number of files indexed.
    pub fn index_directory(&mut self, dir_path: &str) -> anyhow::Result<usize> {
        let path = Path::new(dir_path);
        self.walk_directory(path)
    }

    /// Curate the most relevant sources for `query` within `budget`.
    ///
    /// Algorithm:
    /// 1. Extract keywords from the query (tokenize + filter stopwords).
    /// 2. Compute TF-IDF relevance for every source.
    /// 3. Apply heuristic boosts (filename match, symbol match, recency, tests).
    /// 4. Sort by score descending.
    /// 5. Greedily pack sources into the available token budget.
    pub fn curate(&mut self, query: &str, budget: &ContextBudget) -> CuratedContext {
        let original_count = self.sources.len();

        if self.sources.is_empty() {
            return CuratedContext {
                sources: Vec::new(),
                scores: Vec::new(),
                total_tokens: 0,
                original_count: 0,
                selected_count: 0,
            };
        }

        // Step 1: extract query keywords.
        let query_keywords = self.extract_keywords(query);
        let query_terms: Vec<&str> = query_keywords.iter().map(|s| s.as_str()).collect();

        // Rebuild IDF cache if empty (invalidated on source add).
        if self.idf_cache.is_empty() {
            self.rebuild_idf_cache();
        }

        // Step 2 + 3: score each source.
        let mut scored: Vec<RelevanceScore> = self
            .sources
            .iter()
            .enumerate()
            .map(|(idx, source)| self.score_source(idx, source, &query_terms, query))
            .collect();

        // Step 4: sort by score descending.
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Step 5: greedy packing into budget.
        let available_tokens = budget.max_tokens.saturating_sub(budget.reserved_tokens);
        let mut total_tokens = 0usize;
        let mut selected_sources = Vec::new();
        let mut selected_scores = Vec::new();

        for rs in &scored {
            if selected_sources.len() >= budget.max_sources {
                break;
            }
            let source = &self.sources[rs.source_idx];
            let tokens = self.estimate_source_tokens(source);
            if total_tokens + tokens > available_tokens {
                continue;
            }
            total_tokens += tokens;
            selected_sources.push(source.clone());
            selected_scores.push(rs.clone());
        }

        let selected_count = selected_sources.len();
        debug!(
            "Curated {}/{} sources ({} tokens) for query '{}'",
            selected_count, original_count, total_tokens, query
        );

        CuratedContext {
            sources: selected_sources,
            scores: selected_scores,
            total_tokens,
            original_count,
            selected_count,
        }
    }

    /// Compute the TF-IDF score for `document` against `query_terms`.
    pub fn compute_tf_idf(&self, query_terms: &[&str], document: &str) -> f64 {
        if query_terms.is_empty() || document.is_empty() {
            return 0.0;
        }

        let doc_tokens = tokenize(document);
        let doc_len = doc_tokens.len() as f64;
        if doc_len == 0.0 {
            return 0.0;
        }

        // Term frequency in document.
        let mut tf_counts: HashMap<&str, usize> = HashMap::new();
        for token in &doc_tokens {
            for &qt in query_terms {
                if token == qt {
                    *tf_counts.entry(qt).or_insert(0) += 1;
                }
            }
        }

        let mut score = 0.0;
        for &term in query_terms {
            let tf = tf_counts.get(term).copied().unwrap_or(0) as f64 / doc_len;
            let idf = self.compute_idf(term);
            score += tf * idf;
        }

        score
    }

    /// Compute inverse document frequency: `ln(N / df)`.
    /// Returns 0.0 if the term does not appear in any source.
    pub fn compute_idf(&self, term: &str) -> f64 {
        if let Some(&cached) = self.idf_cache.get(term) {
            return cached;
        }

        let n = self.sources.len() as f64;
        if n == 0.0 {
            return 0.0;
        }
        let df = self
            .sources
            .iter()
            .filter(|s| {
                let lower = s.content.to_lowercase();
                lower.contains(term)
            })
            .count() as f64;

        if df == 0.0 {
            return 0.0;
        }
        (n / df).ln()
    }

    /// Extract keywords from text: tokenize, lowercase, and filter stopwords.
    pub fn extract_keywords(&self, text: &str) -> Vec<String> {
        tokenize(text)
            .into_iter()
            .filter(|w| !is_stopword(w))
            .collect()
    }

    /// Estimate the token count of a source's content using
    /// `hive_core::context::estimate_tokens`.
    pub fn estimate_source_tokens(&self, source: &ContextSource) -> usize {
        estimate_tokens(&source.content)
    }

    /// Return aggregate statistics about the sources in this engine.
    pub fn summary_stats(&self) -> ContextStats {
        let mut by_type: HashMap<SourceType, usize> = HashMap::new();
        let mut total_tokens = 0usize;

        for source in &self.sources {
            *by_type.entry(source.source_type).or_insert(0) += 1;
            total_tokens += estimate_tokens(&source.content);
        }

        ContextStats {
            total_sources: self.sources.len(),
            total_tokens_approx: total_tokens,
            by_type,
        }
    }

    // -- Private helpers ----------------------------------------------------

    /// Rebuild the IDF cache from all current sources.
    fn rebuild_idf_cache(&mut self) {
        let n = self.sources.len() as f64;
        if n == 0.0 {
            return;
        }

        // Collect the unique tokens per source.
        let doc_token_sets: Vec<HashSet<String>> = self
            .sources
            .iter()
            .map(|s| tokenize(&s.content).into_iter().collect())
            .collect();

        // Gather all unique terms.
        let all_terms: HashSet<&str> = doc_token_sets
            .iter()
            .flat_map(|s| s.iter().map(|t| t.as_str()))
            .collect();

        for term in all_terms {
            let df = doc_token_sets
                .iter()
                .filter(|set| set.contains(term))
                .count() as f64;
            let idf = if df == 0.0 { 0.0 } else { (n / df).ln() };
            self.idf_cache.insert(term.to_string(), idf);
        }
    }

    /// Score a single source against the query. Returns a `RelevanceScore`
    /// with the raw TF-IDF score plus heuristic boosts.
    fn score_source(
        &self,
        idx: usize,
        source: &ContextSource,
        query_terms: &[&str],
        query_raw: &str,
    ) -> RelevanceScore {
        let mut score = self.compute_tf_idf(query_terms, &source.content);
        let mut reasons: Vec<String> = Vec::new();

        if score > 0.0 {
            reasons.push(format!("tf-idf: {:.4}", score));
        }

        // Boost: filename/path contains a query term (+0.5).
        let path_lower = source.path.to_lowercase();
        let query_lower = query_raw.to_lowercase();
        let has_filename_match = query_terms
            .iter()
            .any(|term| path_lower.contains(term));
        if has_filename_match {
            score += 0.5;
            reasons.push("filename match (+0.5)".to_string());
        }

        // Boost: symbol name match (+0.3) — only for Symbol sources.
        if source.source_type == SourceType::Symbol {
            let name_lower = source.path.to_lowercase();
            let has_symbol_match = query_terms
                .iter()
                .any(|term| name_lower.contains(term));
            if has_symbol_match {
                score += 0.3;
                reasons.push("symbol name match (+0.3)".to_string());
            }
        }

        // Boost: recently modified (+0.2) — within the last hour.
        let age = Utc::now().signed_duration_since(source.last_modified);
        if age.num_hours() < 1 {
            score += 0.2;
            reasons.push("recent modification (+0.2)".to_string());
        }

        // Boost: test files when querying code (+0.1).
        let looks_like_code_query = query_lower.contains("fn ")
            || query_lower.contains("struct ")
            || query_lower.contains("impl ")
            || query_lower.contains("test")
            || query_lower.contains("error")
            || query_lower.contains("bug");
        if source.source_type == SourceType::Test && looks_like_code_query {
            score += 0.1;
            reasons.push("test file for code query (+0.1)".to_string());
        }

        RelevanceScore {
            source_idx: idx,
            score,
            reasons,
        }
    }

    /// Recursively walk a directory and add text files as sources.
    fn walk_directory(&mut self, path: &Path) -> anyhow::Result<usize> {
        let mut count = 0;

        let entries: Vec<_> = fs::read_dir(path)
            .map_err(|e| anyhow::anyhow!("Failed to read directory {}: {}", path.display(), e))?
            .collect();

        for entry in entries {
            let entry = entry?;
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files/directories.
            if file_name.starts_with('.') {
                continue;
            }

            if entry_path.is_dir() {
                count += self.walk_directory(&entry_path)?;
            } else if entry_path.is_file() {
                if is_likely_binary(&entry_path) {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&entry_path) {
                    let path_str = entry_path.to_string_lossy().to_string();
                    let source_type = infer_source_type(&entry_path);
                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(DateTime::<Utc>::from)
                        .unwrap_or_else(Utc::now);

                    self.add_source(ContextSource {
                        path: path_str,
                        content,
                        source_type,
                        last_modified: modified,
                    });
                    count += 1;
                }
            }
        }

        Ok(count)
    }
}

impl Default for ContextEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

/// Heuristic: check first 512 bytes for null bytes.
fn is_likely_binary(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    use std::io::Read;
    let mut buf = [0u8; 512];
    let mut reader = std::io::BufReader::new(file);
    let Ok(n) = reader.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

/// Infer the `SourceType` from a file path based on common patterns.
fn infer_source_type(path: &Path) -> SourceType {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if name.contains("test") || name.starts_with("test_") || name.ends_with("_test.rs") {
        return SourceType::Test;
    }
    if ext == "md" || ext == "txt" || ext == "adoc" || ext == "rst" {
        return SourceType::Documentation;
    }
    if name == "cargo.toml"
        || name == "package.json"
        || name == "go.mod"
        || name == "requirements.txt"
    {
        return SourceType::Dependency;
    }
    if ext == "toml" || ext == "yaml" || ext == "yml" || ext == "json" || ext == "ini" {
        return SourceType::Config;
    }

    SourceType::File
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a source with sensible defaults.
    fn make_source(path: &str, content: &str, source_type: SourceType) -> ContextSource {
        ContextSource {
            path: path.to_string(),
            content: content.to_string(),
            source_type,
            last_modified: Utc::now(),
        }
    }

    fn default_budget() -> ContextBudget {
        ContextBudget {
            max_tokens: 100_000,
            max_sources: 100,
            reserved_tokens: 0,
        }
    }

    // -- Core curate behavior -----------------------------------------------

    #[test]
    fn test_empty_engine_curate() {
        let mut engine = ContextEngine::new();
        let result = engine.curate("anything", &default_budget());

        assert_eq!(result.original_count, 0);
        assert_eq!(result.selected_count, 0);
        assert_eq!(result.total_tokens, 0);
        assert!(result.sources.is_empty());
        assert!(result.scores.is_empty());
    }

    #[test]
    fn test_add_source_and_curate() {
        let mut engine = ContextEngine::new();
        engine.add_source(make_source(
            "main.rs",
            "fn main() { println!(\"hello world\"); }",
            SourceType::File,
        ));

        let result = engine.curate("main hello", &default_budget());

        assert_eq!(result.original_count, 1);
        assert_eq!(result.selected_count, 1);
        assert!(result.total_tokens > 0);
        assert!(!result.scores.is_empty());
        assert!(result.scores[0].score > 0.0);
    }

    #[test]
    fn test_relevance_scoring_prefers_matching_content() {
        let mut engine = ContextEngine::new();
        engine.add_source(make_source(
            "math.rs",
            "fn add(a: i32, b: i32) -> i32 { a + b }",
            SourceType::File,
        ));
        engine.add_source(make_source(
            "greet.rs",
            "fn greet(name: &str) { println!(\"Hello {}\", name); }",
            SourceType::File,
        ));

        let result = engine.curate("add numbers", &default_budget());

        // math.rs should rank higher because it contains "add".
        assert_eq!(result.selected_count, 2);
        assert_eq!(result.sources[0].path, "math.rs");
    }

    #[test]
    fn test_budget_token_limit_respected() {
        let mut engine = ContextEngine::new();
        // Each source is ~50 chars => ~13 tokens.
        for i in 0..20 {
            engine.add_file(
                &format!("file_{}.rs", i),
                &format!("fn func_{}() {{ /* body with some content */ }}", i),
            );
        }

        let budget = ContextBudget {
            max_tokens: 30, // Very tight — should fit only a couple.
            max_sources: 100,
            reserved_tokens: 0,
        };
        let result = engine.curate("func", &budget);

        assert!(result.total_tokens <= 30);
        assert!(result.selected_count < 20);
    }

    #[test]
    fn test_budget_source_limit_respected() {
        let mut engine = ContextEngine::new();
        for i in 0..20 {
            engine.add_file(&format!("file_{}.rs", i), "fn hello() {}");
        }

        let budget = ContextBudget {
            max_tokens: 100_000,
            max_sources: 3,
            reserved_tokens: 0,
        };
        let result = engine.curate("hello", &budget);

        assert!(result.selected_count <= 3);
    }

    #[test]
    fn test_filename_match_boost() {
        let mut engine = ContextEngine::new();
        // Source whose path contains "auth" but content does not.
        engine.add_source(make_source(
            "auth_handler.rs",
            "fn process_request(r: Request) -> Response { r.into() }",
            SourceType::File,
        ));
        // Source whose content mentions auth but path does not.
        engine.add_source(make_source(
            "handler.rs",
            "fn auth_check(token: &str) -> bool { !token.is_empty() }",
            SourceType::File,
        ));

        let result = engine.curate("auth", &default_budget());

        // auth_handler.rs should get the filename boost and rank first.
        assert_eq!(result.sources[0].path, "auth_handler.rs");
        let first_score = &result.scores[0];
        assert!(first_score.reasons.iter().any(|r| r.contains("filename")));
    }

    // -- Keyword extraction -------------------------------------------------

    #[test]
    fn test_keyword_extraction() {
        let engine = ContextEngine::new();
        let keywords = engine.extract_keywords("the quick brown fox");
        assert!(keywords.contains(&"quick".to_string()));
        assert!(keywords.contains(&"brown".to_string()));
        assert!(keywords.contains(&"fox".to_string()));
    }

    #[test]
    fn test_stopword_filtering() {
        let engine = ContextEngine::new();
        let keywords = engine.extract_keywords("the a an is in to of and or");
        assert!(keywords.is_empty(), "All stopwords should be filtered");
    }

    // -- TF-IDF computation -------------------------------------------------

    #[test]
    fn test_tf_idf_computation() {
        let mut engine = ContextEngine::new();
        engine.add_file("a.rs", "fn alpha beta gamma");
        engine.add_file("b.rs", "fn delta epsilon alpha");

        // "alpha" appears in both docs, "gamma" in one.
        let score_alpha = engine.compute_tf_idf(&["alpha"], "fn alpha beta gamma");
        let score_gamma = engine.compute_tf_idf(&["gamma"], "fn alpha beta gamma");

        // "gamma" is rarer (higher IDF) so its TF-IDF should be higher.
        assert!(
            score_gamma > score_alpha,
            "Rarer term 'gamma' should score higher: gamma={}, alpha={}",
            score_gamma,
            score_alpha
        );
    }

    #[test]
    fn test_tf_idf_empty_inputs() {
        let engine = ContextEngine::new();
        assert_eq!(engine.compute_tf_idf(&[], "some content"), 0.0);
        assert_eq!(engine.compute_tf_idf(&["query"], ""), 0.0);
    }

    // -- Ranking and stats --------------------------------------------------

    #[test]
    fn test_multiple_sources_ranking() {
        let mut engine = ContextEngine::new();
        engine.add_file("config.toml", "database_url = localhost");
        engine.add_file("database.rs", "fn connect_database(url: &str) { /* connect */ }");
        engine.add_file("utils.rs", "fn format_string(s: &str) -> String { s.to_string() }");

        let result = engine.curate("database connect", &default_budget());

        // database.rs should rank first (content + filename match).
        assert!(!result.sources.is_empty());
        assert_eq!(result.sources[0].path, "database.rs");
    }

    #[test]
    fn test_summary_stats() {
        let mut engine = ContextEngine::new();
        engine.add_source(make_source("a.rs", "fn a() {}", SourceType::File));
        engine.add_source(make_source("b.rs", "fn b() {}", SourceType::File));
        engine.add_source(make_source("c_test.rs", "#[test] fn t() {}", SourceType::Test));
        engine.add_source(make_source("readme.md", "# Title", SourceType::Documentation));

        let stats = engine.summary_stats();
        assert_eq!(stats.total_sources, 4);
        assert!(stats.total_tokens_approx > 0);
        assert_eq!(stats.by_type[&SourceType::File], 2);
        assert_eq!(stats.by_type[&SourceType::Test], 1);
        assert_eq!(stats.by_type[&SourceType::Documentation], 1);
    }

    #[test]
    fn test_source_type_variants() {
        // Verify all SourceType variants can be serialized round-tripped.
        let variants = [
            SourceType::File,
            SourceType::Symbol,
            SourceType::Documentation,
            SourceType::GitHistory,
            SourceType::Dependency,
            SourceType::Config,
            SourceType::Test,
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let deserialized: SourceType = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, deserialized);
        }
    }

    // -- Convenience methods ------------------------------------------------

    #[test]
    fn test_add_file_convenience() {
        let mut engine = ContextEngine::new();
        engine.add_file("main.rs", "fn main() {}");

        let stats = engine.summary_stats();
        assert_eq!(stats.total_sources, 1);
        assert_eq!(stats.by_type[&SourceType::File], 1);
    }

    #[test]
    fn test_add_symbol_convenience() {
        let mut engine = ContextEngine::new();
        engine.add_symbol("MyStruct::process", "fn process(&self) { todo!() }");

        let stats = engine.summary_stats();
        assert_eq!(stats.total_sources, 1);
        assert_eq!(stats.by_type[&SourceType::Symbol], 1);
    }

    #[test]
    fn test_reserved_tokens_reduces_budget() {
        let mut engine = ContextEngine::new();
        // ~100 chars => ~25 tokens each.
        for i in 0..10 {
            let content = format!(
                "fn function_{}() {{ let x = {}; let y = x + 1; println!(\"{{x}} {{y}}\"); }}",
                i, i
            );
            engine.add_file(&format!("f_{}.rs", i), &content);
        }

        let tight_budget = ContextBudget {
            max_tokens: 60,
            max_sources: 100,
            reserved_tokens: 40,
        };
        let result = engine.curate("function", &tight_budget);

        // Only 20 tokens available after reservation.
        assert!(result.total_tokens <= 20);
    }

    #[test]
    fn test_curated_context_serialization() {
        let mut engine = ContextEngine::new();
        engine.add_file("test.rs", "fn test() {}");
        let result = engine.curate("test", &default_budget());

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CuratedContext = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.selected_count, result.selected_count);
        assert_eq!(deserialized.original_count, result.original_count);
    }

    #[test]
    fn test_infer_source_type_from_path() {
        assert_eq!(
            infer_source_type(Path::new("src/tests/foo_test.rs")),
            SourceType::Test
        );
        assert_eq!(
            infer_source_type(Path::new("README.md")),
            SourceType::Documentation
        );
        assert_eq!(
            infer_source_type(Path::new("Cargo.toml")),
            SourceType::Dependency
        );
        assert_eq!(
            infer_source_type(Path::new("config.yaml")),
            SourceType::Config
        );
        assert_eq!(
            infer_source_type(Path::new("src/main.rs")),
            SourceType::File
        );
    }
}
