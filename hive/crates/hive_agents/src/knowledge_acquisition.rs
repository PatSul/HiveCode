//! Knowledge Acquisition Agent — research new domains by fetching documentation,
//! parsing HTML, caching results, and synthesizing knowledge summaries via AI.
//!
//! This module enables Hive to autonomously learn about new technologies and
//! domains by fetching official documentation from a curated allowlist of
//! domains, extracting structured content (text + code blocks), and using an
//! AI executor to synthesize concise knowledge summaries.

use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::collective_memory::{CollectiveMemory, MemoryCategory, MemoryEntry};
use crate::hivemind::AiExecutor;
use hive_ai::context_engine::{ContextEngine, ContextSource, SourceType};
use hive_ai::types::{ChatMessage, ChatRequest, MessageRole};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Curated allowlist of documentation domains considered safe to fetch.
const DEFAULT_KNOWLEDGE_DOMAINS: &[&str] = &[
    "docs.rs",
    "doc.rust-lang.org",
    "docs.python.org",
    "developer.mozilla.org",
    "kubernetes.io",
    "docs.docker.com",
    "react.dev",
    "nextjs.org",
    "docs.aws.amazon.com",
    "cloud.google.com",
    "learn.microsoft.com",
    "redis.io",
    "www.postgresql.org",
    "graphql.org",
    "developer.hashicorp.com",
    "go.dev",
    "typescriptlang.org",
    "svelte.dev",
    "vuejs.org",
    "angular.io",
    "tailwindcss.com",
    "expressjs.com",
    "fastapi.tiangolo.com",
];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Configuration for the knowledge acquisition agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    /// Domains from which pages may be fetched.
    pub allowed_domains: Vec<String>,
    /// Local directory for cached page data.
    pub cache_dir: PathBuf,
    /// Maximum number of pages to fetch per research session.
    pub max_pages_per_session: usize,
    /// Maximum allowed response body size in bytes.
    pub max_page_bytes: usize,
    /// Time-to-live for cached pages, in hours.
    pub cache_ttl_hours: u64,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            allowed_domains: DEFAULT_KNOWLEDGE_DOMAINS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            cache_dir: PathBuf::from(".hive/knowledge_cache"),
            max_pages_per_session: 10,
            max_page_bytes: 512_000,
            cache_ttl_hours: 168, // 7 days
        }
    }
}

/// A fetched and parsed documentation page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePage {
    /// The original URL.
    pub url: String,
    /// The `<title>` content extracted from the page.
    pub title: String,
    /// Cleaned plain-text content of the page.
    pub text_content: String,
    /// Code blocks extracted from `<pre>` / `<code>` elements.
    pub code_blocks: Vec<CodeBlock>,
    /// When the page was fetched.
    pub fetched_at: DateTime<Utc>,
    /// SHA-256 hex digest of the raw HTML body.
    pub content_hash: String,
    /// The domain the page was fetched from.
    pub source_domain: String,
}

/// A code snippet extracted from a documentation page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    /// Programming language, if detected from a `class` attribute.
    pub language: Option<String>,
    /// The raw code content.
    pub content: String,
}

/// AI-synthesized summary of a research topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSummary {
    /// The research topic.
    pub topic: String,
    /// Prose summary of the documentation.
    pub summary: String,
    /// Key concepts discovered.
    pub key_concepts: Vec<String>,
    /// Relevant CLI commands or API calls.
    pub relevant_commands: Vec<String>,
    /// Illustrative code examples.
    pub code_examples: Vec<CodeBlock>,
    /// URLs that contributed to the summary.
    pub source_urls: Vec<String>,
    /// When the summary was created.
    pub created_at: DateTime<Utc>,
}

/// Result returned by a complete research session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionResult {
    /// The topic that was researched.
    pub topic: String,
    /// Number of pages fetched from the network.
    pub pages_fetched: usize,
    /// Number of pages served from local cache.
    pub pages_from_cache: usize,
    /// The synthesized knowledge summary.
    pub summary: KnowledgeSummary,
    /// Whether the summary was injected into a `ContextEngine`.
    pub stored_as_context: bool,
    /// Total AI cost incurred during the session.
    pub total_cost: f64,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// HTML parsing
// ---------------------------------------------------------------------------

