//! Jira Cloud REST API v3 client.
//!
//! Wraps the Jira REST API at `https://{domain}.atlassian.net/rest/api/3/`
//! using `reqwest` for HTTP and Basic authentication (email + API token).

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::{debug, warn};

use super::{
    Comment, CreateIssueRequest, Issue, IssueFilters, IssuePriority, IssueStatus, IssueUpdate,
    PMPlatform, Project, ProjectManagementProvider, Sprint,
};

// ── Jira API response types ──────────────────────────────────────

// These structs map to Jira's JSON schema. Some fields are kept for
// completeness even when they are not directly read in Rust code.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct JiraSearchResponse {
    #[serde(default)]
    issues: Vec<JiraIssue>,
    #[serde(default)]
    total: u64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct JiraIssue {
    id: String,
    key: String,
    #[serde(rename = "self")]
    self_url: Option<String>,
    fields: JiraIssueFields,
}

#[derive(Debug, Deserialize)]
struct JiraIssueFields {
    summary: Option<String>,
    description: Option<serde_json::Value>,
    status: Option<JiraStatus>,
    priority: Option<JiraPriority>,
    assignee: Option<JiraUser>,
    #[serde(default)]
    labels: Vec<String>,
    sprint: Option<JiraSprint>,
    created: Option<String>,
    updated: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JiraStatus {
    name: Option<String>,
    #[serde(rename = "statusCategory")]
    status_category: Option<JiraStatusCategory>,
}

#[derive(Debug, Deserialize)]
struct JiraStatusCategory {
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JiraPriority {
    name: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct JiraUser {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "accountId")]
    account_id: Option<String>,
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct JiraSprint {
    id: Option<u64>,
    name: Option<String>,
    state: Option<String>,
    #[serde(rename = "startDate")]
    start_date: Option<String>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JiraProject {
    id: String,
    key: String,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JiraComment {
    id: String,
    author: Option<JiraUser>,
    body: Option<serde_json::Value>,
    created: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JiraTransitionsResponse {
    #[serde(default)]
    transitions: Vec<JiraTransition>,
}

#[derive(Debug, Deserialize)]
struct JiraTransition {
    id: String,
    name: String,
    to: Option<JiraStatus>,
}

#[derive(Debug, Deserialize)]
struct JiraBoardResponse {
    #[serde(default)]
    values: Vec<JiraBoard>,
}

#[derive(Debug, Deserialize)]
struct JiraBoard {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct JiraSprintResponse {
    #[serde(default)]
    values: Vec<JiraSprintValue>,
}

#[derive(Debug, Deserialize)]
struct JiraSprintValue {
    id: u64,
    name: String,
    state: Option<String>,
    #[serde(rename = "startDate")]
    start_date: Option<String>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
}

// ── Client ─────────────────────────────────────────────────────────

/// Jira Cloud REST API client.
pub struct JiraClient {
    domain: String,
    email: String,
    api_token: String,
    base_url: String,
    client: Client,
}

impl JiraClient {
    /// Create a new Jira client for the given Atlassian domain.
    ///
    /// `domain` is the subdomain portion, e.g. "mycompany" for
    /// `https://mycompany.atlassian.net`.
    pub fn new(domain: &str, email: &str, api_token: &str) -> Result<Self> {
        let base_url = format!("https://{domain}.atlassian.net/rest/api/3");
        Self::with_base_url(domain, email, api_token, &base_url)
    }

    /// Create a new Jira client pointing at a custom base URL (useful for tests).
    pub fn with_base_url(
        domain: &str,
        email: &str,
        api_token: &str,
        base_url: &str,
    ) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Jira")?;

        Ok(Self {
            domain: domain.to_string(),
            email: email.to_string(),
            api_token: api_token.to_string(),
            base_url,
            client,
        })
    }

    /// Return the configured domain.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Perform an authenticated GET request and parse the JSON response.
    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        debug!(url = %url, "Jira GET request");

        let resp = self
            .client
            .get(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .send()
            .await
            .context("Jira GET request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Jira API error ({}): {}", status, body);
        }

        resp.json::<T>()
            .await
            .context("failed to parse Jira response")
    }

    /// Perform an authenticated POST request with a JSON body.
    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<T> {
        debug!(url = %url, "Jira POST request");

        let resp = self
            .client
            .post(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .json(payload)
            .send()
            .await
            .context("Jira POST request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Jira API error ({}): {}", status, body);
        }

        resp.json::<T>()
            .await
            .context("failed to parse Jira response")
    }

    /// Perform an authenticated PUT request with a JSON body, returning no body.
    async fn put_no_content(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        debug!(url = %url, "Jira PUT request");

        let resp = self
            .client
            .put(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .json(payload)
            .send()
            .await
            .context("Jira PUT request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Jira API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// Perform an authenticated POST request that returns no JSON body (e.g. 204).
    async fn post_no_content(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        debug!(url = %url, "Jira POST (no content) request");

        let resp = self
            .client
            .post(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .json(payload)
            .send()
            .await
            .context("Jira POST request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Jira API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// Build the browse URL for an issue key.
    fn browse_url(&self, key: &str) -> String {
        format!("https://{}.atlassian.net/browse/{}", self.domain, key)
    }

    /// Convert a Jira issue to our common Issue type.
    fn convert_issue(&self, jira: &JiraIssue) -> Issue {
        let status = self.map_status(&jira.fields.status);
        let priority = Self::map_priority(&jira.fields.priority);
        let assignee = jira
            .fields
            .assignee
            .as_ref()
            .and_then(|a| a.display_name.clone());
        let sprint_name = jira
            .fields
            .sprint
            .as_ref()
            .and_then(|s| s.name.clone());
        let description = jira
            .fields
            .description
            .as_ref()
            .map(|d| Self::extract_text_from_adf(d));

        Issue {
            id: jira.id.clone(),
            key: Some(jira.key.clone()),
            title: jira.fields.summary.clone().unwrap_or_default(),
            description,
            status,
            priority,
            assignee,
            labels: jira.fields.labels.clone(),
            sprint: sprint_name,
            created_at: jira
                .fields
                .created
                .as_deref()
                .and_then(Self::parse_datetime),
            updated_at: jira
                .fields
                .updated
                .as_deref()
                .and_then(Self::parse_datetime),
            platform: PMPlatform::Jira,
            url: Some(self.browse_url(&jira.key)),
        }
    }

    /// Map a Jira status to our IssueStatus enum.
    ///
    /// Uses the status category key first (more reliable), then falls back
    /// to matching the status name.
    fn map_status(&self, status: &Option<JiraStatus>) -> IssueStatus {
        let Some(status) = status else {
            return IssueStatus::Todo;
        };

        // Try category key first.
        if let Some(ref cat) = status.status_category {
            if let Some(ref key) = cat.key {
                return match key.as_str() {
                    "new" => IssueStatus::Todo,
                    "indeterminate" => IssueStatus::InProgress,
                    "done" => IssueStatus::Done,
                    _ => IssueStatus::Todo,
                };
            }
        }

        // Fall back to status name matching.
        let name = status.name.as_deref().unwrap_or("").to_lowercase();
        match name.as_str() {
            "backlog" => IssueStatus::Backlog,
            "to do" | "todo" | "open" | "new" => IssueStatus::Todo,
            "in progress" | "in development" | "started" => IssueStatus::InProgress,
            "in review" | "code review" | "review" => IssueStatus::InReview,
            "done" | "closed" | "resolved" | "complete" => IssueStatus::Done,
            "cancelled" | "canceled" | "won't do" | "rejected" => IssueStatus::Cancelled,
            _ => IssueStatus::Todo,
        }
    }

    /// Map a Jira priority to our IssuePriority enum.
    fn map_priority(priority: &Option<JiraPriority>) -> IssuePriority {
        let Some(p) = priority else {
            return IssuePriority::None;
        };
        let name = p.name.as_deref().unwrap_or("").to_lowercase();
        match name.as_str() {
            "highest" | "blocker" | "critical" => IssuePriority::Critical,
            "high" | "major" => IssuePriority::High,
            "medium" | "normal" => IssuePriority::Medium,
            "low" | "minor" => IssuePriority::Low,
            "lowest" | "trivial" => IssuePriority::None,
            _ => IssuePriority::None,
        }
    }

    /// Map our IssuePriority back to a Jira priority name.
    fn priority_to_jira(priority: IssuePriority) -> &'static str {
        match priority {
            IssuePriority::Critical => "Highest",
            IssuePriority::High => "High",
            IssuePriority::Medium => "Medium",
            IssuePriority::Low => "Low",
            IssuePriority::None => "Lowest",
        }
    }

    /// Map our IssueStatus to a Jira category key for matching transitions.
    fn status_to_jira_category(status: IssueStatus) -> &'static str {
        match status {
            IssueStatus::Backlog | IssueStatus::Todo => "new",
            IssueStatus::InProgress | IssueStatus::InReview => "indeterminate",
            IssueStatus::Done | IssueStatus::Cancelled => "done",
        }
    }

    /// Extract plain text from a Jira ADF (Atlassian Document Format) value.
    fn extract_text_from_adf(value: &serde_json::Value) -> String {
        if let Some(s) = value.as_str() {
            return s.to_string();
        }

        let mut text = String::new();
        if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
            for block in content {
                if let Some(inner) = block.get("content").and_then(|c| c.as_array()) {
                    for node in inner {
                        if let Some(t) = node.get("text").and_then(|t| t.as_str()) {
                            text.push_str(t);
                        }
                    }
                }
                text.push('\n');
            }
        }
        text.trim().to_string()
    }

    /// Build an ADF document from plain text.
    fn text_to_adf(text: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }
            ]
        })
    }

    /// Parse an ISO 8601 datetime string to `DateTime<Utc>`.
    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|| {
                // Jira sometimes uses the format "2024-01-15T10:30:00.000+0000"
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%z")
                    .ok()
                    .map(|ndt| ndt.and_utc())
            })
    }

