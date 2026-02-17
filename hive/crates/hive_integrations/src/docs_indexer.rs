use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ── Types ──────────────────────────────────────────────────────────

/// A single indexed documentation page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub headings: Vec<String>,
    pub code_blocks: Vec<String>,
}

/// A complete index of a documentation site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsIndex {
    pub name: String,
    pub base_url: String,
    pub pages: Vec<DocPage>,
    pub indexed_at: DateTime<Utc>,
}

/// A single search result from querying an index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocSearchResult {
    pub page_url: String,
    pub title: String,
    pub snippet: String,
    pub relevance_score: f64,
}

/// Parsed rules from a robots.txt file.
#[derive(Debug, Clone, Default)]
struct RobotsTxt {
    disallowed: Vec<String>,
}

// ── DocsIndexer ────────────────────────────────────────────────────

/// Documentation site indexer that can crawl, index, and search any
/// documentation website.
///
/// Uses simple HTML parsing (regex-based) to extract text, headings,
/// and code blocks from pages. Provides TF-IDF-like full-text search
/// across indexed pages and respects robots.txt.
pub struct DocsIndexer {
    indexes: HashMap<String, DocsIndex>,
    client: Client,
}

impl DocsIndexer {
    /// Create a new empty indexer.
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("Hive-DocsIndexer/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client for docs indexer")?;

