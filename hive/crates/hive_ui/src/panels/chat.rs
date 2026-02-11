use gpui::*;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind};

use hive_ai::MessageRole;
use crate::chat_service;
use crate::theme::HiveTheme;
use crate::welcome::WelcomeScreen;

// ---------------------------------------------------------------------------
// Display types
// ---------------------------------------------------------------------------

/// A fully-resolved message ready for rendering.
pub struct DisplayMessage {
    pub role: MessageRole,
    pub content: String,
    pub thinking: Option<String>,
    pub model: Option<String>,
    pub cost: Option<f64>,
    pub tokens: Option<u32>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub show_thinking: bool,
}

impl DisplayMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            thinking: None,
            model: None,
            cost: None,
            tokens: None,
            timestamp: chrono::Utc::now(),
            show_thinking: false,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            thinking: None,
            model: None,
            cost: None,
            tokens: None,
            timestamp: chrono::Utc::now(),
            show_thinking: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Error,
            content: content.into(),
            thinking: None,
            model: None,
            cost: None,
            tokens: None,
            timestamp: chrono::Utc::now(),
            show_thinking: false,
        }
    }
}

// ---------------------------------------------------------------------------
// ChatPanel
// ---------------------------------------------------------------------------

/// Chat panel: message list with streaming, markdown, code blocks, thinking indicator.
pub struct ChatPanel {
    pub messages: Vec<DisplayMessage>,
    pub streaming_content: String,
    pub streaming_thinking: Option<String>,
    pub is_streaming: bool,
    pub total_cost: f64,
    pub total_tokens: u32,
    pub current_model: String,
}

impl ChatPanel {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            streaming_content: String::new(),
            streaming_thinking: None,
            is_streaming: false,
            total_cost: 0.0,
            total_tokens: 0,
            current_model: "claude-sonnet-4-5".into(),
        }
    }

    /// Push a completed message and accumulate cost/tokens.
    pub fn push_message(&mut self, msg: DisplayMessage) {
        if let Some(cost) = msg.cost {
            self.total_cost += cost;
        }
        if let Some(tokens) = msg.tokens {
            self.total_tokens += tokens;
        }
        self.messages.push(msg);
    }

    /// Start a new streaming response.
    pub fn start_streaming(&mut self) {
        self.is_streaming = true;
        self.streaming_content.clear();
        self.streaming_thinking = None;
    }

    /// Append a chunk to the current streaming response.
    pub fn append_streaming(&mut self, content: &str, thinking: Option<&str>) {
        self.streaming_content.push_str(content);
        if let Some(t) = thinking {
            self.streaming_thinking
                .get_or_insert_with(String::new)
                .push_str(t);
        }
    }

    /// Finish streaming and convert to a completed message.
    pub fn finish_streaming(
        &mut self,
        model: Option<String>,
        cost: Option<f64>,
        tokens: Option<u32>,
    ) {
        let mut msg = DisplayMessage::assistant(std::mem::take(&mut self.streaming_content));
        msg.thinking = self.streaming_thinking.take();
        msg.model = model;
        msg.cost = cost;
        msg.tokens = tokens;
        self.push_message(msg);
        self.is_streaming = false;
    }

    /// Toggle the thinking section visibility on a message.
    pub fn toggle_thinking(&mut self, index: usize) {
        if let Some(msg) = self.messages.get_mut(index) {
            msg.show_thinking = !msg.show_thinking;
        }
    }

    pub fn render(&self, theme: &HiveTheme) -> AnyElement {
        if self.messages.is_empty() && !self.is_streaming {
            return div()
                .flex_1()
                .size_full()
                .child(WelcomeScreen::render(theme))
                .into_any_element();
        }

        let mut content = div()
            .id("chat-messages")
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_3);

        // Render completed messages
        for msg in &self.messages {
            content = content.child(render_message_bubble(msg, theme));
        }

        // Render streaming bubble
        if self.is_streaming {
            content = content.child(render_streaming_bubble(
                &self.streaming_content,
                self.streaming_thinking.as_deref(),
                &self.current_model,
                theme,
            ));
        }

        // Session totals bar at the bottom
        if self.total_cost > 0.0 || self.total_tokens > 0 {
            content = content.child(render_session_totals(
                self.total_cost,
                self.total_tokens,
                theme,
            ));
        }

        content.into_any_element()
    }

    /// Render the chat panel from ChatService data (used by the workspace).
    ///
    /// Converts `ChatService::ChatMessage` → `DisplayMessage` on the fly and
    /// renders using the same bubble/markdown infrastructure.
    pub fn render_from_service(
        messages: &[chat_service::ChatMessage],
        streaming_content: &str,
        is_streaming: bool,
        current_model: &str,
        theme: &HiveTheme,
    ) -> AnyElement {
        if messages.is_empty() && !is_streaming {
            return div()
                .flex_1()
                .size_full()
                .child(WelcomeScreen::render(theme))
                .into_any_element();
        }

        let mut content = div()
            .id("chat-messages")
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_3);

        // Convert ChatService messages to DisplayMessages and render
        let mut total_cost: f64 = 0.0;
        let mut total_tokens: u32 = 0;

        for msg in messages {
            // Skip empty placeholder assistant messages
            if msg.role == chat_service::MessageRole::Assistant && msg.content.is_empty() {
                continue;
            }
            let display_msg = convert_service_message(msg);
            if let Some(c) = display_msg.cost {
                total_cost += c;
            }
            if let Some(t) = display_msg.tokens {
                total_tokens += t;
            }
            content = content.child(render_message_bubble(&display_msg, theme));
        }

        // Streaming bubble
        if is_streaming {
            content = content.child(render_streaming_bubble(
                streaming_content,
                None,
                current_model,
                theme,
            ));
        }

        // Session totals
        if total_cost > 0.0 || total_tokens > 0 {
            content = content.child(render_session_totals(total_cost, total_tokens, theme));
        }

        content.into_any_element()
    }
}

