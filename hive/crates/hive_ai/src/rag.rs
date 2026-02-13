//! Retrieval-Augmented Generation (RAG) service.
//!
//! Provides document chunking, TF-IDF indexing, and context assembly
//! for feeding relevant code/document snippets into LLM prompts.

use anyhow::{Context, Result};
use hive_fs::is_likely_binary;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A chunk of a document with optional embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: String,
    pub source_file: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub embedding: Option<Vec<f32>>,
}

/// A query against the RAG index.
#[derive(Debug, Clone)]
pub struct RagQuery {
    pub query: String,
    pub max_results: usize,
    pub min_similarity: f32,
}

/// Result of a RAG query with scored chunks and assembled context.
#[derive(Debug, Clone)]
pub struct RagResult {
    pub chunks: Vec<ScoredChunk>,
    /// Pre-assembled context string suitable for LLM injection.
    pub context: String,
}

/// A document chunk with its relevance score.
#[derive(Debug, Clone)]
pub struct ScoredChunk {
    pub chunk: DocumentChunk,
    pub score: f32,
}

/// Statistics about the current RAG index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_chunks: usize,
    pub total_files: usize,
    pub total_tokens_estimate: usize,
}

// ---------------------------------------------------------------------------
// TF-IDF helpers
// ---------------------------------------------------------------------------

/// Tokenize text into lowercase word tokens, stripping non-alphanumeric chars.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Compute term frequency (TF) for a list of tokens.
fn term_frequency(tokens: &[String]) -> HashMap<String, f32> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for token in tokens {
        *counts.entry(token.clone()).or_insert(0) += 1;
    }
    let total = tokens.len() as f32;
    if total == 0.0 {
        return HashMap::new();
    }
    counts
        .into_iter()
        .map(|(term, count)| (term, count as f32 / total))
        .collect()
}

/// Compute inverse document frequency (IDF) for terms across documents.
fn inverse_document_frequency(term: &str, document_token_sets: &[HashSet<String>]) -> f32 {
    let n = document_token_sets.len() as f32;
    if n == 0.0 {
        return 0.0;
    }
    let df = document_token_sets
        .iter()
        .filter(|doc| doc.contains(term))
        .count() as f32;
    if df == 0.0 {
        return 0.0;
    }
    (n / df).ln() + 1.0
}

/// Compute TF-IDF vector for a token list given pre-computed IDF values.
fn tfidf_vector(tokens: &[String], idf_map: &HashMap<String, f32>) -> HashMap<String, f32> {
    let tf = term_frequency(tokens);
    tf.into_iter()
        .map(|(term, tf_val)| {
            let idf_val = idf_map.get(&term).copied().unwrap_or(0.0);
            (term, tf_val * idf_val)
        })
        .collect()
}

/// Cosine similarity between two sparse vectors represented as HashMaps.
pub fn cosine_similarity(a: &HashMap<String, f32>, b: &HashMap<String, f32>) -> f32 {
    let dot: f32 = a
        .iter()
        .filter_map(|(k, v)| b.get(k).map(|bv| v * bv))
        .sum();
    let mag_a: f32 = a.values().map(|v| v * v).sum::<f32>().sqrt();
    let mag_b: f32 = b.values().map(|v| v * v).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}

/// Rough token estimate (~4 chars per token).
fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

// ---------------------------------------------------------------------------
// RagService
// ---------------------------------------------------------------------------

/// RAG service that indexes documents into chunks and retrieves relevant
/// context for LLM queries using TF-IDF similarity.
pub struct RagService {
    index: Vec<DocumentChunk>,
    chunk_size: usize,
    overlap: usize,
    /// Cached IDF values for all terms across indexed documents.
    cached_idf: HashMap<String, f32>,
    /// Cached token sets per document chunk (parallel to `index`).
    cached_doc_tokens: Vec<HashSet<String>>,
    /// Cached TF-IDF vectors per document chunk (parallel to `index`).
    cached_tfidf_vectors: Vec<HashMap<String, f32>>,
}

