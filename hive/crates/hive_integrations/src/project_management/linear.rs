//! Linear GraphQL API client.
//!
//! Wraps the Linear GraphQL API at `https://api.linear.app/graphql`
//! using `reqwest` for HTTP and Bearer token authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::{debug, warn};

use super::{
    Comment, CreateIssueRequest, Issue, IssueFilters, IssuePriority, IssueStatus, IssueUpdate,
    PMPlatform, Project, ProjectManagementProvider, Sprint,
};

const DEFAULT_BASE_URL: &str = "https://api.linear.app/graphql";

// ── Linear API response types ────────────────────────────────────

// These structs map to Linear's GraphQL schema. Some fields are kept for
// completeness even when they are not directly read in Rust code.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct TeamsData {
    teams: LinearConnection<LinearTeam>,
}

#[derive(Debug, Deserialize)]
struct LinearConnection<T> {
    nodes: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct LinearTeam {
    id: String,
    name: String,
    key: String,
    description: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct IssuesData {
    issues: LinearConnection<LinearIssue>,
}

#[derive(Debug, Deserialize)]
struct TeamIssuesData {
    team: Option<TeamIssuesInner>,
}

#[derive(Debug, Deserialize)]
struct TeamIssuesInner {
    issues: LinearConnection<LinearIssue>,
}

#[derive(Debug, Deserialize)]
struct IssueData {
    issue: LinearIssue,
}

#[derive(Debug, Deserialize)]
struct IssueCreateData {
    #[serde(rename = "issueCreate")]
    issue_create: IssueCreatePayload,
}

#[derive(Debug, Deserialize)]
struct IssueCreatePayload {
    success: bool,
    issue: Option<LinearIssue>,
}

#[derive(Debug, Deserialize)]
struct IssueUpdateData {
    #[serde(rename = "issueUpdate")]
    issue_update: IssueUpdatePayload,
}

#[derive(Debug, Deserialize)]
struct IssueUpdatePayload {
    success: bool,
    issue: Option<LinearIssue>,
}

#[derive(Debug, Deserialize)]
struct CommentCreateData {
    #[serde(rename = "commentCreate")]
    comment_create: CommentCreatePayload,
}

#[derive(Debug, Deserialize)]
struct CommentCreatePayload {
    success: bool,
    comment: Option<LinearComment>,
}

#[derive(Debug, Deserialize)]
struct IssueSearchData {
    #[serde(rename = "issueSearch")]
    issue_search: LinearConnection<LinearIssue>,
}

#[derive(Debug, Deserialize)]
struct CyclesData {
    team: Option<TeamCyclesInner>,
}

#[derive(Debug, Deserialize)]
struct TeamCyclesInner {
    cycles: LinearConnection<LinearCycle>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WorkflowStatesData {
    #[serde(rename = "workflowStates")]
    workflow_states: LinearConnection<LinearWorkflowState>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinearIssue {
    id: String,
    identifier: Option<String>,
    title: String,
    description: Option<String>,
    state: Option<LinearWorkflowState>,
    priority: Option<f64>,
    priority_label: Option<String>,
    assignee: Option<LinearUser>,
    #[serde(default)]
    labels: LinearLabelConnection,
    cycle: Option<LinearCycleRef>,
    created_at: Option<String>,
    updated_at: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct LinearLabelConnection {
    #[serde(default)]
    nodes: Vec<LinearLabel>,
}

#[derive(Debug, Deserialize)]
struct LinearLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct LinearWorkflowState {
    id: String,
    name: String,
    #[serde(rename = "type")]
    state_type: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinearUser {
    id: String,
    name: String,
    display_name: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct LinearCycleRef {
    id: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinearCycle {
    id: String,
    name: Option<String>,
    number: Option<u64>,
    starts_at: Option<String>,
    ends_at: Option<String>,
    #[serde(rename = "completedAt")]
    completed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinearComment {
    id: String,
    body: String,
    user: Option<LinearUser>,
    created_at: Option<String>,
}

// ── Client ─────────────────────────────────────────────────────────

/// Linear GraphQL API client.
pub struct LinearClient {
    api_key: String,
    base_url: String,
    client: Client,
}

impl LinearClient {
    /// Create a new Linear client with the given API key.
    pub fn new(api_key: &str) -> Result<Self> {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Create a new Linear client pointing at a custom base URL (useful for tests).
    pub fn with_base_url(api_key: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {api_key}"))
            .context("invalid characters in Linear API key")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Linear")?;

        Ok(Self {
            api_key: api_key.to_string(),
            base_url,
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Execute a GraphQL query and parse the typed response.
    async fn graphql<T: serde::de::DeserializeOwned>(
        &self,
        query: &str,
        variables: Option<serde_json::Value>,
    ) -> Result<T> {
        let mut payload = serde_json::json!({ "query": query });
        if let Some(vars) = variables {
            payload["variables"] = vars;
        }

        debug!(url = %self.base_url, "Linear GraphQL request");

        let resp = self
            .client
            .post(&self.base_url)
            .json(&payload)
            .send()
            .await
            .context("Linear GraphQL request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Linear API HTTP error ({}): {}", status, body);
        }

        let gql_resp: GraphQLResponse<T> = resp
            .json()
            .await
            .context("failed to parse Linear GraphQL response")?;

        if let Some(errors) = gql_resp.errors {
            if !errors.is_empty() {
                let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
                anyhow::bail!("Linear GraphQL errors: {}", messages.join("; "));
            }
        }

        gql_resp
            .data
            .context("Linear GraphQL response contained no data")
    }

    /// Convert a Linear issue to our common Issue type.
    fn convert_issue(issue: &LinearIssue) -> Issue {
        let status = Self::map_status(&issue.state);
        let priority = Self::map_priority(issue.priority);
        let assignee = issue
            .assignee
            .as_ref()
            .map(|a| a.display_name.as_deref().unwrap_or(&a.name).to_string());
        let labels: Vec<String> = issue.labels.nodes.iter().map(|l| l.name.clone()).collect();
        let sprint = issue.cycle.as_ref().and_then(|c| c.name.clone());

        Issue {
            id: issue.id.clone(),
            key: issue.identifier.clone(),
            title: issue.title.clone(),
            description: issue.description.clone(),
            status,
            priority,
            assignee,
            labels,
            sprint,
            created_at: issue
                .created_at
                .as_deref()
                .and_then(Self::parse_datetime),
            updated_at: issue
                .updated_at
                .as_deref()
                .and_then(Self::parse_datetime),
            platform: PMPlatform::Linear,
            url: issue.url.clone(),
        }
    }

    /// Map a Linear workflow state to our IssueStatus enum.
    ///
    /// Linear states have a type field: backlog, unstarted, started, completed, cancelled.
    fn map_status(state: &Option<LinearWorkflowState>) -> IssueStatus {
        let Some(state) = state else {
            return IssueStatus::Todo;
        };

        // Linear has a `type` field on workflow states.
        if let Some(ref state_type) = state.state_type {
            return match state_type.as_str() {
                "backlog" => IssueStatus::Backlog,
                "unstarted" => IssueStatus::Todo,
                "started" => IssueStatus::InProgress,
                "completed" => IssueStatus::Done,
                "cancelled" | "canceled" => IssueStatus::Cancelled,
                _ => IssueStatus::Todo,
            };
        }

        // Fallback to name matching.
        let name = state.name.to_lowercase();
        match name.as_str() {
            "backlog" | "triage" => IssueStatus::Backlog,
            "todo" | "to do" | "unstarted" => IssueStatus::Todo,
            "in progress" | "in development" | "started" => IssueStatus::InProgress,
            "in review" | "review" | "code review" => IssueStatus::InReview,
            "done" | "completed" | "closed" => IssueStatus::Done,
            "cancelled" | "canceled" => IssueStatus::Cancelled,
            _ => IssueStatus::Todo,
        }
    }

    /// Map a Linear numeric priority (0=none, 1=urgent, 2=high, 3=medium, 4=low) to IssuePriority.
    fn map_priority(priority: Option<f64>) -> IssuePriority {
        match priority.map(|p| p as u32) {
            Some(0) => IssuePriority::None,
            Some(1) => IssuePriority::Critical,
            Some(2) => IssuePriority::High,
            Some(3) => IssuePriority::Medium,
            Some(4) => IssuePriority::Low,
            _ => IssuePriority::None,
        }
    }

    /// Map our IssuePriority to a Linear numeric priority.
    fn priority_to_linear(priority: IssuePriority) -> u32 {
        match priority {
            IssuePriority::Critical => 1,
            IssuePriority::High => 2,
            IssuePriority::Medium => 3,
            IssuePriority::Low => 4,
            IssuePriority::None => 0,
        }
    }

    /// Map our IssueStatus to the Linear state type string.
    fn status_to_linear_type(status: IssueStatus) -> &'static str {
        match status {
            IssueStatus::Backlog => "backlog",
            IssueStatus::Todo => "unstarted",
            IssueStatus::InProgress | IssueStatus::InReview => "started",
            IssueStatus::Done => "completed",
            IssueStatus::Cancelled => "cancelled",
        }
    }

    /// Parse an ISO 8601 datetime string to `DateTime<Utc>`.
    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Build a GraphQL filter object from our IssueFilters.
    fn build_filter_json(filters: &IssueFilters) -> serde_json::Value {
        let mut filter = serde_json::Map::new();

        if let Some(ref status) = filters.status {
            let state_type = Self::status_to_linear_type(*status);
            filter.insert(
                "state".into(),
                serde_json::json!({ "type": { "eq": state_type } }),
            );
        }

        if let Some(ref assignee) = filters.assignee {
            filter.insert(
                "assignee".into(),
                serde_json::json!({ "name": { "eq": assignee } }),
            );
        }

        if let Some(ref priority) = filters.priority {
            let num = Self::priority_to_linear(*priority);
            filter.insert(
                "priority".into(),
                serde_json::json!({ "eq": num }),
            );
        }

        if !filters.labels.is_empty() {
            filter.insert(
                "labels".into(),
                serde_json::json!({
                    "some": {
                        "name": { "in": filters.labels }
                    }
                }),
            );
        }

        if let Some(ref cycle_id) = filters.sprint_id {
            filter.insert(
                "cycle".into(),
                serde_json::json!({ "id": { "eq": cycle_id } }),
            );
        }

        serde_json::Value::Object(filter)
    }

    /// Find a workflow state ID matching the given target status on the team
    /// that owns the issue.
    async fn find_state_id_for_issue(
        &self,
        issue_id: &str,
        target_status: IssueStatus,
    ) -> Result<String> {
        let target_type = Self::status_to_linear_type(target_status);

        let query = r#"
            query IssueTeamStates($issueId: String!) {
                issue(id: $issueId) {
                    team {
                        states {
                            nodes {
                                id
                                name
                                type
                            }
                        }
                    }
                }
            }
        "#;

        #[derive(Debug, Deserialize)]
        struct IssueTeamStatesData {
            issue: IssueTeamStatesIssue,
        }

        #[derive(Debug, Deserialize)]
        struct IssueTeamStatesIssue {
            team: IssueTeamStatesTeam,
        }

        #[derive(Debug, Deserialize)]
        struct IssueTeamStatesTeam {
            states: LinearConnection<LinearWorkflowState>,
        }

        let vars = serde_json::json!({ "issueId": issue_id });
        let data: IssueTeamStatesData = self.graphql(query, Some(vars)).await?;

        // Find a state matching the target type.
        data.issue
            .team
            .states
            .nodes
            .iter()
            .find(|s| s.state_type.as_deref() == Some(target_type))
            .map(|s| s.id.clone())
            .context(format!(
                "no workflow state of type '{}' found for issue {}",
                target_type, issue_id
            ))
    }
}

// ── GraphQL query fragments ────────────────────────────────────────

const ISSUE_FIELDS: &str = r#"
    id
    identifier
    title
    description
    url
    priority
    priorityLabel
    createdAt
    updatedAt
    state {
        id
        name
        type
    }
    assignee {
        id
        name
        displayName
    }
    labels {
        nodes {
            name
        }
    }
    cycle {
        id
        name
    }
"#;

#[async_trait]
impl ProjectManagementProvider for LinearClient {
    fn platform(&self) -> PMPlatform {
        PMPlatform::Linear
    }

    async fn list_projects(&self) -> Result<Vec<Project>> {
        let query = r#"
            query {
                teams {
                    nodes {
                        id
                        name
                        key
                        description
                    }
                }
            }
        "#;

        let data: TeamsData = self.graphql(query, None).await?;

        Ok(data
            .teams
            .nodes
            .into_iter()
            .map(|t| Project {
                id: t.id,
                name: t.name,
                key: Some(t.key),
                description: t.description,
                platform: PMPlatform::Linear,
            })
            .collect())
    }

    async fn list_issues(
        &self,
        project_id: &str,
        filters: &IssueFilters,
    ) -> Result<Vec<Issue>> {
        let filter = Self::build_filter_json(filters);

        let query = format!(
            r#"
            query TeamIssues($teamId: String!, $filter: IssueFilter) {{
                team(id: $teamId) {{
                    issues(first: 50, filter: $filter, orderBy: updatedAt) {{
                        nodes {{
                            {ISSUE_FIELDS}
                        }}
                    }}
                }}
            }}
            "#
        );

        let vars = serde_json::json!({
            "teamId": project_id,
            "filter": filter,
        });

        let data: TeamIssuesData = self.graphql(&query, Some(vars)).await?;

        let issues = data
            .team
            .map(|t| t.issues.nodes)
            .unwrap_or_default();

        Ok(issues.iter().map(Self::convert_issue).collect())
    }

    async fn get_issue(&self, issue_id: &str) -> Result<Issue> {
        let query = format!(
            r#"
            query GetIssue($issueId: String!) {{
                issue(id: $issueId) {{
                    {ISSUE_FIELDS}
                }}
            }}
            "#
        );

        let vars = serde_json::json!({ "issueId": issue_id });
        let data: IssueData = self.graphql(&query, Some(vars)).await?;

        Ok(Self::convert_issue(&data.issue))
    }

    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<Issue> {
        let query = format!(
            r#"
            mutation CreateIssue($input: IssueCreateInput!) {{
                issueCreate(input: $input) {{
                    success
                    issue {{
                        {ISSUE_FIELDS}
                    }}
                }}
            }}
            "#
        );

        let mut input = serde_json::json!({
            "teamId": request.project_id,
            "title": request.title,
        });

        if let Some(ref desc) = request.description {
            input["description"] = serde_json::json!(desc);
        }

        if let Some(ref priority) = request.priority {
            input["priority"] = serde_json::json!(Self::priority_to_linear(*priority));
        }

        if let Some(ref assignee) = request.assignee {
            input["assigneeId"] = serde_json::json!(assignee);
        }

        if !request.labels.is_empty() {
            input["labelIds"] = serde_json::json!(request.labels);
        }

        let vars = serde_json::json!({ "input": input });
        let data: IssueCreateData = self.graphql(&query, Some(vars)).await?;

        if !data.issue_create.success {
            anyhow::bail!("Linear issue creation failed");
        }

        data.issue_create
            .issue
            .as_ref()
            .map(Self::convert_issue)
            .context("Linear returned success but no issue data")
    }

    async fn update_issue(&self, issue_id: &str, update: &IssueUpdate) -> Result<Issue> {
        let query = format!(
            r#"
            mutation UpdateIssue($issueId: String!, $input: IssueUpdateInput!) {{
                issueUpdate(id: $issueId, input: $input) {{
                    success
                    issue {{
                        {ISSUE_FIELDS}
                    }}
                }}
            }}
            "#
        );

        let mut input = serde_json::Map::new();

        if let Some(ref title) = update.title {
            input.insert("title".into(), serde_json::json!(title));
        }

        if let Some(ref desc) = update.description {
            input.insert("description".into(), serde_json::json!(desc));
        }

        if let Some(ref priority) = update.priority {
            input.insert(
                "priority".into(),
                serde_json::json!(Self::priority_to_linear(*priority)),
            );
        }

        if let Some(ref assignee) = update.assignee {
            input.insert("assigneeId".into(), serde_json::json!(assignee));
        }

        if let Some(ref labels) = update.labels {
            input.insert("labelIds".into(), serde_json::json!(labels));
        }

        if let Some(status) = update.status {
            let state_id = self.find_state_id_for_issue(issue_id, status).await?;
            input.insert("stateId".into(), serde_json::json!(state_id));
        }

        let vars = serde_json::json!({
            "issueId": issue_id,
            "input": serde_json::Value::Object(input),
        });

        let data: IssueUpdateData = self.graphql(&query, Some(vars)).await?;

        if !data.issue_update.success {
            anyhow::bail!("Linear issue update failed");
        }

        data.issue_update
            .issue
            .as_ref()
            .map(Self::convert_issue)
            .context("Linear returned success but no issue data")
    }

    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<Comment> {
        let query = r#"
            mutation CreateComment($input: CommentCreateInput!) {
                commentCreate(input: $input) {
                    success
                    comment {
                        id
                        body
                        createdAt
                        user {
                            id
                            name
                            displayName
                        }
                    }
                }
            }
        "#;

        let vars = serde_json::json!({
            "input": {
                "issueId": issue_id,
                "body": body,
            }
        });

        let data: CommentCreateData = self.graphql(query, Some(vars)).await?;

        if !data.comment_create.success {
            anyhow::bail!("Linear comment creation failed");
        }

        let comment = data
            .comment_create
            .comment
            .context("Linear returned success but no comment data")?;

        Ok(Comment {
            id: comment.id,
            author: comment
                .user
                .map(|u| u.display_name.unwrap_or(u.name)),
            body: comment.body,
            created_at: comment
                .created_at
                .as_deref()
                .and_then(Self::parse_datetime),
        })
    }

    async fn transition_issue(&self, issue_id: &str, status: IssueStatus) -> Result<Issue> {
        let state_id = self.find_state_id_for_issue(issue_id, status).await?;

        let update = IssueUpdate {
            status: None, // We set stateId directly below.
            ..Default::default()
        };

        let query = format!(
            r#"
            mutation UpdateIssue($issueId: String!, $input: IssueUpdateInput!) {{
                issueUpdate(id: $issueId, input: $input) {{
                    success
                    issue {{
                        {ISSUE_FIELDS}
                    }}
                }}
            }}
            "#
        );

        let vars = serde_json::json!({
            "issueId": issue_id,
            "input": {
                "stateId": state_id,
            }
        });

        let data: IssueUpdateData = self.graphql(&query, Some(vars)).await?;

        if !data.issue_update.success {
            anyhow::bail!("Linear issue transition failed");
        }

        let _ = update; // Consumed for clarity.

        data.issue_update
            .issue
            .as_ref()
            .map(Self::convert_issue)
            .context("Linear returned success but no issue data")
    }

    async fn search_issues(&self, query: &str, limit: u32) -> Result<Vec<Issue>> {
        let gql_query = format!(
            r#"
            query SearchIssues($query: String!, $first: Int) {{
                issueSearch(query: $query, first: $first) {{
                    nodes {{
                        {ISSUE_FIELDS}
                    }}
                }}
            }}
            "#
        );

        let vars = serde_json::json!({
            "query": query,
            "first": limit,
        });

        let data: IssueSearchData = self.graphql(&gql_query, Some(vars)).await?;

        Ok(data
            .issue_search
            .nodes
            .iter()
            .map(Self::convert_issue)
            .collect())
    }

    async fn get_sprints(&self, project_id: &str) -> Result<Vec<Sprint>> {
        let query = r#"
            query TeamCycles($teamId: String!) {
                team(id: $teamId) {
                    cycles(first: 20, orderBy: createdAt) {
                        nodes {
                            id
                            name
                            number
                            startsAt
                            endsAt
                            completedAt
                        }
                    }
                }
            }
        "#;

        let vars = serde_json::json!({ "teamId": project_id });
        let data: CyclesData = match self.graphql(query, Some(vars)).await {
            Ok(d) => d,
            Err(e) => {
                warn!(
                    project_id = %project_id,
                    error = %e,
                    "failed to fetch Linear cycles"
                );
                return Ok(vec![]);
            }
        };

        let cycles = data
            .team
            .map(|t| t.cycles.nodes)
            .unwrap_or_default();

        Ok(cycles
            .into_iter()
            .map(|c| {
                let state = if c.completed_at.is_some() {
                    Some("completed".into())
                } else if c.starts_at.is_some() {
                    Some("active".into())
                } else {
                    Some("future".into())
                };

                let name = c
                    .name
                    .unwrap_or_else(|| format!("Cycle {}", c.number.unwrap_or(0)));

                Sprint {
                    id: c.id,
                    name,
                    state,
                    start_date: c.starts_at.as_deref().and_then(Self::parse_datetime),
                    end_date: c.ends_at.as_deref().and_then(Self::parse_datetime),
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn make_client() -> LinearClient {
        LinearClient::with_base_url("lin_test_key_123", "https://api.linear.app/graphql").unwrap()
    }

    #[test]
    fn test_new_sets_default_base_url() {
        let client = LinearClient::new("lin_key").unwrap();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_custom_base_url_strips_trailing_slash() {
        let client =
            LinearClient::with_base_url("lin_key", "https://api.linear.app/graphql/").unwrap();
        assert_eq!(client.base_url(), "https://api.linear.app/graphql");
    }

    #[test]
    fn test_api_key_stored() {
        let client = make_client();
        assert_eq!(client.api_key(), "lin_test_key_123");
    }

    #[test]
    fn test_platform() {
        let client = make_client();
        assert_eq!(client.platform(), PMPlatform::Linear);
    }

    #[test]
    fn test_invalid_api_key_rejected() {
        let result = LinearClient::new("key\nwith\nnewlines");
        assert!(result.is_err());
    }

    #[test]
    fn test_map_priority_urgent() {
        assert_eq!(LinearClient::map_priority(Some(1.0)), IssuePriority::Critical);
    }

    #[test]
    fn test_map_priority_high() {
        assert_eq!(LinearClient::map_priority(Some(2.0)), IssuePriority::High);
    }

    #[test]
    fn test_map_priority_medium() {
        assert_eq!(LinearClient::map_priority(Some(3.0)), IssuePriority::Medium);
    }

    #[test]
    fn test_map_priority_low() {
        assert_eq!(LinearClient::map_priority(Some(4.0)), IssuePriority::Low);
    }

    #[test]
    fn test_map_priority_none() {
        assert_eq!(LinearClient::map_priority(Some(0.0)), IssuePriority::None);
        assert_eq!(LinearClient::map_priority(None), IssuePriority::None);
    }

    #[test]
    fn test_priority_to_linear() {
        assert_eq!(LinearClient::priority_to_linear(IssuePriority::Critical), 1);
        assert_eq!(LinearClient::priority_to_linear(IssuePriority::High), 2);
        assert_eq!(LinearClient::priority_to_linear(IssuePriority::Medium), 3);
        assert_eq!(LinearClient::priority_to_linear(IssuePriority::Low), 4);
        assert_eq!(LinearClient::priority_to_linear(IssuePriority::None), 0);
    }

    #[test]
    fn test_map_status_backlog() {
        let state = Some(LinearWorkflowState {
            id: "s1".into(),
            name: "Backlog".into(),
            state_type: Some("backlog".into()),
        });
        assert_eq!(LinearClient::map_status(&state), IssueStatus::Backlog);
    }

    #[test]
    fn test_map_status_unstarted() {
        let state = Some(LinearWorkflowState {
            id: "s2".into(),
            name: "Todo".into(),
            state_type: Some("unstarted".into()),
        });
        assert_eq!(LinearClient::map_status(&state), IssueStatus::Todo);
    }

    #[test]
    fn test_map_status_started() {
        let state = Some(LinearWorkflowState {
            id: "s3".into(),
            name: "In Progress".into(),
            state_type: Some("started".into()),
        });
        assert_eq!(LinearClient::map_status(&state), IssueStatus::InProgress);
    }

    #[test]
    fn test_map_status_completed() {
        let state = Some(LinearWorkflowState {
            id: "s4".into(),
            name: "Done".into(),
            state_type: Some("completed".into()),
        });
        assert_eq!(LinearClient::map_status(&state), IssueStatus::Done);
    }

    #[test]
    fn test_map_status_cancelled() {
        let state = Some(LinearWorkflowState {
            id: "s5".into(),
            name: "Cancelled".into(),
            state_type: Some("cancelled".into()),
        });
        assert_eq!(LinearClient::map_status(&state), IssueStatus::Cancelled);
    }

    #[test]
    fn test_map_status_by_name_fallback() {
        let state = Some(LinearWorkflowState {
            id: "s6".into(),
            name: "In Review".into(),
            state_type: None,
        });
        assert_eq!(LinearClient::map_status(&state), IssueStatus::InReview);
    }

    #[test]
    fn test_map_status_none() {
        assert_eq!(LinearClient::map_status(&None), IssueStatus::Todo);
    }

    #[test]
    fn test_status_to_linear_type() {
        assert_eq!(LinearClient::status_to_linear_type(IssueStatus::Backlog), "backlog");
        assert_eq!(LinearClient::status_to_linear_type(IssueStatus::Todo), "unstarted");
        assert_eq!(
            LinearClient::status_to_linear_type(IssueStatus::InProgress),
            "started"
        );
        assert_eq!(LinearClient::status_to_linear_type(IssueStatus::InReview), "started");
        assert_eq!(LinearClient::status_to_linear_type(IssueStatus::Done), "completed");
        assert_eq!(
            LinearClient::status_to_linear_type(IssueStatus::Cancelled),
            "cancelled"
        );
    }

    #[test]
    fn test_convert_issue() {
        let linear_issue = LinearIssue {
            id: "issue-1".into(),
            identifier: Some("ENG-42".into()),
            title: "Fix auth flow".into(),
            description: Some("The auth flow is broken".into()),
            state: Some(LinearWorkflowState {
                id: "state-1".into(),
                name: "In Progress".into(),
                state_type: Some("started".into()),
            }),
            priority: Some(2.0),
            priority_label: Some("High".into()),
            assignee: Some(LinearUser {
                id: "user-1".into(),
                name: "alice".into(),
                display_name: Some("Alice Smith".into()),
            }),
            labels: LinearLabelConnection {
                nodes: vec![LinearLabel {
                    name: "bug".into(),
                }],
            },
            cycle: Some(LinearCycleRef {
                id: "cycle-1".into(),
                name: Some("Cycle 5".into()),
            }),
            created_at: Some("2024-01-15T10:30:00.000Z".into()),
            updated_at: Some("2024-01-16T14:00:00.000Z".into()),
            url: Some("https://linear.app/team/issue/ENG-42".into()),
        };

        let issue = LinearClient::convert_issue(&linear_issue);
        assert_eq!(issue.id, "issue-1");
        assert_eq!(issue.key.as_deref(), Some("ENG-42"));
        assert_eq!(issue.title, "Fix auth flow");
        assert_eq!(issue.status, IssueStatus::InProgress);
        assert_eq!(issue.priority, IssuePriority::High);
        assert_eq!(issue.assignee.as_deref(), Some("Alice Smith"));
        assert_eq!(issue.labels, vec!["bug"]);
        assert_eq!(issue.sprint.as_deref(), Some("Cycle 5"));
        assert_eq!(issue.platform, PMPlatform::Linear);
        assert!(issue.url.is_some());
        assert!(issue.created_at.is_some());
        assert!(issue.updated_at.is_some());
    }

    #[test]
    fn test_build_filter_json_empty() {
        let filters = IssueFilters::default();
        let json = LinearClient::build_filter_json(&filters);
        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn test_build_filter_json_with_status() {
        let filters = IssueFilters {
            status: Some(IssueStatus::InProgress),
            ..Default::default()
        };
        let json = LinearClient::build_filter_json(&filters);
        assert!(json.get("state").is_some());
    }

    #[test]
    fn test_build_filter_json_with_labels() {
        let filters = IssueFilters {
            labels: vec!["bug".into(), "urgent".into()],
            ..Default::default()
        };
        let json = LinearClient::build_filter_json(&filters);
        assert!(json.get("labels").is_some());
    }

    #[test]
    fn test_build_filter_json_with_priority() {
        let filters = IssueFilters {
            priority: Some(IssuePriority::High),
            ..Default::default()
        };
        let json = LinearClient::build_filter_json(&filters);
        assert!(json.get("priority").is_some());
    }

    #[test]
    fn test_parse_datetime_valid() {
        let dt = LinearClient::parse_datetime("2024-01-15T10:30:00.000Z");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2024);
    }

    #[test]
    fn test_parse_datetime_invalid() {
        let dt = LinearClient::parse_datetime("not-a-date");
        assert!(dt.is_none());
    }

    #[test]
    fn test_graphql_error_deserialization() {
        let json = r#"{"message": "Not found"}"#;
        let err: GraphQLError = serde_json::from_str(json).unwrap();
        assert_eq!(err.message, "Not found");
    }

    #[test]
    fn test_linear_team_deserialization() {
        let json = r#"{
            "id": "team-1",
            "name": "Engineering",
            "key": "ENG",
            "description": "Engineering team"
        }"#;
        let team: LinearTeam = serde_json::from_str(json).unwrap();
        assert_eq!(team.id, "team-1");
        assert_eq!(team.name, "Engineering");
        assert_eq!(team.key, "ENG");
    }
}
