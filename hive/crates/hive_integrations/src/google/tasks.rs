//! Google Tasks API v1 client.
//!
//! Wraps the REST API at `https://tasks.googleapis.com/tasks/v1`
//! using `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://tasks.googleapis.com/tasks/v1";

/// A Google Tasks task list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskList {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

/// A single task within a task list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GTask {
    #[serde(default)]
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Wrapper for the `items` array in list responses.
#[derive(Deserialize)]
struct ListResponse<T> {
    #[serde(default)]
    items: Vec<T>,
}

/// Client for the Google Tasks v1 REST API.
pub struct GoogleTasksClient {
    base_url: String,
    client: Client,
}

impl GoogleTasksClient {
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

    /// List all task lists for the authenticated user.
    pub async fn list_task_lists(&self) -> Result<Vec<TaskList>> {
        let url = format!("{}/users/@me/lists", self.base_url);
        debug!(url = %url, "listing task lists");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Tasks list_task_lists request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Tasks API error ({}): {}", status, body);
        }

        let wrapper: ListResponse<TaskList> = resp
            .json()
            .await
            .context("failed to parse task lists response")?;

        Ok(wrapper.items)
    }

    /// List all tasks in a given task list.
    pub async fn list_tasks(&self, list_id: &str) -> Result<Vec<GTask>> {
        let url = format!("{}/lists/{}/tasks", self.base_url, list_id);
        debug!(url = %url, "listing tasks");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Tasks list_tasks request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Tasks API error ({}): {}", status, body);
        }

        let wrapper: ListResponse<GTask> = resp
            .json()
            .await
            .context("failed to parse tasks response")?;

        Ok(wrapper.items)
    }

    /// Create a new task in the given list.
    pub async fn create_task(&self, list_id: &str, title: &str) -> Result<GTask> {
        let url = format!("{}/lists/{}/tasks", self.base_url, list_id);
        let body = serde_json::json!({ "title": title });

        debug!(url = %url, title = %title, "creating task");

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Tasks create_task request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Tasks API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse created task")
    }

    /// Mark a task as completed.
    pub async fn complete_task(&self, list_id: &str, task_id: &str) -> Result<()> {
        let url = format!("{}/lists/{}/tasks/{}", self.base_url, list_id, task_id);
        let body = serde_json::json!({ "status": "completed" });

        debug!(url = %url, "completing task");

        let resp = self
            .client
            .patch(&url)
            .json(&body)
            .send()
            .await
            .context("Tasks complete_task request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Tasks API error ({}): {}", status, body);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_task_list_deserialization() {
        let json = r#"{ "id": "list1", "title": "My Tasks" }"#;
        let tl: TaskList = serde_json::from_str(json).unwrap();
        assert_eq!(tl.id, "list1");
        assert_eq!(tl.title, "My Tasks");
    }

    #[test]
    fn test_gtask_deserialization() {
        let json = r#"{
            "id": "t1",
            "title": "Buy groceries",
            "status": "needsAction",
            "notes": "milk, eggs, bread"
        }"#;
        let task: GTask = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "t1");
        assert_eq!(task.title, "Buy groceries");
        assert_eq!(task.status, "needsAction");
        assert_eq!(task.notes.as_deref(), Some("milk, eggs, bread"));
    }

    #[test]
    fn test_gtask_without_optional_fields() {
        let json = r#"{ "title": "Simple task" }"#;
        let task: GTask = serde_json::from_str(json).unwrap();
        assert_eq!(task.title, "Simple task");
        assert!(task.id.is_empty());
        assert!(task.notes.is_none());
    }

    #[test]
    fn test_client_default_base_url() {
        let client = GoogleTasksClient::new("tok");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url() {
        let client = GoogleTasksClient::with_base_url("tok", "https://tasks.test/v1/");
        assert_eq!(client.base_url(), "https://tasks.test/v1");
    }

    #[test]
    fn test_list_task_lists_url() {
        let client = GoogleTasksClient::new("tok");
        let url = build_url(client.base_url(), "/users/@me/lists");
        assert_eq!(url, "https://tasks.googleapis.com/tasks/v1/users/@me/lists");
    }

    #[test]
    fn test_list_tasks_url() {
        let client = GoogleTasksClient::new("tok");
        let url = build_url(client.base_url(), "/lists/list1/tasks");
        assert!(url.contains("/lists/list1/tasks"));
    }

    #[test]
    fn test_complete_task_url() {
        let client = GoogleTasksClient::new("tok");
        let url = build_url(client.base_url(), "/lists/list1/tasks/t1");
        assert!(url.contains("/lists/list1/tasks/t1"));
    }

    #[test]
    fn test_list_response_deserialization() {
        let json = r#"{
            "items": [
                { "id": "l1", "title": "Work" },
                { "id": "l2", "title": "Personal" }
            ]
        }"#;
        let resp: ListResponse<TaskList> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.items.len(), 2);
    }

    #[test]
    fn test_list_response_empty() {
        let json = r#"{}"#;
        let resp: ListResponse<TaskList> = serde_json::from_str(json).unwrap();
        assert!(resp.items.is_empty());
    }

    #[test]
    fn test_gtask_serialization_roundtrip() {
        let task = GTask {
            id: "t1".into(),
            title: "Test task".into(),
            status: "needsAction".into(),
            notes: Some("some notes".into()),
        };
        let json = serde_json::to_string(&task).unwrap();
        let back: GTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, task.id);
        assert_eq!(back.title, task.title);
        assert_eq!(back.notes, task.notes);
    }
}
