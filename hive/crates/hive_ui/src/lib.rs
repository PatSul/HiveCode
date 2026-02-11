#![recursion_limit = "1024"]

pub mod chat_service;
pub mod globals;
pub mod theme;
pub mod workspace;
pub mod titlebar;
pub mod sidebar;
pub mod statusbar;
pub mod chat_input;
pub mod welcome;
pub mod panels;
pub mod components;

pub use chat_service::{ChatMessage, ChatService, MessageRole};
pub use globals::*;
pub use workspace::HiveWorkspace;
pub use theme::HiveTheme;