    /// Build a JQL query from our IssueFilters.
    fn build_jql(&self, project_id: &str, filters: &IssueFilters) -> String {
        let mut clauses = vec![format!("project = \"{}\"", project_id)];

        if let Some(ref status) = filters.status {
            let jira_cat = Self::status_to_jira_category(*status);
            clauses.push(format!("statusCategory = \"{}\"", jira_cat));
        }

        if let Some(ref assignee) = filters.assignee {
            clauses.push(format!("assignee = \"{}\"", assignee));
        }

        if let Some(ref priority) = filters.priority {
            let jira_priority = Self::priority_to_jira(*priority);
            clauses.push(format!("priority = \"{}\"", jira_priority));
        }

        for label in &filters.labels {
            clauses.push(format!("labels = \"{}\"", label));
        }

        if let Some(ref sprint_id) = filters.sprint_id {
            clauses.push(format!("sprint = {}", sprint_id));
        }

        if let Some(ref query) = filters.search_query {
            clauses.push(format!("text ~ \"{}\"", query));
        }

        clauses.join(" AND ") + " ORDER BY updated DESC"
    }
}

#[async_trait]
impl ProjectManagementProvider for JiraClient {
    fn platform(&self) -> PMPlatform {
        PMPlatform::Jira
    }

    async fn list_projects(&self) -> Result<Vec<Project>> {
        let url = format!("{}/project", self.base_url);
        let jira_projects: Vec<JiraProject> = self.get(&url).await?;

        Ok(jira_projects
            .into_iter()
            .map(|p| Project {
                id: p.id,
                name: p.name,
                key: Some(p.key),
                description: p.description,
                platform: PMPlatform::Jira,
            })
            .collect())
    }

