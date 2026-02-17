//! Knowledge base provider trait, shared types, and hub.
//!
//! Defines the [`KnowledgeBaseProvider`] trait that all platform-specific
//! implementations (Notion, Obsidian, etc.) must satisfy, along with the
//! common data types exchanged across providers and a [`KnowledgeHub`]
//! that routes operations to the appropriate provider and supports
//! cross-platform search.

pub mod notion;
pub mod obsidian;

pub use notion::NotionClient;
pub use obsidian::ObsidianProvider;

use std::collections::HashMap;
use std::fmt;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

// -- Platform enum ----------------------------------------------------------

/// Supported knowledge base platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KBPlatform {
    Notion,
    Obsidian,
}

impl fmt::Display for KBPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KBPlatform::Notion => write!(f, "notion"),
            KBPlatform::Obsidian => write!(f, "obsidian"),
        }
    }
}

// -- Shared data types ------------------------------------------------------

/// A full knowledge base page with content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KBPage {
    pub id: String,
    pub title: String,
    pub content: String,
    pub url: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A lightweight page summary without content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KBPageSummary {
    pub id: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub has_children: bool,
}

/// A search result referencing a knowledge base page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KBSearchResult {
    pub page_id: String,
    pub title: String,
    pub snippet: String,
    pub relevance_score: f64,
    pub url: Option<String>,
    pub platform: KBPlatform,
}

/// Request to create a new page in a knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePageRequest {
    pub parent_id: Option<String>,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

// -- Provider trait ----------------------------------------------------------

/// Trait that every knowledge base platform integration must implement.
#[async_trait]
pub trait KnowledgeBaseProvider: Send + Sync {
    /// Return the platform this provider handles.
    fn platform(&self) -> KBPlatform;

    /// Search pages by a text query, returning up to `limit` results.
    async fn search(&self, query: &str, limit: u32) -> Result<Vec<KBSearchResult>>;

    /// Get a full page by its ID, including content.
    async fn get_page(&self, page_id: &str) -> Result<KBPage>;

    /// List pages under a given parent, or list root-level pages when
    /// `parent_id` is `None`.
    async fn list_pages(&self, parent_id: Option<&str>) -> Result<Vec<KBPageSummary>>;

    /// Create a new page.
    async fn create_page(&self, request: &CreatePageRequest) -> Result<KBPage>;

    /// Update the content of an existing page.
    async fn update_page(&self, page_id: &str, content: &str) -> Result<KBPage>;

    /// Retrieve relevant context for an AI prompt by searching and formatting
    /// the most relevant pages into a single string.
    async fn get_context(&self, query: &str) -> Result<String> {
        let results = self.search(query, 5).await?;

        if results.is_empty() {
            return Ok(String::new());
        }

        let mut context = String::new();
        for result in &results {
            match self.get_page(&result.page_id).await {
                Ok(page) => {
                    context.push_str(&format!("## {}\n\n", page.title));
                    // Truncate very long pages to keep context manageable.
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
                        "failed to fetch page for context"
                    );
                }
            }
        }

        Ok(context)
    }
}

// -- Hub --------------------------------------------------------------------

/// Central hub that manages and dispatches to knowledge base providers.
///
/// Supports cross-platform search by aggregating results from every
/// registered provider, sorted by relevance score.
pub struct KnowledgeHub {
    providers: HashMap<KBPlatform, Box<dyn KnowledgeBaseProvider>>,
}

impl KnowledgeHub {
    /// Create a new empty hub with no providers registered.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider for its platform, replacing any previous one.
    pub fn register_provider(&mut self, provider: Box<dyn KnowledgeBaseProvider>) {
        let platform = provider.platform();
        debug!(platform = %platform, "registering knowledge base provider");
        self.providers.insert(platform, provider);
    }

    /// Return the number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Check whether a provider is registered for the given platform.
    pub fn has_provider(&self, platform: KBPlatform) -> bool {
        self.providers.contains_key(&platform)
    }

