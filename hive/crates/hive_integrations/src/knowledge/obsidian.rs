//! Obsidian vault knowledge base provider.
//!
//! Reads a local Obsidian vault directory, parsing Markdown files with
//! YAML front matter, `[[wiki links]]`, and `#tags`. Provides full-text
//! search using a TF-IDF scoring algorithm over an in-memory index.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{
    CreatePageRequest, KBPage, KBPageSummary, KBPlatform, KBSearchResult,
    KnowledgeBaseProvider,
};

// -- Types ------------------------------------------------------------------

/// A parsed Obsidian page with metadata extracted from front matter and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianPage {
    /// Relative path from vault root (e.g. "folder/note.md").
    pub path: String,
    /// Title derived from file name or front matter.
    pub title: String,
    /// Raw Markdown content (excluding front matter).
    pub content: String,
    /// Parsed YAML front matter key-value pairs.
    #[serde(default)]
    pub front_matter: HashMap<String, String>,
    /// Tags extracted from front matter and inline `#tag` syntax.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Pages that link to this page via `[[wiki links]]`.
    #[serde(default)]
    pub backlinks: Vec<String>,
    /// Pages this page links to via `[[wiki links]]`.
    #[serde(default)]
    pub outlinks: Vec<String>,
}

/// In-memory index of all Markdown files in an Obsidian vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsidianIndex {
    /// Map from relative path to parsed page.
    pub pages: HashMap<String, ObsidianPage>,
    /// When the index was last built.
    pub last_indexed: DateTime<Utc>,
}

impl ObsidianIndex {
    /// Create a new empty index.
    fn new() -> Self {
        Self {
            pages: HashMap::new(),
            last_indexed: Utc::now(),
        }
    }
}

// -- Provider ---------------------------------------------------------------

/// Obsidian vault knowledge base provider.
///
/// Reads `.md` files from the configured vault path, parses YAML front
/// matter, extracts wiki links and tags, and provides TF-IDF search
/// across the vault contents.
pub struct ObsidianProvider {
    vault_path: PathBuf,
    index: ObsidianIndex,
}

impl ObsidianProvider {
    /// Create a new provider for the vault at the given path.
    ///
    /// The vault is **not** indexed automatically; call [`index_vault`]
    /// to build the search index.
    pub fn new(vault_path: impl Into<PathBuf>) -> Self {
        Self {
            vault_path: vault_path.into(),
            index: ObsidianIndex::new(),
        }
    }