    async fn list_issues(
        &self,
        project_id: &str,
        filters: &IssueFilters,
    ) -> Result<Vec<Issue>> {
        let jql = self.build_jql(project_id, filters);
        let url = format!(
            "{}/search?jql={}&maxResults=50&fields=summary,description,status,priority,assignee,labels,sprint,created,updated",
            self.base_url,
            urlencod(&jql),
        );

        let resp: JiraSearchResponse = self.get(&url).await?;
        debug!(total = resp.total, returned = resp.issues.len(), "Jira search results");

        Ok(resp.issues.iter().map(|i| self.convert_issue(i)).collect())
    }

    async fn get_issue(&self, issue_id: &str) -> Result<Issue> {
        let url = format!(
            "{}/issue/{}?fields=summary,description,status,priority,assignee,labels,sprint,created,updated",
            self.base_url, issue_id,
        );

        let jira_issue: JiraIssue = self.get(&url).await?;
        Ok(self.convert_issue(&jira_issue))
    }

    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<Issue> {
        let mut fields = serde_json::json!({
            "project": { "id": request.project_id },
            "summary": request.title,
            "issuetype": { "name": "Task" },
        });

        if let Some(ref desc) = request.description {
            fields["description"] = Self::text_to_adf(desc);
        }

        if let Some(ref priority) = request.priority {
            fields["priority"] = serde_json::json!({
                "name": Self::priority_to_jira(*priority)
            });
        }

        if let Some(ref assignee) = request.assignee {
            fields["assignee"] = serde_json::json!({
                "accountId": assignee
            });
        }

        if !request.labels.is_empty() {
            fields["labels"] = serde_json::json!(request.labels);
        }

        let payload = serde_json::json!({ "fields": fields });
        let url = format!("{}/issue", self.base_url);

        let created: JiraIssue = self.post(&url, &payload).await?;
        // The response from create is minimal, so fetch the full issue.
        self.get_issue(&created.key).await
    }