/// Parse raw HTML into (title, text_content, code_blocks).
///
/// 1. Extracts `<title>` content.
/// 2. Extracts `<pre>` and `<code>` blocks with optional language detection.
/// 3. Strips `<script>`, `<style>`, `<nav>`, `<footer>`, `<header>` blocks.
/// 4. Removes remaining HTML tags.
/// 5. Decodes common HTML entities.
/// 6. Collapses excessive whitespace.
pub fn parse_html(html: &str) -> (String, String, Vec<CodeBlock>) {
    // 1. Extract title
    let title = extract_between(html, "<title>", "</title>")
        .unwrap_or_default()
        .trim()
        .to_string();

    // 2. Extract code blocks before stripping tags
    let code_blocks = extract_code_blocks(html);

    // 3. Remove unwanted sections
    let mut text = html.to_string();
    for tag in &["script", "style", "nav", "footer", "header"] {
        text = strip_tag_block(&text, tag);
    }

    // 4. Strip all HTML tags
    let tag_re = Regex::new(r"<[^>]*>").expect("valid regex");
    let text = tag_re.replace_all(&text, " ").to_string();

    // 5. Decode entities
    let text = decode_entities(&text);

    // 6. Collapse whitespace
    let text = collapse_whitespace(&text);

    (title, text, code_blocks)
}

/// Extract the text between the first occurrence of `open` and `close` (case-insensitive).
fn extract_between(html: &str, open: &str, close: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find(&open.to_lowercase())? + open.len();
    let end = lower[start..].find(&close.to_lowercase())? + start;
    Some(html[start..end].to_string())
}

/// Extract `<pre><code ...>` and standalone `<code ...>` blocks.
fn extract_code_blocks(html: &str) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    let re = Regex::new(
        r#"(?is)<(?:pre\s*>\s*<code|code)\s*(?:class\s*=\s*["']([^"']*)["'])?\s*>(.*?)</code>"#,
    )
    .expect("valid regex");

    for cap in re.captures_iter(html) {
        let class_attr = cap.get(1).map(|m| m.as_str().to_string());
        let raw_content = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        let language = class_attr.and_then(|cls| {
            // Try "language-xxx" first, then bare class name
            if let Some(rest) = cls.strip_prefix("language-") {
                Some(rest.split_whitespace().next().unwrap_or(rest).to_string())
            } else {
                let first = cls.split_whitespace().next().unwrap_or(&cls);
                if first.is_empty() {
                    None
                } else {
                    Some(first.to_string())
                }
            }
        });

        // Strip inner tags from code content
        let tag_re = Regex::new(r"<[^>]*>").expect("valid regex");
        let content = tag_re.replace_all(raw_content, "").to_string();
        let content = decode_entities(&content);

        if !content.trim().is_empty() {
            blocks.push(CodeBlock { language, content });
        }
    }

    blocks
}

/// Remove entire `<tag>...</tag>` blocks (case-insensitive, greedy for nesting safety).
fn strip_tag_block(html: &str, tag: &str) -> String {
    let pattern = format!(r"(?is)<{tag}[\s>].*?</{tag}\s*>");
    let re = Regex::new(&pattern).expect("valid regex");
    re.replace_all(html, " ").to_string()
}

/// Decode common HTML entities.
fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Collapse runs of whitespace into single spaces and normalize paragraph breaks.
fn collapse_whitespace(text: &str) -> String {
    let re = Regex::new(r"\n{3,}").expect("valid regex");
    let text = re.replace_all(text, "\n\n").to_string();
    let re = Regex::new(r"[^\S\n]+").expect("valid regex");
    let text = re.replace_all(&text, " ").to_string();
    text.trim().to_string()
}

// ---------------------------------------------------------------------------
// URL validation
// ---------------------------------------------------------------------------

