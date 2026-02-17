use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://gitlab.com/api/v4";

// ── Types ──────────────────────────────────────────────────────────────

/// Visibility level of a GitLab project.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Internal,
    Public,
}

/// Filter for merge request state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MRState {
    Opened,
    Closed,
    Merged,
    All,
}

impl MRState {
    fn as_str(self) -> &'static str {
        match self {
            MRState::Opened => "opened",
            MRState::Closed => "closed",
            MRState::Merged => "merged",
            MRState::All => "all",
        }
    }
}

/// Filter for issue state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Opened,
    Closed,
    All,
}

impl IssueState {
    fn as_str(self) -> &'static str {
        match self {
            IssueState::Opened => "opened",
            IssueState::Closed => "closed",
            IssueState::All => "all",
        }
    }
}

/// A user reference embedded in various GitLab objects.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabUser {
    pub id: u64,
    pub username: String,
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
}

/// A GitLab project (repository).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabProject {
    pub id: u64,
    pub name: String,
    pub path_with_namespace: String,
    #[serde(default)]
    pub description: Option<String>,
    pub web_url: String,
    #[serde(default)]
    pub default_branch: Option<String>,
    pub visibility: Visibility,
}

/// A GitLab merge request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MergeRequest {
    pub id: u64,
    pub iid: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub state: String,
    pub source_branch: String,
    pub target_branch: String,
    pub author: GitLabUser,
    pub web_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A GitLab issue.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabIssue {
    pub id: u64,
    pub iid: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<GitLabUser>,
    pub web_url: String,
    pub created_at: DateTime<Utc>,
}

/// A CI/CD pipeline.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Pipeline {
    pub id: u64,
    pub status: String,
    /// The branch or tag name for the pipeline.
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
    pub web_url: String,
    pub created_at: DateTime<Utc>,
}

/// A repository branch.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Branch {
    pub name: String,
    pub commit: BranchCommit,
    #[serde(default)]
    pub protected: bool,
}

/// Commit info attached to a branch listing.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BranchCommit {
    pub id: String,
}

impl Branch {
    /// Convenience accessor for the commit SHA.
    pub fn commit_sha(&self) -> &str {
        &self.commit.id
    }
}

/// A result from the project-scoped code search endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchResult {
    pub filename: String,
    pub data: String,
    #[serde(default)]
    pub project_id: Option<u64>,
    #[serde(rename = "ref")]
    #[serde(default)]
    pub ref_name: Option<String>,
}

/// A note (comment) on a merge request, issue, or snippet.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Note {
    pub id: u64,
    pub body: String,
    pub author: GitLabUser,
    pub created_at: DateTime<Utc>,
}

/// Request body for creating a merge request.
#[derive(Debug, Clone, Serialize)]
pub struct CreateMergeRequestRequest {
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_source_branch: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub squash: Option<bool>,
}

/// Request body for creating an issue.
#[derive(Debug, Clone, Serialize)]
pub struct CreateIssueRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_ids: Option<Vec<u64>>,
}

// ── Client ─────────────────────────────────────────────────────────────

/// Client for interacting with the GitLab REST API v4.
///
/// Supports project, merge request, issue, pipeline, branch, and code
/// search operations. Uses Private-Token header authentication.
pub struct GitLabClient {
    base_url: String,
    private_token: String,
    client: Client,
}