    async fn update_issue(&self, issue_id: &str, update: &IssueUpdate) -> Result<Issue> {
        let mut fields = serde_json::Map::new();

        if let Some(ref title) = update.title {
            fields.insert("summary".into(), serde_json::json!(title));
        }

        if let Some(ref desc) = update.description {
            fields.insert("description".into(), Self::text_to_adf(desc));
        }

        if let Some(ref priority) = update.priority {
            fields.insert(
                "priority".into(),
                serde_json::json!({ "name": Self::priority_to_jira(*priority) }),
            );
        }

        if let Some(ref assignee) = update.assignee {
            fields.insert(
                "assignee".into(),
                serde_json::json!({ "accountId": assignee }),
            );
        }

        if let Some(ref labels) = update.labels {
            fields.insert("labels".into(), serde_json::json!(labels));
        }

        if !fields.is_empty() {
            let payload = serde_json::json!({ "fields": fields });
            let url = format!("{}/issue/{}", self.base_url, issue_id);
            self.put_no_content(&url, &payload).await?;
        }

        // Handle status transition separately.
        if let Some(status) = update.status {
            self.transition_issue(issue_id, status).await?;
        }

        self.get_issue(issue_id).await
    }

    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<Comment> {
        let url = format!("{}/issue/{}/comment", self.base_url, issue_id);
        let payload = serde_json::json!({
            "body": Self::text_to_adf(body)
        });

        let jira_comment: JiraComment = self.post(&url, &payload).await?;

        Ok(Comment {
            id: jira_comment.id,
            author: jira_comment
                .author
                .and_then(|a| a.display_name),
            body: jira_comment
                .body
                .as_ref()
                .map(Self::extract_text_from_adf)
                .unwrap_or_default(),
            created_at: jira_comment
                .created
                .as_deref()
                .and_then(Self::parse_datetime),
        })
    }

    async fn transition_issue(&self, issue_id: &str, status: IssueStatus) -> Result<Issue> {
        // First, get available transitions.
        let transitions_url = format!("{}/issue/{}/transitions", self.base_url, issue_id);
        let transitions_resp: JiraTransitionsResponse = self.get(&transitions_url).await?;

        let target_category = Self::status_to_jira_category(status);

        // Find the best matching transition.
        let transition = transitions_resp
            .transitions
            .iter()
            .find(|t| {
                t.to.as_ref()
                    .and_then(|s| s.status_category.as_ref())
                    .and_then(|c| c.key.as_deref())
                    == Some(target_category)
            })
            .or_else(|| {
                // Fallback: match by name patterns.
                let status_lower = status.to_string();
                transitions_resp
                    .transitions
                    .iter()
                    .find(|t| t.name.to_lowercase().contains(&status_lower))
            })
            .context(format!(
                "no matching transition found for status '{}' on issue {}",
                status, issue_id
            ))?;

        let payload = serde_json::json!({
            "transition": { "id": transition.id }
        });

        self.post_no_content(&transitions_url, &payload).await?;
        debug!(issue_id = %issue_id, transition = %transition.name, "transitioned Jira issue");

        self.get_issue(issue_id).await
    }

