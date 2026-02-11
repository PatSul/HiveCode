pub mod clawdtalk;
pub mod cloud;
pub mod github;
pub mod google;
pub mod ide;
pub mod messaging;
pub mod microsoft;
pub mod oauth;
pub mod smart_home;
pub mod webhooks;

pub use cloud::cloudflare::CloudflareClient;
pub use cloud::supabase::SupabaseClient;
pub use cloud::vercel::VercelClient;
pub use github::GitHubClient;
pub use google::{
    Attendee, CalendarEvent, CalendarList, CalendarListEntry, ClassificationResult,
    Contact, CreateEventRequest, Document, DriveFile, DriveFileList, EmailCategory,
    EmailClassifier, EmailList, EmailMessage, EventDateTime, EventList, FreeBusyRequest,
    FreeBusyResponse, GTask, GmailClient, GoogleCalendarClient, GoogleContactsClient,
    GoogleDocsClient, GoogleDriveClient, GoogleSheetsClient, GoogleTasksClient, SheetValues,
    Subscription, SubscriptionManager, SubscriptionStats, TaskList, UnsubscribeMethod,
};
pub use messaging::{
    Attachment, Channel, CrossChannelService, DiscordProvider, IncomingMessage, MatrixProvider,
    MessagingHub, MessagingProvider, Platform, SentMessage, SlackProvider, TeamsProvider,
    TelegramProvider, WebChatProvider,
};
pub use microsoft::outlook_calendar::OutlookCalendarClient;
pub use microsoft::outlook_email::OutlookEmailClient;
pub use oauth::{OAuthClient, OAuthConfig, OAuthToken};
pub use smart_home::PhilipsHueClient;
pub use ide::{
    CommandResult, Diagnostic, DiagnosticSeverity, EditorCommand, IdeIntegrationService, Location,
    Symbol, SymbolKind, WorkspaceInfo,
};
pub use webhooks::{Webhook, WebhookRegistry};
