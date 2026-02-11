pub mod cross_channel;
pub mod discord;
pub mod hub;
pub mod matrix;
pub mod provider;
pub mod slack;
pub mod teams;
pub mod telegram;
pub mod webchat;

pub use cross_channel::CrossChannelService;
pub use discord::DiscordProvider;
pub use hub::MessagingHub;
pub use matrix::MatrixProvider;
pub use provider::{
    Attachment, Channel, IncomingMessage, MessagingProvider, Platform, SentMessage,
};
pub use slack::SlackProvider;
pub use teams::TeamsProvider;
pub use telegram::TelegramProvider;
pub use webchat::WebChatProvider;