    async fn search_issues(&self, query: &str, limit: u32) -> Result<Vec<Issue>> {
        let jql = format!("text ~ \"{}\" ORDER BY updated DESC", query);
        let url = format!(
            "{}/search?jql={}&maxResults={}&fields=summary,description,status,priority,assignee,labels,sprint,created,updated",
            self.base_url,
            urlencod(&jql),
            limit,
        );

        let resp: JiraSearchResponse = self.get(&url).await?;
        Ok(resp.issues.iter().map(|i| self.convert_issue(i)).collect())
    }

    async fn get_sprints(&self, project_id: &str) -> Result<Vec<Sprint>> {
        // Sprints are accessed via the Agile API, which requires finding the board first.
        let agile_base = self
            .base_url
            .replace("/rest/api/3", "/rest/agile/1.0");

        let boards_url = format!(
            "{}/board?projectKeyOrId={}",
            agile_base, project_id
        );

        let boards: JiraBoardResponse = match self.get(&boards_url).await {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    project_id = %project_id,
                    error = %e,
                    "failed to fetch Jira boards, project may not use Scrum"
                );
                return Ok(vec![]);
            }
        };

        let Some(board) = boards.values.first() else {
            return Ok(vec![]);
        };

        let sprints_url = format!("{}/board/{}/sprint", agile_base, board.id);
        let sprints_resp: JiraSprintResponse = self.get(&sprints_url).await?;