/// Convert a ChatService message to a DisplayMessage for rendering.
fn convert_service_message(msg: &chat_service::ChatMessage) -> DisplayMessage {
    let role = match msg.role {
        chat_service::MessageRole::User => MessageRole::User,
        chat_service::MessageRole::Assistant => MessageRole::Assistant,
        chat_service::MessageRole::System => MessageRole::System,
        chat_service::MessageRole::Error => MessageRole::Error,
    };
    DisplayMessage {
        role,
        content: msg.content.clone(),
        thinking: None,
        model: msg.model.clone(),
        cost: msg.cost,
        tokens: msg.tokens.map(|(i, o)| (i + o) as u32),
        timestamp: msg.timestamp,
        show_thinking: false,
    }
}

// ---------------------------------------------------------------------------
// Message bubble
// ---------------------------------------------------------------------------

fn render_message_bubble(msg: &DisplayMessage, theme: &HiveTheme) -> AnyElement {
    let is_user = msg.role == MessageRole::User;
    let is_error = msg.role == MessageRole::Error;

    let bg = match msg.role {
        MessageRole::User => theme.bg_tertiary,
        MessageRole::Assistant | MessageRole::System => theme.bg_surface,
        MessageRole::Error => theme.accent_red,
    };

    let role_label = match msg.role {
        MessageRole::User => "You",
        MessageRole::Assistant => "Hive",
        MessageRole::System => "System",
        MessageRole::Error => "Error",
    };

    let role_color = match msg.role {
        MessageRole::User => theme.accent_powder,
        MessageRole::Assistant => theme.accent_cyan,
        MessageRole::System => theme.accent_yellow,
        MessageRole::Error => theme.text_on_accent,
    };

    let text_color = if is_error {
        theme.text_on_accent
    } else {
        theme.text_primary
    };

    // Timestamp string
    let ts = msg.timestamp.format("%H:%M").to_string();

    // Build the bubble content
    let mut bubble = div()
        .max_w(px(720.0))
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(bg);

    // Header row: role label + timestamp + model badge + cost badge
    let mut header = div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .mb(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(role_color)
                .font_weight(FontWeight::SEMIBOLD)
                .child(role_label),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(ts),
        );

    // Model badge for assistant messages
    if let Some(ref model) = msg.model {
        header = header.child(render_model_badge(model, theme));
    }

    // Cost badge
    if let Some(cost) = msg.cost {
        if cost > 0.0 {
            header = header.child(render_cost_badge(cost, msg.tokens, theme));
        }
    }

    bubble = bubble.child(header);

    // Thinking section (collapsible)
    if let Some(ref thinking) = msg.thinking {
        bubble = bubble.child(render_thinking_section(thinking, msg.show_thinking, theme));
    }

    // Content — rendered as markdown for assistant/system, plain for user
    let content_el = if is_user {
        div()
            .text_size(theme.font_size_base)
            .text_color(text_color)
            .child(msg.content.clone())
            .into_any_element()
    } else {
        render_markdown(&msg.content, theme)
    };
    bubble = bubble.child(content_el);

    // Row alignment: user right-aligned, others left-aligned
    let row = div().flex().w_full();
    let row = if is_user {
        row.flex_row_reverse()
    } else {
        row.flex_row()
    };
    row.child(bubble).into_any_element()
}

