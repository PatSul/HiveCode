use std::collections::HashMap;

use gpui::*;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};


use hive_ui_core::HiveTheme;
use hive_ui_core::WelcomeScreen;
use hive_ai::MessageRole;

// ---------------------------------------------------------------------------
// Markdown cache — parsed AST intermediate representation
// ---------------------------------------------------------------------------

/// Owned intermediate representation of a parsed markdown document.
/// Avoids re-parsing pulldown_cmark on every render frame.
type MarkdownIR = Vec<MarkdownBlock>;

/// A single block-level element in the parsed markdown.
#[derive(Clone)]
enum MarkdownBlock {
    Paragraph(Vec<InlineSegment>),
    Heading {
        level: u8,
        segments: Vec<InlineSegment>,
    },
    CodeBlock {
        lang: String,
        code: String,
    },
    List(Vec<Vec<InlineSegment>>),
    HorizontalRule,
}

/// An inline segment within a paragraph, heading, or list item.
#[derive(Clone)]
enum InlineSegment {
    Text {
        content: String,
        bold: bool,
        italic: bool,
    },
    InlineCode(String),
}

/// Cache of parsed markdown keyed by a simple content hash.
///
/// Since message content is immutable once finalized, the cache grows at most
/// to the number of distinct messages ever displayed. During streaming, the
/// last message keeps changing, so its entry is evicted and re-inserted each
/// time the content changes.
pub struct MarkdownCache {
    entries: HashMap<u64, MarkdownIR>,
}

impl Default for MarkdownCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Clear the entire cache (e.g. when switching conversations).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get or parse the markdown IR for the given source content.
    fn get_or_parse(&mut self, source: &str) -> &MarkdownIR {
        let hash = Self::hash_content(source);
        self.entries
            .entry(hash)
            .or_insert_with(|| Self::parse_to_ir(source))
    }

    fn hash_content(source: &str) -> u64 {
        // FNV-1a for speed — good enough for a display cache key.
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in source.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    fn parse_to_ir(source: &str) -> MarkdownIR {
        let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(source, options);

        let mut blocks: Vec<MarkdownBlock> = Vec::new();

        // State tracking
        let mut in_code_block = false;
        let mut code_block_content = String::new();
        let mut code_block_lang = String::new();
        let mut bold_active = false;
        let mut emphasis_active = false;
        let mut _in_heading = false;
        let mut heading_level: u8 = 0;
        let mut inline_segments: Vec<InlineSegment> = Vec::new();
        let mut _in_list = false;
        let mut list_items: Vec<Vec<InlineSegment>> = Vec::new();
        let mut list_item_segments: Vec<InlineSegment> = Vec::new();
        let mut in_list_item = false;

        for event in parser {
            match event {
                // -- Code blocks --
                Event::Start(Tag::CodeBlock(kind)) => {
                    if !inline_segments.is_empty() {
                        blocks.push(MarkdownBlock::Paragraph(std::mem::take(
                            &mut inline_segments,
                        )));
                    }
                    in_code_block = true;
                    code_block_content.clear();
                    code_block_lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                    blocks.push(MarkdownBlock::CodeBlock {
                        lang: std::mem::take(&mut code_block_lang),
                        code: std::mem::take(&mut code_block_content),
                    });
                }

                // -- Headings --
                Event::Start(Tag::Heading { level, .. }) => {
                    if !inline_segments.is_empty() {
                        blocks.push(MarkdownBlock::Paragraph(std::mem::take(
                            &mut inline_segments,
                        )));
                    }
                    _in_heading = true;
                    heading_level = level as u8;
                }
                Event::End(TagEnd::Heading(_)) => {
                    _in_heading = false;
                    blocks.push(MarkdownBlock::Heading {
                        level: heading_level,
                        segments: std::mem::take(&mut inline_segments),
                    });
                    heading_level = 0;
                }

                // -- Paragraphs --
                Event::Start(Tag::Paragraph) => {}
                Event::End(TagEnd::Paragraph) => {
                    if !inline_segments.is_empty() {
                        blocks.push(MarkdownBlock::Paragraph(std::mem::take(
                            &mut inline_segments,
                        )));
                    }
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
                    if !inline_segments.is_empty() {
                        blocks.push(MarkdownBlock::Paragraph(std::mem::take(
                            &mut inline_segments,
                        )));
                    }
                    _in_list = true;
                    list_items.clear();
                }
                Event::End(TagEnd::List(_)) => {
                    if in_list_item && !list_item_segments.is_empty() {
                        list_items.push(std::mem::take(&mut list_item_segments));
                        in_list_item = false;
                    }
                    _in_list = false;
                    blocks.push(MarkdownBlock::List(std::mem::take(&mut list_items)));
                }
                Event::Start(Tag::Item) => {
                    if in_list_item && !list_item_segments.is_empty() {
                        list_items.push(std::mem::take(&mut list_item_segments));
                    }
                    in_list_item = true;
                    list_item_segments.clear();
                }
                Event::End(TagEnd::Item) => {
                    if !list_item_segments.is_empty() {
                        list_items.push(std::mem::take(&mut list_item_segments));
                    }
                    in_list_item = false;
                }

                // -- Inline code --
                Event::Code(text) => {
                    let seg = InlineSegment::InlineCode(text.to_string());
                    if in_list_item {
                        list_item_segments.push(seg);
                    } else {
                        inline_segments.push(seg);
                    }
                }

                // -- Text --
                Event::Text(text) => {
                    if in_code_block {
                        code_block_content.push_str(&text);
                    } else {
                        let seg = InlineSegment::Text {
                            content: text.to_string(),
                            bold: bold_active,
                            italic: emphasis_active,
                        };
                        if in_list_item {
                            list_item_segments.push(seg);
                        } else {
                            inline_segments.push(seg);
                        }
                    }
                }

                // -- Breaks --
                Event::SoftBreak | Event::HardBreak => {
                    if in_code_block {
                        code_block_content.push('\n');
                    }
                }

                // -- Horizontal rule --
                Event::Rule => {
                    if !inline_segments.is_empty() {
                        blocks.push(MarkdownBlock::Paragraph(std::mem::take(
                            &mut inline_segments,
                        )));
                    }
                    blocks.push(MarkdownBlock::HorizontalRule);
                }

                _ => {}
            }
        }

        // Flush remaining inline segments
        if !inline_segments.is_empty() {
            blocks.push(MarkdownBlock::Paragraph(inline_segments));
        }

        blocks
    }
}