        Ok(sprints_resp
            .values
            .into_iter()
            .map(|s| Sprint {
                id: s.id.to_string(),
                name: s.name,
                state: s.state,
                start_date: s.start_date.as_deref().and_then(Self::parse_datetime),
                end_date: s.end_date.as_deref().and_then(Self::parse_datetime),
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
    use chrono::Datelike;

    fn make_client() -> JiraClient {
        JiraClient::with_base_url(
            "testdomain",
            "user@example.com",
            "test-api-token",
            "https://testdomain.atlassian.net/rest/api/3",
        )
        .unwrap()
    }

    #[test]
    fn test_new_sets_correct_base_url() {
        let client = JiraClient::new("mycompany", "me@co.com", "tok123").unwrap();
        assert_eq!(
            client.base_url(),
            "https://mycompany.atlassian.net/rest/api/3"
        );
    }

    #[test]
    fn test_domain_stored() {
        let client = make_client();
        assert_eq!(client.domain(), "testdomain");
    }

    #[test]
    fn test_custom_base_url_strips_trailing_slash() {
        let client = JiraClient::with_base_url(
            "test",
            "u@t.com",
            "tok",
            "https://test.atlassian.net/rest/api/3/",
        )
        .unwrap();
        assert_eq!(
            client.base_url(),
            "https://test.atlassian.net/rest/api/3"
        );
    }

    #[test]
    fn test_platform() {
        let client = make_client();
        assert_eq!(client.platform(), PMPlatform::Jira);
    }

    #[test]
    fn test_browse_url() {
        let client = make_client();
        assert_eq!(
            client.browse_url("PROJ-123"),
            "https://testdomain.atlassian.net/browse/PROJ-123"
        );
    }

    #[test]
    fn test_map_priority_highest() {
        let p = Some(JiraPriority {
            name: Some("Highest".into()),
        });
        assert_eq!(JiraClient::map_priority(&p), IssuePriority::Critical);
    }

    #[test]
    fn test_map_priority_high() {
        let p = Some(JiraPriority {
            name: Some("High".into()),
        });
        assert_eq!(JiraClient::map_priority(&p), IssuePriority::High);
    }

    #[test]
    fn test_map_priority_medium() {
        let p = Some(JiraPriority {
            name: Some("Medium".into()),
        });
        assert_eq!(JiraClient::map_priority(&p), IssuePriority::Medium);
    }

    #[test]
    fn test_map_priority_low() {
        let p = Some(JiraPriority {
            name: Some("Low".into()),
        });
        assert_eq!(JiraClient::map_priority(&p), IssuePriority::Low);
    }

    #[test]
    fn test_map_priority_lowest() {
        let p = Some(JiraPriority {
            name: Some("Lowest".into()),
        });
        assert_eq!(JiraClient::map_priority(&p), IssuePriority::None);
    }

    #[test]
    fn test_map_priority_none() {
        assert_eq!(JiraClient::map_priority(&None), IssuePriority::None);
    }

    #[test]
    fn test_map_status_by_category_new() {
        let client = make_client();
        let status = Some(JiraStatus {
            name: Some("Open".into()),
            status_category: Some(JiraStatusCategory {
                key: Some("new".into()),
            }),
        });
        assert_eq!(client.map_status(&status), IssueStatus::Todo);
    }

    #[test]
    fn test_map_status_by_category_indeterminate() {
        let client = make_client();
        let status = Some(JiraStatus {
            name: Some("In Progress".into()),
            status_category: Some(JiraStatusCategory {
                key: Some("indeterminate".into()),
            }),
        });
        assert_eq!(client.map_status(&status), IssueStatus::InProgress);
    }

    #[test]
    fn test_map_status_by_category_done() {
        let client = make_client();
        let status = Some(JiraStatus {
            name: Some("Done".into()),
            status_category: Some(JiraStatusCategory {
                key: Some("done".into()),
            }),
        });
        assert_eq!(client.map_status(&status), IssueStatus::Done);
    }

    #[test]
    fn test_map_status_by_name_backlog() {
        let client = make_client();
        let status = Some(JiraStatus {
            name: Some("Backlog".into()),
            status_category: None,
        });
        assert_eq!(client.map_status(&status), IssueStatus::Backlog);
    }

    #[test]
    fn test_map_status_by_name_in_review() {
        let client = make_client();
        let status = Some(JiraStatus {
            name: Some("In Review".into()),
            status_category: None,
        });
        assert_eq!(client.map_status(&status), IssueStatus::InReview);
    }

    #[test]
    fn test_map_status_by_name_cancelled() {
        let client = make_client();
        let status = Some(JiraStatus {
            name: Some("Cancelled".into()),
            status_category: None,
        });
        assert_eq!(client.map_status(&status), IssueStatus::Cancelled);
    }

    #[test]
    fn test_map_status_none() {
        let client = make_client();
        assert_eq!(client.map_status(&None), IssueStatus::Todo);
    }

    #[test]
    fn test_priority_to_jira() {
        assert_eq!(JiraClient::priority_to_jira(IssuePriority::Critical), "Highest");
        assert_eq!(JiraClient::priority_to_jira(IssuePriority::High), "High");
        assert_eq!(JiraClient::priority_to_jira(IssuePriority::Medium), "Medium");
        assert_eq!(JiraClient::priority_to_jira(IssuePriority::Low), "Low");
        assert_eq!(JiraClient::priority_to_jira(IssuePriority::None), "Lowest");
    }

    #[test]
    fn test_extract_text_from_adf_string() {
        let val = serde_json::json!("plain text");
        assert_eq!(JiraClient::extract_text_from_adf(&val), "plain text");
    }

    #[test]
    fn test_extract_text_from_adf_doc() {
        let val = serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        { "type": "text", "text": "Hello " },
                        { "type": "text", "text": "world" }
                    ]
                }
            ]
        });
        assert_eq!(JiraClient::extract_text_from_adf(&val), "Hello world");
    }

    #[test]
    fn test_text_to_adf() {
        let adf = JiraClient::text_to_adf("Hello");
        assert_eq!(adf["type"], "doc");
        assert_eq!(adf["version"], 1);
        let content = &adf["content"][0]["content"][0];
        assert_eq!(content["text"], "Hello");
    }

    #[test]
    fn test_build_jql_basic() {
        let client = make_client();
        let filters = IssueFilters::default();
        let jql = client.build_jql("PROJ", &filters);
        assert!(jql.contains("project = \"PROJ\""));
        assert!(jql.contains("ORDER BY updated DESC"));
    }

    #[test]
    fn test_build_jql_with_status() {
        let client = make_client();
        let filters = IssueFilters {
            status: Some(IssueStatus::InProgress),
            ..Default::default()
        };
        let jql = client.build_jql("PROJ", &filters);
        assert!(jql.contains("statusCategory = \"indeterminate\""));
    }

    #[test]
    fn test_build_jql_with_assignee() {
        let client = make_client();
        let filters = IssueFilters {
            assignee: Some("alice".into()),
            ..Default::default()
        };
        let jql = client.build_jql("PROJ", &filters);
        assert!(jql.contains("assignee = \"alice\""));
    }

    #[test]
    fn test_build_jql_with_labels() {
        let client = make_client();
        let filters = IssueFilters {
            labels: vec!["bug".into(), "urgent".into()],
            ..Default::default()
        };
        let jql = client.build_jql("PROJ", &filters);
        assert!(jql.contains("labels = \"bug\""));
        assert!(jql.contains("labels = \"urgent\""));
    }

    #[test]
    fn test_build_jql_with_search_query() {
        let client = make_client();
        let filters = IssueFilters {
            search_query: Some("login error".into()),
            ..Default::default()
        };
        let jql = client.build_jql("PROJ", &filters);
        assert!(jql.contains("text ~ \"login error\""));
    }

    #[test]
    fn test_urlencod() {
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencod("safe-string_123.txt"), "safe-string_123.txt");
    }

    #[test]
    fn test_convert_issue() {
        let client = make_client();
        let jira_issue = JiraIssue {
            id: "10001".into(),
            key: "PROJ-42".into(),
            self_url: Some("https://testdomain.atlassian.net/rest/api/3/issue/10001".into()),
            fields: JiraIssueFields {
                summary: Some("Fix login bug".into()),
                description: Some(serde_json::json!("Users cannot log in")),
                status: Some(JiraStatus {
                    name: Some("In Progress".into()),
                    status_category: Some(JiraStatusCategory {
                        key: Some("indeterminate".into()),
                    }),
                }),
                priority: Some(JiraPriority {
                    name: Some("High".into()),
                }),
                assignee: Some(JiraUser {
                    display_name: Some("Alice".into()),
                    account_id: Some("abc123".into()),
                    email_address: Some("alice@example.com".into()),
                }),
                labels: vec!["bug".into()],
                sprint: Some(JiraSprint {
                    id: Some(5),
                    name: Some("Sprint 5".into()),
                    state: Some("active".into()),
                    start_date: None,
                    end_date: None,
                }),
                created: None,
                updated: None,
            },
        };

        let issue = client.convert_issue(&jira_issue);
        assert_eq!(issue.id, "10001");
        assert_eq!(issue.key.as_deref(), Some("PROJ-42"));
        assert_eq!(issue.title, "Fix login bug");
        assert_eq!(issue.status, IssueStatus::InProgress);
        assert_eq!(issue.priority, IssuePriority::High);
        assert_eq!(issue.assignee.as_deref(), Some("Alice"));
        assert_eq!(issue.labels, vec!["bug"]);
        assert_eq!(issue.sprint.as_deref(), Some("Sprint 5"));
        assert_eq!(issue.platform, PMPlatform::Jira);
        assert!(issue.url.as_ref().unwrap().contains("PROJ-42"));
    }

    #[test]
    fn test_status_to_jira_category() {
        assert_eq!(JiraClient::status_to_jira_category(IssueStatus::Backlog), "new");
        assert_eq!(JiraClient::status_to_jira_category(IssueStatus::Todo), "new");
        assert_eq!(
            JiraClient::status_to_jira_category(IssueStatus::InProgress),
            "indeterminate"
        );
        assert_eq!(
            JiraClient::status_to_jira_category(IssueStatus::InReview),
            "indeterminate"
        );
        assert_eq!(JiraClient::status_to_jira_category(IssueStatus::Done), "done");
        assert_eq!(JiraClient::status_to_jira_category(IssueStatus::Cancelled), "done");
    }

    #[test]
    fn test_parse_datetime_rfc3339() {
        let dt = JiraClient::parse_datetime("2024-01-15T10:30:00.000+00:00");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2024);
    }

    #[test]
    fn test_parse_datetime_invalid() {
        let dt = JiraClient::parse_datetime("not-a-date");
        assert!(dt.is_none());
    }

    #[test]
    fn test_jira_project_deserialization() {
        let json = r#"{"id": "10000", "key": "PROJ", "name": "My Project"}"#;
        let p: JiraProject = serde_json::from_str(json).unwrap();
        assert_eq!(p.id, "10000");
        assert_eq!(p.key, "PROJ");
        assert_eq!(p.name, "My Project");
    }

    #[test]
    fn test_jira_search_response_deserialization() {
        let json = r#"{"issues": [], "total": 0}"#;
        let resp: JiraSearchResponse = serde_json::from_str(json).unwrap();
        assert!(resp.issues.is_empty());
        assert_eq!(resp.total, 0);
    }
}