/// Validate that a URL uses HTTPS, targets an allowed domain, and does not
/// point at a private/local IP address.
pub fn validate_url(url: &str, allowed_domains: &[String]) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("URL must use HTTPS".to_string());
    }

    let without_scheme = &url["https://".len()..];
    let domain = without_scheme
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");

    if domain.is_empty() {
        return Err("URL has no domain".to_string());
    }

    // Block private/local addresses
    let blocked = [
        "localhost",
        "127.",
        "10.",
        "192.168.",
        "0.0.0.0",
    ];
    let lower_domain = domain.to_lowercase();
    if lower_domain.ends_with(".local") {
        return Err(format!("Private domain blocked: {domain}"));
    }
    for prefix in &blocked {
        if lower_domain.starts_with(prefix) {
            return Err(format!("Private/local address blocked: {domain}"));
        }
    }
    // 172.16.0.0 – 172.31.255.255
    if lower_domain.starts_with("172.")
        && let Some(second_octet) = lower_domain
            .strip_prefix("172.")
            .and_then(|rest| rest.split('.').next())
            .and_then(|s| s.parse::<u8>().ok())
            && (16..=31).contains(&second_octet) {
                return Err(format!("Private address blocked: {domain}"));
            }

    // Check allowed domains
    if !allowed_domains.iter().any(|d| lower_domain == d.to_lowercase()) {
        return Err(format!("Domain not in allowlist: {domain}"));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Caching
// ---------------------------------------------------------------------------

/// Compute the cache file path for a given URL.
fn cache_path(cache_dir: &Path, url: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    cache_dir.join(format!("{hash}.json"))
}

/// Load a cached page if the file exists and is within the TTL.
fn load_cached(path: &Path, ttl_hours: u64) -> Option<KnowledgePage> {
    let data = std::fs::read_to_string(path).ok()?;
    let page: KnowledgePage = serde_json::from_str(&data).ok()?;

    let ttl = Duration::hours(ttl_hours as i64);
    if Utc::now() - page.fetched_at > ttl {
        debug!(url = %page.url, "Cache entry expired");
        return None;
    }

    Some(page)
}

/// Write a page to the cache directory.
fn save_to_cache(path: &Path, page: &KnowledgePage) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create cache dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(page)
        .map_err(|e| format!("Failed to serialize page: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write cache file: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// KnowledgeAcquisitionAgent
// ---------------------------------------------------------------------------

/// Agent responsible for researching new domains by fetching, parsing,
/// caching, and summarizing documentation.
pub struct KnowledgeAcquisitionAgent {
    config: KnowledgeConfig,
    http_client: reqwest::Client,
}

impl KnowledgeAcquisitionAgent {
    /// Create a new agent with the given configuration.
    pub fn new(config: KnowledgeConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .user_agent("HiveCode-KnowledgeAgent/0.1")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            config,
            http_client,
        }
    }

    /// Create a new agent with default configuration.
    pub fn with_default_config() -> Self {
        Self::new(KnowledgeConfig::default())
    }

    /// Fetch a single page, validate its URL, download, and parse it.
    pub async fn fetch_page(&self, url: &str) -> Result<KnowledgePage, String> {
        validate_url(url, &self.config.allowed_domains)?;

        info!(url, "Fetching documentation page");
        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let body = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read body: {e}"))?;

        if body.len() > self.config.max_page_bytes {
            return Err(format!(
                "Page too large: {} bytes (max {})",
                body.len(),
                self.config.max_page_bytes
            ));
        }

        let html = String::from_utf8_lossy(&body).to_string();

        // Compute content hash
        let mut hasher = Sha256::new();
        hasher.update(html.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        let (title, text_content, code_blocks) = parse_html(&html);

        // Extract domain
        let source_domain = url
            .strip_prefix("https://")
            .unwrap_or(url)
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .to_string();

        Ok(KnowledgePage {
            url: url.to_string(),
            title,
            text_content,
            code_blocks,
            fetched_at: Utc::now(),
            content_hash,
            source_domain,
        })
    }

    /// Fetch a page, using the local cache when possible.
    pub async fn fetch_or_cache(&self, url: &str) -> Result<KnowledgePage, String> {
        let path = cache_path(&self.config.cache_dir, url);
        if let Some(cached) = load_cached(&path, self.config.cache_ttl_hours) {
            debug!(url, "Serving from cache");
            return Ok(cached);
        }

        let page = self.fetch_page(url).await?;
        if let Err(e) = save_to_cache(&path, &page) {
            warn!(url, error = %e, "Failed to cache page");
        }
        Ok(page)
    }

    /// Run a full research session for the given topic.
    ///
    /// 1. Asks the AI to suggest documentation URLs for the topic.
    /// 2. Fetches each URL (using cache where possible).
    /// 3. Combines all content and asks the AI to synthesize a summary.
    /// 4. Optionally stores the summary in `CollectiveMemory`.
    pub async fn research<E: AiExecutor>(
        &self,
        topic: &str,
        executor: &E,
        memory: Option<&CollectiveMemory>,
    ) -> Result<AcquisitionResult, String> {
        let start = std::time::Instant::now();
        let mut total_cost = 0.0;

        // -- Step 1: Ask AI for URLs ----------------------------------------
        let domains_list = self
            .config
            .allowed_domains
            .iter()
            .map(|d| d.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let url_prompt = format!(
            "Given topic '{}' and these allowed documentation domains: [{}], \
             generate a JSON array of 3-5 documentation URLs to research. \
             Return ONLY a JSON array of URL strings, no other text.",
            topic, domains_list
        );

        let url_request = ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, &url_prompt)],
            model: String::new(),
            max_tokens: 1024,
            temperature: Some(0.3),
            system_prompt: Some(
                "You are a documentation research assistant. \
                 Respond with only valid JSON arrays of URL strings."
                    .to_string(),
            ),
            tools: None,
        };

        let url_response = executor.execute(&url_request).await?;
        total_cost += estimate_cost(&url_response.usage.total_tokens);

        let urls = parse_url_array(&url_response.content);
        info!(topic, url_count = urls.len(), "AI suggested URLs");

        // -- Step 2: Fetch pages -------------------------------------------
        let mut pages: Vec<KnowledgePage> = Vec::new();
        let mut pages_fetched = 0usize;
        let mut pages_from_cache = 0usize;

        for url in urls.iter().take(self.config.max_pages_per_session) {
            let cache_hit = {
                let path = cache_path(&self.config.cache_dir, url);
                load_cached(&path, self.config.cache_ttl_hours).is_some()
            };

            match self.fetch_or_cache(url).await {
                Ok(page) => {
                    if cache_hit {
                        pages_from_cache += 1;
                    } else {
                        pages_fetched += 1;
                    }
                    pages.push(page);
                }
                Err(e) => {
                    warn!(url, error = %e, "Failed to fetch page");
                }
            }
        }

        if pages.is_empty() {
            return Err("No pages could be fetched for the given topic".to_string());
        }

        // -- Step 3: Build context and synthesize --------------------------
        let mut context_parts: Vec<String> = Vec::new();
        for page in &pages {
            context_parts.push(format!("--- {} ({}) ---\n{}", page.title, page.url, page.text_content));
            for cb in &page.code_blocks {
                let lang = cb.language.as_deref().unwrap_or("unknown");
                context_parts.push(format!("```{lang}\n{}\n```", cb.content));
            }
        }
        let combined = context_parts.join("\n\n");

        // Truncate if excessively long
        let combined = if combined.len() > 60_000 {
            combined[..60_000].to_string()
        } else {
            combined
        };

        let synth_prompt = format!(
            "Synthesize this documentation into a knowledge summary about '{topic}'. \
             Return valid JSON with these fields: \
             \"summary\" (string), \"key_concepts\" (array of strings), \
             \"relevant_commands\" (array of strings), \
             \"code_examples\" (array of objects with optional \"language\" and \"content\" fields). \
             Return ONLY JSON, no other text.\n\n{combined}"
        );

        let synth_request = ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, &synth_prompt)],
            model: String::new(),
            max_tokens: 4096,
            temperature: Some(0.2),
            system_prompt: Some(
                "You are a technical documentation synthesizer. \
                 Respond only with valid JSON."
                    .to_string(),
            ),
            tools: None,
        };

        let synth_response = executor.execute(&synth_request).await?;
        total_cost += estimate_cost(&synth_response.usage.total_tokens);

        let summary = parse_knowledge_summary(topic, &synth_response.content, &pages);

        // -- Step 4: Store in memory if available --------------------------
        if let Some(mem) = memory {
            let mut entry = MemoryEntry::new(
                MemoryCategory::General,
                serde_json::to_string(&summary).unwrap_or_default(),
            );
            entry.tags = vec![
                "knowledge".to_string(),
                topic.to_string(),
            ];
            if let Err(e) = mem.remember(&entry) {
                warn!(error = %e, "Failed to store knowledge in memory");
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(AcquisitionResult {
            topic: topic.to_string(),
            pages_fetched,
            pages_from_cache,
            summary,
            stored_as_context: false,
            total_cost,
            duration_ms,
        })
    }

    /// Inject a knowledge summary into a `ContextEngine` as a documentation source.
    pub fn inject_into_context(summary: &KnowledgeSummary, engine: &mut ContextEngine) {
        let mut content = format!("# Knowledge Summary: {}\n\n{}\n", summary.topic, summary.summary);

        if !summary.key_concepts.is_empty() {
            content.push_str("\n## Key Concepts\n");
            for concept in &summary.key_concepts {
                content.push_str(&format!("- {concept}\n"));
            }
        }

        if !summary.relevant_commands.is_empty() {
            content.push_str("\n## Commands\n");
            for cmd in &summary.relevant_commands {
                content.push_str(&format!("- `{cmd}`\n"));
            }
        }

        for example in &summary.code_examples {
            let lang = example.language.as_deref().unwrap_or("");
            content.push_str(&format!("\n```{lang}\n{}\n```\n", example.content));
        }

        engine.add_source(ContextSource {
            path: format!("knowledge://{}", summary.topic),
            content,
            source_type: SourceType::Documentation,
            last_modified: summary.created_at,
        });

        info!(topic = %summary.topic, "Injected knowledge summary into context engine");
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Rough cost estimate based on total token count (placeholder heuristic).
fn estimate_cost(total_tokens: &u32) -> f64 {
    (*total_tokens as f64) * 0.000003
}

/// Parse a JSON array of URL strings from AI output, tolerating surrounding text.
fn parse_url_array(text: &str) -> Vec<String> {
    // Try to find a JSON array in the text
    let trimmed = text.trim();
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    serde_json::from_str::<Vec<String>>(json_str).unwrap_or_default()
}

/// Parse the AI synthesis response into a `KnowledgeSummary`.
fn parse_knowledge_summary(
    topic: &str,
    text: &str,
    pages: &[KnowledgePage],
) -> KnowledgeSummary {
    // Attempt to extract JSON from the response
    let trimmed = text.trim();
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    #[derive(Deserialize)]
    struct RawSummary {
        #[serde(default)]
        summary: String,
        #[serde(default)]
        key_concepts: Vec<String>,
        #[serde(default)]
        relevant_commands: Vec<String>,
        #[serde(default)]
        code_examples: Vec<CodeBlock>,
    }

    let raw: RawSummary = serde_json::from_str(json_str).unwrap_or(RawSummary {
        summary: text.to_string(),
        key_concepts: Vec::new(),
        relevant_commands: Vec::new(),
        code_examples: Vec::new(),
    });

    KnowledgeSummary {
        topic: topic.to_string(),
        summary: raw.summary,
        key_concepts: raw.key_concepts,
        relevant_commands: raw.relevant_commands,
        code_examples: raw.code_examples,
        source_urls: pages.iter().map(|p| p.url.clone()).collect(),
        created_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_html ---------------------------------------------------------

    #[test]
    fn parse_html_strips_tags() {
        let html = "<p>Hello <b>world</b>!</p>";
        let (_, text, _) = parse_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
        assert!(!text.contains("<b>"));
        assert!(!text.contains("<p>"));
    }

    #[test]
    fn parse_html_extracts_code_blocks() {
        let html = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#;
        let (_, _, blocks) = parse_html(html);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language.as_deref(), Some("rust"));
        assert!(blocks[0].content.contains("fn main()"));
    }

    #[test]
    fn parse_html_decodes_entities() {
        let html = "<p>&amp; &lt; &gt; &quot; &#39; &nbsp;</p>";
        let (_, text, _) = parse_html(html);
        assert!(text.contains("& < > \" '"));
    }

    #[test]
    fn parse_html_removes_script_style() {
        let html = "<p>keep</p><script>alert('bad')</script><style>.x{}</style><p>also keep</p>";
        let (_, text, _) = parse_html(html);
        assert!(text.contains("keep"));
        assert!(text.contains("also keep"));
        assert!(!text.contains("alert"));
        assert!(!text.contains(".x{}"));
    }

    #[test]
    fn parse_html_removes_nav_footer_header() {
        let html = "<nav>navigation</nav><p>content</p><footer>foot</footer><header>head</header>";
        let (_, text, _) = parse_html(html);
        assert!(text.contains("content"));
        assert!(!text.contains("navigation"));
        assert!(!text.contains("foot"));
        assert!(!text.contains("head"));
    }

    #[test]
    fn parse_html_handles_empty_input() {
        let (title, text, blocks) = parse_html("");
        assert!(title.is_empty());
        assert!(text.is_empty());
        assert!(blocks.is_empty());
    }

    #[test]
    fn parse_html_handles_malformed() {
        let html = "<p>unclosed <b>bold <i>italic</p>";
        let (_, text, _) = parse_html(html);
        // Should still produce some text without panicking
        assert!(text.contains("unclosed"));
        assert!(text.contains("bold"));
    }

    #[test]
    fn parse_html_extracts_title() {
        let html = "<html><head><title>My Page</title></head><body>body</body></html>";
        let (title, _, _) = parse_html(html);
        assert_eq!(title, "My Page");
    }

    // -- validate_url -------------------------------------------------------

    #[test]
    fn validate_url_rejects_http() {
        let allowed = vec!["docs.rs".to_string()];
        let result = validate_url("http://docs.rs/some-crate", &allowed);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTPS"));
    }

    #[test]
    fn validate_url_rejects_non_allowed() {
        let allowed = vec!["docs.rs".to_string()];
        let result = validate_url("https://evil.com/bad", &allowed);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("allowlist"));
    }

    #[test]
    fn validate_url_accepts_allowed() {
        let allowed = vec!["docs.rs".to_string()];
        assert!(validate_url("https://docs.rs/tokio/latest", &allowed).is_ok());
    }

    #[test]
    fn validate_url_rejects_private_ips() {
        let allowed = vec![
            "127.0.0.1".to_string(),
            "localhost".to_string(),
            "192.168.1.1".to_string(),
            "10.0.0.1".to_string(),
            "172.16.0.1".to_string(),
            "myhost.local".to_string(),
        ];
        assert!(validate_url("https://127.0.0.1/x", &allowed).is_err());
        assert!(validate_url("https://localhost/x", &allowed).is_err());
        assert!(validate_url("https://192.168.1.1/x", &allowed).is_err());
        assert!(validate_url("https://10.0.0.1/x", &allowed).is_err());
        assert!(validate_url("https://172.16.0.1/x", &allowed).is_err());
        assert!(validate_url("https://myhost.local/x", &allowed).is_err());
    }

    // -- caching ------------------------------------------------------------

    #[test]
    fn cache_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let page = KnowledgePage {
            url: "https://docs.rs/test".to_string(),
            title: "Test Page".to_string(),
            text_content: "Some content".to_string(),
            code_blocks: vec![CodeBlock {
                language: Some("rust".to_string()),
                content: "fn test() {}".to_string(),
            }],
            fetched_at: Utc::now(),
            content_hash: "abc123".to_string(),
            source_domain: "docs.rs".to_string(),
        };

        let path = cache_path(dir.path(), &page.url);
        save_to_cache(&path, &page).unwrap();

        let loaded = load_cached(&path, 168).unwrap();
        assert_eq!(loaded.url, page.url);
        assert_eq!(loaded.title, page.title);
        assert_eq!(loaded.code_blocks.len(), 1);
    }

    #[test]
    fn cache_respects_ttl() {
        let dir = tempfile::tempdir().unwrap();
        let page = KnowledgePage {
            url: "https://docs.rs/old".to_string(),
            title: "Old".to_string(),
            text_content: "expired".to_string(),
            code_blocks: Vec::new(),
            fetched_at: Utc::now() - Duration::hours(200),
            content_hash: "old".to_string(),
            source_domain: "docs.rs".to_string(),
        };

        let path = cache_path(dir.path(), &page.url);
        save_to_cache(&path, &page).unwrap();

        // TTL of 168 hours (7 days) — page is 200 hours old, should be expired
        assert!(load_cached(&path, 168).is_none());

        // But a generous TTL should still work
        assert!(load_cached(&path, 300).is_some());
    }

    // -- config defaults ----------------------------------------------------

    #[test]
    fn default_config_has_sane_defaults() {
        let config = KnowledgeConfig::default();
        assert_eq!(config.max_pages_per_session, 10);
        assert_eq!(config.max_page_bytes, 512_000);
        assert_eq!(config.cache_ttl_hours, 168);
        assert!(!config.allowed_domains.is_empty());
        assert!(config.allowed_domains.contains(&"docs.rs".to_string()));
    }

    // -- serialization ------------------------------------------------------

    #[test]
    fn knowledge_summary_serialization() {
        let summary = KnowledgeSummary {
            topic: "Rust async".to_string(),
            summary: "Async programming in Rust uses futures.".to_string(),
            key_concepts: vec!["Future".to_string(), "async/await".to_string()],
            relevant_commands: vec!["cargo add tokio".to_string()],
            code_examples: vec![CodeBlock {
                language: Some("rust".to_string()),
                content: "async fn hello() {}".to_string(),
            }],
            source_urls: vec!["https://doc.rust-lang.org/async".to_string()],
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let round: KnowledgeSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(round.topic, summary.topic);
        assert_eq!(round.key_concepts.len(), 2);
        assert_eq!(round.code_examples.len(), 1);
        assert_eq!(round.code_examples[0].language.as_deref(), Some("rust"));
    }
}
