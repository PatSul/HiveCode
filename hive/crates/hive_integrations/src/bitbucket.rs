use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use tracing::debug;

const BASE_URL: &str = "https://api.bitbucket.org/2.0";

// ── Types ──────────────────────────────────────────────────────────────

/// Filter for pull request state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PRState {
    Open,
    Merged,
    Declined,
    Superseded,
}

impl PRState {
    fn as_str(self) -> &'static str {
        match self {
            PRState::Open => "OPEN",
            PRState::Merged => "MERGED",
            PRState::Declined => "DECLINED",
            PRState::Superseded => "SUPERSEDED",
        }
    }
}

/// A Bitbucket user reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BitbucketUser {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub nickname: Option<String>,
}

/// Bitbucket paginated response wrapper.
///
/// Most list endpoints return results inside a `values` array with
/// pagination metadata.
#[derive(Debug, Clone, Deserialize)]
struct Paginated<T> {
    values: Vec<T>,
    #[serde(default)]
    #[allow(dead_code)]
    next: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    size: Option<u64>,
}

/// A Bitbucket Cloud repository.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BitbucketRepo {
    #[serde(default)]
    pub uuid: Option<String>,
    pub name: String,
    pub slug: String,
    pub full_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub language: Option<String>,
}

/// Branch reference as used inside pull request source/destination.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PRBranchRef {
    pub repository: Option<PRBranchRepo>,
    pub branch: PRBranch,
}

/// Minimal repo info inside a PR branch reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PRBranchRepo {
    pub full_name: String,
}

/// Branch name inside a PR branch reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PRBranch {
    pub name: String,
}

/// Links object attached to a pull request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PRLinks {
    #[serde(rename = "self")]
    #[serde(default)]
    pub self_link: Option<Link>,
    #[serde(default)]
    pub html: Option<Link>,
}

/// A single link entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Link {
    pub href: String,
}

/// A Bitbucket pull request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullRequest {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub state: String,
    pub source: PRBranchRef,
    pub destination: PRBranchRef,
    #[serde(default)]
    pub author: Option<BitbucketUser>,
    #[serde(default)]
    pub links: Option<PRLinks>,
    #[serde(default)]
    pub created_on: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_on: Option<DateTime<Utc>>,
}

impl PullRequest {
    /// Convenience accessor for the source branch name.
    pub fn source_branch(&self) -> &str {
        &self.source.branch.name
    }

    /// Convenience accessor for the destination branch name.
    pub fn destination_branch(&self) -> &str {
        &self.destination.branch.name
    }
}

/// Bitbucket pipeline state object.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PipelineState {
    pub name: String,
}

/// Bitbucket pipeline target.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PipelineTarget {
    #[serde(rename = "ref_name")]
    #[serde(default)]
    pub ref_name: Option<String>,
}

/// A Bitbucket Pipelines build.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BitbucketPipeline {
    #[serde(default)]
    pub uuid: Option<String>,
    pub state: PipelineState,
    #[serde(default)]
    pub target: Option<PipelineTarget>,
    #[serde(default)]
    pub created_on: Option<DateTime<Utc>>,
}

impl BitbucketPipeline {
    /// Convenience accessor for the target branch name.
    pub fn target_branch(&self) -> Option<&str> {
        self.target.as_ref().and_then(|t| t.ref_name.as_deref())
    }
}

/// A repository branch.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BitbucketBranch {
    pub name: String,
    #[serde(default)]
    pub target: Option<BranchTarget>,
}

/// Commit target on a branch.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BranchTarget {
    pub hash: String,
}

impl BitbucketBranch {
    /// Convenience accessor for the branch's head commit hash.
    pub fn target_hash(&self) -> Option<&str> {
        self.target.as_ref().map(|t| t.hash.as_str())
    }
}

/// Content wrapper used in PR comments.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommentContent {
    pub raw: String,
}

/// A comment on a pull request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PRComment {
    pub id: u64,
    pub content: CommentContent,
    #[serde(default)]
    pub user: Option<BitbucketUser>,
    #[serde(default)]
    pub created_on: Option<DateTime<Utc>>,
}

/// Request body for creating a pull request.
#[derive(Debug, Clone, Serialize)]
pub struct CreatePullRequestRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: CreatePRBranch,
    pub destination: CreatePRBranch,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_source_branch: Option<bool>,
}

/// Branch specification for creating a pull request.
#[derive(Debug, Clone, Serialize)]
pub struct CreatePRBranch {
    pub branch: CreatePRBranchName,
}

/// Branch name for PR creation.
#[derive(Debug, Clone, Serialize)]
pub struct CreatePRBranchName {
    pub name: String,
}

// ── Client ─────────────────────────────────────────────────────────────

