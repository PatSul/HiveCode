//! Notion knowledge base provider.
//!
//! Wraps the Notion API at `https://api.notion.com/v1/` using
//! `reqwest` for HTTP and integration token authentication.
//! Converts Notion block trees to and from Markdown for a uniform
//! content representation.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::Value;
use tracing::debug;

use super::{
    CreatePageRequest, KBPage, KBPageSummary, KBPlatform, KBSearchResult,
    KnowledgeBaseProvider,
};

const DEFAULT_BASE_URL: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";

// -- Client -----------------------------------------------------------------

/// Notion API client implementing [`KnowledgeBaseProvider`].
pub struct NotionClient {
    api_key: String,
    base_url: String,
    client: Client,
}

impl NotionClient {
    /// Create a new Notion client with the given integration token.
    pub fn new(api_key: &str) -> Result<Self> {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Create a new Notion client pointing at a custom base URL (useful for tests).
    pub fn with_base_url(api_key: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {api_key}"))
            .context("invalid characters in Notion API key")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "Notion-Version",
            HeaderValue::from_static(NOTION_VERSION),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Notion")?;

        Ok(Self {
            api_key: api_key.to_string(),
            base_url,
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    // -- HTTP helpers -------------------------------------------------------

    /// Perform a GET request and parse the JSON body.
    async fn get_json(&self, url: &str) -> Result<Value> {
        debug!(url = %url, "Notion GET");

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("Notion GET request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Notion API HTTP error ({}): {}", status, body);
        }

        resp.json::<Value>()
            .await
            .context("failed to parse Notion response JSON")
    }

    /// Perform a POST request with a JSON body and parse the response.
    async fn post_json(&self, url: &str, body: &Value) -> Result<Value> {
        debug!(url = %url, "Notion POST");

        let resp = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .context("Notion POST request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Notion API HTTP error ({}): {}", status, body_text);
        }

        resp.json::<Value>()
            .await
            .context("failed to parse Notion response JSON")
    }

    /// Perform a PATCH request with a JSON body and parse the response.
    async fn patch_json(&self, url: &str, body: &Value) -> Result<Value> {
        debug!(url = %url, "Notion PATCH");

        let resp = self
            .client
            .patch(url)
            .json(body)
            .send()
            .await
            .context("Notion PATCH request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Notion API HTTP error ({}): {}", status, body_text);
        }

        resp.json::<Value>()
            .await
            .context("failed to parse Notion response JSON")
    }

    // -- Notion-specific helpers --------------------------------------------

    /// Fetch all child blocks of a given block (page) ID.
    async fn get_blocks(&self, block_id: &str) -> Result<Vec<Value>> {
        let mut blocks = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = format!("{}/blocks/{}/children?page_size=100", self.base_url, block_id);
            if let Some(ref c) = cursor {
                url.push_str(&format!("&start_cursor={c}"));
            }

            let data = self.get_json(&url).await?;

            if let Some(results) = data["results"].as_array() {
                blocks.extend(results.iter().cloned());
            }

            if data["has_more"].as_bool() == Some(true) {
                cursor = data["next_cursor"].as_str().map(String::from);
            } else {
                break;
            }
        }

        Ok(blocks)
    }

    /// Extract the title from a Notion page object.
    fn extract_title(page: &Value) -> String {
        // Try "properties" -> "title" or "Name" -> "title" array.
        if let Some(props) = page["properties"].as_object() {
            for (_key, prop) in props {
                if prop["type"].as_str() == Some("title") {
                    if let Some(titles) = prop["title"].as_array() {
                        let title: String = titles
                            .iter()
                            .filter_map(|t| t["plain_text"].as_str())
                            .collect::<Vec<_>>()
                            .join("");
                        if !title.is_empty() {
                            return title;
                        }
                    }
                }
            }
        }
        String::from("Untitled")
    }

    /// Extract a parent page ID from a Notion page object.
    fn extract_parent_id(page: &Value) -> Option<String> {
        let parent = &page["parent"];
        parent["page_id"]
            .as_str()
            .or_else(|| parent["database_id"].as_str())
            .map(|s| s.replace('-', ""))
    }