    /// Return the list of platforms that have registered providers.
    pub fn platforms(&self) -> Vec<KBPlatform> {
        self.providers.keys().copied().collect()
    }

    /// Get a reference to the provider for the given platform.
    fn provider(&self, platform: KBPlatform) -> Result<&dyn KnowledgeBaseProvider> {
        self.providers
            .get(&platform)
            .map(|p| p.as_ref())
            .context(format!("no provider registered for {platform}"))
    }

    /// Search pages on a specific platform.
    pub async fn search(
        &self,
        platform: KBPlatform,
        query: &str,
        limit: u32,
    ) -> Result<Vec<KBSearchResult>> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, query = %query, "searching knowledge base via hub");
        provider.search(query, limit).await
    }

    /// Search across **all** registered providers and return results sorted
    /// by relevance score (highest first).
    pub async fn search_all(&self, query: &str, limit: u32) -> Vec<KBSearchResult> {
        let mut all_results = Vec::new();

        for (platform, provider) in &self.providers {
            match provider.search(query, limit).await {
                Ok(results) => all_results.extend(results),
                Err(e) => {
                    tracing::warn!(
                        platform = %platform,
                        error = %e,
                        "failed to search knowledge base"
                    );
                }
            }
        }

        // Sort by relevance score descending.
        all_results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to the requested limit.
        all_results.truncate(limit as usize);
        all_results
    }

    /// Get a full page from a specific platform.
    pub async fn get_page(&self, platform: KBPlatform, page_id: &str) -> Result<KBPage> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, page_id = %page_id, "getting page via hub");
        provider.get_page(page_id).await
    }

    /// List pages on a specific platform.
    pub async fn list_pages(
        &self,
        platform: KBPlatform,
        parent_id: Option<&str>,
    ) -> Result<Vec<KBPageSummary>> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, "listing pages via hub");
        provider.list_pages(parent_id).await
    }

    /// Create a page on a specific platform.
    pub async fn create_page(
        &self,
        platform: KBPlatform,
        request: &CreatePageRequest,
    ) -> Result<KBPage> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, title = %request.title, "creating page via hub");
        provider.create_page(request).await
    }

    /// Update a page on a specific platform.
    pub async fn update_page(
        &self,
        platform: KBPlatform,
        page_id: &str,
        content: &str,
    ) -> Result<KBPage> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, page_id = %page_id, "updating page via hub");
        provider.update_page(page_id, content).await
    }

    /// Retrieve AI context from a specific platform.
    pub async fn get_context(&self, platform: KBPlatform, query: &str) -> Result<String> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, query = %query, "getting AI context via hub");
        provider.get_context(query).await
    }

    /// Retrieve AI context from **all** registered providers, concatenated.
    pub async fn get_context_all(&self, query: &str) -> String {
        let mut context = String::new();

        for (platform, provider) in &self.providers {
            match provider.get_context(query).await {
                Ok(ctx) if !ctx.is_empty() => {
                    context.push_str(&format!("# Context from {platform}\n\n"));
                    context.push_str(&ctx);
                    context.push_str("\n---\n\n");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        platform = %platform,
                        error = %e,
                        "failed to get AI context from knowledge base"
                    );
                }
            }
        }

        context
    }
}

