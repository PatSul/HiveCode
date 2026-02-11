//! Google Docs API v1 client.
//!
//! Wraps the REST API at `https://docs.googleapis.com/v1` using
//! `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://docs.googleapis.com/v1";
const DRIVE_EXPORT_BASE_URL: &str = "https://www.googleapis.com/drive/v3";

/// A Google Docs document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub document_id: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<DocumentBody>,
    #[serde(default)]
    pub revision_id: Option<String>,
}

/// The body of a document, containing structural elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentBody {
    #[serde(default)]
    pub content: Vec<StructuralElement>,
}

/// A structural element within a document body (e.g. a paragraph).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralElement {
    #[serde(default)]
    pub start_index: Option<i64>,
    #[serde(default)]
    pub end_index: Option<i64>,
    #[serde(default)]
    pub paragraph: Option<Paragraph>,
}

/// A paragraph within a structural element.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    #[serde(default)]
    pub elements: Vec<ParagraphElement>,
}

/// An element within a paragraph (e.g. a text run).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphElement {
    #[serde(default)]
    pub start_index: Option<i64>,
    #[serde(default)]
    pub end_index: Option<i64>,
    #[serde(default)]
    pub text_run: Option<TextRun>,
}

/// A run of text with uniform formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub content: String,
}

/// Request payload for creating a new document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDocumentRequest {
    pub title: String,
}

/// Parameters for inserting text into a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertTextRequest {
    pub text: String,
    pub location_index: i64,
}

/// Client for the Google Docs v1 REST API.
pub struct GoogleDocsClient {
    base_url: String,
    drive_export_url: String,
    client: Client,
}

impl GoogleDocsClient {
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

        Self {
            base_url,
            drive_export_url: DRIVE_EXPORT_BASE_URL.to_string(),
            client,
        }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Override the Drive export base URL (useful for testing export methods).
    pub fn set_drive_export_url(&mut self, url: &str) {
        self.drive_export_url = url.trim_end_matches('/').to_string();
    }

    /// Get a document by its ID, including full body content.
    pub async fn get_document(&self, document_id: &str) -> Result<Document> {
        let url = format!("{}/documents/{}", self.base_url, document_id);
        debug!(url = %url, "getting Docs document");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Docs get_document request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Docs API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse Docs document")
    }

    /// Create a new empty document with the given title.
    pub async fn create_document(&self, title: &str) -> Result<Document> {
        let url = format!("{}/documents", self.base_url);
        let request = CreateDocumentRequest {
            title: title.to_string(),
        };

        debug!(title = %title, "creating Docs document");

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Docs create_document request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Docs API error ({}): {}", status, body);
        }

        resp.json()
            .await
            .context("failed to parse created Docs document")
    }

    /// Read all text content from a document, concatenating all text runs.
    pub async fn read_text(&self, document_id: &str) -> Result<String> {
        let doc = self
            .get_document(document_id)
            .await
            .context("failed to fetch document for text extraction")?;

        Ok(extract_text(&doc))
    }

    /// Insert text at a specific index in the document.
    pub async fn insert_text(
        &self,
        document_id: &str,
        text: &str,
        index: i64,
    ) -> Result<()> {
        let url = format!(
            "{}/documents/{}:batchUpdate",
            self.base_url, document_id
        );

        let body = serde_json::json!({
            "requests": [
                {
                    "insertText": {
                        "text": text,
                        "location": {
                            "index": index
                        }
                    }
                }
            ]
        });

        debug!(document_id = %document_id, index = index, "inserting text into Docs document");

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Docs insert_text request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Docs API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// Export a document as PDF using the Drive API export endpoint.
    pub async fn export_pdf(&self, document_id: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/files/{}?alt=media&mimeType=application%2Fpdf",
            self.drive_export_url, document_id
        );
        debug!(url = %url, "exporting Docs document as PDF");

        self.export_bytes(&url, "PDF").await
    }

    /// Export a document as DOCX using the Drive API export endpoint.
    pub async fn export_docx(&self, document_id: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/files/{}?alt=media&mimeType=application%2Fvnd.openxmlformats-officedocument.wordprocessingml.document",
            self.drive_export_url, document_id
        );
        debug!(url = %url, "exporting Docs document as DOCX");

        self.export_bytes(&url, "DOCX").await
    }

    /// Shared helper to download exported bytes from Drive.
    async fn export_bytes(&self, url: &str, format_label: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Docs export_{} request failed", format_label.to_lowercase()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Drive export API error ({}) for {} export: {}",
                status,
                format_label,
                body
            );
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .with_context(|| format!("failed to read {} export bytes", format_label))
    }
}

