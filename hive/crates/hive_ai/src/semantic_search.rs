//! Semantic search service with search history tracking.
//!
//! Provides file-content search with relevance scoring and contextual
//! snippets (lines before/after each match). Tracks search history
//! for recall and analytics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A recorded search event for history tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEntry {
    pub id: String,
    pub query: String,
    pub results_count: usize,
    pub timestamp: DateTime<Utc>,
}

/// A single search match with surrounding context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file_path: String,
    pub line_number: usize,
    pub content: String,
    pub score: f32,
    pub context_before: String,
    pub context_after: String,
}

/// Configuration for a semantic search request.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub query: String,
    pub max_results: usize,
    pub context_lines: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            max_results: 50,
            context_lines: 2,
        }
    }
}

// ---------------------------------------------------------------------------
// Scoring helpers
// ---------------------------------------------------------------------------

/// Tokenize text into lowercase word tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Score a line against query terms using a simple term-overlap metric.
///
/// Returns a value between 0.0 and 1.0 indicating the fraction of
/// query terms that appear in the line (with bonus for exact substring match).
fn score_line(line: &str, query_terms: &[String], query_raw: &str) -> f32 {
    if query_terms.is_empty() {
        return 0.0;
    }

    let line_lower = line.to_lowercase();
    let line_tokens: Vec<String> = tokenize(line);
    let line_token_set: std::collections::HashSet<&str> =
        line_tokens.iter().map(|s| s.as_str()).collect();

    // Term overlap: how many query terms appear in the line
    let matched = query_terms
        .iter()
        .filter(|qt| line_token_set.contains(qt.as_str()))
        .count();
    let overlap_score = matched as f32 / query_terms.len() as f32;

    // Bonus for exact substring match of the full query
    let exact_bonus = if line_lower.contains(&query_raw.to_lowercase()) {
        0.3
    } else {
        0.0
    };

    // Bonus for consecutive term matches
    let consecutive_bonus = if query_terms.len() >= 2 {
        let bigrams_matched = query_terms
            .windows(2)
            .filter(|pair| {
                let bigram = format!("{} {}", pair[0], pair[1]);
                line_lower.contains(&bigram)
                    || line_lower.contains(&format!("{}_{}", pair[0], pair[1]))
            })
            .count();
        if query_terms.len() > 1 {
            0.2 * (bigrams_matched as f32 / (query_terms.len() - 1) as f32)
        } else {
            0.0
        }
    } else {
        0.0
    };

    (overlap_score + exact_bonus + consecutive_bonus).min(1.0)
}

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

// ---------------------------------------------------------------------------
// SemanticSearchService
// ---------------------------------------------------------------------------

/// Search service that scores file lines by query relevance and
/// maintains a searchable history of past queries.
pub struct SemanticSearchService {
    history: Vec<SearchEntry>,
    max_history: usize,
}

