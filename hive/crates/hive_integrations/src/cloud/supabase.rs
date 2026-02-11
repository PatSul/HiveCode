use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde_json::Value;
use tracing::debug;

/// Client for the Supabase REST API (PostgREST).
///
/// Provides basic table queries, inserts, and deletes against
/// a Supabase project's auto-generated REST endpoints.
pub struct SupabaseClient {
    url: String,
    anon_key: String,
    client: Client,
}

impl SupabaseClient {
    /// Create a new client with the project URL and anonymous key.
    ///
    /// The `url` should be the Supabase project URL, e.g. `https://abc.supabase.co`.
    pub fn new(url: impl Into<String>, anon_key: impl Into<String>) -> Result<Self> {
        let url = url.into().trim_end_matches('/').to_string();
        let anon_key = anon_key.into();

        let mut headers = HeaderMap::new();
        let key_value = HeaderValue::from_str(&anon_key)
            .context("invalid characters in Supabase anon key")?;
        headers.insert("apikey", key_value.clone());
        headers.insert("Authorization", HeaderValue::from_str(&format!("Bearer {anon_key}"))
            .context("invalid characters in Supabase auth header")?);
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("Prefer", HeaderValue::from_static("return=representation"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            url,
            anon_key,
            client,
        })
    }

    /// Return the configured project URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Return a reference to the stored anon key.
    pub fn anon_key(&self) -> &str {
        &self.anon_key
    }

    /// Check if the Supabase instance is reachable.
    pub async fn health_check(&self) -> Result<Value> {
        let url = format!("{}/rest/v1/", self.url);
        debug!(url = %url, "Supabase health check");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Supabase health check request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .unwrap_or(Value::Null);

        if !status.is_success() {
            anyhow::bail!("Supabase health check failed ({}): {}", status, body);
        }

        Ok(serde_json::json!({ "status": "ok", "code": status.as_u16() }))
    }

    /// Query a table with optional PostgREST query parameters.
    ///
    /// `params` is appended directly to the URL, e.g. `"select=id,name&limit=10"`.
    pub async fn query_table(&self, table: &str, params: &str) -> Result<Value> {
        let url = if params.is_empty() {
            format!("{}/rest/v1/{table}", self.url)
        } else {
            format!("{}/rest/v1/{table}?{params}", self.url)
        };
        debug!(url = %url, "querying Supabase table");
        self.get(&url).await
    }

    /// Insert a row into a table.
    pub async fn insert_row(&self, table: &str, data: &Value) -> Result<Value> {
        let url = format!("{}/rest/v1/{table}", self.url);
        debug!(url = %url, "inserting into Supabase table");

        let response = self
            .client
            .post(&url)
            .json(data)
            .send()
            .await
            .context("Supabase insert request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse Supabase insert response")?;

        if !status.is_success() {
            anyhow::bail!("Supabase insert error ({}): {}", status, body);
        }

        Ok(body)
    }

    /// Delete a row from a table by its `id` column.
    pub async fn delete_row(&self, table: &str, id: &str) -> Result<Value> {
        let url = format!("{}/rest/v1/{table}?id=eq.{id}", self.url);
        debug!(url = %url, "deleting from Supabase table");

        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .context("Supabase delete request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .unwrap_or(Value::Null);

        if !status.is_success() {
            anyhow::bail!("Supabase delete error ({}): {}", status, body);
        }

        Ok(body)
    }

    async fn get(&self, url: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Supabase GET request failed")?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse Supabase response")?;

        if !status.is_success() {
            anyhow::bail!("Supabase API error ({}): {}", status, body);
        }

        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full REST URL for a table.
    fn rest_url(base: &str, table: &str) -> String {
        format!("{base}/rest/v1/{table}")
    }

    fn make_client() -> SupabaseClient {
        SupabaseClient::new("https://abc.supabase.co", "eyJ0eXAi.test.key").unwrap()
    }

    #[test]
    fn test_url_stored_without_trailing_slash() {
        let client = SupabaseClient::new("https://abc.supabase.co/", "key").unwrap();
        assert_eq!(client.url(), "https://abc.supabase.co");
    }

    #[test]
    fn test_anon_key_stored() {
        let client = make_client();
        assert_eq!(client.anon_key(), "eyJ0eXAi.test.key");
    }

    #[test]
    fn test_rest_url_for_table() {
        let client = make_client();
        let url = rest_url(client.url(), "users");
        assert_eq!(url, "https://abc.supabase.co/rest/v1/users");
    }

    #[test]
    fn test_query_table_url_with_params() {
        let base = "https://abc.supabase.co";
        let url = format!("{base}/rest/v1/posts?select=id,title&limit=5");
        assert!(url.contains("select=id,title"));
        assert!(url.contains("limit=5"));
    }

    #[test]
    fn test_delete_row_url_format() {
        let base = "https://abc.supabase.co";
        let url = format!("{base}/rest/v1/items?id=eq.42");
        assert!(url.contains("id=eq.42"));
    }

    #[test]
    fn test_insert_row_payload() {
        let data = serde_json::json!({ "name": "Widget", "price": 9.99 });
        assert_eq!(data["name"], "Widget");
        assert_eq!(data["price"], 9.99);
    }
}