// ---------------------------------------------------------------------------
// Streaming bubble
// ---------------------------------------------------------------------------

fn render_streaming_bubble(
    content: &str,
    thinking: Option<&str>,
    model: &str,
    theme: &HiveTheme,
) -> AnyElement {
    let mut bubble = div()
        .max_w(px(720.0))
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.accent_cyan);

    // Header
    let header = div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .mb(theme.space_1)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_cyan)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Hive"),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_cyan)
                .child("Generating..."),
        )
        .child(render_model_badge(model, theme));

    bubble = bubble.child(header);

    // Thinking section if present
    if let Some(thinking_text) = thinking {
        bubble = bubble.child(render_thinking_section(thinking_text, true, theme));
    }

    // Content so far
    if !content.is_empty() {
        bubble = bubble.child(render_markdown(content, theme));
    } else {
        // Placeholder dots
        bubble = bubble.child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_muted)
                .child("..."),
        );
    }

    div().flex().w_full().child(bubble).into_any_element()
}

// ---------------------------------------------------------------------------
// Thinking section (collapsible)
// ---------------------------------------------------------------------------

fn render_thinking_section(thinking: &str, expanded: bool, theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .mt(theme.space_1)
        .mb(theme.space_2)
        .pl(theme.space_3)
        .border_l_2()
        .border_color(theme.accent_cyan);

    // Header: always visible
    let toggle_label = if expanded {
        "Thinking (collapse)"
    } else {
        "Thinking (expand)"
    };

    section = section.child(
        div()
            .flex()
            .items_center()
            .gap(theme.space_1)
            .cursor_pointer()
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.accent_cyan)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(toggle_label),
            ),
    );

    // Body: only when expanded
    if expanded {
        section = section.child(
            div()
                .mt(theme.space_1)
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(thinking.to_string()),
        );
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Badges
// ---------------------------------------------------------------------------

fn render_model_badge(model: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .px(theme.space_2)
        .py(px(1.0))
        .rounded(theme.radius_sm)
        .bg(theme.bg_tertiary)
        .text_size(theme.font_size_xs)
        .text_color(theme.accent_cyan)
        .child(model.to_string())
        .into_any_element()
}

fn render_cost_badge(cost: f64, tokens: Option<u32>, theme: &HiveTheme) -> AnyElement {
    let label = match tokens {
        Some(t) if t >= 1000 => format!("${:.4} | {:.1}k tok", cost, t as f64 / 1000.0),
        Some(t) => format!("${:.4} | {} tok", cost, t),
        None => format!("${:.4}", cost),
    };
    div()
        .text_size(theme.font_size_xs)
        .text_color(theme.text_muted)
        .child(label)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Session totals
// ---------------------------------------------------------------------------

fn render_session_totals(cost: f64, tokens: u32, theme: &HiveTheme) -> AnyElement {
    let tokens_label = if tokens >= 1000 {
        format!("{:.1}k tokens", tokens as f64 / 1000.0)
    } else {
        format!("{} tokens", tokens)
    };

    div()
        .flex()
        .justify_center()
        .w_full()
        .pt(theme.space_2)
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme.space_3)
                .px(theme.space_3)
                .py(theme.space_1)
                .rounded(theme.radius_sm)
                .bg(theme.bg_secondary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("Session: ${:.4}", cost))
                .child(
                    div()
                        .w(px(1.0))
                        .h(px(10.0))
                        .bg(theme.border),
                )
                .child(tokens_label),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Markdown renderer
// ---------------------------------------------------------------------------

/// Render a markdown string into GPUI elements.
fn render_markdown(source: &str, theme: &HiveTheme) -> AnyElement {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(source, options);

    let mut container_children: Vec<AnyElement> = Vec::new();

    // State tracking
    let mut in_code_block = false;
    let mut code_block_content = String::new();
    let mut code_block_lang = String::new();
    let mut bold_active = false;
    let mut emphasis_active = false;
    let mut in_heading = false;
    let mut heading_level: u8 = 0;
    let mut inline_segments: Vec<AnyElement> = Vec::new();
    let mut _in_list = false;
    let mut list_items: Vec<AnyElement> = Vec::new();
    let mut list_item_segments: Vec<AnyElement> = Vec::new();
    let mut in_list_item = false;

    for event in parser {
        match event {
            // -- Code blocks --
            Event::Start(Tag::CodeBlock(kind)) => {
                // Flush any pending inline text
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                in_code_block = true;
                code_block_content.clear();
                code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                container_children.push(render_code_block(
                    &code_block_content,
                    &code_block_lang,
                    theme,
                ));
                code_block_content.clear();
                code_block_lang.clear();
            }

            // -- Headings --
            Event::Start(Tag::Heading { level, .. }) => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                in_heading = true;
                heading_level = level as u8;
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                let size = match heading_level {
                    1 => theme.font_size_xl,
                    2 => theme.font_size_lg,
                    _ => theme.font_size_base,
                };
                let heading_el = div()
                    .mt(theme.space_2)
                    .mb(theme.space_1)
                    .text_size(size)
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .children(inline_segments.drain(..))
                    .into_any_element();
                container_children.push(heading_el);
                heading_level = 0;
            }

            // -- Paragraphs --
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
            }

            // -- Bold / Emphasis --
            Event::Start(Tag::Strong) => {
                bold_active = true;
            }
            Event::End(TagEnd::Strong) => {
                bold_active = false;
            }
            Event::Start(Tag::Emphasis) => {
                emphasis_active = true;
            }
            Event::End(TagEnd::Emphasis) => {
                emphasis_active = false;
            }

            // -- Lists --
            Event::Start(Tag::List(_)) => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                _in_list = true;
                list_items.clear();
            }
            Event::End(TagEnd::List(_)) => {
                // Flush any remaining list item
                if in_list_item {
                    flush_list_item(&mut list_item_segments, &mut list_items, theme);
                    in_list_item = false;
                }
                _in_list = false;
                let list_el = div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_1)
                    .pl(theme.space_3)
                    .my(theme.space_1)
                    .children(list_items.drain(..))
                    .into_any_element();
                container_children.push(list_el);
            }
            Event::Start(Tag::Item) => {
                // Flush previous list item if any
                if in_list_item {
                    flush_list_item(&mut list_item_segments, &mut list_items, theme);
                }
                in_list_item = true;
                list_item_segments.clear();
            }
            Event::End(TagEnd::Item) => {
                flush_list_item(&mut list_item_segments, &mut list_items, theme);
                in_list_item = false;
            }

            // -- Inline code --
            Event::Code(text) => {
                let code_el = div()
                    .px(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_sm)
                    .font_family(theme.font_mono.clone())
                    .text_color(theme.accent_powder)
                    .child(text.to_string())
                    .into_any_element();

                if in_list_item {
                    list_item_segments.push(code_el);
                } else {
                    inline_segments.push(code_el);
                }
            }

            // -- Text --
            Event::Text(text) => {
                if in_code_block {
                    code_block_content.push_str(&text);
                } else {
                    let mut el = div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary);

                    if bold_active {
                        el = el.font_weight(FontWeight::BOLD);
                    }
                    if emphasis_active {
                        el = el.italic();
                    }

                    let el = el.child(text.to_string()).into_any_element();
                    if in_list_item {
                        list_item_segments.push(el);
                    } else if in_heading {
                        inline_segments.push(el);
                    } else {
                        inline_segments.push(el);
                    }
                }
            }

            // -- Breaks --
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_block_content.push('\n');
                }
                // Soft breaks in paragraphs are handled by paragraph end
            }

            // -- Horizontal rule --
            Event::Rule => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                container_children.push(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .my(theme.space_2)
                        .bg(theme.border)
                        .into_any_element(),
                );
            }

            // Catch-all for unhandled events
            _ => {}
        }
    }

    // Flush remaining inline segments
    flush_inline_segments(&mut inline_segments, &mut container_children, theme);

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .text_color(theme.text_primary)
        .children(container_children)
        .into_any_element()
}