    /// Extract tags from Notion page properties.
    ///
    /// Looks for a multi-select property named "Tags" or "tags".
    fn extract_tags(page: &Value) -> Vec<String> {
        if let Some(props) = page["properties"].as_object() {
            for key in &["Tags", "tags", "Tag", "tag"] {
                if let Some(prop) = props.get(*key) {
                    if let Some(options) = prop["multi_select"].as_array() {
                        return options
                            .iter()
                            .filter_map(|o| o["name"].as_str().map(String::from))
                            .collect();
                    }
                }
            }
        }
        vec![]
    }

    /// Parse an ISO 8601 timestamp from a Notion object.
    fn parse_datetime(value: &Value, key: &str) -> Option<DateTime<Utc>> {
        value[key]
            .as_str()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
    }

    /// Build a [`KBPage`] from a Notion page object and its block content.
    fn build_page(page: &Value, content: String) -> KBPage {
        let id = page["id"]
            .as_str()
            .unwrap_or_default()
            .replace('-', "");

        KBPage {
            id: id.clone(),
            title: Self::extract_title(page),
            content,
            url: Some(format!("https://notion.so/{id}")),
            parent_id: Self::extract_parent_id(page),
            created_at: Self::parse_datetime(page, "created_time"),
            updated_at: Self::parse_datetime(page, "last_edited_time"),
            tags: Self::extract_tags(page),
        }
    }
}

#[async_trait]
impl KnowledgeBaseProvider for NotionClient {
    fn platform(&self) -> KBPlatform {
        KBPlatform::Notion
    }

    async fn search(&self, query: &str, limit: u32) -> Result<Vec<KBSearchResult>> {
        let url = format!("{}/search", self.base_url);
        let payload = serde_json::json!({
            "query": query,
            "page_size": limit.min(100),
            "filter": {
                "value": "page",
                "property": "object"
            }
        });

        debug!(url = %url, query = %query, limit = limit, "searching Notion");

        let data = self.post_json(&url, &payload).await?;

        let results = data["results"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let search_results: Vec<KBSearchResult> = results
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let id = page["id"]
                    .as_str()
                    .unwrap_or_default()
                    .replace('-', "");
                let title = Self::extract_title(page);
                // Notion search does not return snippets natively; use the
                // title and any available description as a best-effort snippet.
                let snippet = page["properties"]["description"]["rich_text"]
                    .as_array()
                    .and_then(|arr| {
                        Some(
                            arr.iter()
                                .filter_map(|t| t["plain_text"].as_str())
                                .collect::<Vec<_>>()
                                .join(""),
                        )
                    })
                    .unwrap_or_default();

                // Approximate relevance: Notion returns results in order of
                // relevance, so we assign a descending score.
                let relevance_score = 1.0 - (i as f64 * 0.05).min(0.99);

                KBSearchResult {
                    page_id: id.clone(),
                    title,
                    snippet,
                    relevance_score,
                    url: Some(format!("https://notion.so/{id}")),
                    platform: KBPlatform::Notion,
                }
            })
            .collect();