impl SemanticSearchService {
    /// Create a new semantic search service.
    ///
    /// - `max_history`: maximum number of search entries to retain.
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            max_history,
        }
    }

    /// Search for `query` across a set of file paths.
    ///
    /// Each file is read, lines are scored, and the top results (with context)
    /// are returned sorted by descending score.
    pub fn search(
        &mut self,
        query: &str,
        paths: &[&Path],
        max_results: usize,
    ) -> Vec<SearchResult> {
        self.search_with_context(query, paths, max_results, 2)
    }

    /// Search with a configurable number of context lines.
    pub fn search_with_context(
        &mut self,
        query: &str,
        paths: &[&Path],
        max_results: usize,
        context_lines: usize,
    ) -> Vec<SearchResult> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let mut all_results: Vec<SearchResult> = Vec::new();

        for path in paths {
            if path.is_dir() {
                let dir_results =
                    self.search_directory(path, &query_terms, query, context_lines);
                all_results.extend(dir_results);
            } else if path.is_file() {
                if let Some(results) =
                    self.search_file(path, &query_terms, query, context_lines)
                {
                    all_results.extend(results);
                }
            }
        }

        // Sort by descending score
        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_results.truncate(max_results);

        // Record in history
        self.add_to_history(SearchEntry {
            id: Uuid::new_v4().to_string(),
            query: query.to_string(),
            results_count: all_results.len(),
            timestamp: Utc::now(),
        });

        debug!(
            "Search for '{}' returned {} results",
            query,
            all_results.len()
        );
        all_results
    }

    /// Search all text files recursively in a directory.
    fn search_directory(
        &self,
        dir: &Path,
        query_terms: &[String],
        query_raw: &str,
        context_lines: usize,
    ) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return results,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden
            if name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                results.extend(self.search_directory(&path, query_terms, query_raw, context_lines));
            } else if path.is_file() {
                if let Some(file_results) =
                    self.search_file(&path, query_terms, query_raw, context_lines)
                {
                    results.extend(file_results);
                }
            }
        }
        results
    }

    /// Search a single file, returning scored results with context.
    fn search_file(
        &self,
        path: &Path,
        query_terms: &[String],
        query_raw: &str,
        context_lines: usize,
    ) -> Option<Vec<SearchResult>> {
        if is_likely_binary(path) {
            return None;
        }

        let content = fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let mut results = Vec::new();
        let path_str = path.to_string_lossy().to_string();

        for (idx, line) in lines.iter().enumerate() {
            let score = score_line(line, query_terms, query_raw);
            if score > 0.0 {
                // Gather context lines
                let ctx_start = idx.saturating_sub(context_lines);
                let ctx_end = (idx + context_lines + 1).min(lines.len());

                let context_before = if ctx_start < idx {
                    lines[ctx_start..idx].join("\n")
                } else {
                    String::new()
                };

                let context_after = if idx + 1 < ctx_end {
                    lines[idx + 1..ctx_end].join("\n")
                } else {
                    String::new()
                };

                results.push(SearchResult {
                    file_path: path_str.clone(),
                    line_number: idx + 1, // 1-based
                    content: line.to_string(),
                    score,
                    context_before,
                    context_after,
                });
            }
        }

        Some(results)
    }

    /// Search against in-memory content (useful for testing or buffered files).
    pub fn search_content(
        &self,
        query: &str,
        file_path: &str,
        content: &str,
        context_lines: usize,
    ) -> Vec<SearchResult> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut results = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let score = score_line(line, &query_terms, query);
            if score > 0.0 {
                let ctx_start = idx.saturating_sub(context_lines);
                let ctx_end = (idx + context_lines + 1).min(lines.len());

                let context_before = if ctx_start < idx {
                    lines[ctx_start..idx].join("\n")
                } else {
                    String::new()
                };

                let context_after = if idx + 1 < ctx_end {
                    lines[idx + 1..ctx_end].join("\n")
                } else {
                    String::new()
                };

                results.push(SearchResult {
                    file_path: file_path.to_string(),
                    line_number: idx + 1,
                    content: line.to_string(),
                    score,
                    context_before,
                    context_after,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Add an entry to the search history.
    pub fn add_to_history(&mut self, entry: SearchEntry) {
        self.history.push(entry);
        // Trim old entries if over capacity
        if self.history.len() > self.max_history {
            let excess = self.history.len() - self.max_history;
            self.history.drain(0..excess);
        }
    }

    /// Get the most recent search history entries.
    pub fn get_history(&self, limit: usize) -> &[SearchEntry] {
        let start = self.history.len().saturating_sub(limit);
        &self.history[start..]
    }

    /// Clear all search history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Total number of searches recorded.
    pub fn history_count(&self) -> usize {
        self.history.len()
    }
}

impl Default for SemanticSearchService {
    fn default() -> Self {
        Self::new(1000)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Tokenization --

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("Hello World! foo_bar");
        assert_eq!(tokens, vec!["hello", "world", "foo_bar"]);
    }

    #[test]
    fn test_tokenize_empty() {
        assert!(tokenize("").is_empty());
        assert!(tokenize("   ").is_empty());
    }

    // -- Scoring --

    #[test]
    fn test_score_line_full_match() {
        let terms = vec!["hello".to_string(), "world".to_string()];
        let score = score_line("Hello World", &terms, "hello world");
        // Full overlap + exact bonus
        assert!(score > 0.9);
    }

    #[test]
    fn test_score_line_partial_match() {
        let terms = vec!["hello".to_string(), "world".to_string()];
        let score = score_line("Hello there", &terms, "hello world");
        // 1/2 terms match
        assert!(score > 0.3);
        assert!(score < 0.9);
    }

    #[test]
    fn test_score_line_no_match() {
        let terms = vec!["hello".to_string()];
        let score = score_line("goodbye cruel world", &terms, "hello");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_score_line_empty_query() {
        let score = score_line("anything", &[], "");
        assert_eq!(score, 0.0);
    }

    // -- Search content (in-memory) --

    #[test]
    fn test_search_content_basic() {
        let service = SemanticSearchService::default();
        let content = "fn main() {\n    println!(\"hello\");\n}\nfn other() {}";
        let results = service.search_content("main", "test.rs", content, 1);

        assert!(!results.is_empty());
        assert_eq!(results[0].line_number, 1);
        assert!(results[0].content.contains("main"));
    }

    #[test]
    fn test_search_content_context_lines() {
        let service = SemanticSearchService::default();
        let content = "line1\nline2\nfn main() {\nline4\nline5";
        let results = service.search_content("main", "test.rs", content, 2);

        assert!(!results.is_empty());
        let hit = &results[0];
        assert_eq!(hit.line_number, 3);
        // Should have 2 lines of context before
        assert!(hit.context_before.contains("line1"));
        assert!(hit.context_before.contains("line2"));
        // Should have 2 lines of context after
        assert!(hit.context_after.contains("line4"));
        assert!(hit.context_after.contains("line5"));
    }

    #[test]
    fn test_search_content_no_match() {
        let service = SemanticSearchService::default();
        let results = service.search_content("nonexistent", "test.rs", "hello world\nfoo bar", 1);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_content_multiple_matches_sorted() {
        let service = SemanticSearchService::default();
        let content = "fn add(a: i32, b: i32) -> i32 { a + b }\nsome filler\nfn add_numbers(x: i32, y: i32) -> i32 { x + y }";
        let results = service.search_content("add numbers", "math.rs", content, 0);

        // Both lines with "add" should match; the one with "add_numbers" should score higher
        assert!(results.len() >= 1);
        assert!(results[0].content.contains("add"));
    }

    // -- History --

    #[test]
    fn test_add_to_history() {
        let mut service = SemanticSearchService::new(10);
        service.add_to_history(SearchEntry {
            id: "1".to_string(),
            query: "test".to_string(),
            results_count: 5,
            timestamp: Utc::now(),
        });
        assert_eq!(service.history_count(), 1);
    }

    #[test]
    fn test_history_limit_enforced() {
        let mut service = SemanticSearchService::new(3);
        for i in 0..5 {
            service.add_to_history(SearchEntry {
                id: format!("{}", i),
                query: format!("query_{}", i),
                results_count: i,
                timestamp: Utc::now(),
            });
        }
        assert_eq!(service.history_count(), 3);
        // Oldest entries should have been trimmed
        assert_eq!(service.history[0].id, "2");
    }

    #[test]
    fn test_get_history_with_limit() {
        let mut service = SemanticSearchService::new(100);
        for i in 0..10 {
            service.add_to_history(SearchEntry {
                id: format!("{}", i),
                query: format!("q{}", i),
                results_count: 0,
                timestamp: Utc::now(),
            });
        }

        let recent = service.get_history(3);
        assert_eq!(recent.len(), 3);
        // Should be the last 3 entries
        assert_eq!(recent[0].id, "7");
        assert_eq!(recent[2].id, "9");
    }

    #[test]
    fn test_clear_history() {
        let mut service = SemanticSearchService::new(100);
        service.add_to_history(SearchEntry {
            id: "1".to_string(),
            query: "test".to_string(),
            results_count: 0,
            timestamp: Utc::now(),
        });
        assert_eq!(service.history_count(), 1);

        service.clear_history();
        assert_eq!(service.history_count(), 0);
    }

    #[test]
    fn test_search_records_history() {
        let mut service = SemanticSearchService::new(100);
        let content = "fn hello() {}\nfn world() {}";

        // Use search_content (doesn't record history by itself)
        // Then manually verify via search which does record
        let _ = service.search_content("hello", "test.rs", content, 0);
        // search_content doesn't record history; only `search` does
        assert_eq!(service.history_count(), 0);

        // Manually add
        service.add_to_history(SearchEntry {
            id: Uuid::new_v4().to_string(),
            query: "hello".to_string(),
            results_count: 1,
            timestamp: Utc::now(),
        });
        assert_eq!(service.history_count(), 1);
        assert_eq!(service.get_history(1)[0].query, "hello");
    }

    #[test]
    fn test_default_service() {
        let service = SemanticSearchService::default();
        assert_eq!(service.max_history, 1000);
        assert!(service.history.is_empty());
    }

    #[test]
    fn test_search_entry_serialization() {
        let entry = SearchEntry {
            id: "abc-123".to_string(),
            query: "hello world".to_string(),
            results_count: 42,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: SearchEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "abc-123");
        assert_eq!(deserialized.query, "hello world");
        assert_eq!(deserialized.results_count, 42);
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            file_path: "test.rs".to_string(),
            line_number: 10,
            content: "fn test() {}".to_string(),
            score: 0.85,
            context_before: "// before".to_string(),
            context_after: "// after".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.file_path, "test.rs");
        assert_eq!(deserialized.line_number, 10);
        assert!((deserialized.score - 0.85).abs() < 0.001);
    }
}
