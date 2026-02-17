//! Project management provider trait, shared types, and hub.
//!
//! Defines the [`ProjectManagementProvider`] trait that all platform-specific
//! implementations (Jira, Linear, Asana, etc.) must satisfy, along with the
//! common data types exchanged across providers and a [`ProjectManagementHub`]
//! that routes operations to the appropriate provider.

pub mod asana;
pub mod jira;
pub mod linear;

pub use asana::AsanaClient;
pub use jira::JiraClient;
pub use linear::LinearClient;

use std::collections::HashMap;
use std::fmt;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

// ── Platform enum ──────────────────────────────────────────────────

/// Supported project management platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PMPlatform {
    Jira,
    Linear,
    Asana,
    GitHubProjects,
}

impl fmt::Display for PMPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PMPlatform::Jira => write!(f, "jira"),
            PMPlatform::Linear => write!(f, "linear"),
            PMPlatform::Asana => write!(f, "asana"),
            PMPlatform::GitHubProjects => write!(f, "github_projects"),
        }
    }
}

// ── Issue priority ─────────────────────────────────────────────────

/// Normalized issue priority across platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuePriority {
    Critical,
    High,
    Medium,
    Low,
    None,
}

impl fmt::Display for IssuePriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssuePriority::Critical => write!(f, "critical"),
            IssuePriority::High => write!(f, "high"),
            IssuePriority::Medium => write!(f, "medium"),
            IssuePriority::Low => write!(f, "low"),
            IssuePriority::None => write!(f, "none"),
        }
    }
}

// ── Issue status ───────────────────────────────────────────────────

/// Normalized issue status across platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Done,
    Cancelled,
}

impl fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueStatus::Backlog => write!(f, "backlog"),
            IssueStatus::Todo => write!(f, "todo"),
            IssueStatus::InProgress => write!(f, "in_progress"),
            IssueStatus::InReview => write!(f, "in_review"),
            IssueStatus::Done => write!(f, "done"),
            IssueStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

// ── Shared data types ──────────────────────────────────────────────

/// A project or workspace on a project management platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub key: Option<String>,
    pub description: Option<String>,
    pub platform: PMPlatform,
}

/// An issue or task on a project management platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub key: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: IssueStatus,
    pub priority: IssuePriority,
    pub assignee: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub sprint: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub platform: PMPlatform,
    pub url: Option<String>,
}

/// Filters for listing issues.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueFilters {
    pub status: Option<IssueStatus>,
    pub assignee: Option<String>,
    pub priority: Option<IssuePriority>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub sprint_id: Option<String>,
    pub search_query: Option<String>,
}

/// Request to create a new issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueRequest {
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<IssuePriority>,
    pub assignee: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// Partial update to an existing issue.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub assignee: Option<String>,
    pub labels: Option<Vec<String>>,
}

/// A comment on an issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: Option<String>,
    pub body: String,
    pub created_at: Option<DateTime<Utc>>,
}

/// A sprint or iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sprint {
    pub id: String,
    pub name: String,
    pub state: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

// ── Provider trait ─────────────────────────────────────────────────

/// Trait that every project management platform integration must implement.
#[async_trait]
pub trait ProjectManagementProvider: Send + Sync {
    /// Return the platform this provider handles.
    fn platform(&self) -> PMPlatform;

    /// List all accessible projects.
    async fn list_projects(&self) -> Result<Vec<Project>>;

    /// List issues in a project, optionally filtered.
    async fn list_issues(&self, project_id: &str, filters: &IssueFilters) -> Result<Vec<Issue>>;

    /// Get a single issue by its ID.
    async fn get_issue(&self, issue_id: &str) -> Result<Issue>;

    /// Create a new issue.
    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<Issue>;

    /// Update an existing issue.
    async fn update_issue(&self, issue_id: &str, update: &IssueUpdate) -> Result<Issue>;

    /// Add a comment to an issue.
    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<Comment>;