/// Extract all plain text from a document by walking its body structure.
fn extract_text(doc: &Document) -> String {
    let mut text = String::new();
    if let Some(body) = &doc.body {
        for element in &body.content {
            if let Some(paragraph) = &element.paragraph {
                for para_element in &paragraph.elements {
                    if let Some(text_run) = &para_element.text_run {
                        text.push_str(&text_run.content);
                    }
                }
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_document_deserialization() {
        let json = r#"{
            "documentId": "doc123",
            "title": "My Document",
            "body": {
                "content": [
                    {
                        "startIndex": 0,
                        "endIndex": 12,
                        "paragraph": {
                            "elements": [
                                {
                                    "startIndex": 0,
                                    "endIndex": 12,
                                    "textRun": {
                                        "content": "Hello World\n"
                                    }
                                }
                            ]
                        }
                    }
                ]
            },
            "revisionId": "rev_abc"
        }"#;
        let doc: Document = serde_json::from_str(json).unwrap();
        assert_eq!(doc.document_id, "doc123");
        assert_eq!(doc.title, "My Document");
        assert!(doc.body.is_some());
        assert_eq!(doc.revision_id.as_deref(), Some("rev_abc"));
    }

    #[test]
    fn test_document_deserialization_minimal() {
        let json = r#"{
            "documentId": "doc456",
            "title": "Empty Doc"
        }"#;
        let doc: Document = serde_json::from_str(json).unwrap();
        assert_eq!(doc.document_id, "doc456");
        assert_eq!(doc.title, "Empty Doc");
        assert!(doc.body.is_none());
        assert!(doc.revision_id.is_none());
    }

    #[test]
    fn test_document_body_deserialization() {
        let json = r#"{
            "content": [
                {
                    "startIndex": 0,
                    "endIndex": 5,
                    "paragraph": {
                        "elements": [
                            {
                                "startIndex": 0,
                                "endIndex": 5,
                                "textRun": { "content": "Test\n" }
                            }
                        ]
                    }
                }
            ]
        }"#;
        let body: DocumentBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.content.len(), 1);
        let para = body.content[0].paragraph.as_ref().unwrap();
        assert_eq!(para.elements.len(), 1);
        assert_eq!(
            para.elements[0].text_run.as_ref().unwrap().content,
            "Test\n"
        );
    }

    #[test]
    fn test_extract_text_from_document() {
        let doc = Document {
            document_id: "d1".into(),
            title: "Test".into(),
            body: Some(DocumentBody {
                content: vec![
                    StructuralElement {
                        start_index: Some(0),
                        end_index: Some(6),
                        paragraph: Some(Paragraph {
                            elements: vec![ParagraphElement {
                                start_index: Some(0),
                                end_index: Some(6),
                                text_run: Some(TextRun {
                                    content: "Hello ".into(),
                                }),
                            }],
                        }),
                    },
                    StructuralElement {
                        start_index: Some(6),
                        end_index: Some(12),
                        paragraph: Some(Paragraph {
                            elements: vec![ParagraphElement {
                                start_index: Some(6),
                                end_index: Some(12),
                                text_run: Some(TextRun {
                                    content: "World\n".into(),
                                }),
                            }],
                        }),
                    },
                ],
            }),
            revision_id: None,
        };
        let text = extract_text(&doc);
        assert_eq!(text, "Hello World\n");
    }

    #[test]
    fn test_extract_text_empty_body() {
        let doc = Document {
            document_id: "d2".into(),
            title: "Empty".into(),
            body: None,
            revision_id: None,
        };
        let text = extract_text(&doc);
        assert_eq!(text, "");
    }

    #[test]
    fn test_extract_text_no_text_runs() {
        let doc = Document {
            document_id: "d3".into(),
            title: "NoRuns".into(),
            body: Some(DocumentBody {
                content: vec![StructuralElement {
                    start_index: Some(0),
                    end_index: Some(1),
                    paragraph: Some(Paragraph {
                        elements: vec![ParagraphElement {
                            start_index: Some(0),
                            end_index: Some(1),
                            text_run: None,
                        }],
                    }),
                }],
            }),
            revision_id: None,
        };
        let text = extract_text(&doc);
        assert_eq!(text, "");
    }

    #[test]
    fn test_client_default_base_url() {
        let client = GoogleDocsClient::new("tok");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url_strips_slash() {
        let client = GoogleDocsClient::with_base_url("tok", "https://docs.test/v1/");
        assert_eq!(client.base_url(), "https://docs.test/v1");
    }

    #[test]
    fn test_get_document_url_construction() {
        let client = GoogleDocsClient::new("tok");
        let url = build_url(client.base_url(), "/documents/doc123");
        assert!(url.starts_with(DEFAULT_BASE_URL));
        assert!(url.contains("/documents/doc123"));
    }

    #[test]
    fn test_create_document_url_construction() {
        let client = GoogleDocsClient::new("tok");
        let url = build_url(client.base_url(), "/documents");
        assert!(url.starts_with(DEFAULT_BASE_URL));
        assert!(url.ends_with("/documents"));
    }

    #[test]
    fn test_insert_text_url_construction() {
        let client = GoogleDocsClient::new("tok");
        let url = build_url(client.base_url(), "/documents/doc123:batchUpdate");
        assert!(url.contains("/documents/doc123:batchUpdate"));
    }

    #[test]
    fn test_create_document_request_serialization() {
        let req = CreateDocumentRequest {
            title: "New Doc".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"title\":\"New Doc\""));
    }

    #[test]
    fn test_insert_text_request_serialization() {
        let req = InsertTextRequest {
            text: "Hello".into(),
            location_index: 1,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"text\":\"Hello\""));
        assert!(json.contains("\"locationIndex\":1"));
    }

    #[test]
    fn test_document_serialization_roundtrip() {
        let doc = Document {
            document_id: "d1".into(),
            title: "Roundtrip".into(),
            body: Some(DocumentBody {
                content: vec![StructuralElement {
                    start_index: Some(0),
                    end_index: Some(4),
                    paragraph: Some(Paragraph {
                        elements: vec![ParagraphElement {
                            start_index: Some(0),
                            end_index: Some(4),
                            text_run: Some(TextRun {
                                content: "Hi!\n".into(),
                            }),
                        }],
                    }),
                }],
            }),
            revision_id: Some("rev1".into()),
        };
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back.document_id, "d1");
        assert_eq!(back.title, "Roundtrip");
        assert_eq!(back.revision_id.as_deref(), Some("rev1"));
        let text = extract_text(&back);
        assert_eq!(text, "Hi!\n");
    }

    #[test]
    fn test_set_drive_export_url() {
        let mut client = GoogleDocsClient::new("tok");
        assert_eq!(client.drive_export_url, DRIVE_EXPORT_BASE_URL);
        client.set_drive_export_url("https://test.drive/v3/");
        assert_eq!(client.drive_export_url, "https://test.drive/v3");
    }

    #[test]
    fn test_structural_element_without_paragraph() {
        let json = r#"{
            "startIndex": 0,
            "endIndex": 1
        }"#;
        let elem: StructuralElement = serde_json::from_str(json).unwrap();
        assert_eq!(elem.start_index, Some(0));
        assert_eq!(elem.end_index, Some(1));
        assert!(elem.paragraph.is_none());
    }

    #[test]
    fn test_extract_text_multiple_runs_in_paragraph() {
        let doc = Document {
            document_id: "d4".into(),
            title: "MultiRun".into(),
            body: Some(DocumentBody {
                content: vec![StructuralElement {
                    start_index: Some(0),
                    end_index: Some(11),
                    paragraph: Some(Paragraph {
                        elements: vec![
                            ParagraphElement {
                                start_index: Some(0),
                                end_index: Some(5),
                                text_run: Some(TextRun {
                                    content: "Hello".into(),
                                }),
                            },
                            ParagraphElement {
                                start_index: Some(5),
                                end_index: Some(6),
                                text_run: Some(TextRun {
                                    content: " ".into(),
                                }),
                            },
                            ParagraphElement {
                                start_index: Some(6),
                                end_index: Some(11),
                                text_run: Some(TextRun {
                                    content: "World".into(),
                                }),
                            },
                        ],
                    }),
                }],
            }),
            revision_id: None,
        };
        let text = extract_text(&doc);
        assert_eq!(text, "Hello World");
    }
}
