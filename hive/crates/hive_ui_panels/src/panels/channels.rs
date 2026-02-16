//! AI Agent Messaging Channels — Telegram/WhatsApp-style channel UI where
//! users chat with multiple AI agents in persistent conversations.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;

use hive_core::channels::{ChannelMessage, ChannelStore, MessageAuthor};
use hive_ui_core::HiveTheme;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when the user sends a message in a channel, so the workspace can
/// trigger AI agent responses.
#[derive(Debug, Clone)]
pub struct ChannelMessageSent {
    pub channel_id: String,
    pub content: String,
    pub assigned_agents: Vec<String>,
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

/// Item for the channel list sidebar.
#[derive(Debug, Clone)]
struct ChannelListItem {
    id: String,
    name: String,
    icon: String,
    description: String,
    message_count: usize,
    assigned_agents: Vec<String>,
}

pub struct ChannelsView {
    theme: HiveTheme,

    // Channel list
    channels: Vec<ChannelListItem>,
    active_channel_id: Option<String>,

    // Messages for the active channel
    messages: Vec<ChannelMessageDisplay>,

    // Input state
    message_input: String,

    // Streaming
    is_streaming: bool,
    streaming_content: String,
    streaming_agent: Option<String>,

    // UI state
    show_channel_list: bool,
    create_channel_mode: bool,
    new_channel_name: String,
}

/// Display-ready message.
#[derive(Debug, Clone)]
struct ChannelMessageDisplay {
    id: String,
    author_name: String,
    author_icon: String,
    author_color: Hsla,
    content: String,
    timestamp: String,
    is_agent: bool,
    model: Option<String>,
}

impl EventEmitter<ChannelMessageSent> for ChannelsView {}

impl ChannelsView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            theme: HiveTheme::dark(),
            channels: Vec::new(),
            active_channel_id: None,
            messages: Vec::new(),
            message_input: String::new(),
            is_streaming: false,
            streaming_content: String::new(),
            streaming_agent: None,
            show_channel_list: true,
            create_channel_mode: false,
            new_channel_name: String::new(),
        }
    }

    /// Refresh channel list from pre-extracted data (avoids borrow issues with
    /// globals). Tuple: (id, name, icon, description, message_count, assigned_agents).
    pub fn refresh_from_data(
        &mut self,
        data: Vec<(String, String, String, String, usize, Vec<String>)>,
        cx: &mut Context<Self>,
    ) {
        self.channels = data
            .into_iter()
            .map(|(id, name, icon, description, message_count, assigned_agents)| {
                ChannelListItem {
                    id,
                    name,
                    icon,
                    description,
                    message_count,
                    assigned_agents,
                }
            })
            .collect();

        // Auto-select first channel if none selected
        if self.active_channel_id.is_none() {
            if let Some(first) = self.channels.first() {
                self.active_channel_id = Some(first.id.clone());
            }
        }
        cx.notify();
    }

    /// Refresh the channel list from the store.
    pub fn refresh_channels(&mut self, store: &ChannelStore, cx: &mut Context<Self>) {
        self.channels = store
            .list_channels()
            .iter()
            .map(|c| ChannelListItem {
                id: c.id.clone(),
                name: c.name.clone(),
                icon: c.icon.clone(),
                description: c.description.clone(),
                message_count: c.messages.len(),
                assigned_agents: c.assigned_agents.clone(),
            })
            .collect();

        // Auto-select first channel if none selected
        if self.active_channel_id.is_none() {
            if let Some(first) = self.channels.first() {
                self.active_channel_id = Some(first.id.clone());
                self.load_channel_messages(store);
            }
        }
        cx.notify();
    }

    /// Load messages for the active channel.
    pub fn load_channel_messages(&mut self, store: &ChannelStore) {
        self.messages.clear();
        if let Some(ref id) = self.active_channel_id {
            if let Some(channel) = store.get_channel(id) {
                self.messages = channel
                    .messages
                    .iter()
                    .map(|m| self.message_to_display(m))
                    .collect();
            }
        }
    }

    /// Switch to a different channel.
    pub fn switch_channel(&mut self, channel_id: &str, store: &ChannelStore, cx: &mut Context<Self>) {
        self.active_channel_id = Some(channel_id.to_string());
        self.load_channel_messages(store);
        cx.notify();
    }

    /// Append a message to the current view and notify.
    pub fn append_message(&mut self, msg: &ChannelMessage, cx: &mut Context<Self>) {
        self.messages.push(self.message_to_display(msg));
        cx.notify();
    }

    /// Update streaming state for an agent response.
    pub fn set_streaming(&mut self, agent: &str, content: &str, cx: &mut Context<Self>) {
        self.is_streaming = true;
        self.streaming_agent = Some(agent.to_string());
        self.streaming_content = content.to_string();
        cx.notify();
    }

    /// Finalize streaming.
    pub fn finish_streaming(&mut self, cx: &mut Context<Self>) {
        self.is_streaming = false;
        self.streaming_content.clear();
        self.streaming_agent = None;
        cx.notify();
    }

    fn message_to_display(&self, msg: &ChannelMessage) -> ChannelMessageDisplay {
        let (author_name, author_icon, author_color, is_agent) = match &msg.author {
            MessageAuthor::User => (
                "You".to_string(),
                "\u{1F464}".to_string(),
                self.theme.accent_aqua,
                false,
            ),
            MessageAuthor::Agent { persona } => {
                let color = self.agent_color(persona);
                (
                    persona.clone(),
                    self.agent_icon(persona),
                    color,
                    true,
                )
            }
            MessageAuthor::System => (
                "System".to_string(),
                "\u{2699}".to_string(),
                self.theme.text_muted,
                false,
            ),
        };

        ChannelMessageDisplay {
            id: msg.id.clone(),
            author_name,
            author_icon,
            author_color,
            content: msg.content.clone(),
            timestamp: msg.timestamp.format("%H:%M").to_string(),
            is_agent,
            model: msg.model.clone(),
        }
    }

    fn agent_color(&self, persona: &str) -> Hsla {
        match persona {
            "Investigate" => self.theme.accent_powder,
            "Implement" => self.theme.accent_cyan,
            "Verify" => self.theme.accent_green,
            "Critique" => self.theme.accent_yellow,
            "Debug" => self.theme.accent_pink,
            "CodeReview" => self.theme.accent_aqua,
            _ => self.theme.text_secondary,
        }
    }

    fn agent_icon(&self, persona: &str) -> String {
        match persona {
            "Investigate" => "\u{1F50D}",
            "Implement" => "\u{1F528}",
            "Verify" => "\u{2705}",
            "Critique" => "\u{1F9D0}",
            "Debug" => "\u{1F41B}",
            "CodeReview" => "\u{1F4CB}",
            _ => "\u{1F916}",
        }
        .to_string()
    }

    // -- Render helpers -------------------------------------------------------

    fn render_channel_list(&self, theme: &HiveTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let mut items: Vec<AnyElement> = Vec::new();

        for channel in &self.channels {
            let is_active = self.active_channel_id.as_deref() == Some(&channel.id);
            let channel_id = channel.id.clone();

            items.push(
                div()
                    .id(ElementId::Name(format!("channel-{}", channel.id).into()))
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    .px(theme.space_2)
                    .py(theme.space_2)
                    .rounded(theme.radius_md)
                    .cursor_pointer()
                    .when(is_active, |el| el.bg(theme.bg_surface))
                    .hover(|s| s.bg(theme.bg_tertiary))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e, _w, cx| {
                            this.active_channel_id = Some(channel_id.clone());
                            // Messages will be loaded when workspace detects the switch
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .text_size(px(18.0))
                            .child(channel.icon.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(if is_active {
                                        theme.text_primary
                                    } else {
                                        theme.text_secondary
                                    })
                                    .font_weight(if is_active {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(channel.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(format!("{} msgs", channel.message_count)),
                            ),
                    )
                    .into_any_element(),
            );
        }

        div()
            .flex()
            .flex_col()
            .w(px(240.0))
            .min_w(px(240.0))
            .border_r_1()
            .border_color(theme.border)
            .child(
                // Header
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(theme.space_3)
                    .py(theme.space_3)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .font_weight(FontWeight::BOLD)
                            .child("CHANNELS"),
                    )
                    .child(
                        div()
                            .id("new-channel-btn")
                            .text_size(theme.font_size_sm)
                            .text_color(theme.accent_cyan)
                            .cursor_pointer()
                            .child("+"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .p(theme.space_2)
                    .gap(theme.space_1)
                    .children(items),
            )
    }

    fn render_message_area(&self, theme: &HiveTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let active_channel = self
            .channels
            .iter()
            .find(|c| self.active_channel_id.as_deref() == Some(&c.id));

        // Channel header
        let header = if let Some(channel) = active_channel {
            div()
                .flex()
                .items_center()
                .gap(theme.space_2)
                .px(theme.space_4)
                .py(theme.space_3)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(px(20.0))
                        .child(channel.icon.clone()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(theme.font_size_base)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::BOLD)
                                .child(channel.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(format!(
                                    "{} agents \u{00B7} {}",
                                    channel.assigned_agents.len(),
                                    channel.description
                                )),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .px(theme.space_4)
                .py(theme.space_3)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Select a channel to start chatting"),
                )
                .into_any_element()
        };

        // Message list
        let mut message_elements: Vec<AnyElement> = Vec::new();
        for msg in &self.messages {
            message_elements.push(self.render_message(msg, theme));
        }

        // Streaming indicator
        if self.is_streaming {
            if let Some(ref agent) = self.streaming_agent {
                let color = self.agent_color(agent);
                message_elements.push(
                    div()
                        .flex()
                        .gap(theme.space_2)
                        .px(theme.space_4)
                        .py(theme.space_2)
                        .child(
                            div()
                                .w(px(28.0))
                                .h(px(28.0))
                                .rounded(theme.radius_full)
                                .bg(color)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(14.0))
                                .child(self.agent_icon(agent)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_size(theme.font_size_xs)
                                        .text_color(color)
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!("{} is typing...", agent)),
                                )
                                .when(!self.streaming_content.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_size(theme.font_size_sm)
                                            .text_color(theme.text_secondary)
                                            .child(self.streaming_content.clone()),
                                    )
                                }),
                        )
                        .into_any_element(),
                );
            }
        }

        // Empty state
        if self.messages.is_empty() && !self.is_streaming {
            message_elements.push(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .gap(theme.space_3)
                    .child(
                        div()
                            .text_size(px(48.0))
                            .child("\u{1F4AC}"),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_base)
                            .text_color(theme.text_muted)
                            .child("No messages yet"),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_muted)
                            .child("Send a message to start chatting with AI agents"),
                    )
                    .into_any_element(),
            );
        }

        // Input area
        let input_area = div()
            .flex()
            .items_center()
            .gap(theme.space_2)
            .px(theme.space_4)
            .py(theme.space_3)
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("channel-msg-input")
                    .flex_1()
                    .min_w(px(0.0))
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_md)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_sm)
                    .text_color(if self.message_input.is_empty() {
                        theme.text_muted
                    } else {
                        theme.text_primary
                    })
                    .child(if self.message_input.is_empty() {
                        "Type a message...".to_string()
                    } else {
                        self.message_input.clone()
                    }),
            )
            .child(
                div()
                    .id("channel-send-btn")
                    .px(theme.space_3)
                    .py(theme.space_2)
                    .rounded(theme.radius_md)
                    .bg(theme.accent_cyan)
                    .text_size(theme.font_size_sm)
                    .text_color(theme.bg_primary)
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .child("Send"),
            );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .p(theme.space_2)
                    .gap(theme.space_1)
                    .children(message_elements),
            )
            .child(input_area)
    }

    fn render_message(&self, msg: &ChannelMessageDisplay, theme: &HiveTheme) -> AnyElement {
        let mut bg = msg.author_color;
        bg.a = 0.08;

        div()
            .flex()
            .gap(theme.space_2)
            .px(theme.space_4)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .hover(|s| s.bg(theme.bg_tertiary))
            .child(
                // Avatar
                div()
                    .w(px(28.0))
                    .h(px(28.0))
                    .rounded(theme.radius_full)
                    .bg(bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.0))
                    .child(msg.author_icon.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(msg.author_color)
                                    .font_weight(FontWeight::BOLD)
                                    .child(msg.author_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(msg.timestamp.clone()),
                            )
                            .when_some(msg.model.as_ref(), |el, model| {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(theme.text_muted)
                                        .px(theme.space_1)
                                        .rounded(theme.radius_sm)
                                        .bg(theme.bg_tertiary)
                                        .child(model.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_primary)
                            .mt(theme.space_1)
                            .child(msg.content.clone()),
                    ),
            )
            .into_any_element()
    }

    fn render_agent_presence(&self, theme: &HiveTheme) -> impl IntoElement {
        let active_channel = self
            .channels
            .iter()
            .find(|c| self.active_channel_id.as_deref() == Some(&c.id));

        let agents = active_channel
            .map(|c| c.assigned_agents.clone())
            .unwrap_or_default();

        div()
            .w(px(200.0))
            .min_w(px(200.0))
            .border_l_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_3)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .font_weight(FontWeight::BOLD)
                            .child("AGENTS"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(theme.space_2)
                    .gap(theme.space_2)
                    .children(agents.iter().map(|agent| {
                        let color = self.agent_color(agent);

                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_2)
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded(theme.radius_full)
                                    .bg(color),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(color)
                                    .child(agent.clone()),
                            )
                            .into_any_element()
                    })),
            )
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for ChannelsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;

        div()
            .id("channels-panel")
            .flex()
            .size_full()
            .child(self.render_channel_list(theme, cx))
            .child(self.render_message_area(theme, cx))
            .child(self.render_agent_presence(theme))
    }
}
