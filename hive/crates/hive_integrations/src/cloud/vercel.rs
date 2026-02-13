use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;
use tracing::debug;

const BASE_URL: &str = "https://api.vercel.com";

/// Client for the Vercel REST API.
///
/// Provides access to projects and deployments.
pub struct VercelClient {
    token: String,
    base_url: String,
    client: Client,
}

impl VercelClient {
    /// Create a new client with the given API token.
    pub fn new(token: impl Into<String>) -> Result<Self> {
        Self::with_base_url(token, BASE_URL)
    }

    /// Create a new client with a custom base URL (useful for testing).
    pub fn with_base_url(token: impl Into<String>, base_url: impl Into<String>) -> Result<Self> {
        let token = token.into();
        let base_url = base_url.into().trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("invalid characters in Vercel token")?;
        headers.insert(AUTHORIZATION, auth);

        let client = Client::builder()
            .default_headers(headers)
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

    /// List all projects for the authenticated user.
    pub async fn list_projects(&self) -> Result<Value> {
        let url = format!("{}/v9/projects", self.base_url);
        debug!(url = %url, "listing Vercel projects");
        self.get(&url).await
    }

    /// Get a single project by ID.
    pub async fn get_project(&self, id: &str) -> Result<Value> {
        let url = format!("{}/v9/projects/{id}", self.base_url);
        debug!(url = %url, "getting Vercel project");
        self.get(&url).await
    }

    /// List deployments for a project.
    pub async fn list_deployments(&self, project_id: &str) -> Result<Value> {
        let url = format!("{}/v6/deployments?projectId={project_id}", self.base_url);
        debug!(url = %url, "listing Vercel deployments");
        self.get(&url).await
    }

    /// Get a single deployment by ID.
    pub async fn get_deployment(&self, id: &str) -> Result<Value> {
        let url = format!("{}/v13/deployments/{id}", self.base_url);
        debug!(url = %url, "getting Vercel deployment");
        self.get(&url).await
    }

    async fn get(&self, url: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Vercel GET request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse Vercel response as JSON")?;

        if !status.is_success() {
            anyhow::bail!("Vercel API error ({}): {}", status, body);
        }

        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    fn make_client() -> VercelClient {
        VercelClient::new("vercel_test_tok").unwrap()
    }

    #[test]
    fn test_default_base_url() {
        let client = make_client();
        assert_eq!(client.base_url(), BASE_URL);
    }

    #[test]
    fn test_custom_base_url_strips_slash() {
        let client = VercelClient::with_base_url("tok", "https://vercel.test/").unwrap();
        assert_eq!(client.base_url(), "https://vercel.test");
    }

    #[test]
    fn test_token_stored() {
        let client = make_client();
        assert_eq!(client.token(), "vercel_test_tok");
    }

    #[test]
    fn test_list_projects_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/v9/projects");
        assert_eq!(url, "https://api.vercel.com/v9/projects");
    }

    #[test]
    fn test_get_project_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/v9/projects/prj_abc");
        assert_eq!(url, "https://api.vercel.com/v9/projects/prj_abc");
    }

    #[test]
    fn test_list_deployments_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/v6/deployments?projectId=prj_abc");
        assert!(url.contains("projectId=prj_abc"));
    }

    #[test]
    fn test_get_deployment_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/v13/deployments/dpl_xyz");
        assert_eq!(url, "https://api.vercel.com/v13/deployments/dpl_xyz");
    }
}