impl RagService {
    /// Create a new RAG service.
    ///
    /// - `chunk_size`: target number of lines per chunk.
    /// - `overlap`: number of overlapping lines between consecutive chunks.
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        Self {
            index: Vec::new(),
            chunk_size: chunk_size.max(1),
            overlap: overlap.min(chunk_size.saturating_sub(1)),
            cached_idf: HashMap::new(),
            cached_doc_tokens: Vec::new(),
            cached_tfidf_vectors: Vec::new(),
        }
    }

    /// Split a file's content into chunks and add them to the index.
    pub fn index_file(&mut self, path: &str, content: &str) {
        self.add_file_chunks(path, content);
        self.rebuild_cache();
    }

    /// Add chunks for a file without rebuilding the cache.
    /// Use `rebuild_cache()` after batch additions.
    fn add_file_chunks(&mut self, path: &str, content: &str) {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return;
        }

        let step = self.chunk_size.saturating_sub(self.overlap).max(1);
        let mut start = 0;

        while start < lines.len() {
            let end = (start + self.chunk_size).min(lines.len());
            let chunk_content = lines[start..end].join("\n");

            self.index.push(DocumentChunk {
                id: Uuid::new_v4().to_string(),
                source_file: path.to_string(),
                content: chunk_content,
                start_line: start + 1, // 1-based
                end_line: end,         // inclusive of last line
                embedding: None,
            });

            start += step;
            if end == lines.len() {
                break;
            }
        }

        debug!(
            "Indexed file '{}': {} lines, {} chunks",
            path,
            lines.len(),
            self.index.iter().filter(|c| c.source_file == path).count()
        );
    }

    /// Rebuild cached IDF map, document token sets, and TF-IDF vectors.
    fn rebuild_cache(&mut self) {
        // Build token sets for all chunks.
        self.cached_doc_tokens = self
            .index
            .iter()
            .map(|chunk| tokenize(&chunk.content).into_iter().collect())
            .collect();

        // Collect all unique terms.
        let all_terms: HashSet<String> = self
            .cached_doc_tokens
            .iter()
            .flat_map(|s| s.iter().cloned())
            .collect();

        // Compute IDF for all terms.
        self.cached_idf = all_terms
            .iter()
            .map(|term| {
                (
                    term.clone(),
                    inverse_document_frequency(term, &self.cached_doc_tokens),
                )
            })
            .collect();

        // Compute TF-IDF vector for each chunk.
        self.cached_tfidf_vectors = self
            .cached_doc_tokens
            .iter()
            .map(|token_set| {
                let tokens: Vec<String> = token_set.iter().cloned().collect();
                tfidf_vector(&tokens, &self.cached_idf)
            })
            .collect();
    }

    /// Recursively index all text files in a directory.
    ///
    /// Skips binary files and hidden directories. Returns the number of
    /// files successfully indexed.
    pub fn index_directory(&mut self, path: &Path) -> Result<usize> {
        let count = self.index_directory_inner(path)?;
        self.rebuild_cache();
        Ok(count)
    }

    /// Recursive directory indexing without cache rebuild (called by `index_directory`).
    fn index_directory_inner(&mut self, path: &Path) -> Result<usize> {
        let mut count = 0;

        let entries: Vec<_> = fs::read_dir(path)
            .with_context(|| format!("Failed to read directory: {}", path.display()))?
            .collect();

        for entry in entries {
            let entry = entry?;
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files/directories
            if file_name.starts_with('.') {
                continue;
            }

            if entry_path.is_dir() {
                count += self.index_directory_inner(&entry_path)?;
            } else if entry_path.is_file() {
                // Skip likely binary files
                if is_likely_binary(&entry_path) {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&entry_path) {
                    let path_str = entry_path.to_string_lossy().to_string();
                    self.add_file_chunks(&path_str, &content);
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Query the index for chunks relevant to the given query.
    pub fn query(&self, rag_query: &RagQuery) -> Result<RagResult> {
        if self.index.is_empty() {
            return Ok(RagResult {
                chunks: Vec::new(),
                context: String::new(),
            });
        }

        // Compute TF-IDF vector for the query using the cached IDF map.
        let query_tokens = tokenize(&rag_query.query);
        let query_vec = tfidf_vector(&query_tokens, &self.cached_idf);

        // Score each chunk using the cached TF-IDF vectors.
        // Collect indices + scores first, then clone only the top results.
        let mut scored_indices: Vec<(usize, f32)> = self
            .cached_tfidf_vectors
            .iter()
            .enumerate()
            .map(|(i, chunk_vec)| {
                let score = cosine_similarity(&query_vec, chunk_vec);
                (i, score)
            })
            .filter(|(_, score)| *score >= rag_query.min_similarity)
            .collect();

        // Sort by descending score
        scored_indices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored_indices.truncate(rag_query.max_results);

        // Clone only the top-scoring chunks.
        let scored: Vec<ScoredChunk> = scored_indices
            .iter()
            .map(|&(i, score)| ScoredChunk {
                chunk: self.index[i].clone(),
                score,
            })
            .collect();

        // Assemble context
        let context = scored
            .iter()
            .map(|sc| {
                format!(
                    "--- {} (lines {}-{}) ---\n{}",
                    sc.chunk.source_file, sc.chunk.start_line, sc.chunk.end_line, sc.chunk.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(RagResult {
            chunks: scored,
            context,
        })
    }

    /// Build a context string from the most relevant chunks, limited by
    /// an approximate token budget.
    pub fn build_context(&self, query: &str, max_tokens: usize) -> String {
        let rag_query = RagQuery {
            query: query.to_string(),
            max_results: 50, // fetch plenty, then trim by token budget
            min_similarity: 0.01,
        };

        let result = match self.query(&rag_query) {
            Ok(r) => r,
            Err(_) => return String::new(),
        };

        let mut context = String::new();
        let mut tokens_used = 0;

        for sc in &result.chunks {
            let snippet = format!(
                "--- {} (lines {}-{}) ---\n{}\n\n",
                sc.chunk.source_file, sc.chunk.start_line, sc.chunk.end_line, sc.chunk.content
            );
            let snippet_tokens = estimate_tokens(&snippet);
            if tokens_used + snippet_tokens > max_tokens {
                break;
            }
            context.push_str(&snippet);
            tokens_used += snippet_tokens;
        }

        context
    }

    /// Clear the entire index.
    pub fn clear_index(&mut self) {
        self.index.clear();
        self.cached_idf.clear();
        self.cached_doc_tokens.clear();
        self.cached_tfidf_vectors.clear();
    }

    /// Return statistics about the current index.
    pub fn stats(&self) -> IndexStats {
        let files: HashSet<&str> = self.index.iter().map(|c| c.source_file.as_str()).collect();
        let total_tokens: usize = self.index.iter().map(|c| estimate_tokens(&c.content)).sum();

        IndexStats {
            total_chunks: self.index.len(),
            total_files: files.len(),
            total_tokens_estimate: total_tokens,
        }
    }

    /// Return a reference to all indexed chunks.
    pub fn chunks(&self) -> &[DocumentChunk] {
        &self.index
    }
}

impl Default for RagService {
    fn default() -> Self {
        Self::new(50, 10) // 50-line chunks with 10-line overlap
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("Hello World! foo_bar");
        assert_eq!(tokens, vec!["hello", "world", "foo_bar"]);
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_term_frequency() {
        let tokens = vec![
            "hello".to_string(),
            "world".to_string(),
            "hello".to_string(),
        ];
        let tf = term_frequency(&tokens);
        assert!((tf["hello"] - 2.0 / 3.0).abs() < 0.001);
        assert!((tf["world"] - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a: HashMap<String, f32> = [("hello".into(), 1.0), ("world".into(), 2.0)].into();
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a: HashMap<String, f32> = [("hello".into(), 1.0)].into();
        let b: HashMap<String, f32> = [("world".into(), 1.0)].into();
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: HashMap<String, f32> = HashMap::new();
        let b: HashMap<String, f32> = [("world".into(), 1.0)].into();
        assert_eq!(cosine_similarity(&a, &b), 0.0);
        assert_eq!(cosine_similarity(&a, &a), 0.0);
    }

    #[test]
    fn test_chunk_splitting_basic() {
        let mut service = RagService::new(3, 1);
        let content = "line1\nline2\nline3\nline4\nline5\nline6";
        service.index_file("test.rs", content);

        // With 6 lines, chunk_size=3, overlap=1 (step=2):
        // Chunk 1: lines 1-3, Chunk 2: lines 3-5, Chunk 3: lines 5-6
        assert!(service.index.len() >= 2);
        assert_eq!(service.index[0].start_line, 1);
        assert_eq!(service.index[0].end_line, 3);
    }

    #[test]
    fn test_chunk_splitting_no_overlap() {
        let mut service = RagService::new(2, 0);
        let content = "line1\nline2\nline3\nline4";
        service.index_file("test.rs", content);

        assert_eq!(service.index.len(), 2);
        assert_eq!(service.index[0].start_line, 1);
        assert_eq!(service.index[0].end_line, 2);
        assert_eq!(service.index[1].start_line, 3);
        assert_eq!(service.index[1].end_line, 4);
    }

    #[test]
    fn test_index_file_empty_content() {
        let mut service = RagService::default();
        service.index_file("empty.rs", "");
        assert_eq!(service.index.len(), 0);
    }

    #[test]
    fn test_query_empty_index() {
        let service = RagService::default();
        let query = RagQuery {
            query: "hello world".to_string(),
            max_results: 5,
            min_similarity: 0.0,
        };
        let result = service.query(&query).unwrap();
        assert!(result.chunks.is_empty());
        assert!(result.context.is_empty());
    }

    #[test]
    fn test_query_returns_relevant_chunks() {
        let mut service = RagService::new(5, 0);
        service.index_file("math.rs", "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}");
        service.index_file(
            "greet.rs",
            "fn greet(name: &str) {\n    println!(\"Hello {}\", name);\n}",
        );

        let query = RagQuery {
            query: "add numbers".to_string(),
            max_results: 5,
            min_similarity: 0.0,
        };
        let result = service.query(&query).unwrap();
        assert!(!result.chunks.is_empty());
        // The math.rs chunk should score higher
        assert!(result.chunks[0].chunk.source_file == "math.rs");
    }

    #[test]
    fn test_query_respects_max_results() {
        let mut service = RagService::new(2, 0);
        for i in 0..10 {
            service.index_file(&format!("file{}.rs", i), &format!("fn func{}() {{}}", i));
        }
        let query = RagQuery {
            query: "fn func".to_string(),
            max_results: 3,
            min_similarity: 0.0,
        };
        let result = service.query(&query).unwrap();
        assert!(result.chunks.len() <= 3);
    }

    #[test]
    fn test_query_respects_min_similarity() {
        let mut service = RagService::new(5, 0);
        service.index_file("rust.rs", "fn main() { println!(\"hello\"); }");
        service.index_file("notes.txt", "the quick brown fox jumps over the lazy dog");

        let query = RagQuery {
            query: "fn main println hello".to_string(),
            max_results: 10,
            min_similarity: 0.5,
        };
        let result = service.query(&query).unwrap();
        // All returned chunks must meet the min similarity threshold
        for sc in &result.chunks {
            assert!(sc.score >= 0.5);
        }
    }

    #[test]
    fn test_build_context_token_limit() {
        let mut service = RagService::new(5, 0);
        for i in 0..20 {
            let content = format!("fn function_{}() {{\n    // implementation {}\n}}", i, i);
            service.index_file(&format!("file{}.rs", i), &content);
        }

        // Very small token budget: should only include a few chunks
        let context = service.build_context("function implementation", 50);
        let token_est = estimate_tokens(&context);
        assert!(token_est <= 60); // some slack for the last partial add
    }

    #[test]
    fn test_stats() {
        let mut service = RagService::new(3, 0);
        service.index_file("a.rs", "line1\nline2\nline3\nline4\nline5\nline6");
        service.index_file("b.rs", "alpha\nbeta\ngamma");

        let stats = service.stats();
        assert_eq!(stats.total_files, 2);
        assert!(stats.total_chunks >= 3); // at least 2 from a.rs + 1 from b.rs
        assert!(stats.total_tokens_estimate > 0);
    }

    #[test]
    fn test_clear_index() {
        let mut service = RagService::new(5, 0);
        service.index_file("test.rs", "fn test() {}");
        assert!(!service.index.is_empty());

        service.clear_index();
        assert!(service.index.is_empty());
        assert_eq!(service.stats().total_chunks, 0);
        assert_eq!(service.stats().total_files, 0);
    }

    #[test]
    fn test_chunk_ids_are_unique() {
        let mut service = RagService::new(2, 0);
        service.index_file("test.rs", "line1\nline2\nline3\nline4");

        let ids: HashSet<&str> = service.index.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), service.index.len());
    }

    #[test]
    fn test_idf_computation() {
        let doc1: HashSet<String> = ["hello", "world"].iter().map(|s| s.to_string()).collect();
        let doc2: HashSet<String> = ["hello", "rust"].iter().map(|s| s.to_string()).collect();
        let docs = vec![doc1, doc2];

        // "hello" appears in both docs, "world" in one
        let idf_hello = inverse_document_frequency("hello", &docs);
        let idf_world = inverse_document_frequency("world", &docs);
        // "world" should have higher IDF (rarer)
        assert!(idf_world > idf_hello);
    }

    #[test]
    fn test_context_format() {
        let mut service = RagService::new(5, 0);
        service.index_file("test.rs", "fn hello() {\n    println!(\"hello\");\n}");

        let query = RagQuery {
            query: "hello".to_string(),
            max_results: 1,
            min_similarity: 0.0,
        };
        let result = service.query(&query).unwrap();
        assert!(result.context.contains("test.rs"));
        assert!(result.context.contains("---"));
        assert!(result.context.contains("hello"));
    }

    #[test]
    fn test_default_service() {
        let service = RagService::default();
        assert_eq!(service.chunk_size, 50);
        assert_eq!(service.overlap, 10);
        assert!(service.index.is_empty());
    }
}