// ---------------------------------------------------------------------------
// Cached chat data — avoids per-frame string cloning from ChatService
// ---------------------------------------------------------------------------

/// Cached display state derived from `ChatService`. Stored on the workspace
/// and rebuilt only when the service's generation counter advances.
pub struct CachedChatData {
    pub display_messages: Vec<DisplayMessage>,
    pub total_cost: f64,
    pub total_tokens: u32,
    pub generation: u64,
    pub markdown_cache: MarkdownCache,
}

impl Default for CachedChatData {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedChatData {
    pub fn new() -> Self {
        Self {
            display_messages: Vec::new(),
            total_cost: 0.0,
            total_tokens: 0,
            generation: u64::MAX, // Force rebuild on first access
            markdown_cache: MarkdownCache::new(),
        }
    }

}


// ---------------------------------------------------------------------------
// Display types
// ---------------------------------------------------------------------------

/// A fully-resolved message ready for rendering.
/// A tool call to display in the chat UI.
#[derive(Clone)]
pub struct ToolCallDisplay {
    pub name: String,
    pub args: String,
}

pub struct DisplayMessage {
    pub role: MessageRole,
    pub content: String,
    pub thinking: Option<String>,
    pub model: Option<String>,
    pub cost: Option<f64>,
    pub tokens: Option<u32>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub show_thinking: bool,
    /// Tool calls made by the assistant (rendered as collapsible blocks).
    pub tool_calls: Vec<ToolCallDisplay>,
    /// For tool result messages: the ID of the tool call this responds to.
    pub tool_call_id: Option<String>,
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
            tool_calls: Vec::new(),
            tool_call_id: None,
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
            tool_calls: Vec::new(),
            tool_call_id: None,
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
            tool_calls: Vec::new(),
            tool_call_id: None,
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

impl Default for ChatPanel {
    fn default() -> Self {
        Self::new()
    }
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