impl GitLabClient {
    /// Create a new client with the given private token, using the default
    /// `https://gitlab.com/api/v4` base URL.
    pub fn new(private_token: impl Into<String>) -> Result<Self> {
        Self::with_base_url(private_token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom API base URL.
    ///
    /// Useful for self-hosted GitLab instances or testing against a mock
    /// server.
    pub fn with_base_url(
        private_token: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self> {
        let private_token = private_token.into();
        let base_url = base_url.into().trim_end_matches('/').to_string();

        let mut default_headers = HeaderMap::new();
        let token_value = HeaderValue::from_str(&private_token)
            .context("invalid characters in GitLab private token")?;
        default_headers.insert("PRIVATE-TOKEN", token_value);
        default_headers.insert(USER_AGENT, HeaderValue::from_static("Hive/1.0"));

        let client = Client::builder()
            .default_headers(default_headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            base_url,
            private_token,
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return a reference to the stored private token.
    pub fn private_token(&self) -> &str {
        &self.private_token
    }

    // ── Project operations ─────────────────────────────────────────

    /// List projects visible to the authenticated user.
    ///
    /// When `owned` is true, only projects owned by the current user are
    /// returned.
    pub async fn list_projects(&self, owned: bool) -> Result<Vec<GitLabProject>> {
        let url = format!("{}/projects?owned={owned}", self.base_url);
        debug!(url = %url, owned = owned, "listing GitLab projects");
        self.get_json(&url).await
    }

    /// Get a single project by numeric ID or URL-encoded path.
    pub async fn get_project(&self, id: &str) -> Result<GitLabProject> {
        let encoded = urlencoded(id);
        let url = format!("{}/projects/{encoded}", self.base_url);
        debug!(url = %url, "getting GitLab project");
        self.get_json(&url).await
    }

    // ── Merge request operations ───────────────────────────────────

    /// List merge requests for a project, filtered by state.
    pub async fn list_merge_requests(
        &self,
        project_id: &str,
        state: MRState,
    ) -> Result<Vec<MergeRequest>> {
        let encoded = urlencoded(project_id);
        let url = format!(
            "{}/projects/{encoded}/merge_requests?state={}",
            self.base_url,
            state.as_str()
        );
        debug!(url = %url, "listing merge requests");
        self.get_json(&url).await
    }

    /// Get a single merge request by its project-scoped IID.
    pub async fn get_merge_request(
        &self,
        project_id: &str,
        mr_iid: u64,
    ) -> Result<MergeRequest> {
        let encoded = urlencoded(project_id);
        let url = format!(
            "{}/projects/{encoded}/merge_requests/{mr_iid}",
            self.base_url
        );
        debug!(url = %url, "getting merge request");
        self.get_json(&url).await
    }

    /// Create a new merge request.
    pub async fn create_merge_request(
        &self,
        project_id: &str,
        request: &CreateMergeRequestRequest,
    ) -> Result<MergeRequest> {
        let encoded = urlencoded(project_id);
        let url = format!("{}/projects/{encoded}/merge_requests", self.base_url);
        debug!(
            url = %url,
            title = %request.title,
            source = %request.source_branch,
            target = %request.target_branch,
            "creating merge request"
        );
        self.post_json(&url, request).await
    }

    /// Add a comment (note) to a merge request.
    pub async fn add_mr_comment(
        &self,
        project_id: &str,
        mr_iid: u64,
        body: &str,
    ) -> Result<Note> {
        let encoded = urlencoded(project_id);
        let url = format!(
            "{}/projects/{encoded}/merge_requests/{mr_iid}/notes",
            self.base_url
        );
        let payload = serde_json::json!({ "body": body });
        debug!(url = %url, "adding merge request comment");
        self.post_json(&url, &payload).await
    }

    // ── Issue operations ───────────────────────────────────────────

    /// List issues for a project, filtered by state.
    pub async fn list_issues(
        &self,
        project_id: &str,
        state: IssueState,
    ) -> Result<Vec<GitLabIssue>> {
        let encoded = urlencoded(project_id);
        let url = format!(
            "{}/projects/{encoded}/issues?state={}",
            self.base_url,
            state.as_str()
        );
        debug!(url = %url, "listing issues");
        self.get_json(&url).await
    }

    /// Create a new issue in a project.
    pub async fn create_issue(
        &self,
        project_id: &str,
        request: &CreateIssueRequest,
    ) -> Result<GitLabIssue> {
        let encoded = urlencoded(project_id);
        let url = format!("{}/projects/{encoded}/issues", self.base_url);
        debug!(url = %url, title = %request.title, "creating issue");
        self.post_json(&url, request).await
    }

    // ── Pipeline operations ────────────────────────────────────────

    /// List recent pipelines for a project.
    pub async fn list_pipelines(&self, project_id: &str) -> Result<Vec<Pipeline>> {
        let encoded = urlencoded(project_id);
        let url = format!("{}/projects/{encoded}/pipelines", self.base_url);
        debug!(url = %url, "listing pipelines");
        self.get_json(&url).await
    }

    /// Get a single pipeline by ID.
    pub async fn get_pipeline(&self, project_id: &str, pipeline_id: u64) -> Result<Pipeline> {
        let encoded = urlencoded(project_id);
        let url = format!(
            "{}/projects/{encoded}/pipelines/{pipeline_id}",
            self.base_url
        );
        debug!(url = %url, "getting pipeline");
        self.get_json(&url).await
    }

    // ── Branch operations ──────────────────────────────────────────

    /// List branches for a project.
    pub async fn list_branches(&self, project_id: &str) -> Result<Vec<Branch>> {
        let encoded = urlencoded(project_id);
        let url = format!(
            "{}/projects/{encoded}/repository/branches",
            self.base_url
        );
        debug!(url = %url, "listing branches");
        self.get_json(&url).await
    }

    // ── File / code operations ─────────────────────────────────────

    /// Get raw file content from a repository.
    ///
    /// `file_path` is the URL-encoded path to the file within the repo.
    /// `ref_name` is the branch, tag, or commit SHA.
    pub async fn get_file_content(
        &self,
        project_id: &str,
        file_path: &str,
        ref_name: &str,
    ) -> Result<String> {
        let encoded_project = urlencoded(project_id);
        let encoded_path = urlencoded(file_path);
        let url = format!(
            "{}/projects/{encoded_project}/repository/files/{encoded_path}/raw?ref={ref_name}",
            self.base_url,
        );
        debug!(url = %url, "getting file content");
        self.get_text(&url).await
    }

    /// Search code within a project.
    pub async fn search_code(
        &self,
        project_id: &str,
        query: &str,
    ) -> Result<Vec<SearchResult>> {
        let encoded = urlencoded(project_id);
        let encoded_query = urlencoded(query);
        let url = format!(
            "{}/projects/{encoded}/search?scope=blobs&search={encoded_query}",
            self.base_url,
        );
        debug!(url = %url, query = %query, "searching code");
        self.get_json(&url).await
    }

    // ── Internal helpers ───────────────────────────────────────────

    /// Perform a GET request and deserialize the JSON response.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("GitLab GET request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            anyhow::bail!("GitLab API error ({}): {}", status, body);
        }

        response
            .json::<T>()
            .await
            .context("failed to parse GitLab response JSON")
    }

    /// Perform a GET request and return the response as plain text.
    async fn get_text(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("GitLab GET request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            anyhow::bail!("GitLab API error ({}): {}", status, body);
        }

        response
            .text()
            .await
            .context("failed to read GitLab response body")
    }

    /// Perform a POST request with a JSON body and deserialize the response.
    async fn post_json<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        payload: &B,
    ) -> Result<T> {
        let response = self
            .client
            .post(url)
            .json(payload)
            .send()
            .await
            .context("GitLab POST request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            anyhow::bail!("GitLab API error ({}): {}", status, body);
        }

        response
            .json::<T>()
            .await
            .context("failed to parse GitLab response JSON")
    }
}

/// Percent-encode a string for use in URL path segments.
///
/// GitLab requires project paths like `group/project` to be URL-encoded
/// as `group%2Fproject` when used as the `:id` parameter.
fn urlencoded(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client() -> GitLabClient {
        GitLabClient::with_base_url("glpat-test-token-abc", "https://gitlab.com/api/v4").unwrap()
    }

    #[test]
    fn test_new_sets_default_base_url() {
        let client = GitLabClient::new("tok").unwrap();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_custom_base_url_strips_trailing_slash() {
        let client =
            GitLabClient::with_base_url("tok", "https://gitlab.example.com/api/v4/").unwrap();
        assert_eq!(client.base_url(), "https://gitlab.example.com/api/v4");
    }

    #[test]
    fn test_private_token_stored_correctly() {
        let client = make_client();
        assert_eq!(client.private_token(), "glpat-test-token-abc");
    }

    #[test]
    fn test_invalid_token_characters_rejected() {
        let result = GitLabClient::new("tok\nen");
        assert!(result.is_err());
    }

    #[test]
    fn test_urlencoded_slashes() {
        assert_eq!(urlencoded("group/project"), "group%2Fproject");
    }

    #[test]
    fn test_urlencoded_plain() {
        assert_eq!(urlencoded("my-project"), "my-project");
    }

    #[test]
    fn test_mr_state_as_str() {
        assert_eq!(MRState::Opened.as_str(), "opened");
        assert_eq!(MRState::Closed.as_str(), "closed");
        assert_eq!(MRState::Merged.as_str(), "merged");
        assert_eq!(MRState::All.as_str(), "all");
    }

    #[test]
    fn test_issue_state_as_str() {
        assert_eq!(IssueState::Opened.as_str(), "opened");
        assert_eq!(IssueState::Closed.as_str(), "closed");
        assert_eq!(IssueState::All.as_str(), "all");
    }

    #[test]
    fn test_deserialize_project() {
        let json = serde_json::json!({
            "id": 42,
            "name": "My Project",
            "path_with_namespace": "group/my-project",
            "description": "A test project",
            "web_url": "https://gitlab.com/group/my-project",
            "default_branch": "main",
            "visibility": "private"
        });
        let project: GitLabProject = serde_json::from_value(json).unwrap();
        assert_eq!(project.id, 42);
        assert_eq!(project.name, "My Project");
        assert_eq!(project.path_with_namespace, "group/my-project");
        assert_eq!(project.description, Some("A test project".to_string()));
        assert_eq!(project.default_branch, Some("main".to_string()));
        assert_eq!(project.visibility, Visibility::Private);
    }

    #[test]
    fn test_deserialize_project_null_description() {
        let json = serde_json::json!({
            "id": 1,
            "name": "P",
            "path_with_namespace": "g/p",
            "description": null,
            "web_url": "https://gitlab.com/g/p",
            "default_branch": null,
            "visibility": "public"
        });
        let project: GitLabProject = serde_json::from_value(json).unwrap();
        assert!(project.description.is_none());
        assert!(project.default_branch.is_none());
        assert_eq!(project.visibility, Visibility::Public);
    }

    #[test]
    fn test_deserialize_merge_request() {
        let json = serde_json::json!({
            "id": 100,
            "iid": 5,
            "title": "Add feature X",
            "description": "Implements X",
            "state": "opened",
            "source_branch": "feature-x",
            "target_branch": "main",
            "author": {
                "id": 1,
                "username": "alice",
                "name": "Alice",
                "avatar_url": null,
                "web_url": "https://gitlab.com/alice"
            },
            "web_url": "https://gitlab.com/g/p/-/merge_requests/5",
            "created_at": "2024-06-01T12:00:00Z",
            "updated_at": "2024-06-02T15:30:00Z"
        });
        let mr: MergeRequest = serde_json::from_value(json).unwrap();
        assert_eq!(mr.iid, 5);
        assert_eq!(mr.state, "opened");
        assert_eq!(mr.source_branch, "feature-x");
        assert_eq!(mr.author.username, "alice");
    }

    #[test]
    fn test_deserialize_issue() {
        let json = serde_json::json!({
            "id": 200,
            "iid": 10,
            "title": "Bug: crash on startup",
            "description": null,
            "state": "opened",
            "labels": ["bug", "critical"],
            "assignees": [],
            "web_url": "https://gitlab.com/g/p/-/issues/10",
            "created_at": "2024-07-01T09:00:00Z"
        });
        let issue: GitLabIssue = serde_json::from_value(json).unwrap();
        assert_eq!(issue.iid, 10);
        assert_eq!(issue.labels, vec!["bug", "critical"]);
        assert!(issue.assignees.is_empty());
    }

    #[test]
    fn test_deserialize_pipeline() {
        let json = serde_json::json!({
            "id": 500,
            "status": "success",
            "ref": "main",
            "sha": "abc123def456",
            "web_url": "https://gitlab.com/g/p/-/pipelines/500",
            "created_at": "2024-08-01T08:00:00Z"
        });
        let pipeline: Pipeline = serde_json::from_value(json).unwrap();
        assert_eq!(pipeline.id, 500);
        assert_eq!(pipeline.status, "success");
        assert_eq!(pipeline.ref_name, "main");
    }

    #[test]
    fn test_deserialize_branch() {
        let json = serde_json::json!({
            "name": "feature-y",
            "commit": { "id": "deadbeef1234" },
            "protected": true
        });
        let branch: Branch = serde_json::from_value(json).unwrap();
        assert_eq!(branch.name, "feature-y");
        assert_eq!(branch.commit_sha(), "deadbeef1234");
        assert!(branch.protected);
    }

    #[test]
    fn test_deserialize_search_result() {
        let json = serde_json::json!({
            "filename": "src/main.rs",
            "data": "fn main() {}",
            "project_id": 42,
            "ref": "main"
        });
        let result: SearchResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.filename, "src/main.rs");
        assert_eq!(result.data, "fn main() {}");
        assert_eq!(result.project_id, Some(42));
        assert_eq!(result.ref_name, Some("main".to_string()));
    }

    #[test]
    fn test_deserialize_note() {
        let json = serde_json::json!({
            "id": 300,
            "body": "Looks good to me!",
            "author": {
                "id": 1,
                "username": "bob",
                "name": "Bob"
            },
            "created_at": "2024-09-01T10:00:00Z"
        });
        let note: Note = serde_json::from_value(json).unwrap();
        assert_eq!(note.id, 300);
        assert_eq!(note.body, "Looks good to me!");
        assert_eq!(note.author.username, "bob");
    }

    #[test]
    fn test_serialize_create_merge_request() {
        let req = CreateMergeRequestRequest {
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: "Add feature".to_string(),
            description: Some("Detailed description".to_string()),
            remove_source_branch: None,
            squash: Some(true),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["source_branch"], "feature");
        assert_eq!(json["title"], "Add feature");
        assert_eq!(json["squash"], true);
        // `remove_source_branch` should be absent (skip_serializing_if)
        assert!(json.get("remove_source_branch").is_none());
    }

    #[test]
    fn test_serialize_create_issue_request() {
        let req = CreateIssueRequest {
            title: "New issue".to_string(),
            description: None,
            labels: Some("bug,urgent".to_string()),
            assignee_ids: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["title"], "New issue");
        assert_eq!(json["labels"], "bug,urgent");
        assert!(json.get("description").is_none());
        assert!(json.get("assignee_ids").is_none());
    }
}