/// Client for interacting with the Bitbucket Cloud REST API 2.0.
///
/// Uses HTTP Basic authentication with an app password. Supports
/// repository, pull request, pipeline, and branch operations.
pub struct BitbucketClient {
    username: String,
    app_password: String,
    base_url: String,
    client: Client,
}

impl BitbucketClient {
    /// Create a new client with the given username and app password,
    /// using the default `https://api.bitbucket.org/2.0` base URL.
    pub fn new(
        username: impl Into<String>,
        app_password: impl Into<String>,
    ) -> Result<Self> {
        Self::with_base_url(username, app_password, BASE_URL)
    }

    /// Create a new client pointing at a custom API base URL.
    ///
    /// Useful for Bitbucket Server or testing against a mock server.
    pub fn with_base_url(
        username: impl Into<String>,
        app_password: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self> {
        let username = username.into();
        let app_password = app_password.into();
        let base_url = base_url.into().trim_end_matches('/').to_string();

        let mut default_headers = HeaderMap::new();
        default_headers.insert(USER_AGENT, HeaderValue::from_static("Hive/1.0"));

        let client = Client::builder()
            .default_headers(default_headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            username,
            app_password,
            base_url,
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return a reference to the stored username.
    pub fn username(&self) -> &str {
        &self.username
    }

    // ── Repository operations ──────────────────────────────────────

    /// List repositories in a workspace.
    pub async fn list_repositories(
        &self,
        workspace: &str,
    ) -> Result<Vec<BitbucketRepo>> {
        let url = format!("{}/repositories/{workspace}", self.base_url);
        debug!(url = %url, workspace = %workspace, "listing Bitbucket repositories");
        let page: Paginated<BitbucketRepo> = self.get_json(&url).await?;
        Ok(page.values)
    }

    /// Get a single repository by workspace and slug.
    pub async fn get_repository(
        &self,
        workspace: &str,
        repo_slug: &str,
    ) -> Result<BitbucketRepo> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}",
            self.base_url
        );
        debug!(url = %url, "getting Bitbucket repository");
        self.get_json(&url).await
    }

    // ── Pull request operations ────────────────────────────────────