    /// Return the vault path.
    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }

    /// Return a reference to the current index.
    pub fn index(&self) -> &ObsidianIndex {
        &self.index
    }

    /// Build (or rebuild) the in-memory index by scanning all `.md` files
    /// in the vault directory recursively.
    pub async fn index_vault(&mut self) -> Result<usize> {
        debug!(vault = %self.vault_path.display(), "indexing Obsidian vault");

        let mut pages = HashMap::new();
        let vault = self.vault_path.clone();
        Self::scan_directory(&vault, &vault, &mut pages).await?;

        // Build backlinks: for each page's outlinks, register itself as a backlink.
        let outlinks_map: HashMap<String, Vec<String>> = pages
            .iter()
            .map(|(path, page)| (path.clone(), page.outlinks.clone()))
            .collect();

        for (source_path, links) in &outlinks_map {
            for link in links {
                // Resolve the link to a page path.
                if let Some(target) = Self::resolve_link(link, &pages) {
                    if let Some(target_page) = pages.get_mut(&target) {
                        if !target_page.backlinks.contains(source_path) {
                            target_page.backlinks.push(source_path.clone());
                        }
                    }
                }
            }
        }

        let count = pages.len();
        self.index = ObsidianIndex {
            pages,
            last_indexed: Utc::now(),
        };

        debug!(count = count, "Obsidian vault indexed");
        Ok(count)
    }

    /// Recursively scan a directory for `.md` files and parse them.
    fn scan_directory<'a>(
        base: &'a Path,
        dir: &'a Path,
        pages: &'a mut HashMap<String, ObsidianPage>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = tokio::fs::read_dir(dir)
                .await
                .with_context(|| format!("failed to read directory: {}", dir.display()))?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                // Skip hidden files and directories.
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }

                if path.is_dir() {
                    Self::scan_directory(base, &path, pages).await?;
                } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    match Self::parse_file(base, &path).await {
                        Ok(page) => {
                            pages.insert(page.path.clone(), page);
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "failed to parse Obsidian file"
                            );
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Parse a single `.md` file into an [`ObsidianPage`].
    async fn parse_file(base: &Path, path: &Path) -> Result<ObsidianPage> {
        let raw = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read file: {}", path.display()))?;

        let relative = path
            .strip_prefix(base)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        let (front_matter, content) = parse_front_matter(&raw);
        let tags = extract_tags(&content, &front_matter);
        let outlinks = extract_wiki_links(&content);

        // Override title from front matter if present.
        let title = front_matter
            .get("title")
            .cloned()
            .unwrap_or(title);

        Ok(ObsidianPage {
            path: relative,
            title,
            content,
            front_matter,
            tags,
            backlinks: vec![],
            outlinks,
        })
    }

    /// Resolve a wiki link target to a page path in the index.
    ///
    /// Obsidian wiki links can be bare filenames ("Note") or contain
    /// relative paths ("folder/Note"). We match against file stems.
    fn resolve_link(link: &str, pages: &HashMap<String, ObsidianPage>) -> Option<String> {
        // First try exact path match with .md extension.
        let with_ext = if link.ends_with(".md") {
            link.to_string()
        } else {
            format!("{link}.md")
        };

        if pages.contains_key(&with_ext) {
            return Some(with_ext);
        }

        // Try matching by file stem (last component).
        let link_stem = link.rsplit('/').next().unwrap_or(link);
        for (path, page) in pages {
            let page_stem = Path::new(&page.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if page_stem.eq_ignore_ascii_case(link_stem) {
                return Some(path.clone());
            }
        }

        None
    }

    /// Perform TF-IDF search across indexed pages.
    fn tfidf_search(&self, query: &str, limit: u32) -> Vec<KBSearchResult> {
        let query_terms: Vec<String> = tokenize(query);
        if query_terms.is_empty() {
            return vec![];
        }

        let total_docs = self.index.pages.len() as f64;
        if total_docs == 0.0 {
            return vec![];
        }

        // Compute document frequency for each query term.
        let mut doc_freq: HashMap<&str, usize> = HashMap::new();
        for term in &query_terms {
            let count = self
                .index
                .pages
                .values()
                .filter(|page| {
                    let lower_content = page.content.to_lowercase();
                    let lower_title = page.title.to_lowercase();
                    lower_content.contains(term.as_str()) || lower_title.contains(term.as_str())
                })
                .count();
            doc_freq.insert(term, count);
        }

        // Score each document.
        let mut scores: Vec<(String, f64)> = self
            .index
            .pages
            .iter()
            .filter_map(|(path, page)| {
                let full_text = format!("{} {}", page.title, page.content).to_lowercase();
                let doc_len = full_text.split_whitespace().count() as f64;

                if doc_len == 0.0 {
                    return None;
                }

                let mut score = 0.0;
                for term in &query_terms {
                    let tf = full_text.matches(term.as_str()).count() as f64 / doc_len;
                    let df = *doc_freq.get(term.as_str()).unwrap_or(&0) as f64;
                    if df > 0.0 {
                        let idf = (total_docs / df).ln() + 1.0;
                        score += tf * idf;
                    }
                }

                // Boost title matches.
                let lower_title = page.title.to_lowercase();
                for term in &query_terms {
                    if lower_title.contains(term.as_str()) {
                        score *= 2.0;
                    }
                }

                if score > 0.0 {
                    Some((path.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        // Sort descending by score.
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scores.truncate(limit as usize);

        // Normalize scores to [0, 1].
        let max_score = scores
            .first()
            .map(|(_, s)| *s)
            .unwrap_or(1.0)
            .max(f64::EPSILON);

        scores
            .into_iter()
            .filter_map(|(path, score)| {
                let page = self.index.pages.get(&path)?;
                let snippet = generate_snippet(&page.content, &query_terms, 200);
                Some(KBSearchResult {
                    page_id: path,
                    title: page.title.clone(),
                    snippet,
                    relevance_score: score / max_score,
                    url: None,
                    platform: KBPlatform::Obsidian,
                })
            })
            .collect()
    }
}

#[async_trait]
impl KnowledgeBaseProvider for ObsidianProvider {
    fn platform(&self) -> KBPlatform {
        KBPlatform::Obsidian
    }

    async fn search(&self, query: &str, limit: u32) -> Result<Vec<KBSearchResult>> {
        debug!(query = %query, limit = limit, "searching Obsidian vault");
        Ok(self.tfidf_search(query, limit))
    }

    async fn get_page(&self, page_id: &str) -> Result<KBPage> {
        // page_id is the relative path within the vault.
        let full_path = self.vault_path.join(page_id);

        let raw = tokio::fs::read_to_string(&full_path)
            .await
            .with_context(|| format!("failed to read Obsidian page: {}", full_path.display()))?;

        let (front_matter, content) = parse_front_matter(&raw);
        let tags = extract_tags(&content, &front_matter);

        let title = front_matter
            .get("title")
            .cloned()
            .unwrap_or_else(|| {
                full_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            });

        // Extract parent directory as parent_id.
        let parent_id = Path::new(page_id)
            .parent()
            .and_then(|p| {
                let s = p.to_string_lossy().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        // Try to get filesystem metadata for timestamps.
        let metadata = tokio::fs::metadata(&full_path).await.ok();
        let updated_at = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(DateTime::<Utc>::from);
        let created_at = metadata
            .as_ref()
            .and_then(|m| m.created().ok())
            .map(DateTime::<Utc>::from);

        Ok(KBPage {
            id: page_id.to_string(),
            title,
            content,
            url: None,
            parent_id,
            created_at,
            updated_at,
            tags,
        })
    }

    async fn list_pages(&self, parent_id: Option<&str>) -> Result<Vec<KBPageSummary>> {
        let dir = match parent_id {
            Some(p) => self.vault_path.join(p),
            None => self.vault_path.clone(),
        };

        debug!(dir = %dir.display(), "listing Obsidian pages");

        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .with_context(|| format!("failed to read directory: {}", dir.display()))?;

        let mut summaries = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Skip hidden files.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }

            if path.is_dir() {
                let relative = path
                    .strip_prefix(&self.vault_path)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string();

                summaries.push(KBPageSummary {
                    id: relative,
                    title: name,
                    parent_id: parent_id.map(String::from),
                    has_children: true,
                });
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let relative = path
                    .strip_prefix(&self.vault_path)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                let title = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string();

                summaries.push(KBPageSummary {
                    id: relative,
                    title,
                    parent_id: parent_id.map(String::from),
                    has_children: false,
                });
            }
        }

        // Sort alphabetically by title.
        summaries.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(summaries)
    }

    async fn create_page(&self, request: &CreatePageRequest) -> Result<KBPage> {
        // Determine the file path.
        let filename = sanitize_filename(&request.title);
        let relative_path = match &request.parent_id {
            Some(parent) => format!("{parent}/{filename}.md"),
            None => format!("{filename}.md"),
        };
        let full_path = self.vault_path.join(&relative_path);

        // Ensure parent directory exists.
        if let Some(parent_dir) = full_path.parent() {
            tokio::fs::create_dir_all(parent_dir)
                .await
                .with_context(|| {
                    format!("failed to create directory: {}", parent_dir.display())
                })?;
        }

        // Build file content with front matter.
        let mut file_content = String::new();
        file_content.push_str("---\n");
        file_content.push_str(&format!("title: \"{}\"\n", request.title));
        file_content.push_str(&format!(
            "created: \"{}\"\n",
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
        ));
        if !request.tags.is_empty() {
            file_content.push_str("tags:\n");
            for tag in &request.tags {
                file_content.push_str(&format!("  - {tag}\n"));
            }
        }
        file_content.push_str("---\n\n");
        file_content.push_str(&request.content);

        debug!(path = %full_path.display(), "creating Obsidian page");

        tokio::fs::write(&full_path, &file_content)
            .await
            .with_context(|| format!("failed to write Obsidian page: {}", full_path.display()))?;

        Ok(KBPage {
            id: relative_path,
            title: request.title.clone(),
            content: request.content.clone(),
            url: None,
            parent_id: request.parent_id.clone(),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            tags: request.tags.clone(),
        })
    }

    async fn update_page(&self, page_id: &str, content: &str) -> Result<KBPage> {
        let full_path = self.vault_path.join(page_id);

        // Read existing file to preserve front matter.
        let existing = tokio::fs::read_to_string(&full_path)
            .await
            .with_context(|| format!("failed to read Obsidian page: {}", full_path.display()))?;

        let (front_matter, _old_content) = parse_front_matter(&existing);

        // Rebuild the file with existing front matter and new content.
        let mut file_content = String::new();
        if !front_matter.is_empty() {
            file_content.push_str("---\n");
            for (key, value) in &front_matter {
                file_content.push_str(&format!("{key}: {value}\n"));
            }
            file_content.push_str("---\n\n");
        }
        file_content.push_str(content);

        tokio::fs::write(&full_path, &file_content)
            .await
            .with_context(|| format!("failed to write Obsidian page: {}", full_path.display()))?;

        let tags = extract_tags(content, &front_matter);
        let title = front_matter
            .get("title")
            .cloned()
            .unwrap_or_else(|| {
                full_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            });

        let parent_id = Path::new(page_id)
            .parent()
            .and_then(|p| {
                let s = p.to_string_lossy().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        Ok(KBPage {
            id: page_id.to_string(),
            title,
            content: content.to_string(),
            url: None,
            parent_id,
            created_at: None,
            updated_at: Some(Utc::now()),
            tags,
        })
    }

    async fn get_context(&self, query: &str) -> Result<String> {
        let results = self.search(query, 5).await?;

        if results.is_empty() {
            return Ok(String::new());
        }

        let mut context = String::new();
        for result in &results {
            match self.get_page(&result.page_id).await {
                Ok(page) => {
                    context.push_str(&format!("## {} ({})\n\n", page.title, result.page_id));
                    let max_chars = 2000;
                    if page.content.len() > max_chars {
                        context.push_str(&page.content[..max_chars]);
                        context.push_str("\n...(truncated)\n\n");
                    } else {
                        context.push_str(&page.content);
                        context.push_str("\n\n");
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        page_id = %result.page_id,
                        error = %e,
                        "failed to fetch Obsidian page for context"
                    );
                }
            }
        }

        Ok(context)
    }
}

// -- Parsing helpers --------------------------------------------------------

/// Parse YAML front matter delimited by `---` and return (front_matter, content).
fn parse_front_matter(raw: &str) -> (HashMap<String, String>, String) {
    let trimmed = raw.trim_start();

    if !trimmed.starts_with("---") {
        return (HashMap::new(), raw.to_string());
    }

    // Find the closing --- delimiter.
    let after_opening = &trimmed[3..];
    let after_opening = after_opening.trim_start_matches(['\r', '\n']);

    if let Some(end_pos) = after_opening.find("\n---") {
        let yaml_block = &after_opening[..end_pos];
        let content_start = end_pos + 4; // skip \n---
        let content = after_opening[content_start..]
            .trim_start_matches(['\r', '\n'])
            .to_string();

        let front_matter = parse_yaml_simple(yaml_block);
        (front_matter, content)
    } else {
        (HashMap::new(), raw.to_string())
    }
}

/// Simple YAML parser for front matter (key: value pairs, one per line).
///
/// Handles simple scalars and basic list syntax (`- item`). Lists are
/// stored as comma-separated values.
fn parse_yaml_simple(yaml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut current_key: Option<String> = None;
    let mut list_values: Vec<String> = Vec::new();

    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Check for list item under current key.
        if trimmed.starts_with("- ") {
            if current_key.is_some() {
                list_values.push(trimmed[2..].trim().to_string());
            }
            continue;
        }

        // Flush any accumulated list values for the previous key.
        if let Some(ref key) = current_key {
            if !list_values.is_empty() {
                map.insert(key.clone(), list_values.join(", "));
                list_values.clear();
            }
        }

        // Parse key: value.
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_string();
            let value = trimmed[colon_pos + 1..].trim().to_string();
            let value = value
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();

            if value.is_empty() {
                // Might be a list header.
                current_key = Some(key);
            } else {
                map.insert(key, value);
                current_key = None;
            }
        }
    }

    // Flush final list.
    if let Some(key) = current_key {
        if !list_values.is_empty() {
            map.insert(key, list_values.join(", "));
        }
    }

    map
}

/// Extract tags from content (`#tag` syntax) and front matter.
fn extract_tags(content: &str, front_matter: &HashMap<String, String>) -> Vec<String> {
    let mut tags = Vec::new();

    // Tags from front matter.
    if let Some(fm_tags) = front_matter.get("tags") {
        for tag in fm_tags.split(',') {
            let tag = tag.trim().to_string();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }

    // Inline #tags from content.
    let tag_re = Regex::new(r"(?:^|\s)#([a-zA-Z][a-zA-Z0-9_/-]*)").unwrap();
    for cap in tag_re.captures_iter(content) {
        let tag = cap[1].to_string();
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }

    tags
}

/// Extract `[[wiki links]]` from Markdown content.
///
/// Handles both `[[Page]]` and `[[Page|Display Text]]` syntax.
fn extract_wiki_links(content: &str) -> Vec<String> {
    let link_re = Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap();
    let mut links = Vec::new();

    for cap in link_re.captures_iter(content) {
        let target = cap[1].trim().to_string();
        if !links.contains(&target) {
            links.push(target);
        }
    }

    links
}

/// Tokenize a string into lowercase words for search.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
}

/// Generate a snippet around the first occurrence of any query term.
fn generate_snippet(content: &str, query_terms: &[String], max_len: usize) -> String {
    let lower = content.to_lowercase();

    // Find the earliest match.
    let mut best_pos: Option<usize> = None;
    for term in query_terms {
        if let Some(pos) = lower.find(term.as_str()) {
            match best_pos {
                Some(bp) if pos < bp => best_pos = Some(pos),
                None => best_pos = Some(pos),
                _ => {}
            }
        }
    }

    let pos = best_pos.unwrap_or(0);

    // Extract a window around the match position.
    let start = pos.saturating_sub(max_len / 4);
    let end = (start + max_len).min(content.len());

    // Adjust to not break words.
    let start = if start > 0 {
        content[start..]
            .find(char::is_whitespace)
            .map(|ws| start + ws + 1)
            .unwrap_or(start)
    } else {
        0
    };

    let mut snippet = content[start..end].to_string();
    // Clean up newlines for snippet display.
    snippet = snippet.replace('\n', " ").replace('\r', "");

    if start > 0 {
        snippet = format!("...{snippet}");
    }
    if end < content.len() {
        snippet.push_str("...");
    }

    snippet
}

/// Sanitize a title for use as a filename.
fn sanitize_filename(title: &str) -> String {
    title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_front_matter_basic() {
        let raw = "---\ntitle: \"My Note\"\ntags:\n  - rust\n  - code\n---\n\n# Hello\n\nContent here.";
        let (fm, content) = parse_front_matter(raw);
        assert_eq!(fm.get("title").unwrap(), "My Note");
        assert!(fm.get("tags").unwrap().contains("rust"));
        assert!(fm.get("tags").unwrap().contains("code"));
        assert!(content.contains("# Hello"));
        assert!(content.contains("Content here."));
    }

    #[test]
    fn test_parse_front_matter_no_front_matter() {
        let raw = "# Just a heading\n\nSome content.";
        let (fm, content) = parse_front_matter(raw);
        assert!(fm.is_empty());
        assert_eq!(content, raw);
    }

    #[test]
    fn test_parse_front_matter_empty() {
        let (fm, content) = parse_front_matter("");
        assert!(fm.is_empty());
        assert!(content.is_empty());
    }

    #[test]
    fn test_parse_front_matter_simple_values() {
        let raw = "---\ntitle: Test\nauthor: Alice\n---\nBody text";
        let (fm, content) = parse_front_matter(raw);
        assert_eq!(fm.get("title").unwrap(), "Test");
        assert_eq!(fm.get("author").unwrap(), "Alice");
        assert_eq!(content, "Body text");
    }

    #[test]
    fn test_extract_tags_inline() {
        let content = "Some text #rust and #code here, but not in #123bad";
        let fm = HashMap::new();
        let tags = extract_tags(content, &fm);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"code".to_string()));
        assert!(!tags.iter().any(|t| t == "123bad"));
    }

    #[test]
    fn test_extract_tags_from_front_matter() {
        let content = "No inline tags here";
        let mut fm = HashMap::new();
        fm.insert("tags".to_string(), "rust, programming".to_string());
        let tags = extract_tags(content, &fm);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"programming".to_string()));
    }

    #[test]
    fn test_extract_tags_deduplication() {
        let content = "Some #rust text #rust again";
        let fm = HashMap::new();
        let tags = extract_tags(content, &fm);
        let rust_count = tags.iter().filter(|t| *t == "rust").count();
        assert_eq!(rust_count, 1);
    }

    #[test]
    fn test_extract_wiki_links_basic() {
        let content = "See [[Page A]] and [[Page B]] for details.";
        let links = extract_wiki_links(content);
        assert_eq!(links.len(), 2);
        assert!(links.contains(&"Page A".to_string()));
        assert!(links.contains(&"Page B".to_string()));
    }

    #[test]
    fn test_extract_wiki_links_with_display_text() {
        let content = "Check [[Target Page|Displayed Name]] here.";
        let links = extract_wiki_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "Target Page");
    }

    #[test]
    fn test_extract_wiki_links_deduplication() {
        let content = "[[Page A]] and again [[Page A]]";
        let links = extract_wiki_links(content);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_extract_wiki_links_empty() {
        let content = "No links here.";
        let links = extract_wiki_links(content);
        assert!(links.is_empty());
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello, World! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"this".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        // Single char words filtered out.
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_generate_snippet() {
        let content = "The quick brown fox jumps over the lazy dog. Rust is a great language for systems programming.";
        let terms = vec!["rust".to_string()];
        let snippet = generate_snippet(content, &terms, 80);
        assert!(snippet.contains("Rust") || snippet.contains("rust"));
    }

    #[test]
    fn test_generate_snippet_no_match() {
        let content = "Hello world";
        let terms = vec!["nonexistent".to_string()];
        let snippet = generate_snippet(content, &terms, 80);
        // Should return from the beginning.
        assert!(snippet.contains("Hello"));
    }

    #[test]
    fn test_generate_snippet_short_content() {
        let content = "Short";
        let terms = vec!["short".to_string()];
        let snippet = generate_snippet(content, &terms, 200);
        assert_eq!(snippet, "Short");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello World"), "Hello World");
        assert_eq!(sanitize_filename("file/with:bad*chars"), "file-with-bad-chars");
        assert_eq!(sanitize_filename("normal_file-name"), "normal_file-name");
    }

    #[test]
    fn test_sanitize_filename_special_chars() {
        assert_eq!(sanitize_filename("a<b>c|d"), "a-b-c-d");
        assert_eq!(
            sanitize_filename("question?mark"),
            "question-mark"
        );
    }

    #[test]
    fn test_obsidian_provider_new() {
        let provider = ObsidianProvider::new("/tmp/vault");
        assert_eq!(provider.vault_path(), Path::new("/tmp/vault"));
        assert_eq!(provider.platform(), KBPlatform::Obsidian);
        assert!(provider.index().pages.is_empty());
    }

    #[test]
    fn test_parse_yaml_simple() {
        let yaml = "title: \"Test Note\"\nauthor: Bob\ndate: 2024-01-15";
        let map = parse_yaml_simple(yaml);
        assert_eq!(map.get("title").unwrap(), "Test Note");
        assert_eq!(map.get("author").unwrap(), "Bob");
        assert_eq!(map.get("date").unwrap(), "2024-01-15");
    }

    #[test]
    fn test_parse_yaml_simple_list() {
        let yaml = "tags:\n  - rust\n  - programming\n  - code\ntitle: Note";
        let map = parse_yaml_simple(yaml);
        assert_eq!(map.get("tags").unwrap(), "rust, programming, code");
        assert_eq!(map.get("title").unwrap(), "Note");
    }

    #[test]
    fn test_parse_yaml_simple_empty() {
        let map = parse_yaml_simple("");
        assert!(map.is_empty());
    }

    #[test]
    fn test_resolve_link_exact() {
        let mut pages = HashMap::new();
        pages.insert(
            "notes/hello.md".to_string(),
            ObsidianPage {
                path: "notes/hello.md".to_string(),
                title: "Hello".to_string(),
                content: String::new(),
                front_matter: HashMap::new(),
                tags: vec![],
                backlinks: vec![],
                outlinks: vec![],
            },
        );

        assert_eq!(
            ObsidianProvider::resolve_link("notes/hello", &pages),
            Some("notes/hello.md".to_string())
        );
    }

    #[test]
    fn test_resolve_link_by_stem() {
        let mut pages = HashMap::new();
        pages.insert(
            "deeply/nested/hello.md".to_string(),
            ObsidianPage {
                path: "deeply/nested/hello.md".to_string(),
                title: "Hello".to_string(),
                content: String::new(),
                front_matter: HashMap::new(),
                tags: vec![],
                backlinks: vec![],
                outlinks: vec![],
            },
        );

        assert_eq!(
            ObsidianProvider::resolve_link("hello", &pages),
            Some("deeply/nested/hello.md".to_string())
        );
    }

    #[test]
    fn test_resolve_link_not_found() {
        let pages = HashMap::new();
        assert_eq!(ObsidianProvider::resolve_link("nonexistent", &pages), None);
    }

    #[test]
    fn test_tfidf_search_empty_index() {
        let provider = ObsidianProvider::new("/tmp/vault");
        let results = provider.tfidf_search("hello", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_tfidf_search_empty_query() {
        let provider = ObsidianProvider::new("/tmp/vault");
        let results = provider.tfidf_search("", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_tfidf_search_with_pages() {
        let mut provider = ObsidianProvider::new("/tmp/vault");

        provider.index.pages.insert(
            "rust-notes.md".to_string(),
            ObsidianPage {
                path: "rust-notes.md".to_string(),
                title: "Rust Notes".to_string(),
                content: "Rust is a systems programming language focused on safety and performance. Rust ownership model is unique.".to_string(),
                front_matter: HashMap::new(),
                tags: vec!["rust".to_string()],
                backlinks: vec![],
                outlinks: vec![],
            },
        );

        provider.index.pages.insert(
            "python-notes.md".to_string(),
            ObsidianPage {
                path: "python-notes.md".to_string(),
                title: "Python Notes".to_string(),
                content: "Python is a high-level programming language. It is widely used for scripting.".to_string(),
                front_matter: HashMap::new(),
                tags: vec!["python".to_string()],
                backlinks: vec![],
                outlinks: vec![],
            },
        );

        let results = provider.tfidf_search("rust programming", 10);
        assert!(!results.is_empty());
        // Rust page should rank higher.
        assert_eq!(results[0].page_id, "rust-notes.md");
        assert!(results[0].relevance_score > 0.0);
    }

    #[test]
    fn test_tfidf_search_limit() {
        let mut provider = ObsidianProvider::new("/tmp/vault");

        for i in 0..10 {
            provider.index.pages.insert(
                format!("note-{i}.md"),
                ObsidianPage {
                    path: format!("note-{i}.md"),
                    title: format!("Note {i}"),
                    content: "This is a test note about programming and code".to_string(),
                    front_matter: HashMap::new(),
                    tags: vec![],
                    backlinks: vec![],
                    outlinks: vec![],
                },
            );
        }

        let results = provider.tfidf_search("programming", 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_obsidian_page_serialization_roundtrip() {
        let page = ObsidianPage {
            path: "notes/test.md".to_string(),
            title: "Test Note".to_string(),
            content: "Some content here".to_string(),
            front_matter: {
                let mut m = HashMap::new();
                m.insert("author".to_string(), "Alice".to_string());
                m
            },
            tags: vec!["test".to_string(), "note".to_string()],
            backlinks: vec!["other.md".to_string()],
            outlinks: vec!["linked.md".to_string()],
        };

        let json = serde_json::to_string(&page).unwrap();
        let back: ObsidianPage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "notes/test.md");
        assert_eq!(back.title, "Test Note");
        assert_eq!(back.tags.len(), 2);
        assert_eq!(back.backlinks.len(), 1);
        assert_eq!(back.outlinks.len(), 1);
    }

    #[test]
    fn test_obsidian_index_serialization() {
        let mut index = ObsidianIndex::new();
        index.pages.insert(
            "test.md".to_string(),
            ObsidianPage {
                path: "test.md".to_string(),
                title: "Test".to_string(),
                content: "Content".to_string(),
                front_matter: HashMap::new(),
                tags: vec![],
                backlinks: vec![],
                outlinks: vec![],
            },
        );

        let json = serde_json::to_string(&index).unwrap();
        let back: ObsidianIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pages.len(), 1);
        assert!(back.pages.contains_key("test.md"));
    }

    #[tokio::test]
    async fn test_index_vault_empty_dir() {
        let tmp = tempdir();
        let mut provider = ObsidianProvider::new(&tmp);
        let count = provider.index_vault().await.unwrap();
        assert_eq!(count, 0);
        cleanup_tempdir(&tmp);
    }

    #[tokio::test]
    async fn test_index_vault_with_files() {
        let tmp = tempdir();
        let file1 = tmp.join("note1.md");
        let file2 = tmp.join("note2.md");

        tokio::fs::write(
            &file1,
            "---\ntitle: Note One\n---\n\nContent of note one. [[note2]]",
        )
        .await
        .unwrap();
        tokio::fs::write(&file2, "# Note Two\n\nContent of note two.")
            .await
            .unwrap();

        let mut provider = ObsidianProvider::new(&tmp);
        let count = provider.index_vault().await.unwrap();
        assert_eq!(count, 2);

        // Verify outlinks were parsed.
        let note1 = provider.index.pages.get("note1.md").unwrap();
        assert!(note1.outlinks.contains(&"note2".to_string()));

        // Verify backlinks were built.
        let note2 = provider.index.pages.get("note2.md").unwrap();
        assert!(note2.backlinks.contains(&"note1.md".to_string()));

        cleanup_tempdir(&tmp);
    }

    #[tokio::test]
    async fn test_get_page() {
        let tmp = tempdir();
        let file = tmp.join("test.md");
        tokio::fs::write(&file, "---\ntitle: Test Page\ntags:\n  - demo\n---\n\n# Hello\n\nWorld")
            .await
            .unwrap();

        let provider = ObsidianProvider::new(&tmp);
        let page = provider.get_page("test.md").await.unwrap();
        assert_eq!(page.title, "Test Page");
        assert!(page.content.contains("# Hello"));
        assert!(page.tags.contains(&"demo".to_string()));

        cleanup_tempdir(&tmp);
    }

    #[tokio::test]
    async fn test_list_pages() {
        let tmp = tempdir();
        tokio::fs::write(tmp.join("a.md"), "Note A").await.unwrap();
        tokio::fs::write(tmp.join("b.md"), "Note B").await.unwrap();
        tokio::fs::create_dir(tmp.join("subfolder")).await.unwrap();

        let provider = ObsidianProvider::new(&tmp);
        let pages = provider.list_pages(None).await.unwrap();

        assert!(pages.len() >= 3); // a.md, b.md, subfolder
        let titles: Vec<&str> = pages.iter().map(|p| p.title.as_str()).collect();
        assert!(titles.contains(&"a"));
        assert!(titles.contains(&"b"));
        assert!(titles.contains(&"subfolder"));

        cleanup_tempdir(&tmp);
    }

    #[tokio::test]
    async fn test_create_page() {
        let tmp = tempdir();
        let provider = ObsidianProvider::new(&tmp);

        let request = CreatePageRequest {
            parent_id: None,
            title: "New Note".to_string(),
            content: "Brand new content".to_string(),
            tags: vec!["new".to_string(), "test".to_string()],
        };

        let page = provider.create_page(&request).await.unwrap();
        assert_eq!(page.title, "New Note");
        assert_eq!(page.content, "Brand new content");
        assert!(page.tags.contains(&"new".to_string()));

        // Verify file was written.
        let file_content = tokio::fs::read_to_string(tmp.join("New Note.md"))
            .await
            .unwrap();
        assert!(file_content.contains("title: \"New Note\""));
        assert!(file_content.contains("Brand new content"));

        cleanup_tempdir(&tmp);
    }

    #[tokio::test]
    async fn test_create_page_with_parent() {
        let tmp = tempdir();
        tokio::fs::create_dir(tmp.join("notes")).await.unwrap();

        let provider = ObsidianProvider::new(&tmp);
        let request = CreatePageRequest {
            parent_id: Some("notes".to_string()),
            title: "Sub Note".to_string(),
            content: "Nested content".to_string(),
            tags: vec![],
        };

        let page = provider.create_page(&request).await.unwrap();
        assert_eq!(page.id, "notes/Sub Note.md");

        let file_content = tokio::fs::read_to_string(tmp.join("notes/Sub Note.md"))
            .await
            .unwrap();
        assert!(file_content.contains("Nested content"));

        cleanup_tempdir(&tmp);
    }

    #[tokio::test]
    async fn test_update_page() {
        let tmp = tempdir();
        let file = tmp.join("updatable.md");
        tokio::fs::write(&file, "---\ntitle: Original\nauthor: Alice\n---\n\nOld content")
            .await
            .unwrap();

        let provider = ObsidianProvider::new(&tmp);
        let page = provider
            .update_page("updatable.md", "New updated content")
            .await
            .unwrap();

        assert_eq!(page.title, "Original");
        assert_eq!(page.content, "New updated content");

        // Verify front matter was preserved.
        let file_content = tokio::fs::read_to_string(&file).await.unwrap();
        assert!(file_content.contains("title:"));
        assert!(file_content.contains("author:"));
        assert!(file_content.contains("New updated content"));
        assert!(!file_content.contains("Old content"));

        cleanup_tempdir(&tmp);
    }

    #[tokio::test]
    async fn test_search_integration() {
        let tmp = tempdir();
        tokio::fs::write(
            tmp.join("rust.md"),
            "# Rust Programming\n\nRust is a systems language focused on safety.",
        )
        .await
        .unwrap();
        tokio::fs::write(
            tmp.join("python.md"),
            "# Python\n\nPython is great for scripting.",
        )
        .await
        .unwrap();

        let mut provider = ObsidianProvider::new(&tmp);
        provider.index_vault().await.unwrap();

        let results = provider.search("rust systems", 10).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].page_id, "rust.md");
        assert_eq!(results[0].platform, KBPlatform::Obsidian);

        cleanup_tempdir(&tmp);
    }

    // -- Test helpers -------------------------------------------------------

    /// Create a temporary directory for tests.
    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hive_obsidian_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Clean up a temporary directory.
    fn cleanup_tempdir(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }
}