    /// Render the chat panel from pre-cached display data.
    ///
    /// Uses `CachedChatData` (synced from `ChatService` via
    /// [`CachedChatData::sync_from_service`]) to avoid per-frame string
    /// cloning, and a `MarkdownCache` inside `CachedChatData` to avoid
    /// re-parsing markdown ASTs for immutable messages.
    pub fn render_cached(
        cached: &mut CachedChatData,
        streaming_content: &str,
        is_streaming: bool,
        current_model: &str,
        theme: &HiveTheme,
    ) -> AnyElement {
        if cached.display_messages.is_empty() && !is_streaming {
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

        // Render cached display messages
        for msg in &cached.display_messages {
            content = content.child(render_message_bubble_cached(
                msg,
                &mut cached.markdown_cache,
                theme,
            ));
        }

        // Streaming bubble (always re-rendered since content changes per frame)
        if is_streaming {
            content = content.child(render_streaming_bubble_cached(
                streaming_content,
                None,
                current_model,
                &mut cached.markdown_cache,
                theme,
            ));
        }

        // Session totals
        if cached.total_cost > 0.0 || cached.total_tokens > 0 {
            content = content.child(render_session_totals(
                cached.total_cost,
                cached.total_tokens,
                theme,
            ));
        }

        content.into_any_element()
    }

}


// ---------------------------------------------------------------------------
// Message bubble
// ---------------------------------------------------------------------------

/// Render tool call blocks (shown on assistant messages that triggered tool use).
fn render_tool_calls(calls: &[ToolCallDisplay], theme: &HiveTheme) -> AnyElement {
    let mut container = div().flex().flex_col().gap(theme.space_1).mt(theme.space_2);

    for call in calls {
        let block = div()
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .child(
                div().flex().items_center().gap(theme.space_1).child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_cyan)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("Tool: {}", call.name)),
                ),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_family("monospace")
                    .overflow_hidden()
                    .max_h(px(80.0))
                    .child(call.args.clone()),
            );
        container = container.child(block);
    }

    container.into_any_element()
}

fn render_message_bubble(msg: &DisplayMessage, theme: &HiveTheme) -> AnyElement {
    let is_user = msg.role == MessageRole::User;
    let is_error = msg.role == MessageRole::Error;

    let bg = match msg.role {
        MessageRole::User => theme.bg_tertiary,
        MessageRole::Assistant | MessageRole::System | MessageRole::Tool => theme.bg_surface,
        MessageRole::Error => theme.accent_red,
    };

    let role_label = match msg.role {
        MessageRole::User => "You",
        MessageRole::Assistant => "Hive",
        MessageRole::System => "System",
        MessageRole::Error => "Error",
        MessageRole::Tool => "Tool",
    };

    let role_color = match msg.role {
        MessageRole::User => theme.accent_powder,
        MessageRole::Assistant => theme.accent_cyan,
        MessageRole::System | MessageRole::Tool => theme.accent_yellow,
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
    if let Some(cost) = msg.cost
        && cost > 0.0 {
            header = header.child(render_cost_badge(cost, msg.tokens, theme));
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

    // Tool calls (shown on assistant messages that triggered tool use)
    if !msg.tool_calls.is_empty() {
        bubble = bubble.child(render_tool_calls(&msg.tool_calls, theme));
    }

    // Row alignment: user right-aligned, others left-aligned
    let row = div().flex().w_full();
    let row = if is_user {
        row.flex_row_reverse()
    } else {
        row.flex_row()
    };
    row.child(bubble).into_any_element()
}

/// Cached variant of `render_message_bubble` — renders markdown from pre-parsed IR.
fn render_message_bubble_cached(
    msg: &DisplayMessage,
    md_cache: &mut MarkdownCache,
    theme: &HiveTheme,
) -> AnyElement {
    let is_user = msg.role == MessageRole::User;
    let is_error = msg.role == MessageRole::Error;

    let bg = match msg.role {
        MessageRole::User => theme.bg_tertiary,
        MessageRole::Assistant | MessageRole::System | MessageRole::Tool => theme.bg_surface,
        MessageRole::Error => theme.accent_red,
    };

    let role_label = match msg.role {
        MessageRole::User => "You",
        MessageRole::Assistant => "Hive",
        MessageRole::System => "System",
        MessageRole::Error => "Error",
        MessageRole::Tool => "Tool",
    };

    let role_color = match msg.role {
        MessageRole::User => theme.accent_powder,
        MessageRole::Assistant => theme.accent_cyan,
        MessageRole::System | MessageRole::Tool => theme.accent_yellow,
        MessageRole::Error => theme.text_on_accent,
    };

    let text_color = if is_error {
        theme.text_on_accent
    } else {
        theme.text_primary
    };

    let ts = msg.timestamp.format("%H:%M").to_string();

    let mut bubble = div()
        .max_w(px(720.0))
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(bg);

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

    if let Some(ref model) = msg.model {
        header = header.child(render_model_badge(model, theme));
    }

    if let Some(cost) = msg.cost
        && cost > 0.0 {
            header = header.child(render_cost_badge(cost, msg.tokens, theme));
        }

    bubble = bubble.child(header);

    if let Some(ref thinking) = msg.thinking {
        bubble = bubble.child(render_thinking_section(thinking, msg.show_thinking, theme));
    }

    // Content — cached markdown parse for assistant/system, plain for user
    let content_el = if is_user {
        div()
            .text_size(theme.font_size_base)
            .text_color(text_color)
            .child(msg.content.clone())
            .into_any_element()
    } else {
        render_markdown_cached(&msg.content, md_cache, theme)
    };
    bubble = bubble.child(content_el);

    // Tool calls (shown on assistant messages that triggered tool use)
    if !msg.tool_calls.is_empty() {
        bubble = bubble.child(render_tool_calls(&msg.tool_calls, theme));
    }

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

/// Cached variant of `render_streaming_bubble`.
fn render_streaming_bubble_cached(
    content: &str,
    thinking: Option<&str>,
    model: &str,
    md_cache: &mut MarkdownCache,
    theme: &HiveTheme,
) -> AnyElement {
    let mut bubble = div()
        .max_w(px(720.0))
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.accent_cyan);

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

    if let Some(thinking_text) = thinking {
        bubble = bubble.child(render_thinking_section(thinking_text, true, theme));
    }

    if !content.is_empty() {
        bubble = bubble.child(render_markdown_cached(content, md_cache, theme));
    } else {
        bubble = bubble.child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_muted)
                .child("..."),
        );
    }

    div().flex().w_full().child(bubble).into_any_element()
}

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
                .child(div().w(px(1.0)).h(px(10.0)).bg(theme.border))
                .child(tokens_label),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Markdown renderer
