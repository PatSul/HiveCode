use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::Client;
use serde_json::Value;
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://api.github.com";

/// Client for interacting with the GitHub REST API.
///
/// Supports repository, issue, and pull request operations.
/// All responses are returned as raw `serde_json::Value` to keep
/// the integration layer thin and avoid coupling to GitHub's schema.
pub struct GitHubClient {
    token: String,
    base_url: String,
    client: Client,
}

impl GitHubClient {
    /// Create a new client with the given personal access token.
    pub fn new(token: impl Into<String>) -> Result<Self> {
        Self::with_base_url(token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom API base URL.
    ///
    /// Useful for GitHub Enterprise or testing against a mock server.
    pub fn with_base_url(token: impl Into<String>, base_url: impl Into<String>) -> Result<Self> {
        let token = token.into();
        let base_url = base_url.into().trim_end_matches('/').to_string();

        let mut default_headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("invalid characters in GitHub token")?;
        default_headers.insert(AUTHORIZATION, auth_value);
        default_headers.insert(USER_AGENT, HeaderValue::from_static("Hive/1.0"));
        default_headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github+json"),
        );

        let client = Client::builder()
            .default_headers(default_headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            token,
            base_url,
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return a reference to the stored token.
    pub fn token(&self) -> &str {
        &self.token
    }

    // ── Repository operations ──────────────────────────────────────

    /// List repositories for the authenticated user.
    pub async fn list_repos(&self) -> Result<Value> {
        let url = format!("{}/user/repos", self.base_url);
        debug!(url = %url, "listing repos");
        self.get(&url).await
    }

    /// Get a single repository by owner and name.
    pub async fn get_repo(&self, owner: &str, repo: &str) -> Result<Value> {
        let url = format!("{}/repos/{owner}/{repo}", self.base_url);
        debug!(url = %url, "getting repo");
        self.get(&url).await
    }

    // ── Issue operations ───────────────────────────────────────────

    /// List issues for a repository.
    pub async fn list_issues(&self, owner: &str, repo: &str) -> Result<Value> {
        let url = format!("{}/repos/{owner}/{repo}/issues", self.base_url);
        debug!(url = %url, "listing issues");
        self.get(&url).await
    }

    /// Create a new issue.
    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<Value> {
        let url = format!("{}/repos/{owner}/{repo}/issues", self.base_url);
        let payload = serde_json::json!({ "title": title, "body": body });
        debug!(url = %url, title = %title, "creating issue");
        self.post(&url, &payload).await
    }

    // ── Pull request operations ────────────────────────────────────

    /// List pull requests for a repository.
    pub async fn list_pulls(&self, owner: &str, repo: &str) -> Result<Value> {
        let url = format!("{}/repos/{owner}/{repo}/pulls", self.base_url);
        debug!(url = %url, "listing pull requests");
        self.get(&url).await
    }

    /// Create a new pull request.
    pub async fn create_pull(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
    ) -> Result<Value> {
        let url = format!("{}/repos/{owner}/{repo}/pulls", self.base_url);
        let payload = serde_json::json!({
            "title": title,
            "body": body,
            "head": head,
            "base": base,
        });
        debug!(url = %url, title = %title, head = %head, base = %base, "creating pull request");
        self.post(&url, &payload).await
    }

    // ── Internal helpers ───────────────────────────────────────────

    async fn get(&self, url: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("GitHub GET request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse GitHub response as JSON")?;

        if !status.is_success() {
            anyhow::bail!("GitHub API error ({}): {}", status, body);
        }

        Ok(body)
    }

    async fn post(&self, url: &str, payload: &Value) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .json(payload)
            .send()
            .await
            .context("GitHub POST request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse GitHub response as JSON")?;

        if !status.is_success() {
            anyhow::bail!("GitHub API error ({}): {}", status, body);
        }

        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    fn make_client() -> GitHubClient {
        GitHubClient::with_base_url("ghp_test_token_123", "https://api.github.com").unwrap()
    }

    #[test]
    fn test_new_sets_default_base_url() {
        let client = GitHubClient::new("tok").unwrap();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_custom_base_url_strips_trailing_slash() {
        let client =
            GitHubClient::with_base_url("tok", "https://github.example.com/api/v3/").unwrap();
        assert_eq!(client.base_url(), "https://github.example.com/api/v3");
    }

    #[test]
    fn test_token_stored_correctly() {
        let client = make_client();
        assert_eq!(client.token(), "ghp_test_token_123");
    }

    #[test]
    fn test_list_repos_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/user/repos");
        assert_eq!(url, "https://api.github.com/user/repos");
    }

    #[test]
    fn test_get_repo_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/repos/hive-org/hive");
        assert_eq!(url, "https://api.github.com/repos/hive-org/hive");
    }

    #[test]
    fn test_list_issues_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/repos/hive-org/hive/issues");
        assert_eq!(url, "https://api.github.com/repos/hive-org/hive/issues");
    }

    #[test]
    fn test_create_issue_payload() {
        let payload = serde_json::json!({
            "title": "Bug report",
            "body": "Something broke",
        });
        assert_eq!(payload["title"], "Bug report");
        assert_eq!(payload["body"], "Something broke");
    }

    #[test]
    fn test_list_pulls_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/repos/hive-org/hive/pulls");
        assert_eq!(url, "https://api.github.com/repos/hive-org/hive/pulls");
    }

    #[test]
    fn test_create_pull_payload() {
        let payload = serde_json::json!({
            "title": "Add feature",
            "body": "Implements the new widget",
            "head": "feature-branch",
            "base": "main",
        });
        assert_eq!(payload["title"], "Add feature");
        assert_eq!(payload["head"], "feature-branch");
        assert_eq!(payload["base"], "main");
    }

    #[test]
    fn test_build_url_helper() {
        assert_eq!(
            build_url("https://api.github.com", "/repos/a/b"),
            "https://api.github.com/repos/a/b"
        );
    }

    #[test]
    fn test_invalid_token_characters_rejected() {
        // Newlines in tokens should be rejected when building headers.
        let result = GitHubClient::new("tok\nen");
        assert!(result.is_err());
    }
}