        Ok(search_results)
    }

    async fn get_page(&self, page_id: &str) -> Result<KBPage> {
        let page_url = format!("{}/pages/{}", self.base_url, page_id);
        let page = self.get_json(&page_url).await?;

        let blocks = self.get_blocks(page_id).await?;
        let content = blocks_to_markdown(&blocks);

        Ok(Self::build_page(&page, content))
    }

    async fn list_pages(&self, parent_id: Option<&str>) -> Result<Vec<KBPageSummary>> {
        let url = format!("{}/search", self.base_url);

        let payload = if let Some(pid) = parent_id {
            serde_json::json!({
                "filter": {
                    "value": "page",
                    "property": "object"
                },
                "page_size": 100,
                "query": "",
                "filter_properties": ["title"],
                "sort": {
                    "direction": "descending",
                    "timestamp": "last_edited_time"
                },
                "parent": {
                    "page_id": pid
                }
            })
        } else {
            serde_json::json!({
                "filter": {
                    "value": "page",
                    "property": "object"
                },
                "page_size": 100,
                "query": ""
            })
        };

        let data = self.post_json(&url, &payload).await?;

        let results = data["results"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let summaries = results
            .iter()
            .filter(|page| {
                if let Some(pid) = parent_id {
                    Self::extract_parent_id(page)
                        .map(|p| p == pid.replace('-', ""))
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|page| {
                let id = page["id"]
                    .as_str()
                    .unwrap_or_default()
                    .replace('-', "");

                KBPageSummary {
                    id,
                    title: Self::extract_title(page),
                    parent_id: Self::extract_parent_id(page),
                    has_children: page["has_children"].as_bool().unwrap_or(false),
                }
            })
            .collect();

        Ok(summaries)
    }

    async fn create_page(&self, request: &CreatePageRequest) -> Result<KBPage> {
        let url = format!("{}/pages", self.base_url);

        let parent = if let Some(ref pid) = request.parent_id {
            serde_json::json!({ "page_id": pid })
        } else {
            anyhow::bail!("Notion requires a parent_id to create a page");
        };

        let blocks = markdown_to_blocks(&request.content);

        let mut properties = serde_json::json!({
            "title": {
                "title": [{
                    "text": {
                        "content": request.title
                    }
                }]
            }
        });

        // Add tags as a multi_select property if provided.
        if !request.tags.is_empty() {
            properties["Tags"] = serde_json::json!({
                "multi_select": request.tags.iter().map(|t| {
                    serde_json::json!({ "name": t })
                }).collect::<Vec<_>>()
            });
        }

        let payload = serde_json::json!({
            "parent": parent,
            "properties": properties,
            "children": blocks
        });

        debug!(title = %request.title, "creating Notion page");

        let page = self.post_json(&url, &payload).await?;
        Ok(Self::build_page(&page, request.content.clone()))
    }

    async fn update_page(&self, page_id: &str, content: &str) -> Result<KBPage> {
        // First, fetch the existing page metadata.
        let page_url = format!("{}/pages/{}", self.base_url, page_id);
        let page = self.get_json(&page_url).await?;

        // Delete existing children by fetching and archiving them.
        let existing_blocks = self.get_blocks(page_id).await?;
        for block in &existing_blocks {
            if let Some(block_id) = block["id"].as_str() {
                let delete_url = format!("{}/blocks/{}", self.base_url, block_id);
                // We use a DELETE-like approach: Notion archives blocks via DELETE.
                let resp = self
                    .client
                    .delete(&delete_url)
                    .send()
                    .await
                    .context("failed to delete Notion block")?;

                if !resp.status().is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    tracing::warn!(
                        block_id = %block_id,
                        body = %body,
                        "failed to delete block during page update"
                    );
                }
            }
        }

        // Append new blocks.
        let blocks = markdown_to_blocks(content);
        let append_url = format!("{}/blocks/{}/children", self.base_url, page_id);
        let payload = serde_json::json!({ "children": blocks });
        self.patch_json(&append_url, &payload).await?;

        Ok(Self::build_page(&page, content.to_string()))
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
                    context.push_str(&format!("## {}\n", page.title));
                    if let Some(ref url) = page.url {
                        context.push_str(&format!("Source: {url}\n\n"));
                    }
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
                        "failed to fetch Notion page for context"
                    );
                }
            }
        }

        Ok(context)
    }
}

// -- Block conversion helpers -----------------------------------------------

