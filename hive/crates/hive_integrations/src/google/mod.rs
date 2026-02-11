pub mod calendar;
pub mod contacts;
pub mod docs;
pub mod drive;
pub mod email;
pub mod email_classifier;
pub mod sheets;
pub mod subscription_manager;
pub mod tasks;

pub use calendar::{
    Attendee, CalendarEvent, CalendarList, CalendarListEntry, CreateEventRequest, EventDateTime,
    EventList, FreeBusyCalendar, FreeBusyCalendarInfo, FreeBusyRequest, FreeBusyResponse,
    GoogleCalendarClient, TimePeriod,
};
pub use contacts::{Contact, GoogleContactsClient};
pub use docs::{Document, GoogleDocsClient};
pub use drive::{DriveFile, DriveFileList, GoogleDriveClient};
pub use email::{
    DraftRequest, EmailList, EmailListEntry, EmailMessage, GmailClient, GmailDraft, GmailLabel,
    ModifyLabelsRequest, SendEmailRequest,
};
pub use email_classifier::{ClassificationResult, EmailCategory, EmailClassifier};
pub use sheets::{GoogleSheetsClient, SheetValues};
pub use subscription_manager::{
    Subscription, SubscriptionManager, SubscriptionStats, UnsubscribeMethod,
};
pub use tasks::{GTask, GoogleTasksClient, TaskList};