        Ok(Self {
            indexes: HashMap::new(),
            client,
        })
    }

    /// Create a minimal indexer with no indexes and a default HTTP client.
    ///
    /// Used as a fallback when `new()` fails, so that downstream code
    /// that holds an `Arc<DocsIndexer>` can still call `search()` (which
    /// returns empty results).
    pub fn empty() -> Self {
        let client = Client::builder()
            .user_agent("Hive-DocsIndexer/1.0")
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            indexes: HashMap::new(),
            client,
        }
    }

    /// Create a new indexer with a custom HTTP client (useful for testing).
    pub fn with_client(client: Client) -> Self {
        Self {
            indexes: HashMap::new(),
            client,
        }
    }

    // ── Indexing ───────────────────────────────────────────────────

    /// Crawl a documentation site and build an index.
    ///
    /// Starts at `base_url`, discovers internal links, and indexes up
    /// to `max_pages` pages. Respects robots.txt disallow rules.
    pub async fn index_site(
        &mut self,
        name: &str,
        base_url: &str,
        max_pages: usize,
    ) -> Result<DocsIndex> {
        let base_url = base_url.trim_end_matches('/').to_string();
        debug!(name = %name, base_url = %base_url, max_pages = max_pages, "starting site index");

        let robots = self.fetch_robots_txt(&base_url).await;
        let mut visited: Vec<String> = Vec::new();
        let mut queue: Vec<String> = vec![base_url.clone()];
        let mut pages: Vec<DocPage> = Vec::new();

        while let Some(url) = queue.pop() {
            if visited.len() >= max_pages {
                break;
            }
            if visited.contains(&url) {
                continue;
            }
            if !url.starts_with(&base_url) {
                continue;
            }

            let path = url.strip_prefix(&base_url).unwrap_or("/");
            if robots.is_disallowed(path) {
                debug!(url = %url, "skipping disallowed URL");
                continue;
            }

            debug!(url = %url, visited = visited.len(), "fetching page");
            visited.push(url.clone());

            match self.fetch_page(&url).await {
                Ok(html) => {
                    let page = parse_html_page(&url, &html);
                    let links = extract_links(&html, &base_url);
                    for link in links {
                        if !visited.contains(&link) && !queue.contains(&link) {
                            queue.push(link);
                        }
                    }
                    pages.push(page);
                }
                Err(e) => {
                    warn!(url = %url, error = %e, "failed to fetch page, skipping");
                }
            }
        }

        debug!(name = %name, page_count = pages.len(), "indexing complete");

        let index = DocsIndex {
            name: name.to_string(),
            base_url: base_url.clone(),
            pages,
            indexed_at: Utc::now(),
        };

        self.indexes.insert(name.to_string(), index.clone());
        Ok(index)
    }

    // ── Search ────────────────────────────────────────────────────

    /// Search an index by name using TF-IDF-like scoring.
    ///
    /// Returns up to `limit` results ranked by relevance.
    pub fn search(&self, name: &str, query: &str, limit: usize) -> Vec<DocSearchResult> {
        let Some(index) = self.indexes.get(name) else {
            return Vec::new();
        };

        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let total_docs = index.pages.len() as f64;
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        let mut doc_tokens: Vec<Vec<String>> = Vec::new();

        // Compute document frequencies.
        for page in &index.pages {
            let tokens = tokenize(&format!("{} {} {}", page.title, page.content, page.headings.join(" ")));
            for term in &query_terms {
                if tokens.contains(term) {
                    *doc_freq.entry(term.clone()).or_insert(0) += 1;
                }
            }
            doc_tokens.push(tokens);
        }

        // Score each document.
        let mut results: Vec<DocSearchResult> = Vec::new();
        for (i, page) in index.pages.iter().enumerate() {
            let tokens = &doc_tokens[i];
            let token_count = tokens.len() as f64;
            if token_count == 0.0 {
                continue;
            }

            let mut score = 0.0;
            for term in &query_terms {
                let tf = tokens.iter().filter(|t| t == &term).count() as f64 / token_count;
                let df = *doc_freq.get(term).unwrap_or(&0) as f64;
                let idf = if df > 0.0 {
                    (total_docs / df).ln() + 1.0
                } else {
                    0.0
                };
                score += tf * idf;
            }

            // Boost title matches.
            let title_lower = page.title.to_lowercase();
            for term in &query_terms {
                if title_lower.contains(term) {
                    score *= 1.5;
                }
            }

            if score > 0.0 {
                let snippet = generate_snippet(&page.content, &query_terms, 200);
                results.push(DocSearchResult {
                    page_url: page.url.clone(),
                    title: page.title.clone(),
                    snippet,
                    relevance_score: score,
                });
            }
        }

        // Sort by relevance, descending.
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    /// Get relevant documentation context formatted for AI prompting.
    ///
    /// Searches the named index and formats matching sections into a
    /// single context string suitable for including in an LLM prompt.
    pub fn get_context(&self, name: &str, query: &str) -> String {
        let results = self.search(name, query, 5);
        if results.is_empty() {
            return format!("No documentation found for '{}' in index '{}'.", query, name);
        }

        let mut context = format!("## Documentation context for: {}\n\n", query);
        for result in &results {
            context.push_str(&format!("### {} ({})\n", result.title, result.page_url));
            context.push_str(&result.snippet);
            context.push_str("\n\n");
        }
        context
    }

    // ── Index management ──────────────────────────────────────────

    /// List all indexes with their metadata.
    ///
    /// Returns tuples of (name, base_url, page_count, indexed_at).
    pub fn list_indexes(&self) -> Vec<(String, String, usize, DateTime<Utc>)> {
        self.indexes
            .values()
            .map(|idx| {
                (
                    idx.name.clone(),
                    idx.base_url.clone(),
                    idx.pages.len(),
                    idx.indexed_at,
                )
            })
            .collect()
    }

    /// Remove an index by name.
    pub fn remove_index(&mut self, name: &str) {
        debug!(name = %name, "removing docs index");
        self.indexes.remove(name);
    }

    /// Get a reference to a specific index.
    pub fn get_index(&self, name: &str) -> Option<&DocsIndex> {
        self.indexes.get(name)
    }

    // ── Internal helpers ──────────────────────────────────────────

    /// Fetch the HTML content of a single page.
    async fn fetch_page(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("HTTP request failed")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {} for {}", status, url);
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !content_type.contains("text/html") && !content_type.is_empty() {
            anyhow::bail!("non-HTML content type: {}", content_type);
        }

        response.text().await.context("failed to read response body")
    }

    /// Fetch and parse robots.txt for the given base URL.
    async fn fetch_robots_txt(&self, base_url: &str) -> RobotsTxt {
        let robots_url = format!("{}/robots.txt", base_url);
        debug!(url = %robots_url, "fetching robots.txt");

        match self.client.get(&robots_url).send().await {
            Ok(response) if response.status().is_success() => {
                match response.text().await {
                    Ok(body) => parse_robots_txt(&body),
                    Err(_) => RobotsTxt::default(),
                }
            }
            _ => RobotsTxt::default(),
        }
    }
}

impl Default for DocsIndexer {
    fn default() -> Self {
        Self::new().expect("failed to create default DocsIndexer")
    }
}

// ── robots.txt ─────────────────────────────────────────────────────

impl RobotsTxt {
    /// Check whether a path is disallowed by the robots.txt rules.
    fn is_disallowed(&self, path: &str) -> bool {
        let path = if path.is_empty() { "/" } else { path };
        self.disallowed.iter().any(|rule| path.starts_with(rule))
    }
}

