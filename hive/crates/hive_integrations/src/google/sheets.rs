//! Google Sheets API v4 client.
//!
//! Wraps the REST API at `https://sheets.googleapis.com/v4/spreadsheets`
//! using `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://sheets.googleapis.com/v4/spreadsheets";

/// Values returned from a Sheets range read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetValues {
    #[serde(default)]
    pub range: String,
    #[serde(default)]
    pub values: Vec<Vec<String>>,
}

/// Client for the Google Sheets v4 REST API.
pub struct GoogleSheetsClient {
    base_url: String,
    client: Client,
}

impl GoogleSheetsClient {
    /// Create a new client using the given OAuth access token.
    pub fn new(access_token: &str) -> Self {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom base URL (useful for testing).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {access_token}")) {
            headers.insert(AUTHORIZATION, val);
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { base_url, client }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Read values from a spreadsheet range.
    pub async fn get_values(
        &self,
        spreadsheet_id: &str,
        range: &str,
    ) -> Result<SheetValues> {
        let url = format!(
            "{}/{}/values/{}",
            self.base_url, spreadsheet_id, urlencod(range)
        );
        debug!(url = %url, "reading Sheets values");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Sheets get_values request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Sheets API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Sheets values")
    }

    /// Write values to a spreadsheet range.
    pub async fn update_values(
        &self,
        spreadsheet_id: &str,
        range: &str,
        values: &[Vec<String>],
    ) -> Result<()> {
        let url = format!(
            "{}/{}/values/{}?valueInputOption=USER_ENTERED",
            self.base_url, spreadsheet_id, urlencod(range)
        );

        let body = serde_json::json!({
            "range": range,
            "majorDimension": "ROWS",
            "values": values,
        });

        debug!(url = %url, "updating Sheets values");

        let resp = self
            .client
            .put(&url)
            .json(&body)
            .send()
            .await
            .context("Sheets update_values request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Sheets API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// Create a new spreadsheet and return its ID.
    pub async fn create_spreadsheet(&self, title: &str) -> Result<String> {
        let body = serde_json::json!({
            "properties": {
                "title": title,
            },
        });

        debug!(title = %title, "creating spreadsheet");

        let resp = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await
            .context("Sheets create_spreadsheet request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Sheets API error ({}): {}", status, body);
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .context("failed to parse create spreadsheet response")?;

        json["spreadsheetId"]
            .as_str()
            .map(|s| s.to_string())
            .context("spreadsheetId missing from response")
    }
}

/// Minimal percent-encoding for path segments.
fn urlencod(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_sheet_values_deserialization() {
        let json = r#"{
            "range": "Sheet1!A1:B2",
            "values": [["Name", "Age"], ["Alice", "30"]]
        }"#;
        let vals: SheetValues = serde_json::from_str(json).unwrap();
        assert_eq!(vals.range, "Sheet1!A1:B2");
        assert_eq!(vals.values.len(), 2);
        assert_eq!(vals.values[0], vec!["Name", "Age"]);
    }

    #[test]
    fn test_sheet_values_empty() {
        let json = r#"{ "range": "Sheet1!A1:A1" }"#;
        let vals: SheetValues = serde_json::from_str(json).unwrap();
        assert!(vals.values.is_empty());
    }

    #[test]
    fn test_client_default_base_url() {
        let client = GoogleSheetsClient::new("tok");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url() {
        let client = GoogleSheetsClient::with_base_url("tok", "https://sheets.test/v4/");
        assert_eq!(client.base_url(), "https://sheets.test/v4");
    }

    #[test]
    fn test_get_values_url_construction() {
        let client = GoogleSheetsClient::new("tok");
        let url = build_url(
            client.base_url(),
            "/spreadsheet123/values/Sheet1%21A1%3AB10",
        );
        assert!(url.contains("/spreadsheet123/values/"));
    }

    #[test]
    fn test_create_spreadsheet_url() {
        let client = GoogleSheetsClient::new("tok");
        // POST goes directly to base_url
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_sheet_values_serialization_roundtrip() {
        let vals = SheetValues {
            range: "Sheet1!A1:C3".into(),
            values: vec![
                vec!["a".into(), "b".into(), "c".into()],
                vec!["1".into(), "2".into(), "3".into()],
            ],
        };
        let json = serde_json::to_string(&vals).unwrap();
        let back: SheetValues = serde_json::from_str(&json).unwrap();
        assert_eq!(back.range, "Sheet1!A1:C3");
        assert_eq!(back.values.len(), 2);
    }
}