impl Default for KnowledgeHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kb_platform_display() {
        assert_eq!(KBPlatform::Notion.to_string(), "notion");
        assert_eq!(KBPlatform::Obsidian.to_string(), "obsidian");
    }

    #[test]
    fn test_kb_platform_serialize() {
        let json = serde_json::to_string(&KBPlatform::Notion).unwrap();
        assert_eq!(json, r#""notion""#);
    }

    #[test]
    fn test_kb_platform_deserialize() {
        let p: KBPlatform = serde_json::from_str(r#""obsidian""#).unwrap();
        assert_eq!(p, KBPlatform::Obsidian);
    }

    #[test]
    fn test_kb_platform_roundtrip() {
        for platform in [KBPlatform::Notion, KBPlatform::Obsidian] {
            let json = serde_json::to_string(&platform).unwrap();
            let back: KBPlatform = serde_json::from_str(&json).unwrap();
            assert_eq!(back, platform);
        }
    }

    #[test]
    fn test_kb_page_serialization_roundtrip() {
        let page = KBPage {
            id: "page-1".into(),
            title: "Meeting Notes".into(),
            content: "# Meeting\n\nDiscussed roadmap.".into(),
            url: Some("https://notion.so/page-1".into()),
            parent_id: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            tags: vec!["meetings".into(), "roadmap".into()],
        };
        let json = serde_json::to_string(&page).unwrap();
        let back: KBPage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "page-1");
        assert_eq!(back.title, "Meeting Notes");
        assert_eq!(back.tags.len(), 2);
    }

    #[test]
    fn test_kb_page_summary_serialization() {
        let summary = KBPageSummary {
            id: "page-1".into(),
            title: "Meeting Notes".into(),
            parent_id: Some("parent-1".into()),
            has_children: true,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let back: KBPageSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "page-1");
        assert!(back.has_children);
    }

    #[test]
    fn test_kb_search_result_serialization() {
        let result = KBSearchResult {
            page_id: "page-1".into(),
            title: "Meeting Notes".into(),
            snippet: "Discussed roadmap items...".into(),
            relevance_score: 0.95,
            url: Some("https://notion.so/page-1".into()),
            platform: KBPlatform::Notion,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: KBSearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.page_id, "page-1");
        assert_eq!(back.relevance_score, 0.95);
        assert_eq!(back.platform, KBPlatform::Notion);
    }

    #[test]
    fn test_create_page_request_serialization() {
        let req = CreatePageRequest {
            parent_id: Some("parent-1".into()),
            title: "New Page".into(),
            content: "Hello world".into(),
            tags: vec!["test".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreatePageRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "New Page");
        assert_eq!(back.tags, vec!["test"]);
    }

    #[test]
    fn test_hub_new_is_empty() {
        let hub = KnowledgeHub::new();
        assert_eq!(hub.provider_count(), 0);
        assert!(hub.platforms().is_empty());
    }

    #[test]
    fn test_hub_default_is_empty() {
        let hub = KnowledgeHub::default();
        assert_eq!(hub.provider_count(), 0);
    }

    #[test]
    fn test_kb_platform_hash_used_as_key() {
        let mut map = HashMap::new();
        map.insert(KBPlatform::Notion, "notion-token");
        map.insert(KBPlatform::Obsidian, "obsidian-path");
        assert_eq!(map.get(&KBPlatform::Notion), Some(&"notion-token"));
        assert_eq!(map.get(&KBPlatform::Obsidian), Some(&"obsidian-path"));
    }

    #[test]
    fn test_kb_page_with_empty_tags() {
        let page = KBPage {
            id: "page-2".into(),
            title: "Empty tags".into(),
            content: "No tags here".into(),
            url: None,
            parent_id: None,
            created_at: None,
            updated_at: None,
            tags: vec![],
        };
        let json = serde_json::to_string(&page).unwrap();
        let back: KBPage = serde_json::from_str(&json).unwrap();
        assert!(back.tags.is_empty());
        assert!(back.url.is_none());
    }

    #[test]
    fn test_search_results_sort_by_relevance() {
        let mut results = vec![
            KBSearchResult {
                page_id: "a".into(),
                title: "Low".into(),
                snippet: String::new(),
                relevance_score: 0.3,
                url: None,
                platform: KBPlatform::Notion,
            },
            KBSearchResult {
                page_id: "b".into(),
                title: "High".into(),
                snippet: String::new(),
                relevance_score: 0.9,
                url: None,
                platform: KBPlatform::Obsidian,
            },
            KBSearchResult {
                page_id: "c".into(),
                title: "Medium".into(),
                snippet: String::new(),
                relevance_score: 0.6,
                url: None,
                platform: KBPlatform::Notion,
            },
        ];
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        assert_eq!(results[0].page_id, "b");
        assert_eq!(results[1].page_id, "c");
        assert_eq!(results[2].page_id, "a");
    }
}