/// Parse robots.txt content and extract Disallow rules for all user agents.
fn parse_robots_txt(content: &str) -> RobotsTxt {
    let mut disallowed = Vec::new();
    let mut applies_to_us = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let lower = line.to_lowercase();
        if lower.starts_with("user-agent:") {
            let agent = lower.strip_prefix("user-agent:").unwrap_or("").trim();
            applies_to_us = agent == "*" || agent.contains("hive");
        } else if applies_to_us && lower.starts_with("disallow:") {
            let path = line
                .split_once(':')
                .map(|(_, v)| v.trim())
                .unwrap_or("");
            if !path.is_empty() {
                disallowed.push(path.to_string());
            }
        }
    }

    RobotsTxt { disallowed }
}

// ── HTML parsing ───────────────────────────────────────────────────

/// Parse an HTML page and extract structured content.
fn parse_html_page(url: &str, html: &str) -> DocPage {
    let title = extract_title(html);
    let headings = extract_headings(html);
    let code_blocks = extract_code_blocks(html);
    let content = strip_html_tags(html);

    DocPage {
        url: url.to_string(),
        title,
        content,
        headings,
        code_blocks,
    }
}

/// Extract the <title> tag content.
fn extract_title(html: &str) -> String {
    let re = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap();
    re.captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| strip_html_tags(m.as_str()).trim().to_string())
        .unwrap_or_default()
}

