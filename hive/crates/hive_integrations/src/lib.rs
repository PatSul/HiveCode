pub mod bitbucket;
pub mod browser;
pub mod clawdtalk;
pub mod cloud;
pub mod database;
pub mod docker;
pub mod docs_indexer;
pub mod github;
pub mod gitlab;
pub mod google;
pub mod ide;
pub mod knowledge;
pub mod kubernetes;
pub mod messaging;
pub mod microsoft;
pub mod project_management;
pub mod oauth;
pub mod oauth_callback;
pub mod smart_home;
pub mod webhooks;

pub use bitbucket::BitbucketClient;
pub use browser::BrowserAutomation;
pub use cloud::{AwsClient, AzureClient, CloudflareClient, GcpClient, SupabaseClient, VercelClient};
pub use database::{DatabaseHub, DatabaseProvider, DatabaseType};
pub use gitlab::GitLabClient;
pub use github::GitHubClient;
pub use google::{
    Attendee, CalendarEvent, CalendarList, CalendarListEntry, ClassificationResult, Contact,
    CreateEventRequest, Document, DriveFile, DriveFileList, EmailCategory, EmailClassifier,
    EmailList, EmailMessage, EventDateTime, EventList, FreeBusyRequest, FreeBusyResponse, GTask,
    GmailClient, GoogleCalendarClient, GoogleContactsClient, GoogleDocsClient, GoogleDriveClient,
    GoogleSheetsClient, GoogleTasksClient, SheetValues, Subscription, SubscriptionManager,
    SubscriptionStats, TaskList, UnsubscribeMethod,
};
pub use ide::{
    CommandResult, Diagnostic, DiagnosticSeverity, EditorCommand, IdeIntegrationService, Location,
    Symbol, SymbolKind, WorkspaceInfo,
};
pub use messaging::{
    Attachment, Channel, CrossChannelService, DiscordProvider, IncomingMessage, MatrixProvider,
    MessagingHub, MessagingProvider, Platform, SentMessage, SlackProvider, TeamsProvider,
    TelegramProvider, WebChatProvider,
};
pub use microsoft::outlook_calendar::OutlookCalendarClient;
pub use microsoft::outlook_email::OutlookEmailClient;
pub use project_management::{
    AsanaClient, Comment as PMComment, CreateIssueRequest, Issue, IssueFilters, IssuePriority,
    IssueStatus, IssueUpdate, JiraClient, LinearClient, PMPlatform, Project as PMProject,
    ProjectManagementHub, ProjectManagementProvider, Sprint,
};
pub use knowledge::{
    CreatePageRequest, KBPage, KBPageSummary, KBPlatform, KBSearchResult, KnowledgeBaseProvider,
    KnowledgeHub, NotionClient, ObsidianProvider,
};
pub use oauth::{OAuthClient, OAuthConfig, OAuthToken};
pub use oauth_callback::OAuthCallbackServer;
pub use docker::{
    Container, DockerClient, DockerImage, DockerInfo, Network as DockerNetwork, PortMapping,
    RunContainerRequest, Volume as DockerVolume,
};
pub use docs_indexer::{DocPage, DocSearchResult, DocsIndex, DocsIndexer};
pub use kubernetes::{
    ClusterInfo, Deployment, K8sContext, K8sEvent, K8sService, KubernetesClient,
    Namespace as K8sNamespace, Pod,
};
pub use smart_home::PhilipsHueClient;
pub use webhooks::{Webhook, WebhookRegistry};
