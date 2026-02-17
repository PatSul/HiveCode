//! Asana REST API client.
//!
//! Wraps the Asana REST API at `https://app.asana.com/api/1.0/`
//! using `reqwest` for HTTP and Bearer token authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::{debug, warn};

use super::{
    Comment, CreateIssueRequest, Issue, IssueFilters, IssuePriority, IssueStatus, IssueUpdate,
    PMPlatform, Project, ProjectManagementProvider, Sprint,
};

const DEFAULT_BASE_URL: &str = "https://app.asana.com/api/1.0";

// ── Asana API response types ─────────────────────────────────────

/// Envelope returned by Asana REST API endpoints.
#[derive(Debug, Deserialize)]
struct AsanaResponse<T> {
    data: Option<T>,
    errors: Option<Vec<AsanaError>>,
}

#[derive(Debug, Deserialize)]
struct AsanaError {
    message: String,
}

/// Envelope for list endpoints that return an array under `data`.
#[derive(Debug, Deserialize)]
struct AsanaListResponse<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct AsanaProject {
    gid: String,
    name: String,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AsanaTask {
    gid: String,
    name: String,
    notes: Option<String>,
    completed: Option<bool>,
    assignee: Option<AsanaUser>,
    #[serde(default)]
    tags: Vec<AsanaTag>,
    #[serde(default)]
    memberships: Vec<AsanaMembership>,
    #[serde(default)]
    custom_fields: Vec<AsanaCustomField>,
    created_at: Option<String>,
    modified_at: Option<String>,
    permalink_url: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AsanaUser {
    gid: String,
    name: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AsanaTag {
    gid: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct AsanaMembership {
    section: Option<AsanaSection>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AsanaSection {
    gid: String,
    name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AsanaCustomField {
    gid: String,
    name: String,
    #[serde(rename = "display_value")]
    display_value: Option<String>,
    #[serde(rename = "enum_value")]
    enum_value: Option<AsanaEnumValue>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AsanaEnumValue {
    gid: String,
    name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AsanaStory {
    gid: String,
    text: Option<String>,
    created_by: Option<AsanaUser>,
    created_at: Option<String>,
    #[serde(rename = "type")]
    story_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AsanaSectionFull {
    gid: String,
    name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AsanaWorkspace {
    gid: String,
    name: String,
}

// ── Client ─────────────────────────────────────────────────────────

/// Asana REST API client.
pub struct AsanaClient {
    access_token: String,
    base_url: String,
    client: Client,
}

impl AsanaClient {
    /// Create a new Asana client with the given personal access token.
    pub fn new(access_token: &str) -> Result<Self> {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new Asana client pointing at a custom base URL (useful for tests).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("invalid characters in Asana access token")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Asana")?;

        Ok(Self {
            access_token: access_token.to_string(),
            base_url,
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored access token.
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// Perform an authenticated GET request and parse the JSON response.
    async fn get_raw(&self, url: &str) -> Result<serde_json::Value> {
        debug!(url = %url, "Asana GET request");

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("Asana GET request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error ({}): {}", status, body);
        }

        resp.json::<serde_json::Value>()
            .await
            .context("failed to parse Asana response")
    }

    /// Perform an authenticated GET request and parse the response data.
    async fn get_list<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<Vec<T>> {
        let raw = self.get_raw(url).await?;
        let list: AsanaListResponse<T> =
            serde_json::from_value(raw).context("failed to parse Asana list response")?;
        Ok(list.data)
    }

    /// Perform an authenticated GET request and parse a single data object.
    async fn get_one<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let raw = self.get_raw(url).await?;
        let envelope: AsanaResponse<T> =
            serde_json::from_value(raw).context("failed to parse Asana response")?;

        if let Some(errors) = envelope.errors {
            if !errors.is_empty() {
                let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
                anyhow::bail!("Asana API errors: {}", messages.join("; "));
            }
        }

        envelope
            .data
            .context("Asana response contained no data")
    }

    /// Perform an authenticated POST request and parse the response data.
    async fn post_one<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<T> {
        debug!(url = %url, "Asana POST request");

        let resp = self
            .client
            .post(url)
            .json(payload)
            .send()
            .await
            .context("Asana POST request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error ({}): {}", status, body);
        }

        let envelope: AsanaResponse<T> = resp
            .json()
            .await
            .context("failed to parse Asana POST response")?;

        if let Some(errors) = envelope.errors {
            if !errors.is_empty() {
                let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
                anyhow::bail!("Asana API errors: {}", messages.join("; "));
            }
        }

        envelope
            .data
            .context("Asana response contained no data")
    }

    /// Perform an authenticated PUT request and parse the response data.
    async fn put_one<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<T> {
        debug!(url = %url, "Asana PUT request");

        let resp = self
            .client
            .put(url)
            .json(payload)
            .send()
            .await
            .context("Asana PUT request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error ({}): {}", status, body);
        }

        let envelope: AsanaResponse<T> = resp
            .json()
            .await
            .context("failed to parse Asana PUT response")?;

        if let Some(errors) = envelope.errors {
            if !errors.is_empty() {
                let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
                anyhow::bail!("Asana API errors: {}", messages.join("; "));
            }
        }

        envelope
            .data
            .context("Asana response contained no data")
    }

    /// Get the first workspace for the authenticated user.
    async fn get_default_workspace(&self) -> Result<String> {
        let url = format!("{}/workspaces?limit=1", self.base_url);
        let workspaces: Vec<AsanaWorkspace> = self.get_list(&url).await?;

        workspaces
            .into_iter()
            .next()
            .map(|w| w.gid)
            .context("no Asana workspaces found for this user")
    }

    /// Standard opt_fields for task retrieval.
    fn task_opt_fields() -> &'static str {
        "name,notes,completed,assignee,assignee.name,tags,tags.name,memberships.section,memberships.section.name,custom_fields,custom_fields.name,custom_fields.display_value,custom_fields.enum_value,custom_fields.enum_value.name,created_at,modified_at,permalink_url"
    }

    /// Convert an Asana task to our common Issue type.
    fn convert_task(task: &AsanaTask) -> Issue {
        let status = Self::map_status(task);
        let priority = Self::map_priority(task);
        let assignee = task
            .assignee
            .as_ref()
            .and_then(|a| a.name.clone());
        let labels: Vec<String> = task.tags.iter().map(|t| t.name.clone()).collect();

        // Derive a key from the section name if available.
        let section_name = task
            .memberships
            .first()
            .and_then(|m| m.section.as_ref())
            .map(|s| s.name.clone());

        Issue {
            id: task.gid.clone(),
            key: None,
            title: task.name.clone(),
            description: task.notes.clone(),
            status,
            priority,
            assignee,
            labels,
            sprint: section_name,
            created_at: task
                .created_at
                .as_deref()
                .and_then(Self::parse_datetime),
            updated_at: task
                .modified_at
                .as_deref()
                .and_then(Self::parse_datetime),
            platform: PMPlatform::Asana,
            url: task.permalink_url.clone(),
        }
    }

    /// Map an Asana task's status from its section name and completed flag.
    fn map_status(task: &AsanaTask) -> IssueStatus {
        if task.completed.unwrap_or(false) {
            return IssueStatus::Done;
        }

        // Check section name for status inference.
        if let Some(section) = task
            .memberships
            .first()
            .and_then(|m| m.section.as_ref())
        {
            let section_lower = section.name.to_lowercase();
            if section_lower.contains("backlog") {
                return IssueStatus::Backlog;
            }
            if section_lower.contains("to do")
                || section_lower.contains("todo")
                || section_lower.contains("new")
                || section_lower.contains("untriaged")
            {
                return IssueStatus::Todo;
            }
            if section_lower.contains("in progress")
                || section_lower.contains("doing")
                || section_lower.contains("active")
            {
                return IssueStatus::InProgress;
            }
            if section_lower.contains("review")
                || section_lower.contains("in review")
                || section_lower.contains("qa")
            {
                return IssueStatus::InReview;
            }
            if section_lower.contains("done")
                || section_lower.contains("complete")
                || section_lower.contains("shipped")
            {
                return IssueStatus::Done;
            }
            if section_lower.contains("cancel")
                || section_lower.contains("archive")
                || section_lower.contains("won't")
            {
                return IssueStatus::Cancelled;
            }
        }

        // Check custom fields for a "Status" field.
        for field in &task.custom_fields {
            let name_lower = field.name.to_lowercase();
            if name_lower == "status" || name_lower == "stage" {
                if let Some(ref enum_val) = field.enum_value {
                    let val = enum_val.name.to_lowercase();
                    return match val.as_str() {
                        "backlog" => IssueStatus::Backlog,
                        "to do" | "todo" | "not started" => IssueStatus::Todo,
                        "in progress" | "doing" | "active" => IssueStatus::InProgress,
                        "in review" | "review" | "qa" => IssueStatus::InReview,
                        "done" | "complete" | "completed" => IssueStatus::Done,
                        "cancelled" | "canceled" => IssueStatus::Cancelled,
                        _ => IssueStatus::Todo,
                    };
                }
                if let Some(ref display) = field.display_value {
                    let val = display.to_lowercase();
                    if val.contains("progress") {
                        return IssueStatus::InProgress;
                    }
                    if val.contains("done") || val.contains("complete") {
                        return IssueStatus::Done;
                    }
                    if val.contains("review") {
                        return IssueStatus::InReview;
                    }
                }
            }
        }

        IssueStatus::Todo
    }

    /// Map an Asana task's priority from custom fields.
    fn map_priority(task: &AsanaTask) -> IssuePriority {
        for field in &task.custom_fields {
            let name_lower = field.name.to_lowercase();
            if name_lower == "priority" || name_lower == "severity" {
                if let Some(ref enum_val) = field.enum_value {
                    let val = enum_val.name.to_lowercase();
                    return match val.as_str() {
                        "critical" | "urgent" | "p0" | "blocker" => IssuePriority::Critical,
                        "high" | "p1" | "major" => IssuePriority::High,
                        "medium" | "p2" | "normal" => IssuePriority::Medium,
                        "low" | "p3" | "minor" => IssuePriority::Low,
                        "none" | "p4" | "trivial" => IssuePriority::None,
                        _ => IssuePriority::None,
                    };
                }
                if let Some(ref display) = field.display_value {
                    let val = display.to_lowercase();
                    if val.contains("critical") || val.contains("urgent") {
                        return IssuePriority::Critical;
                    }
                    if val.contains("high") {
                        return IssuePriority::High;
                    }
                    if val.contains("medium") {
                        return IssuePriority::Medium;
                    }
                    if val.contains("low") {
                        return IssuePriority::Low;
                    }
                }
            }
        }

        IssuePriority::None
    }

    /// Parse an ISO 8601 datetime string to `DateTime<Utc>`.
    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Find the section GID for a target status within a project.
    async fn find_section_for_status(
        &self,
        project_id: &str,
        target_status: IssueStatus,
    ) -> Result<String> {
        let url = format!("{}/projects/{}/sections", self.base_url, project_id);
        let sections: Vec<AsanaSectionFull> = self.get_list(&url).await?;

        let target_names = match target_status {
            IssueStatus::Backlog => vec!["backlog"],
            IssueStatus::Todo => vec!["to do", "todo", "new", "not started"],
            IssueStatus::InProgress => vec!["in progress", "doing", "active"],
            IssueStatus::InReview => vec!["review", "in review", "qa"],
            IssueStatus::Done => vec!["done", "complete", "shipped"],
            IssueStatus::Cancelled => vec!["cancel", "archive"],
        };

        sections
            .iter()
            .find(|s| {
                let name = s.name.to_lowercase();
                target_names.iter().any(|t| name.contains(t))
            })
            .map(|s| s.gid.clone())
            .context(format!(
                "no section matching status '{}' found in project {}",
                target_status, project_id
            ))
    }

    /// Build query parameters for task filtering.
    fn build_task_filter_params(filters: &IssueFilters) -> Vec<(String, String)> {
        let mut params = Vec::new();

        if let Some(ref status) = filters.status {
            match status {
                IssueStatus::Done => {
                    params.push(("completed_since".into(), "2000-01-01T00:00:00.000Z".into()));
                }
                _ => {
                    // For non-done statuses, we filter incomplete tasks.
                    // Further filtering by section happens client-side.
                }
            }
        }

        if let Some(ref assignee) = filters.assignee {
            params.push(("assignee".into(), assignee.clone()));
        }

        params
    }
}

#[async_trait]
impl ProjectManagementProvider for AsanaClient {
    fn platform(&self) -> PMPlatform {
        PMPlatform::Asana
    }

    async fn list_projects(&self) -> Result<Vec<Project>> {
        let workspace_gid = self.get_default_workspace().await?;
        let url = format!(
            "{}/workspaces/{}/projects?opt_fields=name,notes&limit=100",
            self.base_url, workspace_gid,
        );

        let asana_projects: Vec<AsanaProject> = self.get_list(&url).await?;

        Ok(asana_projects
            .into_iter()
            .map(|p| Project {
                id: p.gid,
                name: p.name,
                key: None,
                description: p.notes,
                platform: PMPlatform::Asana,
            })
            .collect())
    }

    async fn list_issues(
        &self,
        project_id: &str,
        filters: &IssueFilters,
    ) -> Result<Vec<Issue>> {
        let opt_fields = Self::task_opt_fields();
        let mut url = format!(
            "{}/projects/{}/tasks?opt_fields={}&limit=100",
            self.base_url, project_id, opt_fields,
        );

        let extra_params = Self::build_task_filter_params(filters);
        for (key, value) in &extra_params {
            url.push_str(&format!("&{}={}", key, value));
        }

        let tasks: Vec<AsanaTask> = self.get_list(&url).await?;
        let mut issues: Vec<Issue> = tasks.iter().map(Self::convert_task).collect();

        // Apply client-side filtering for fields Asana doesn't natively filter on.
        if let Some(ref status) = filters.status {
            issues.retain(|i| i.status == *status);
        }

        if let Some(ref priority) = filters.priority {
            issues.retain(|i| i.priority == *priority);
        }

        if !filters.labels.is_empty() {
            issues.retain(|i| {
                filters
                    .labels
                    .iter()
                    .any(|l| i.labels.iter().any(|il| il.eq_ignore_ascii_case(l)))
            });
        }

        if let Some(ref query) = filters.search_query {
            let query_lower = query.to_lowercase();
            issues.retain(|i| {
                i.title.to_lowercase().contains(&query_lower)
                    || i.description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            });
        }

        Ok(issues)
    }

    async fn get_issue(&self, issue_id: &str) -> Result<Issue> {
        let opt_fields = Self::task_opt_fields();
        let url = format!(
            "{}/tasks/{}?opt_fields={}",
            self.base_url, issue_id, opt_fields,
        );

        let task: AsanaTask = self.get_one(&url).await?;
        Ok(Self::convert_task(&task))
    }

    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<Issue> {
        let url = format!("{}/tasks", self.base_url);

        let mut data = serde_json::json!({
            "name": request.title,
            "projects": [request.project_id],
        });

        if let Some(ref desc) = request.description {
            data["notes"] = serde_json::json!(desc);
        }

        if let Some(ref assignee) = request.assignee {
            data["assignee"] = serde_json::json!(assignee);
        }

        let payload = serde_json::json!({ "data": data });
        let task: AsanaTask = self.post_one(&url, &payload).await?;
        let issue = Self::convert_task(&task);

        // Add tags if specified.
        if !request.labels.is_empty() {
            for tag_gid in &request.labels {
                let tag_url = format!("{}/tasks/{}/addTag", self.base_url, task.gid);
                let tag_payload = serde_json::json!({
                    "data": { "tag": tag_gid }
                });
                if let Err(e) = self.post_one::<serde_json::Value>(&tag_url, &tag_payload).await {
                    warn!(
                        task_gid = %task.gid,
                        tag = %tag_gid,
                        error = %e,
                        "failed to add tag to Asana task"
                    );
                }
            }
        }

        Ok(issue)
    }

    async fn update_issue(&self, issue_id: &str, update: &IssueUpdate) -> Result<Issue> {
        let url = format!("{}/tasks/{}", self.base_url, issue_id);
        let mut data = serde_json::Map::new();

        if let Some(ref title) = update.title {
            data.insert("name".into(), serde_json::json!(title));
        }

        if let Some(ref desc) = update.description {
            data.insert("notes".into(), serde_json::json!(desc));
        }

        if let Some(ref assignee) = update.assignee {
            data.insert("assignee".into(), serde_json::json!(assignee));
        }

        // Mark task as completed if status is Done.
        if let Some(status) = update.status {
            match status {
                IssueStatus::Done => {
                    data.insert("completed".into(), serde_json::json!(true));
                }
                _ => {
                    data.insert("completed".into(), serde_json::json!(false));
                }
            }
        }

        if !data.is_empty() {
            let payload = serde_json::json!({ "data": serde_json::Value::Object(data) });
            let _: AsanaTask = self.put_one(&url, &payload).await?;
        }

        // Handle section-based status transition if not just done/not-done.
        if let Some(status) = update.status {
            if status != IssueStatus::Done {
                // We need to move the task to the right section.
                // First find which project the task is in.
                if let Ok(issue) = self.get_issue(issue_id).await {
                    // We need to find the project ID from memberships.
                    // Re-fetch the task to get project membership.
                    let task_url = format!(
                        "{}/tasks/{}?opt_fields=memberships.project.gid",
                        self.base_url, issue_id
                    );

                    #[derive(Deserialize)]
                    struct TaskProjects {
                        #[serde(default)]
                        memberships: Vec<TaskProjectMembership>,
                    }

                    #[derive(Deserialize)]
                    struct TaskProjectMembership {
                        project: Option<TaskProjectRef>,
                    }

                    #[derive(Deserialize)]
                    struct TaskProjectRef {
                        gid: String,
                    }

                    if let Ok(task_p) = self.get_one::<TaskProjects>(&task_url).await {
                        if let Some(project_gid) =
                            task_p.memberships.first().and_then(|m| m.project.as_ref())
                        {
                            match self
                                .find_section_for_status(&project_gid.gid, status)
                                .await
                            {
                                Ok(section_gid) => {
                                    let section_url = format!(
                                        "{}/sections/{}/addTask",
                                        self.base_url, section_gid
                                    );
                                    let section_payload = serde_json::json!({
                                        "data": { "task": issue_id }
                                    });
                                    if let Err(e) = self
                                        .post_one::<serde_json::Value>(
                                            &section_url,
                                            &section_payload,
                                        )
                                        .await
                                    {
                                        warn!(
                                            issue_id = %issue_id,
                                            error = %e,
                                            "failed to move task to new section"
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        issue_id = %issue_id,
                                        error = %e,
                                        "could not find matching section for status transition"
                                    );
                                }
                            }
                        }
                    }

                    let _ = issue; // Used above for context.
                }
            }
        }

        // Handle tag updates if labels are specified.
        if let Some(ref labels) = update.labels {
            // Note: Asana tag management is more complex (remove old, add new).
            // For now, we add the specified tags.
            for tag_gid in labels {
                let tag_url = format!("{}/tasks/{}/addTag", self.base_url, issue_id);
                let tag_payload = serde_json::json!({
                    "data": { "tag": tag_gid }
                });
                if let Err(e) =
                    self.post_one::<serde_json::Value>(&tag_url, &tag_payload).await
                {
                    warn!(
                        issue_id = %issue_id,
                        tag = %tag_gid,
                        error = %e,
                        "failed to add tag to Asana task"
                    );
                }
            }
        }

        self.get_issue(issue_id).await
    }

    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<Comment> {
        let url = format!("{}/tasks/{}/stories", self.base_url, issue_id);
        let payload = serde_json::json!({
            "data": {
                "text": body,
            }
        });

        let story: AsanaStory = self.post_one(&url, &payload).await?;

        Ok(Comment {
            id: story.gid,
            author: story.created_by.and_then(|u| u.name),
            body: story.text.unwrap_or_default(),
            created_at: story
                .created_at
                .as_deref()
                .and_then(Self::parse_datetime),
        })
    }

    async fn transition_issue(&self, issue_id: &str, status: IssueStatus) -> Result<Issue> {
        let update = IssueUpdate {
            status: Some(status),
            ..Default::default()
        };
        self.update_issue(issue_id, &update).await
    }

    async fn search_issues(&self, query: &str, limit: u32) -> Result<Vec<Issue>> {
        let workspace_gid = self.get_default_workspace().await?;
        let opt_fields = Self::task_opt_fields();
        let url = format!(
            "{}/workspaces/{}/tasks/search?text={}&opt_fields={}&limit={}",
            self.base_url,
            workspace_gid,
            urlencod(query),
            opt_fields,
            limit,
        );

        let tasks: Vec<AsanaTask> = self.get_list(&url).await?;
        Ok(tasks.iter().map(Self::convert_task).collect())
    }

    async fn get_sprints(&self, project_id: &str) -> Result<Vec<Sprint>> {
        // Asana doesn't have native sprints. We represent sections as sprints
        // since they are often used to model sprint-like workflows.
        let url = format!("{}/projects/{}/sections", self.base_url, project_id);
        let sections: Vec<AsanaSectionFull> = match self.get_list(&url).await {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    project_id = %project_id,
                    error = %e,
                    "failed to fetch Asana sections"
                );
                return Ok(vec![]);
            }
        };

        Ok(sections
            .into_iter()
            .map(|s| Sprint {
                id: s.gid,
                name: s.name,
                state: Some("active".into()),
                start_date: None,
                end_date: None,
            })
            .collect())
    }
}

/// Minimal percent-encoding for URL query parameters.
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

    fn make_client() -> AsanaClient {
        AsanaClient::with_base_url(
            "asana_test_token",
            "https://app.asana.com/api/1.0",
        )
        .unwrap()
    }

    #[test]
    fn test_new_sets_default_base_url() {
        let client = AsanaClient::new("tok").unwrap();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_custom_base_url_strips_trailing_slash() {
        let client =
            AsanaClient::with_base_url("tok", "https://app.asana.com/api/1.0/").unwrap();
        assert_eq!(client.base_url(), "https://app.asana.com/api/1.0");
    }

    #[test]
    fn test_access_token_stored() {
        let client = make_client();
        assert_eq!(client.access_token(), "asana_test_token");
    }

    #[test]
    fn test_platform() {
        let client = make_client();
        assert_eq!(client.platform(), PMPlatform::Asana);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = AsanaClient::new("tok\nwith\nnewlines");
        assert!(result.is_err());
    }

    #[test]
    fn test_map_status_completed_task() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: Some(true),
            assignee: None,
            tags: vec![],
            memberships: vec![],
            custom_fields: vec![],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_status(&task), IssueStatus::Done);
    }

    #[test]
    fn test_map_status_from_section_in_progress() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: Some(false),
            assignee: None,
            tags: vec![],
            memberships: vec![AsanaMembership {
                section: Some(AsanaSection {
                    gid: "s1".into(),
                    name: "In Progress".into(),
                }),
            }],
            custom_fields: vec![],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_status(&task), IssueStatus::InProgress);
    }

    #[test]
    fn test_map_status_from_section_backlog() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: Some(false),
            assignee: None,
            tags: vec![],
            memberships: vec![AsanaMembership {
                section: Some(AsanaSection {
                    gid: "s1".into(),
                    name: "Backlog".into(),
                }),
            }],
            custom_fields: vec![],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_status(&task), IssueStatus::Backlog);
    }

    #[test]
    fn test_map_status_from_section_review() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: Some(false),
            assignee: None,
            tags: vec![],
            memberships: vec![AsanaMembership {
                section: Some(AsanaSection {
                    gid: "s1".into(),
                    name: "Code Review".into(),
                }),
            }],
            custom_fields: vec![],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_status(&task), IssueStatus::InReview);
    }

    #[test]
    fn test_map_status_from_custom_field() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: Some(false),
            assignee: None,
            tags: vec![],
            memberships: vec![],
            custom_fields: vec![AsanaCustomField {
                gid: "cf1".into(),
                name: "Status".into(),
                display_value: None,
                enum_value: Some(AsanaEnumValue {
                    gid: "ev1".into(),
                    name: "In Progress".into(),
                }),
            }],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_status(&task), IssueStatus::InProgress);
    }

    #[test]
    fn test_map_status_default() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: Some(false),
            assignee: None,
            tags: vec![],
            memberships: vec![],
            custom_fields: vec![],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_status(&task), IssueStatus::Todo);
    }

    #[test]
    fn test_map_priority_from_custom_field_critical() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: None,
            assignee: None,
            tags: vec![],
            memberships: vec![],
            custom_fields: vec![AsanaCustomField {
                gid: "cf1".into(),
                name: "Priority".into(),
                display_value: None,
                enum_value: Some(AsanaEnumValue {
                    gid: "ev1".into(),
                    name: "Critical".into(),
                }),
            }],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_priority(&task), IssuePriority::Critical);
    }

    #[test]
    fn test_map_priority_from_custom_field_high() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: None,
            assignee: None,
            tags: vec![],
            memberships: vec![],
            custom_fields: vec![AsanaCustomField {
                gid: "cf1".into(),
                name: "Priority".into(),
                display_value: None,
                enum_value: Some(AsanaEnumValue {
                    gid: "ev1".into(),
                    name: "High".into(),
                }),
            }],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_priority(&task), IssuePriority::High);
    }

    #[test]
    fn test_map_priority_from_display_value() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: None,
            assignee: None,
            tags: vec![],
            memberships: vec![],
            custom_fields: vec![AsanaCustomField {
                gid: "cf1".into(),
                name: "Priority".into(),
                display_value: Some("Medium Priority".into()),
                enum_value: None,
            }],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_priority(&task), IssuePriority::Medium);
    }

    #[test]
    fn test_map_priority_none() {
        let task = AsanaTask {
            gid: "1".into(),
            name: "Test".into(),
            notes: None,
            completed: None,
            assignee: None,
            tags: vec![],
            memberships: vec![],
            custom_fields: vec![],
            created_at: None,
            modified_at: None,
            permalink_url: None,
        };
        assert_eq!(AsanaClient::map_priority(&task), IssuePriority::None);
    }

    #[test]
    fn test_convert_task() {
        let task = AsanaTask {
            gid: "12345".into(),
            name: "Fix auth bug".into(),
            notes: Some("Auth is broken".into()),
            completed: Some(false),
            assignee: Some(AsanaUser {
                gid: "u1".into(),
                name: Some("Alice".into()),
            }),
            tags: vec![
                AsanaTag {
                    gid: "t1".into(),
                    name: "bug".into(),
                },
                AsanaTag {
                    gid: "t2".into(),
                    name: "urgent".into(),
                },
            ],
            memberships: vec![AsanaMembership {
                section: Some(AsanaSection {
                    gid: "s1".into(),
                    name: "In Progress".into(),
                }),
            }],
            custom_fields: vec![],
            created_at: Some("2024-01-15T10:30:00.000Z".into()),
            modified_at: Some("2024-01-16T14:00:00.000Z".into()),
            permalink_url: Some("https://app.asana.com/0/12345".into()),
        };

        let issue = AsanaClient::convert_task(&task);
        assert_eq!(issue.id, "12345");
        assert!(issue.key.is_none());
        assert_eq!(issue.title, "Fix auth bug");
        assert_eq!(issue.description.as_deref(), Some("Auth is broken"));
        assert_eq!(issue.status, IssueStatus::InProgress);
        assert_eq!(issue.assignee.as_deref(), Some("Alice"));
        assert_eq!(issue.labels, vec!["bug", "urgent"]);
        assert_eq!(issue.sprint.as_deref(), Some("In Progress"));
        assert_eq!(issue.platform, PMPlatform::Asana);
        assert!(issue.url.is_some());
        assert!(issue.created_at.is_some());
        assert!(issue.updated_at.is_some());
    }

    #[test]
    fn test_parse_datetime_valid() {
        let dt = AsanaClient::parse_datetime("2024-01-15T10:30:00.000Z");
        assert!(dt.is_some());
    }

    #[test]
    fn test_parse_datetime_invalid() {
        let dt = AsanaClient::parse_datetime("not-a-date");
        assert!(dt.is_none());
    }

    #[test]
    fn test_urlencod() {
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("a+b=c"), "a%2Bb%3Dc");
    }

    #[test]
    fn test_asana_project_deserialization() {
        let json = r#"{"gid": "12345", "name": "My Project", "notes": "A project"}"#;
        let p: AsanaProject = serde_json::from_str(json).unwrap();
        assert_eq!(p.gid, "12345");
        assert_eq!(p.name, "My Project");
        assert_eq!(p.notes.as_deref(), Some("A project"));
    }

    #[test]
    fn test_asana_response_deserialization() {
        let json = r#"{"data": {"gid": "1", "name": "Task"}}"#;
        let resp: AsanaResponse<AsanaProject> = serde_json::from_str(json).unwrap();
        assert!(resp.data.is_some());
        assert!(resp.errors.is_none());
    }

    #[test]
    fn test_asana_error_deserialization() {
        let json = r#"{"data": null, "errors": [{"message": "Not found"}]}"#;
        let resp: AsanaResponse<AsanaProject> = serde_json::from_str(json).unwrap();
        assert!(resp.errors.is_some());
        assert_eq!(resp.errors.unwrap()[0].message, "Not found");
    }

    #[test]
    fn test_build_task_filter_params_empty() {
        let filters = IssueFilters::default();
        let params = AsanaClient::build_task_filter_params(&filters);
        assert!(params.is_empty());
    }

    #[test]
    fn test_build_task_filter_params_with_assignee() {
        let filters = IssueFilters {
            assignee: Some("alice".into()),
            ..Default::default()
        };
        let params = AsanaClient::build_task_filter_params(&filters);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "assignee");
        assert_eq!(params[0].1, "alice");
    }

    #[test]
    fn test_build_task_filter_params_with_done_status() {
        let filters = IssueFilters {
            status: Some(IssueStatus::Done),
            ..Default::default()
        };
        let params = AsanaClient::build_task_filter_params(&filters);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "completed_since");
    }

    #[test]
    fn test_task_opt_fields_not_empty() {
        let fields = AsanaClient::task_opt_fields();
        assert!(!fields.is_empty());
        assert!(fields.contains("name"));
        assert!(fields.contains("completed"));
        assert!(fields.contains("assignee"));
    }
}