// ---------------------------------------------------------------------------

/// Render markdown from cached IR (avoids re-parsing the AST every frame).
fn render_markdown_cached(
    source: &str,
    md_cache: &mut MarkdownCache,
    theme: &HiveTheme,
) -> AnyElement {
    let ir = md_cache.get_or_parse(source);
    render_markdown_ir(ir, theme)
}

/// Convert a pre-parsed `MarkdownIR` into GPUI elements.
fn render_markdown_ir(ir: &MarkdownIR, theme: &HiveTheme) -> AnyElement {
    let mut container_children: Vec<AnyElement> = Vec::with_capacity(ir.len());

    for block in ir {
        match block {
            MarkdownBlock::Paragraph(segments) => {
                let children: Vec<AnyElement> = segments
                    .iter()
                    .map(|seg| render_inline_segment(seg, theme))
                    .collect();
                if !children.is_empty() {
                    container_children.push(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(0.0))
                            .text_size(theme.font_size_base)
                            .children(children)
                            .into_any_element(),
                    );
                }
            }
            MarkdownBlock::Heading { level, segments } => {
                let size = match level {
                    1 => theme.font_size_xl,
                    2 => theme.font_size_lg,
                    _ => theme.font_size_base,
                };
                let children: Vec<AnyElement> = segments
                    .iter()
                    .map(|seg| render_inline_segment(seg, theme))
                    .collect();
                container_children.push(
                    div()
                        .mt(theme.space_2)
                        .mb(theme.space_1)
                        .text_size(size)
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .children(children)
                        .into_any_element(),
                );
            }
            MarkdownBlock::CodeBlock { lang, code } => {
                container_children.push(render_code_block(code, lang, theme));
            }
            MarkdownBlock::List(items) => {
                let item_elements: Vec<AnyElement> = items
                    .iter()
                    .map(|segments| {
                        let children: Vec<AnyElement> = segments
                            .iter()
                            .map(|seg| render_inline_segment(seg, theme))
                            .collect();
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(0.0))
                            .text_size(theme.font_size_base)
                            .child(
                                div()
                                    .text_color(theme.text_muted)
                                    .mr(theme.space_1)
                                    .child("\u{2022}"),
                            )
                            .children(children)
                            .into_any_element()
                    })
                    .collect();
                container_children.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme.space_1)
                        .pl(theme.space_3)
                        .my(theme.space_1)
                        .children(item_elements)
                        .into_any_element(),
                );
            }
            MarkdownBlock::HorizontalRule => {
                container_children.push(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .my(theme.space_2)
                        .bg(theme.border)
                        .into_any_element(),
                );
            }
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .text_color(theme.text_primary)
        .children(container_children)
        .into_any_element()
}

/// Render a single inline segment to a GPUI element.
fn render_inline_segment(seg: &InlineSegment, theme: &HiveTheme) -> AnyElement {
    match seg {
        InlineSegment::Text {
            content,
            bold,
            italic,
        } => {
            let mut el = div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary);
            if *bold {
                el = el.font_weight(FontWeight::BOLD);
            }
            if *italic {
                el = el.italic();
            }
            el.child(content.clone()).into_any_element()
        }
        InlineSegment::InlineCode(text) => div()
            .px(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_sm)
            .font_family(theme.font_mono.clone())
            .text_color(theme.accent_powder)
            .child(text.clone())
            .into_any_element(),
    }
}

/// Render a markdown string into GPUI elements.
fn render_markdown(source: &str, theme: &HiveTheme) -> AnyElement {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
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