    /// Transition an issue to a new status.
    async fn transition_issue(&self, issue_id: &str, status: IssueStatus) -> Result<Issue>;

    /// Search issues across the platform.
    async fn search_issues(&self, query: &str, limit: u32) -> Result<Vec<Issue>>;

    /// Get sprints for a project (if the platform supports sprints).
    async fn get_sprints(&self, project_id: &str) -> Result<Vec<Sprint>>;
}

// ── Hub ────────────────────────────────────────────────────────────

/// Central hub that manages and dispatches to project management providers.
pub struct ProjectManagementHub {
    providers: HashMap<PMPlatform, Box<dyn ProjectManagementProvider>>,
}

impl ProjectManagementHub {
    /// Create a new empty hub with no providers registered.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider for its platform, replacing any previous one.
    pub fn register_provider(&mut self, provider: Box<dyn ProjectManagementProvider>) {
        let platform = provider.platform();
        debug!(platform = %platform, "registering project management provider");
        self.providers.insert(platform, provider);
    }

    /// Return the number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Check whether a provider is registered for the given platform.
    pub fn has_provider(&self, platform: PMPlatform) -> bool {
        self.providers.contains_key(&platform)
    }

    /// Return the list of platforms that have registered providers.
    pub fn platforms(&self) -> Vec<PMPlatform> {
        self.providers.keys().copied().collect()
    }

    /// Get a reference to the provider for the given platform.
    fn provider(&self, platform: PMPlatform) -> Result<&dyn ProjectManagementProvider> {
        self.providers
            .get(&platform)
            .map(|p| p.as_ref())
            .context(format!("no provider registered for {platform}"))
    }