    /// List pull requests for a repository, filtered by state.
    pub async fn list_pull_requests(
        &self,
        workspace: &str,
        repo_slug: &str,
        state: PRState,
    ) -> Result<Vec<PullRequest>> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}/pullrequests?state={}",
            self.base_url,
            state.as_str()
        );
        debug!(url = %url, "listing pull requests");
        let page: Paginated<PullRequest> = self.get_json(&url).await?;
        Ok(page.values)
    }

    /// Get a single pull request by ID.
    pub async fn get_pull_request(
        &self,
        workspace: &str,
        repo_slug: &str,
        pr_id: u64,
    ) -> Result<PullRequest> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}",
            self.base_url
        );
        debug!(url = %url, "getting pull request");
        self.get_json(&url).await
    }

    /// Create a new pull request.
    pub async fn create_pull_request(
        &self,
        workspace: &str,
        repo_slug: &str,
        request: &CreatePullRequestRequest,
    ) -> Result<PullRequest> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}/pullrequests",
            self.base_url
        );
        debug!(
            url = %url,
            title = %request.title,
            source = %request.source.branch.name,
            destination = %request.destination.branch.name,
            "creating pull request"
        );
        self.post_json(&url, request).await
    }

    /// Add a comment to a pull request.
    pub async fn add_pr_comment(
        &self,
        workspace: &str,
        repo_slug: &str,
        pr_id: u64,
        body: &str,
    ) -> Result<PRComment> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments",
            self.base_url
        );
        let payload = serde_json::json!({
            "content": {
                "raw": body
            }
        });
        debug!(url = %url, "adding pull request comment");
        self.post_json(&url, &payload).await
    }

    // ── Pipeline operations ────────────────────────────────────────

    /// List recent pipelines for a repository.
    pub async fn list_pipelines(
        &self,
        workspace: &str,
        repo_slug: &str,
    ) -> Result<Vec<BitbucketPipeline>> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}/pipelines/",
            self.base_url
        );
        debug!(url = %url, "listing pipelines");
        let page: Paginated<BitbucketPipeline> = self.get_json(&url).await?;
        Ok(page.values)
    }

    // ── Branch operations ──────────────────────────────────────────

    /// List branches for a repository.
    pub async fn list_branches(
        &self,
        workspace: &str,
        repo_slug: &str,
    ) -> Result<Vec<BitbucketBranch>> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}/refs/branches",
            self.base_url
        );
        debug!(url = %url, "listing branches");
        let page: Paginated<BitbucketBranch> = self.get_json(&url).await?;
        Ok(page.values)
    }

    // ── File operations ────────────────────────────────────────────

    /// Get raw file content from a repository.
    ///
    /// `path` is the file path within the repo. `ref_name` is the branch,
    /// tag, or commit hash.
    pub async fn get_file_content(
        &self,
        workspace: &str,
        repo_slug: &str,
        path: &str,
        ref_name: &str,
    ) -> Result<String> {
        let url = format!(
            "{}/repositories/{workspace}/{repo_slug}/src/{ref_name}/{path}",
            self.base_url
        );
        debug!(url = %url, "getting file content");
        self.get_text(&url).await
    }

    // ── Internal helpers ───────────────────────────────────────────

    /// Perform a GET request with Basic auth and deserialize the JSON
    /// response.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self
            .client
            .get(url)
            .basic_auth(&self.username, Some(&self.app_password))
            .send()
            .await
            .context("Bitbucket GET request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            anyhow::bail!("Bitbucket API error ({}): {}", status, body);
        }

        response
            .json::<T>()
            .await
            .context("failed to parse Bitbucket response JSON")
    }

    /// Perform a GET request with Basic auth and return response as plain
    /// text.
    async fn get_text(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .basic_auth(&self.username, Some(&self.app_password))
            .send()
            .await
            .context("Bitbucket GET request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            anyhow::bail!("Bitbucket API error ({}): {}", status, body);
        }

        response
            .text()
            .await
            .context("failed to read Bitbucket response body")
    }

    /// Perform a POST request with Basic auth and a JSON body, then
    /// deserialize the response.
    async fn post_json<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        payload: &B,
    ) -> Result<T> {
        let response = self
            .client
            .post(url)
            .basic_auth(&self.username, Some(&self.app_password))
            .json(payload)
            .send()
            .await
            .context("Bitbucket POST request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            anyhow::bail!("Bitbucket API error ({}): {}", status, body);
        }

        response
            .json::<T>()
            .await
            .context("failed to parse Bitbucket response JSON")
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client() -> BitbucketClient {
        BitbucketClient::with_base_url(
            "testuser",
            "app-password-abc",
            "https://api.bitbucket.org/2.0",
        )
        .unwrap()
    }

    #[test]
    fn test_new_sets_default_base_url() {
        let client = BitbucketClient::new("user", "pass").unwrap();
        assert_eq!(client.base_url(), BASE_URL);
    }

    #[test]
    fn test_custom_base_url_strips_trailing_slash() {
        let client =
            BitbucketClient::with_base_url("u", "p", "https://bitbucket.example.com/2.0/")
                .unwrap();
        assert_eq!(client.base_url(), "https://bitbucket.example.com/2.0");
    }

    #[test]
    fn test_username_stored_correctly() {
        let client = make_client();
        assert_eq!(client.username(), "testuser");
    }

    #[test]
    fn test_pr_state_as_str() {
        assert_eq!(PRState::Open.as_str(), "OPEN");
        assert_eq!(PRState::Merged.as_str(), "MERGED");
        assert_eq!(PRState::Declined.as_str(), "DECLINED");
        assert_eq!(PRState::Superseded.as_str(), "SUPERSEDED");
    }

    #[test]
    fn test_deserialize_repo() {
        let json = serde_json::json!({
            "uuid": "{abc-123}",
            "name": "my-repo",
            "slug": "my-repo",
            "full_name": "workspace/my-repo",
            "description": "A test repository",
            "is_private": true,
            "language": "rust"
        });
        let repo: BitbucketRepo = serde_json::from_value(json).unwrap();
        assert_eq!(repo.name, "my-repo");
        assert_eq!(repo.slug, "my-repo");
        assert_eq!(repo.full_name, "workspace/my-repo");
        assert!(repo.is_private);
        assert_eq!(repo.language, Some("rust".to_string()));
    }

    #[test]
    fn test_deserialize_repo_minimal() {
        let json = serde_json::json!({
            "name": "bare",
            "slug": "bare",
            "full_name": "ws/bare"
        });
        let repo: BitbucketRepo = serde_json::from_value(json).unwrap();
        assert!(repo.uuid.is_none());
        assert!(repo.description.is_none());
        assert!(!repo.is_private);
        assert!(repo.language.is_none());
    }

    #[test]
    fn test_deserialize_pull_request() {
        let json = serde_json::json!({
            "id": 42,
            "title": "Add new feature",
            "description": "Implements something useful",
            "state": "OPEN",
            "source": {
                "repository": { "full_name": "ws/repo" },
                "branch": { "name": "feature-branch" }
            },
            "destination": {
                "repository": { "full_name": "ws/repo" },
                "branch": { "name": "main" }
            },
            "author": {
                "display_name": "Alice",
                "uuid": "{user-1}",
                "nickname": "alice"
            },
            "links": {
                "self": { "href": "https://api.bitbucket.org/2.0/repositories/ws/repo/pullrequests/42" },
                "html": { "href": "https://bitbucket.org/ws/repo/pull-requests/42" }
            },
            "created_on": "2024-06-01T12:00:00.000000+00:00",
            "updated_on": "2024-06-02T15:30:00.000000+00:00"
        });
        let pr: PullRequest = serde_json::from_value(json).unwrap();
        assert_eq!(pr.id, 42);
        assert_eq!(pr.title, "Add new feature");
        assert_eq!(pr.state, "OPEN");
        assert_eq!(pr.source_branch(), "feature-branch");
        assert_eq!(pr.destination_branch(), "main");
        assert_eq!(
            pr.author.as_ref().unwrap().display_name,
            Some("Alice".to_string())
        );
    }

    #[test]
    fn test_deserialize_pipeline() {
        let json = serde_json::json!({
            "uuid": "{pipe-1}",
            "state": { "name": "COMPLETED" },
            "target": { "ref_name": "main" },
            "created_on": "2024-08-01T08:00:00.000000+00:00"
        });
        let pipeline: BitbucketPipeline = serde_json::from_value(json).unwrap();
        assert_eq!(pipeline.state.name, "COMPLETED");
        assert_eq!(pipeline.target_branch(), Some("main"));
    }

    #[test]
    fn test_deserialize_branch() {
        let json = serde_json::json!({
            "name": "develop",
            "target": { "hash": "abc123def456" }
        });
        let branch: BitbucketBranch = serde_json::from_value(json).unwrap();
        assert_eq!(branch.name, "develop");
        assert_eq!(branch.target_hash(), Some("abc123def456"));
    }

    #[test]
    fn test_deserialize_branch_no_target() {
        let json = serde_json::json!({
            "name": "orphan"
        });
        let branch: BitbucketBranch = serde_json::from_value(json).unwrap();
        assert_eq!(branch.name, "orphan");
        assert!(branch.target_hash().is_none());
    }

    #[test]
    fn test_deserialize_pr_comment() {
        let json = serde_json::json!({
            "id": 100,
            "content": { "raw": "Ship it!" },
            "user": {
                "display_name": "Bob",
                "uuid": "{user-2}"
            },
            "created_on": "2024-09-01T10:00:00.000000+00:00"
        });
        let comment: PRComment = serde_json::from_value(json).unwrap();
        assert_eq!(comment.id, 100);
        assert_eq!(comment.content.raw, "Ship it!");
        assert_eq!(
            comment.user.as_ref().unwrap().display_name,
            Some("Bob".to_string())
        );
    }

    #[test]
    fn test_deserialize_paginated_repos() {
        let json = serde_json::json!({
            "values": [
                {
                    "name": "repo-a",
                    "slug": "repo-a",
                    "full_name": "ws/repo-a"
                },
                {
                    "name": "repo-b",
                    "slug": "repo-b",
                    "full_name": "ws/repo-b"
                }
            ],
            "next": "https://api.bitbucket.org/2.0/repositories/ws?page=2",
            "size": 25
        });
        let page: Paginated<BitbucketRepo> = serde_json::from_value(json).unwrap();
        assert_eq!(page.values.len(), 2);
        assert_eq!(page.values[0].slug, "repo-a");
        assert_eq!(page.values[1].slug, "repo-b");
        assert!(page.next.is_some());
        assert_eq!(page.size, Some(25));
    }

    #[test]
    fn test_serialize_create_pull_request() {
        let req = CreatePullRequestRequest {
            title: "My PR".to_string(),
            description: Some("Detailed description".to_string()),
            source: CreatePRBranch {
                branch: CreatePRBranchName {
                    name: "feature".to_string(),
                },
            },
            destination: CreatePRBranch {
                branch: CreatePRBranchName {
                    name: "main".to_string(),
                },
            },
            close_source_branch: Some(true),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["title"], "My PR");
        assert_eq!(json["source"]["branch"]["name"], "feature");
        assert_eq!(json["destination"]["branch"]["name"], "main");
        assert_eq!(json["close_source_branch"], true);
    }

    #[test]
    fn test_serialize_create_pull_request_minimal() {
        let req = CreatePullRequestRequest {
            title: "Quick fix".to_string(),
            description: None,
            source: CreatePRBranch {
                branch: CreatePRBranchName {
                    name: "fix".to_string(),
                },
            },
            destination: CreatePRBranch {
                branch: CreatePRBranchName {
                    name: "main".to_string(),
                },
            },
            close_source_branch: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["title"], "Quick fix");
        assert!(json.get("description").is_none());
        assert!(json.get("close_source_branch").is_none());
    }
}