/// Extract plain text from a Notion rich text array.
fn rich_text_to_string(rich_text: &[Value]) -> String {
    rich_text
        .iter()
        .filter_map(|t| {
            // Try `plain_text` first (Notion API format), fall back to
            // `text.content` (our own markdown_to_blocks output).
            t["plain_text"]
                .as_str()
                .or_else(|| t["text"]["content"].as_str())
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Convert a slice of Notion block objects into Markdown text.
pub fn blocks_to_markdown(blocks: &[Value]) -> String {
    let mut md = String::new();

    for block in blocks {
        let block_type = block["type"].as_str().unwrap_or("");

        match block_type {
            "paragraph" => {
                let text = block["paragraph"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&text);
                md.push_str("\n\n");
            }
            "heading_1" => {
                let text = block["heading_1"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("# {text}\n\n"));
            }
            "heading_2" => {
                let text = block["heading_2"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("## {text}\n\n"));
            }
            "heading_3" => {
                let text = block["heading_3"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("### {text}\n\n"));
            }
            "bulleted_list_item" => {
                let text = block["bulleted_list_item"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("- {text}\n"));
            }
            "numbered_list_item" => {
                let text = block["numbered_list_item"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("1. {text}\n"));
            }
            "to_do" => {
                let text = block["to_do"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                let checked = block["to_do"]["checked"].as_bool().unwrap_or(false);
                let marker = if checked { "[x]" } else { "[ ]" };
                md.push_str(&format!("- {marker} {text}\n"));
            }
            "code" => {
                let text = block["code"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                let language = block["code"]["language"]
                    .as_str()
                    .unwrap_or("plain text");
                md.push_str(&format!("```{language}\n{text}\n```\n\n"));
            }
            "quote" => {
                let text = block["quote"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                for line in text.lines() {
                    md.push_str(&format!("> {line}\n"));
                }
                md.push('\n');
            }
            "callout" => {
                let text = block["callout"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("> {text}\n\n"));
            }
            "divider" => {
                md.push_str("---\n\n");
            }
            "toggle" => {
                let text = block["toggle"]["rich_text"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("**{text}**\n\n"));
            }
            "image" => {
                let url = block["image"]["file"]["url"]
                    .as_str()
                    .or_else(|| block["image"]["external"]["url"].as_str())
                    .unwrap_or("");
                let caption = block["image"]["caption"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                md.push_str(&format!("![{caption}]({url})\n\n"));
            }
            "bookmark" => {
                let url = block["bookmark"]["url"].as_str().unwrap_or("");
                let caption = block["bookmark"]["caption"]
                    .as_array()
                    .map(|arr| rich_text_to_string(arr))
                    .unwrap_or_default();
                let label = if caption.is_empty() {
                    url.to_string()
                } else {
                    caption
                };
                md.push_str(&format!("[{label}]({url})\n\n"));
            }
            "table_of_contents" => {
                md.push_str("[Table of Contents]\n\n");
            }
            _ => {
                // For unsupported block types, try extracting rich_text if present.
                if let Some(inner) = block.get(block_type) {
                    if let Some(arr) = inner["rich_text"].as_array() {
                        let text = rich_text_to_string(arr);
                        if !text.is_empty() {
                            md.push_str(&text);
                            md.push_str("\n\n");
                        }
                    }
                }
            }
        }
    }

    md.trim_end().to_string()
}

/// Convert a Markdown string into a vector of Notion block objects.
///
/// Supports headings, bullet lists, numbered lists, code blocks, quotes,
/// horizontal rules, and paragraphs.
pub fn markdown_to_blocks(md: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Code fences.
        if line.starts_with("```") {
            let language = line.trim_start_matches('`').trim();
            let language = if language.is_empty() {
                "plain text"
            } else {
                language
            };
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            // Skip closing ```.
            if i < lines.len() {
                i += 1;
            }
            let code_text = code_lines.join("\n");
            blocks.push(serde_json::json!({
                "object": "block",
                "type": "code",
                "code": {
                    "rich_text": [{ "type": "text", "text": { "content": code_text } }],
                    "language": language
                }
            }));
            continue;
        }

        // Headings.
        if line.starts_with("### ") {
            blocks.push(serde_json::json!({
                "object": "block",
                "type": "heading_3",
                "heading_3": {
                    "rich_text": [{ "type": "text", "text": { "content": &line[4..] } }]
                }
            }));
            i += 1;
            continue;
        }
        if line.starts_with("## ") {
            blocks.push(serde_json::json!({
                "object": "block",
                "type": "heading_2",
                "heading_2": {
                    "rich_text": [{ "type": "text", "text": { "content": &line[3..] } }]
                }
            }));
            i += 1;
            continue;
        }
        if line.starts_with("# ") {
            blocks.push(serde_json::json!({
                "object": "block",
                "type": "heading_1",
                "heading_1": {
                    "rich_text": [{ "type": "text", "text": { "content": &line[2..] } }]
                }
            }));
            i += 1;
            continue;
        }

        // Horizontal rule.
        if line == "---" || line == "***" || line == "___" {
            blocks.push(serde_json::json!({
                "object": "block",
                "type": "divider",
                "divider": {}
            }));
            i += 1;
            continue;
        }

        // Bulleted list item.
        if line.starts_with("- ") || line.starts_with("* ") {
            let text = &line[2..];
            // Check for checkbox syntax.
            if text.starts_with("[ ] ") || text.starts_with("[x] ") || text.starts_with("[X] ") {
                let checked = text.starts_with("[x] ") || text.starts_with("[X] ");
                let todo_text = &text[4..];
                blocks.push(serde_json::json!({
                    "object": "block",
                    "type": "to_do",
                    "to_do": {
                        "rich_text": [{ "type": "text", "text": { "content": todo_text } }],
                        "checked": checked
                    }
                }));
            } else {
                blocks.push(serde_json::json!({
                    "object": "block",
                    "type": "bulleted_list_item",
                    "bulleted_list_item": {
                        "rich_text": [{ "type": "text", "text": { "content": text } }]
                    }
                }));
            }
            i += 1;
            continue;
        }

        // Numbered list item.
        if let Some(rest) = strip_numbered_prefix(line) {
            blocks.push(serde_json::json!({
                "object": "block",
                "type": "numbered_list_item",
                "numbered_list_item": {
                    "rich_text": [{ "type": "text", "text": { "content": rest } }]
                }
            }));
            i += 1;
            continue;
        }

        // Blockquote.
        if line.starts_with("> ") {
            let mut quote_lines = Vec::new();
            while i < lines.len() && lines[i].starts_with("> ") {
                quote_lines.push(&lines[i][2..]);
                i += 1;
            }
            let quote_text = quote_lines.join("\n");
            blocks.push(serde_json::json!({
                "object": "block",
                "type": "quote",
                "quote": {
                    "rich_text": [{ "type": "text", "text": { "content": quote_text } }]
                }
            }));
            continue;
        }

        // Empty lines are skipped.
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Default: paragraph.
        blocks.push(serde_json::json!({
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "type": "text", "text": { "content": line } }]
            }
        }));
        i += 1;
    }

    blocks
}

/// Strip a numbered list prefix like "1. ", "2. ", etc. and return the rest.
fn strip_numbered_prefix(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut pos = 0;

    // Expect one or more digits.
    if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
        return None;
    }
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }

    // Expect ". ".
    if pos + 1 >= bytes.len() || bytes[pos] != b'.' || bytes[pos + 1] != b' ' {
        return None;
    }

    Some(&line[pos + 2..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notion_client_default_base_url() {
        let client = NotionClient::new("ntn_test_token").unwrap();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_notion_client_custom_base_url() {
        let client =
            NotionClient::with_base_url("ntn_tok", "https://api.notion.test/v1/").unwrap();
        assert_eq!(client.base_url(), "https://api.notion.test/v1");
    }

    #[test]
    fn test_notion_client_token_stored() {
        let client = NotionClient::new("ntn_my_key").unwrap();
        assert_eq!(client.api_key(), "ntn_my_key");
    }

    #[test]
    fn test_notion_client_platform() {
        let client = NotionClient::new("ntn_tok").unwrap();
        assert_eq!(client.platform(), KBPlatform::Notion);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = NotionClient::new("tok\nwith\nnewlines");
        assert!(result.is_err());
    }

    // -- blocks_to_markdown tests -------------------------------------------

    #[test]
    fn test_blocks_to_markdown_paragraph() {
        let blocks = vec![serde_json::json!({
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "plain_text": "Hello world" }]
            }
        })];
        assert_eq!(blocks_to_markdown(&blocks), "Hello world");
    }

    #[test]
    fn test_blocks_to_markdown_headings() {
        let blocks = vec![
            serde_json::json!({
                "type": "heading_1",
                "heading_1": { "rich_text": [{ "plain_text": "Title" }] }
            }),
            serde_json::json!({
                "type": "heading_2",
                "heading_2": { "rich_text": [{ "plain_text": "Subtitle" }] }
            }),
            serde_json::json!({
                "type": "heading_3",
                "heading_3": { "rich_text": [{ "plain_text": "Section" }] }
            }),
        ];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("# Title"));
        assert!(md.contains("## Subtitle"));
        assert!(md.contains("### Section"));
    }

    #[test]
    fn test_blocks_to_markdown_bulleted_list() {
        let blocks = vec![
            serde_json::json!({
                "type": "bulleted_list_item",
                "bulleted_list_item": { "rich_text": [{ "plain_text": "Item A" }] }
            }),
            serde_json::json!({
                "type": "bulleted_list_item",
                "bulleted_list_item": { "rich_text": [{ "plain_text": "Item B" }] }
            }),
        ];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("- Item A"));
        assert!(md.contains("- Item B"));
    }

    #[test]
    fn test_blocks_to_markdown_code() {
        let blocks = vec![serde_json::json!({
            "type": "code",
            "code": {
                "rich_text": [{ "plain_text": "fn main() {}" }],
                "language": "rust"
            }
        })];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("```rust"));
        assert!(md.contains("fn main() {}"));
        assert!(md.contains("```"));
    }

    #[test]
    fn test_blocks_to_markdown_quote() {
        let blocks = vec![serde_json::json!({
            "type": "quote",
            "quote": {
                "rich_text": [{ "plain_text": "To be or not to be" }]
            }
        })];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("> To be or not to be"));
    }

    #[test]
    fn test_blocks_to_markdown_divider() {
        let blocks = vec![serde_json::json!({
            "type": "divider",
            "divider": {}
        })];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("---"));
    }

    #[test]
    fn test_blocks_to_markdown_todo() {
        let blocks = vec![
            serde_json::json!({
                "type": "to_do",
                "to_do": {
                    "rich_text": [{ "plain_text": "Buy milk" }],
                    "checked": false
                }
            }),
            serde_json::json!({
                "type": "to_do",
                "to_do": {
                    "rich_text": [{ "plain_text": "Write tests" }],
                    "checked": true
                }
            }),
        ];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("- [ ] Buy milk"));
        assert!(md.contains("- [x] Write tests"));
    }

    #[test]
    fn test_blocks_to_markdown_numbered_list() {
        let blocks = vec![
            serde_json::json!({
                "type": "numbered_list_item",
                "numbered_list_item": { "rich_text": [{ "plain_text": "First" }] }
            }),
            serde_json::json!({
                "type": "numbered_list_item",
                "numbered_list_item": { "rich_text": [{ "plain_text": "Second" }] }
            }),
        ];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("1. First"));
        assert!(md.contains("1. Second"));
    }

    #[test]
    fn test_blocks_to_markdown_image() {
        let blocks = vec![serde_json::json!({
            "type": "image",
            "image": {
                "external": { "url": "https://example.com/img.png" },
                "caption": [{ "plain_text": "My image" }]
            }
        })];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("![My image](https://example.com/img.png)"));
    }

    #[test]
    fn test_blocks_to_markdown_empty() {
        let blocks: Vec<Value> = vec![];
        assert_eq!(blocks_to_markdown(&blocks), "");
    }

    // -- markdown_to_blocks tests -------------------------------------------

    #[test]
    fn test_markdown_to_blocks_paragraph() {
        let blocks = markdown_to_blocks("Hello world");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "paragraph");
        assert_eq!(
            blocks[0]["paragraph"]["rich_text"][0]["text"]["content"],
            "Hello world"
        );
    }

    #[test]
    fn test_markdown_to_blocks_headings() {
        let md = "# Title\n## Subtitle\n### Section";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0]["type"], "heading_1");
        assert_eq!(blocks[1]["type"], "heading_2");
        assert_eq!(blocks[2]["type"], "heading_3");
    }

    #[test]
    fn test_markdown_to_blocks_bulleted_list() {
        let md = "- Item A\n- Item B\n* Item C";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 3);
        for block in &blocks {
            assert_eq!(block["type"], "bulleted_list_item");
        }
    }

    #[test]
    fn test_markdown_to_blocks_numbered_list() {
        let md = "1. First\n2. Second\n3. Third";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 3);
        for block in &blocks {
            assert_eq!(block["type"], "numbered_list_item");
        }
    }

    #[test]
    fn test_markdown_to_blocks_code_fence() {
        let md = "```rust\nfn main() {}\n```";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "code");
        assert_eq!(blocks[0]["code"]["language"], "rust");
        assert_eq!(
            blocks[0]["code"]["rich_text"][0]["text"]["content"],
            "fn main() {}"
        );
    }

    #[test]
    fn test_markdown_to_blocks_blockquote() {
        let md = "> Line one\n> Line two";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "quote");
        assert_eq!(
            blocks[0]["quote"]["rich_text"][0]["text"]["content"],
            "Line one\nLine two"
        );
    }

    #[test]
    fn test_markdown_to_blocks_divider() {
        let md = "---";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "divider");
    }

    #[test]
    fn test_markdown_to_blocks_todo() {
        let md = "- [ ] Unchecked\n- [x] Checked";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "to_do");
        assert_eq!(blocks[0]["to_do"]["checked"], false);
        assert_eq!(blocks[1]["type"], "to_do");
        assert_eq!(blocks[1]["to_do"]["checked"], true);
    }

    #[test]
    fn test_markdown_to_blocks_empty_lines_skipped() {
        let md = "Hello\n\n\nWorld";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_markdown_roundtrip_headings() {
        let original = "# Title\n\n## Subtitle\n\n### Section";
        let blocks = markdown_to_blocks(original);
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("# Title"));
        assert!(md.contains("## Subtitle"));
        assert!(md.contains("### Section"));
    }

    #[test]
    fn test_strip_numbered_prefix() {
        assert_eq!(strip_numbered_prefix("1. Hello"), Some("Hello"));
        assert_eq!(strip_numbered_prefix("42. World"), Some("World"));
        assert_eq!(strip_numbered_prefix("Not a list"), None);
        assert_eq!(strip_numbered_prefix("1.No space"), None);
        assert_eq!(strip_numbered_prefix(""), None);
    }

    #[test]
    fn test_extract_title_from_page() {
        let page = serde_json::json!({
            "properties": {
                "Name": {
                    "type": "title",
                    "title": [
                        { "plain_text": "My Page" }
                    ]
                }
            }
        });
        assert_eq!(NotionClient::extract_title(&page), "My Page");
    }

    #[test]
    fn test_extract_title_untitled() {
        let page = serde_json::json!({ "properties": {} });
        assert_eq!(NotionClient::extract_title(&page), "Untitled");
    }

    #[test]
    fn test_extract_parent_id() {
        let page = serde_json::json!({
            "parent": { "page_id": "abc-def-123" }
        });
        assert_eq!(
            NotionClient::extract_parent_id(&page),
            Some("abcdef123".into())
        );
    }

    #[test]
    fn test_extract_parent_id_database() {
        let page = serde_json::json!({
            "parent": { "database_id": "db-123-456" }
        });
        assert_eq!(
            NotionClient::extract_parent_id(&page),
            Some("db123456".into())
        );
    }

    #[test]
    fn test_extract_parent_id_none() {
        let page = serde_json::json!({ "parent": { "type": "workspace" } });
        assert_eq!(NotionClient::extract_parent_id(&page), None);
    }

    #[test]
    fn test_extract_tags() {
        let page = serde_json::json!({
            "properties": {
                "Tags": {
                    "multi_select": [
                        { "name": "engineering" },
                        { "name": "roadmap" }
                    ]
                }
            }
        });
        let tags = NotionClient::extract_tags(&page);
        assert_eq!(tags, vec!["engineering", "roadmap"]);
    }

    #[test]
    fn test_extract_tags_empty() {
        let page = serde_json::json!({ "properties": {} });
        let tags = NotionClient::extract_tags(&page);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_rich_text_to_string() {
        let arr = vec![
            serde_json::json!({ "plain_text": "Hello " }),
            serde_json::json!({ "plain_text": "world" }),
        ];
        assert_eq!(rich_text_to_string(&arr), "Hello world");
    }

    #[test]
    fn test_rich_text_to_string_empty() {
        let arr: Vec<Value> = vec![];
        assert_eq!(rich_text_to_string(&arr), "");
    }
}