    /// List all projects on a specific platform.
    pub async fn list_projects(&self, platform: PMPlatform) -> Result<Vec<Project>> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, "listing projects via hub");
        provider.list_projects().await
    }

    /// List issues in a project on a specific platform.
    pub async fn list_issues(
        &self,
        platform: PMPlatform,
        project_id: &str,
        filters: &IssueFilters,
    ) -> Result<Vec<Issue>> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, project_id = %project_id, "listing issues via hub");
        provider.list_issues(project_id, filters).await
    }

    /// Get a single issue by ID on a specific platform.
    pub async fn get_issue(&self, platform: PMPlatform, issue_id: &str) -> Result<Issue> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, issue_id = %issue_id, "getting issue via hub");
        provider.get_issue(issue_id).await
    }

    /// Create a new issue on a specific platform.
    pub async fn create_issue(
        &self,
        platform: PMPlatform,
        request: &CreateIssueRequest,
    ) -> Result<Issue> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, title = %request.title, "creating issue via hub");
        provider.create_issue(request).await
    }

    /// Update an issue on a specific platform.
    pub async fn update_issue(
        &self,
        platform: PMPlatform,
        issue_id: &str,
        update: &IssueUpdate,
    ) -> Result<Issue> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, issue_id = %issue_id, "updating issue via hub");
        provider.update_issue(issue_id, update).await
    }

    /// Add a comment to an issue on a specific platform.
    pub async fn add_comment(
        &self,
        platform: PMPlatform,
        issue_id: &str,
        body: &str,
    ) -> Result<Comment> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, issue_id = %issue_id, "adding comment via hub");
        provider.add_comment(issue_id, body).await
    }

    /// Transition an issue to a new status on a specific platform.
    pub async fn transition_issue(
        &self,
        platform: PMPlatform,
        issue_id: &str,
        status: IssueStatus,
    ) -> Result<Issue> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, issue_id = %issue_id, status = %status, "transitioning issue via hub");
        provider.transition_issue(issue_id, status).await
    }

    /// Search issues on a specific platform.
    pub async fn search_issues(
        &self,
        platform: PMPlatform,
        query: &str,
        limit: u32,
    ) -> Result<Vec<Issue>> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, query = %query, "searching issues via hub");
        provider.search_issues(query, limit).await
    }

    /// Get sprints for a project on a specific platform.
    pub async fn get_sprints(
        &self,
        platform: PMPlatform,
        project_id: &str,
    ) -> Result<Vec<Sprint>> {
        let provider = self.provider(platform)?;
        debug!(platform = %platform, project_id = %project_id, "getting sprints via hub");
        provider.get_sprints(project_id).await
    }

    /// Collect all issues across all registered providers, applying the given filters.
    ///
    /// Results are aggregated from every provider. Providers that return errors
    /// are logged and skipped rather than failing the entire operation.
    pub async fn all_issues(&self, filters: &IssueFilters) -> Vec<Issue> {
        let mut all = Vec::new();

        for (platform, provider) in &self.providers {
            match provider.list_projects().await {
                Ok(projects) => {
                    for project in &projects {
                        match provider.list_issues(&project.id, filters).await {
                            Ok(issues) => all.extend(issues),
                            Err(e) => {
                                tracing::warn!(
                                    platform = %platform,
                                    project_id = %project.id,
                                    error = %e,
                                    "failed to list issues for project"
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        platform = %platform,
                        error = %e,
                        "failed to list projects"
                    );
                }
            }
        }

        all
    }
}

impl Default for ProjectManagementHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pm_platform_display() {
        assert_eq!(PMPlatform::Jira.to_string(), "jira");
        assert_eq!(PMPlatform::Linear.to_string(), "linear");
        assert_eq!(PMPlatform::Asana.to_string(), "asana");
        assert_eq!(PMPlatform::GitHubProjects.to_string(), "github_projects");
    }

    #[test]
    fn test_pm_platform_serialize() {
        let json = serde_json::to_string(&PMPlatform::Jira).unwrap();
        assert_eq!(json, r#""jira""#);
    }

    #[test]
    fn test_pm_platform_deserialize() {
        let p: PMPlatform = serde_json::from_str(r#""linear""#).unwrap();
        assert_eq!(p, PMPlatform::Linear);
    }

    #[test]
    fn test_pm_platform_roundtrip() {
        for platform in [
            PMPlatform::Jira,
            PMPlatform::Linear,
            PMPlatform::Asana,
            PMPlatform::GitHubProjects,
        ] {
            let json = serde_json::to_string(&platform).unwrap();
            let back: PMPlatform = serde_json::from_str(&json).unwrap();
            assert_eq!(back, platform);
        }
    }

    #[test]
    fn test_issue_priority_display() {
        assert_eq!(IssuePriority::Critical.to_string(), "critical");
        assert_eq!(IssuePriority::High.to_string(), "high");
        assert_eq!(IssuePriority::Medium.to_string(), "medium");
        assert_eq!(IssuePriority::Low.to_string(), "low");
        assert_eq!(IssuePriority::None.to_string(), "none");
    }

    #[test]
    fn test_issue_status_display() {
        assert_eq!(IssueStatus::Backlog.to_string(), "backlog");
        assert_eq!(IssueStatus::Todo.to_string(), "todo");
        assert_eq!(IssueStatus::InProgress.to_string(), "in_progress");
        assert_eq!(IssueStatus::InReview.to_string(), "in_review");
        assert_eq!(IssueStatus::Done.to_string(), "done");
        assert_eq!(IssueStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_issue_priority_roundtrip() {
        for p in [
            IssuePriority::Critical,
            IssuePriority::High,
            IssuePriority::Medium,
            IssuePriority::Low,
            IssuePriority::None,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            let back: IssuePriority = serde_json::from_str(&json).unwrap();
            assert_eq!(back, p);
        }
    }

    #[test]
    fn test_issue_status_roundtrip() {
        for s in [
            IssueStatus::Backlog,
            IssueStatus::Todo,
            IssueStatus::InProgress,
            IssueStatus::InReview,
            IssueStatus::Done,
            IssueStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: IssueStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn test_project_serialization_roundtrip() {
        let project = Project {
            id: "PRJ-1".into(),
            name: "Backend".into(),
            key: Some("BE".into()),
            description: Some("Backend services".into()),
            platform: PMPlatform::Jira,
        };
        let json = serde_json::to_string(&project).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "PRJ-1");
        assert_eq!(back.name, "Backend");
        assert_eq!(back.key.as_deref(), Some("BE"));
        assert_eq!(back.platform, PMPlatform::Jira);
    }

    #[test]
    fn test_issue_serialization_roundtrip() {
        let issue = Issue {
            id: "ISS-42".into(),
            key: Some("BE-42".into()),
            title: "Fix login bug".into(),
            description: Some("Users cannot log in".into()),
            status: IssueStatus::InProgress,
            priority: IssuePriority::High,
            assignee: Some("alice".into()),
            labels: vec!["bug".into(), "auth".into()],
            sprint: Some("Sprint 5".into()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            platform: PMPlatform::Linear,
            url: Some("https://linear.app/issue/BE-42".into()),
        };
        let json = serde_json::to_string(&issue).unwrap();
        let back: Issue = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "ISS-42");
        assert_eq!(back.title, "Fix login bug");
        assert_eq!(back.status, IssueStatus::InProgress);
        assert_eq!(back.priority, IssuePriority::High);
        assert_eq!(back.labels.len(), 2);
    }

    #[test]
    fn test_issue_filters_default() {
        let filters = IssueFilters::default();
        assert!(filters.status.is_none());
        assert!(filters.assignee.is_none());
        assert!(filters.priority.is_none());
        assert!(filters.labels.is_empty());
        assert!(filters.sprint_id.is_none());
        assert!(filters.search_query.is_none());
    }

    #[test]
    fn test_create_issue_request_serialization() {
        let req = CreateIssueRequest {
            project_id: "PRJ-1".into(),
            title: "New feature".into(),
            description: Some("Implement widget".into()),
            priority: Some(IssuePriority::Medium),
            assignee: Some("bob".into()),
            labels: vec!["feature".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateIssueRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.project_id, "PRJ-1");
        assert_eq!(back.title, "New feature");
    }

    #[test]
    fn test_issue_update_default() {
        let update = IssueUpdate::default();
        assert!(update.title.is_none());
        assert!(update.description.is_none());
        assert!(update.status.is_none());
        assert!(update.priority.is_none());
        assert!(update.assignee.is_none());
        assert!(update.labels.is_none());
    }

    #[test]
    fn test_comment_serialization() {
        let comment = Comment {
            id: "C-1".into(),
            author: Some("charlie".into()),
            body: "Looks good to me".into(),
            created_at: Some(Utc::now()),
        };
        let json = serde_json::to_string(&comment).unwrap();
        let back: Comment = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "C-1");
        assert_eq!(back.body, "Looks good to me");
    }

    #[test]
    fn test_sprint_serialization() {
        let sprint = Sprint {
            id: "S-1".into(),
            name: "Sprint 5".into(),
            state: Some("active".into()),
            start_date: Some(Utc::now()),
            end_date: None,
        };
        let json = serde_json::to_string(&sprint).unwrap();
        let back: Sprint = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "S-1");
        assert_eq!(back.name, "Sprint 5");
        assert_eq!(back.state.as_deref(), Some("active"));
    }

    #[test]
    fn test_hub_new_is_empty() {
        let hub = ProjectManagementHub::new();
        assert_eq!(hub.provider_count(), 0);
        assert!(hub.platforms().is_empty());
    }

    #[test]
    fn test_hub_default_is_empty() {
        let hub = ProjectManagementHub::default();
        assert_eq!(hub.provider_count(), 0);
    }

    #[test]
    fn test_pm_platform_hash_used_as_key() {
        let mut map = HashMap::new();
        map.insert(PMPlatform::Jira, "jira-token");
        map.insert(PMPlatform::Linear, "linear-token");
        assert_eq!(map.get(&PMPlatform::Jira), Some(&"jira-token"));
        assert_eq!(map.get(&PMPlatform::Linear), Some(&"linear-token"));
        assert_eq!(map.get(&PMPlatform::Asana), None);
    }
}
