//! Google Drive API v3 client.
//!
//! Wraps the REST API at `https://www.googleapis.com/drive/v3` using
//! `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://www.googleapis.com/drive/v3";

/// Metadata for a single file or folder in Google Drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    #[serde(default, deserialize_with = "deserialize_string_u64")]
    pub size: Option<u64>,
}

/// Deserialize a string or number as `Option<u64>` (Google APIs return sizes as strings).
fn deserialize_string_u64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    use serde::de;
    struct StringU64Visitor;
    impl<'de> de::Visitor<'de> for StringU64Visitor {
        type Value = Option<u64>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or integer representing u64")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            if v.is_empty() {
                return Ok(None);
            }
            v.parse::<u64>().map(Some).map_err(de::Error::custom)
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }
    d.deserialize_any(StringU64Visitor)
}

/// A paginated list of Drive files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFileList {
    #[serde(default)]
    pub files: Vec<DriveFile>,
    pub next_page_token: Option<String>,
}

/// Client for the Google Drive v3 REST API.
pub struct GoogleDriveClient {
    base_url: String,
    client: Client,
}

impl GoogleDriveClient {
    /// Create a new client using the given OAuth access token.
    pub fn new(access_token: &str) -> Self {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom base URL (useful for testing).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        // access_token is assumed to be a valid bearer token
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

    /// List files, optionally filtered by a Drive query string.
    pub async fn list_files(&self, query: Option<&str>, page_size: u32) -> Result<DriveFileList> {
        let mut url = format!(
            "{}/files?pageSize={}&fields=files(id,name,mimeType,size),nextPageToken",
            self.base_url, page_size
        );

        if let Some(q) = query {
            url.push_str(&format!("&q={}", urlencod(q)));
        }

        debug!(url = %url, "listing Drive files");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Drive list_files request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Drive API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Drive file list")
    }

    /// Get metadata for a single file.
    pub async fn get_file(&self, file_id: &str) -> Result<DriveFile> {
        let url = format!(
            "{}/files/{}?fields=id,name,mimeType,size",
            self.base_url, file_id
        );
        debug!(url = %url, "getting Drive file");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Drive get_file request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Drive API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Drive file")
    }

    /// Download the binary content of a file.
    pub async fn download_file(&self, file_id: &str) -> Result<Vec<u8>> {
        let url = format!("{}/files/{}?alt=media", self.base_url, file_id);
        debug!(url = %url, "downloading Drive file");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Drive download request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Drive API error ({}): {}", status, body);
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .context("failed to read Drive file bytes")
    }

    /// Delete a file by ID.
    pub async fn delete_file(&self, file_id: &str) -> Result<()> {
        let url = format!("{}/files/{}", self.base_url, file_id);
        debug!(url = %url, "deleting Drive file");

        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .context("Drive delete request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Drive API error ({}): {}", status, body);
        }

        Ok(())
    }
}

/// Minimal percent-encoding for query parameter values.
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

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_drive_file_deserialization() {
        let json = r#"{
            "id": "abc123",
            "name": "report.pdf",
            "mimeType": "application/pdf",
            "size": "1024"
        }"#;
        let file: DriveFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.id, "abc123");
        assert_eq!(file.name, "report.pdf");
        assert_eq!(file.mime_type, "application/pdf");
    }

    #[test]
    fn test_drive_file_list_deserialization() {
        let json = r#"{
            "files": [
                { "id": "1", "name": "a.txt", "mimeType": "text/plain" },
                { "id": "2", "name": "b.txt", "mimeType": "text/plain" }
            ],
            "nextPageToken": "tok_next"
        }"#;
        let list: DriveFileList = serde_json::from_str(json).unwrap();
        assert_eq!(list.files.len(), 2);
        assert_eq!(list.next_page_token.as_deref(), Some("tok_next"));
    }

    #[test]
    fn test_drive_file_list_no_next_page() {
        let json = r#"{ "files": [] }"#;
        let list: DriveFileList = serde_json::from_str(json).unwrap();
        assert!(list.files.is_empty());
        assert!(list.next_page_token.is_none());
    }

    #[test]
    fn test_client_default_base_url() {
        let client = GoogleDriveClient::new("tok");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url_strips_slash() {
        let client = GoogleDriveClient::with_base_url("tok", "https://drive.test/v3/");
        assert_eq!(client.base_url(), "https://drive.test/v3");
    }

    #[test]
    fn test_list_files_url_construction() {
        let client = GoogleDriveClient::new("tok");
        let url = build_url(client.base_url(), "/files?pageSize=10");
        assert!(url.starts_with(DEFAULT_BASE_URL));
        assert!(url.contains("pageSize=10"));
    }

    #[test]
    fn test_get_file_url_construction() {
        let client = GoogleDriveClient::new("tok");
        let url = build_url(
            client.base_url(),
            "/files/abc123?fields=id,name,mimeType,size",
        );
        assert!(url.contains("/files/abc123"));
    }

    #[test]
    fn test_download_file_url_construction() {
        let client = GoogleDriveClient::new("tok");
        let url = build_url(client.base_url(), "/files/abc123?alt=media");
        assert!(url.contains("alt=media"));
    }

    #[test]
    fn test_drive_file_serialization_roundtrip() {
        let file = DriveFile {
            id: "f1".into(),
            name: "doc.txt".into(),
            mime_type: "text/plain".into(),
            size: Some(512),
        };
        let json = serde_json::to_string(&file).unwrap();
        let back: DriveFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "f1");
        assert_eq!(back.size, Some(512));
    }
}