/// Flush accumulated inline segments into a paragraph element.
fn flush_inline_segments(
    segments: &mut Vec<AnyElement>,
    container: &mut Vec<AnyElement>,
    theme: &HiveTheme,
) {
    if segments.is_empty() {
        return;
    }
    let p = div()
        .flex()
        .flex_wrap()
        .gap(px(0.0))
        .text_size(theme.font_size_base)
        .children(segments.drain(..))
        .into_any_element();
    container.push(p);
}

/// Flush accumulated list item segments into a list item element.
fn flush_list_item(
    segments: &mut Vec<AnyElement>,
    list_items: &mut Vec<AnyElement>,
    theme: &HiveTheme,
) {
    if segments.is_empty() {
        return;
    }
    let item = div()
        .flex()
        .flex_wrap()
        .gap(px(0.0))
        .text_size(theme.font_size_base)
        .child(
            div()
                .text_color(theme.text_muted)
                .mr(theme.space_1)
                .child("\u{2022}"), // bullet
        )
        .children(segments.drain(..))
        .into_any_element();
    list_items.push(item);
}

// ---------------------------------------------------------------------------
// Code block renderer
// ---------------------------------------------------------------------------

fn render_code_block(code: &str, lang: &str, theme: &HiveTheme) -> AnyElement {
    let trimmed = code.trim_end_matches('\n');
    let mut header = div()
        .flex()
        .items_center()
        .justify_between()
        .px(theme.space_3)
        .py(theme.space_1)
        .border_b_1()
        .border_color(theme.border);

    // Language label
    if !lang.is_empty() {
        header = header.child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(lang.to_string()),
        );
    } else {
        header = header.child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("code"),
        );
    }

    // Copy label
    header = header.child(
        div()
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .cursor_pointer()
            .child("Copy"),
    );

    div()
        .w_full()
        .my(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_tertiary)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(header)
        .child(
            div()
                .px(theme.space_3)
                .py(theme.space_2)
                .text_size(theme.font_size_sm)
                .font_family(theme.font_mono.clone())
                .text_color(theme.text_primary)
                .child(trimmed.to_string()),
        )
        .into_any_element()
}