/// Extract all heading tags (h1-h6) content.
fn extract_headings(html: &str) -> Vec<String> {
    let re = Regex::new(r"(?is)<h[1-6][^>]*>(.*?)</h[1-6]>").unwrap();
    re.captures_iter(html)
        .filter_map(|cap| cap.get(1))
        .map(|m| strip_html_tags(m.as_str()).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extract code blocks from <pre> and <code> tags.
fn extract_code_blocks(html: &str) -> Vec<String> {
    let re = Regex::new(r"(?is)<(?:pre|code)[^>]*>(.*?)</(?:pre|code)>").unwrap();
    re.captures_iter(html)
        .filter_map(|cap| cap.get(1))
        .map(|m| decode_html_entities(m.as_str()).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extract internal links from HTML.
fn extract_links(html: &str, base_url: &str) -> Vec<String> {
    let re = Regex::new(r#"(?i)href\s*=\s*["']([^"'#]+)"#).unwrap();
    let mut links = Vec::new();

    for cap in re.captures_iter(html) {
        if let Some(href) = cap.get(1) {
            let href = href.as_str().trim();
            let full_url = resolve_url(href, base_url);
            if let Some(url) = full_url {
                if url.starts_with(base_url) {
                    links.push(url);
                }
            }
        }
    }

    links
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(href: &str, base_url: &str) -> Option<String> {
    if href.starts_with("http://") || href.starts_with("https://") {
        Some(href.to_string())
    } else if href.starts_with('/') {
        // Absolute path: join with origin.
        if let Some(origin) = extract_origin(base_url) {
            Some(format!("{}{}", origin, href))
        } else {
            None
        }
    } else if href.starts_with("mailto:")
        || href.starts_with("javascript:")
        || href.starts_with("tel:")
    {
        None
    } else {
        // Relative path.
        Some(format!("{}/{}", base_url, href))
    }
}

/// Extract the origin (scheme + host) from a URL.
fn extract_origin(url: &str) -> Option<String> {
    let re = Regex::new(r"^(https?://[^/]+)").unwrap();
    re.captures(url)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

/// Strip all HTML tags, leaving only text content.
fn strip_html_tags(html: &str) -> String {
    // Remove script and style blocks entirely.
    let re_script = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let cleaned = re_script.replace_all(html, " ");
    let re_style = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
    let cleaned = re_style.replace_all(&cleaned, " ");

    // Replace block-level tags with newlines.
    let re_block = Regex::new(r"(?i)</?(?:p|div|br|li|tr|h[1-6])[^>]*>").unwrap();
    let cleaned = re_block.replace_all(&cleaned, "\n");

    // Remove all remaining tags.
    let re_tags = Regex::new(r"<[^>]+>").unwrap();
    let cleaned = re_tags.replace_all(&cleaned, " ");

    // Decode common HTML entities.
    let text = decode_html_entities(&cleaned);

    // Collapse whitespace.
    let re_ws = Regex::new(r"[ \t]+").unwrap();
    let text = re_ws.replace_all(&text, " ");

    // Collapse multiple newlines.
    let re_nl = Regex::new(r"\n{3,}").unwrap();
    let text = re_nl.replace_all(&text, "\n\n");

    text.trim().to_string()
}

/// Decode common HTML entities.
fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

// ── TF-IDF helpers ─────────────────────────────────────────────────

/// Tokenize text into lowercase words for search.
fn tokenize(text: &str) -> Vec<String> {
    let re = Regex::new(r"[a-zA-Z0-9_]+").unwrap();
    re.find_iter(&text.to_lowercase())
        .map(|m| m.as_str().to_string())
        .filter(|w| w.len() >= 2)
        .collect()
}

/// Generate a text snippet around the best matching region.
fn generate_snippet(content: &str, query_terms: &[String], max_len: usize) -> String {
    let lower = content.to_lowercase();
    let mut best_pos = 0;
    let mut best_count = 0;

    // Slide a window through the content and find the region with the
    // most query term matches.
    let chars: Vec<char> = lower.chars().collect();
    let window_size = max_len.min(chars.len());

    if chars.is_empty() {
        return String::new();
    }

    let step = 50.max(1);
    let mut pos = 0;
    while pos + window_size <= chars.len() {
        let window: String = chars[pos..pos + window_size].iter().collect();
        let count: usize = query_terms.iter().filter(|t| window.contains(t.as_str())).count();
        if count > best_count {
            best_count = count;
            best_pos = pos;
        }
        pos += step;
    }

    let content_chars: Vec<char> = content.chars().collect();
    let end = (best_pos + max_len).min(content_chars.len());
    let snippet: String = content_chars[best_pos..end].iter().collect();

    let snippet = snippet.trim().to_string();
    if best_pos > 0 {
        format!("...{}", snippet)
    } else if end < content_chars.len() {
        format!("{}...", snippet)
    } else {
        snippet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── HTML parsing ──────────────────────────────────────────────

    #[test]
    fn test_extract_title() {
        let html = "<html><head><title>My Docs</title></head><body></body></html>";
        assert_eq!(extract_title(html), "My Docs");
    }

    #[test]
    fn test_extract_title_missing() {
        let html = "<html><body>no title</body></html>";
        assert_eq!(extract_title(html), "");
    }

    #[test]
    fn test_extract_headings() {
        let html = "<h1>Intro</h1><p>text</p><h2>Details</h2><h3>Sub</h3>";
        let headings = extract_headings(html);
        assert_eq!(headings, vec!["Intro", "Details", "Sub"]);
    }

    #[test]
    fn test_extract_headings_empty() {
        let html = "<p>no headings here</p>";
        let headings = extract_headings(html);
        assert!(headings.is_empty());
    }

    #[test]
    fn test_extract_code_blocks() {
        let html = "<p>intro</p><pre>fn main() {}</pre><code>let x = 1;</code>";
        let blocks = extract_code_blocks(html);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "fn main() {}");
        assert_eq!(blocks[1], "let x = 1;");
    }

    #[test]
    fn test_strip_html_tags() {
        let html = "<p>Hello <b>world</b></p><script>alert('x')</script>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("<p>"));
    }

    #[test]
    fn test_decode_html_entities() {
        assert_eq!(decode_html_entities("&amp; &lt; &gt;"), "& < >");
        assert_eq!(decode_html_entities("&quot;hello&quot;"), "\"hello\"");
        assert_eq!(decode_html_entities("it&#39;s"), "it's");
    }

    // ── Link extraction ───────────────────────────────────────────

    #[test]
    fn test_extract_links_absolute() {
        let html = r#"<a href="https://docs.example.com/guide">Guide</a>"#;
        let links = extract_links(html, "https://docs.example.com");
        assert_eq!(links, vec!["https://docs.example.com/guide"]);
    }

    #[test]
    fn test_extract_links_relative() {
        let html = r#"<a href="/api/reference">API</a>"#;
        let links = extract_links(html, "https://docs.example.com");
        assert_eq!(links, vec!["https://docs.example.com/api/reference"]);
    }

    #[test]
    fn test_extract_links_external_filtered() {
        let html = r#"<a href="https://other.com/page">External</a>"#;
        let links = extract_links(html, "https://docs.example.com");
        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_links_mailto_filtered() {
        let html = r#"<a href="mailto:test@example.com">Email</a>"#;
        let links = extract_links(html, "https://docs.example.com");
        assert!(links.is_empty());
    }

    // ── URL resolution ────────────────────────────────────────────

    #[test]
    fn test_resolve_url_absolute() {
        let url = resolve_url("https://docs.example.com/page", "https://docs.example.com");
        assert_eq!(url, Some("https://docs.example.com/page".to_string()));
    }

    #[test]
    fn test_resolve_url_relative_path() {
        let url = resolve_url("/guide/intro", "https://docs.example.com");
        assert_eq!(url, Some("https://docs.example.com/guide/intro".to_string()));
    }

    #[test]
    fn test_resolve_url_relative_segment() {
        let url = resolve_url("page2", "https://docs.example.com/guide");
        assert_eq!(url, Some("https://docs.example.com/guide/page2".to_string()));
    }

    #[test]
    fn test_extract_origin() {
        assert_eq!(
            extract_origin("https://docs.example.com/path/to/page"),
            Some("https://docs.example.com".to_string())
        );
    }

    // ── robots.txt ────────────────────────────────────────────────

    #[test]
    fn test_parse_robots_txt_disallow() {
        let content = "User-agent: *\nDisallow: /admin\nDisallow: /private/\n";
        let robots = parse_robots_txt(content);
        assert!(robots.is_disallowed("/admin"));
        assert!(robots.is_disallowed("/admin/page"));
        assert!(robots.is_disallowed("/private/secret"));
        assert!(!robots.is_disallowed("/public"));
    }

    #[test]
    fn test_parse_robots_txt_empty() {
        let robots = parse_robots_txt("");
        assert!(!robots.is_disallowed("/anything"));
    }

    #[test]
    fn test_parse_robots_txt_only_other_agent() {
        let content = "User-agent: Googlebot\nDisallow: /secret\n";
        let robots = parse_robots_txt(content);
        assert!(!robots.is_disallowed("/secret"));
    }

    // ── Tokenizer ─────────────────────────────────────────────────

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello, World! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"this".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        // Single-character words filtered out.
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    // ── Snippet generation ────────────────────────────────────────

    #[test]
    fn test_generate_snippet() {
        let content = "This is a document about Rust programming. Rust is fast and safe.";
        let snippet = generate_snippet(content, &["rust".to_string()], 50);
        assert!(!snippet.is_empty());
    }

    #[test]
    fn test_generate_snippet_empty_content() {
        let snippet = generate_snippet("", &["rust".to_string()], 100);
        assert!(snippet.is_empty());
    }

    // ── TF-IDF search ─────────────────────────────────────────────

    #[test]
    fn test_search_nonexistent_index() {
        let indexer = DocsIndexer::new().unwrap();
        let results = indexer.search("nonexistent", "rust", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_query() {
        let mut indexer = DocsIndexer::new().unwrap();
        indexer.indexes.insert(
            "test".to_string(),
            DocsIndex {
                name: "test".to_string(),
                base_url: "https://example.com".to_string(),
                pages: vec![DocPage {
                    url: "https://example.com/page".to_string(),
                    title: "Test Page".to_string(),
                    content: "Some content here".to_string(),
                    headings: vec![],
                    code_blocks: vec![],
                }],
                indexed_at: Utc::now(),
            },
        );
        let results = indexer.search("test", "", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_returns_results() {
        let mut indexer = DocsIndexer::new().unwrap();
        indexer.indexes.insert(
            "rust".to_string(),
            DocsIndex {
                name: "rust".to_string(),
                base_url: "https://doc.rust-lang.org".to_string(),
                pages: vec![
                    DocPage {
                        url: "https://doc.rust-lang.org/ownership".to_string(),
                        title: "Ownership".to_string(),
                        content: "Rust ownership is a key concept. Ownership rules govern memory.".to_string(),
                        headings: vec!["Ownership".to_string()],
                        code_blocks: vec![],
                    },
                    DocPage {
                        url: "https://doc.rust-lang.org/borrowing".to_string(),
                        title: "Borrowing".to_string(),
                        content: "Borrowing lets you reference data without ownership.".to_string(),
                        headings: vec!["Borrowing".to_string()],
                        code_blocks: vec![],
                    },
                    DocPage {
                        url: "https://doc.rust-lang.org/types".to_string(),
                        title: "Types".to_string(),
                        content: "Rust has a strong type system with generics and traits.".to_string(),
                        headings: vec!["Types".to_string()],
                        code_blocks: vec![],
                    },
                ],
                indexed_at: Utc::now(),
            },
        );

        let results = indexer.search("rust", "ownership", 10);
        assert!(!results.is_empty());
        // The ownership page should rank highest.
        assert_eq!(results[0].page_url, "https://doc.rust-lang.org/ownership");
        assert!(results[0].relevance_score > 0.0);
    }

    #[test]
    fn test_search_respects_limit() {
        let mut indexer = DocsIndexer::new().unwrap();
        let pages: Vec<DocPage> = (0..20)
            .map(|i| DocPage {
                url: format!("https://example.com/page{}", i),
                title: format!("Page {}", i),
                content: "rust programming language".to_string(),
                headings: vec![],
                code_blocks: vec![],
            })
            .collect();
        indexer.indexes.insert(
            "many".to_string(),
            DocsIndex {
                name: "many".to_string(),
                base_url: "https://example.com".to_string(),
                pages,
                indexed_at: Utc::now(),
            },
        );

        let results = indexer.search("many", "rust", 5);
        assert!(results.len() <= 5);
    }

    // ── Context generation ────────────────────────────────────────

    #[test]
    fn test_get_context_no_index() {
        let indexer = DocsIndexer::new().unwrap();
        let ctx = indexer.get_context("missing", "query");
        assert!(ctx.contains("No documentation found"));
    }

    #[test]
    fn test_get_context_with_results() {
        let mut indexer = DocsIndexer::new().unwrap();
        indexer.indexes.insert(
            "test".to_string(),
            DocsIndex {
                name: "test".to_string(),
                base_url: "https://example.com".to_string(),
                pages: vec![DocPage {
                    url: "https://example.com/setup".to_string(),
                    title: "Setup Guide".to_string(),
                    content: "Follow this setup guide to install the application.".to_string(),
                    headings: vec!["Setup".to_string()],
                    code_blocks: vec![],
                }],
                indexed_at: Utc::now(),
            },
        );

        let ctx = indexer.get_context("test", "setup");
        assert!(ctx.contains("Documentation context for"));
        assert!(ctx.contains("Setup Guide"));
    }

    // ── Index management ──────────────────────────────────────────

    #[test]
    fn test_list_indexes_empty() {
        let indexer = DocsIndexer::new().unwrap();
        assert!(indexer.list_indexes().is_empty());
    }

    #[test]
    fn test_list_and_remove_index() {
        let mut indexer = DocsIndexer::new().unwrap();
        indexer.indexes.insert(
            "docs".to_string(),
            DocsIndex {
                name: "docs".to_string(),
                base_url: "https://docs.example.com".to_string(),
                pages: vec![],
                indexed_at: Utc::now(),
            },
        );

        let list = indexer.list_indexes();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "docs");
        assert_eq!(list[0].1, "https://docs.example.com");
        assert_eq!(list[0].2, 0);

        indexer.remove_index("docs");
        assert!(indexer.list_indexes().is_empty());
    }

    #[test]
    fn test_get_index() {
        let mut indexer = DocsIndexer::new().unwrap();
        indexer.indexes.insert(
            "test".to_string(),
            DocsIndex {
                name: "test".to_string(),
                base_url: "https://example.com".to_string(),
                pages: vec![],
                indexed_at: Utc::now(),
            },
        );

        assert!(indexer.get_index("test").is_some());
        assert!(indexer.get_index("missing").is_none());
    }

    // ── Page parsing integration ──────────────────────────────────

    #[test]
    fn test_parse_html_page() {
        let html = r#"
            <html>
            <head><title>API Reference</title></head>
            <body>
                <h1>API Reference</h1>
                <p>This is the API documentation.</p>
                <h2>Authentication</h2>
                <p>Use bearer tokens.</p>
                <pre>Authorization: Bearer token123</pre>
            </body>
            </html>
        "#;

        let page = parse_html_page("https://example.com/api", html);
        assert_eq!(page.url, "https://example.com/api");
        assert_eq!(page.title, "API Reference");
        assert!(page.headings.contains(&"API Reference".to_string()));
        assert!(page.headings.contains(&"Authentication".to_string()));
        assert!(!page.code_blocks.is_empty());
        assert!(page.content.contains("API documentation"));
    }

    #[test]
    fn test_default_indexer() {
        let indexer = DocsIndexer::default();
        assert!(indexer.list_indexes().is_empty());
    }
}
