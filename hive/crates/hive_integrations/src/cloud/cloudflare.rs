use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde_json::Value;
use tracing::debug;

const BASE_URL: &str = "https://api.cloudflare.com/client/v4";

/// Client for the Cloudflare REST API.
///
/// Provides access to zones, DNS records, and cache management.
pub struct CloudflareClient {
    token: String,
    base_url: String,
    client: Client,
}

impl CloudflareClient {
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
            .context("invalid characters in Cloudflare token")?;
        headers.insert(AUTHORIZATION, auth);
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));

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

    /// List all zones in the account.
    pub async fn list_zones(&self) -> Result<Value> {
        let url = format!("{}/zones", self.base_url);
        debug!(url = %url, "listing Cloudflare zones");
        self.get(&url).await
    }

    /// Get a single zone by ID.
    pub async fn get_zone(&self, id: &str) -> Result<Value> {
        let url = format!("{}/zones/{id}", self.base_url);
        debug!(url = %url, "getting Cloudflare zone");
        self.get(&url).await
    }

    /// List DNS records for a zone.
    pub async fn list_dns_records(&self, zone_id: &str) -> Result<Value> {
        let url = format!("{}/zones/{zone_id}/dns_records", self.base_url);
        debug!(url = %url, "listing DNS records");
        self.get(&url).await
    }

    /// Purge all cached content for a zone.
    pub async fn purge_cache(&self, zone_id: &str) -> Result<Value> {
        let url = format!("{}/zones/{zone_id}/purge_cache", self.base_url);
        let payload = serde_json::json!({ "purge_everything": true });
        debug!(url = %url, "purging Cloudflare cache");

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Cloudflare purge cache request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse Cloudflare purge response")?;

        if !status.is_success() {
            anyhow::bail!("Cloudflare purge error ({}): {}", status, body);
        }

        Ok(body)
    }

    async fn get(&self, url: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Cloudflare GET request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse Cloudflare response")?;

        if !status.is_success() {
            anyhow::bail!("Cloudflare API error ({}): {}", status, body);
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

    fn make_client() -> CloudflareClient {
        CloudflareClient::new("cf_test_token").unwrap()
    }

    #[test]
    fn test_default_base_url() {
        let client = make_client();
        assert_eq!(client.base_url(), BASE_URL);
    }

    #[test]
    fn test_custom_base_url() {
        let client = CloudflareClient::with_base_url("tok", "https://cf.test/v4/").unwrap();
        assert_eq!(client.base_url(), "https://cf.test/v4");
    }

    #[test]
    fn test_token_stored() {
        let client = make_client();
        assert_eq!(client.token(), "cf_test_token");
    }

    #[test]
    fn test_list_zones_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/zones");
        assert_eq!(url, "https://api.cloudflare.com/client/v4/zones");
    }

    #[test]
    fn test_get_zone_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/zones/zone123");
        assert!(url.ends_with("/zones/zone123"));
    }

    #[test]
    fn test_dns_records_url() {
        let client = make_client();
        let url = build_url(client.base_url(), "/zones/z1/dns_records");
        assert!(url.contains("/zones/z1/dns_records"));
    }

    #[test]
    fn test_purge_cache_payload() {
        let payload = serde_json::json!({ "purge_everything": true });
        assert_eq!(payload["purge_everything"], true);
    }
}
