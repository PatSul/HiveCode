use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Icon, IconName};
use std::path::PathBuf;

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Event emitted when the user submits a chat message via Enter or the send
/// button. The workspace subscribes to this and feeds the text into the AI
/// streaming flow.
#[derive(Debug, Clone)]
pub struct SubmitMessage(pub String);

// ---------------------------------------------------------------------------
// ChatInputView
// ---------------------------------------------------------------------------

/// Interactive chat input bar backed by a gpui-component `InputState`.
///
/// Owns an `Entity<InputState>` for real keyboard input, an attach button,
/// a send button, and a cost-prediction display. Emits `SubmitMessage` on
/// Enter (plain) or send-button click.
pub struct ChatInputView {
    input_state: Entity<InputState>,
    input_focus: FocusHandle,
    attachments: Vec<PathBuf>,
    estimated_cost: Option<f64>,
    is_sending: bool,
    theme: HiveTheme,
}

impl EventEmitter<SubmitMessage> for ChatInputView {}

impl ChatInputView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 8)
                .placeholder("Type a message\u{2026} (Enter to send, Shift+Enter for newline)")
        });

        let input_focus = input_state.read(cx).focus_handle(cx).clone();

        // Subscribe to input events so we can intercept Enter to submit.
        cx.subscribe_in(&input_state, window, Self::on_input_event)
            .detach();

        Self {
            input_state,
            input_focus,
            attachments: Vec::new(),
            estimated_cost: None,
            is_sending: false,
            theme: HiveTheme::dark(),
        }
    }

    /// Returns a clone of the input's FocusHandle for external focus management.
    pub fn input_focus_handle(&self) -> FocusHandle {
        self.input_focus.clone()
    }

    /// Returns the current text in the input field.
    pub fn current_text(&self, cx: &App) -> String {
        self.input_state.read(cx).value().to_string()
    }

    /// Clear the input field.
    pub fn clear(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input_state.update(cx, |state, cx| {
            state.replace("", window, cx);
        });
    }

    /// Toggle the sending/disabled state and update the placeholder text.
    pub fn set_sending(&mut self, sending: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.is_sending = sending;
        let placeholder = if sending {
            "Generating\u{2026}"
        } else {
            "Type a message\u{2026} (Enter to send, Shift+Enter for newline)"
        };
        self.input_state.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx);
        });
        cx.notify();
    }

    /// Update the estimated cost display.
    pub fn set_estimated_cost(&mut self, cost: Option<f64>) {
        self.estimated_cost = cost;
    }

    /// Remove an attachment by index.
    pub fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.attachments.len() {
            self.attachments.remove(index);
            cx.notify();
        }
    }

    // -- Internal handlers --------------------------------------------------

    /// Called for every `InputEvent` from the underlying `InputState`.
    fn on_input_event(
        &mut self,
        _state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => {
                // Both plain Enter and Ctrl+Enter (secondary) submit.
                // Shift+Enter bypasses the action system entirely and inserts
                // a newline through the IME path, so it never reaches here.
                // The auto-grow input already inserted a trailing newline;
                // `submit()` trims it before emitting.
                self.submit(window, cx);
            }
            InputEvent::Change => {
                cx.notify();
            }
            _ => {}
        }
    }

    /// Read text, trim, emit `SubmitMessage`, and clear the input.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_sending {
            return;
        }

        let raw = self.input_state.read(cx).value().to_string();
        let text = raw.trim().to_string();
        if text.is_empty() && self.attachments.is_empty() {
            // Nothing to send -- clear the stray newline and bail.
            self.clear(window, cx);
            return;
        }

        self.clear(window, cx);
        self.attachments.clear();
        cx.emit(SubmitMessage(text));
    }

    /// Open a native file picker and add selected files as attachments.
    fn handle_attach(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });

        cx.spawn(async move |this, app: &mut AsyncApp| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                let _ = this.update(app, |this, cx| {
                    this.attachments.extend(paths);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

impl Render for ChatInputView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let cost_text = self
            .estimated_cost
            .map(|c| format!("~${:.4}", c))
            .unwrap_or_default();

        let has_text = !self.input_state.read(cx).value().is_empty();
        let has_attachments = !self.attachments.is_empty();
        let send_enabled = (has_text || has_attachments) && !self.is_sending;
        let send_bg = if send_enabled {
            theme.accent_aqua
        } else {
            theme.bg_surface
        };
        let send_text_color = if send_enabled {
            theme.text_on_accent
        } else {
            theme.text_muted
        };

        // Build attachment chips
        let attachment_chips: Vec<_> = self
            .attachments
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string());
                div()
                    .id(ElementId::Name(format!("attachment-{i}").into()))
                    .flex()
                    .items_center()
                    .gap(theme.space_1)
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .bg(theme.bg_tertiary)
                    .rounded(theme.radius_sm)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(Icon::new(IconName::File).size_3p5())
                    .child(name)
                    .child(
                        div()
                            .id(ElementId::Name(format!("rm-attach-{i}").into()))
                            .cursor_pointer()
                            .text_color(theme.text_muted)
                            .hover(|el| el.text_color(theme.accent_red))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.remove_attachment(i, cx);
                                }),
                            )
                            .child(Icon::new(IconName::Close).size_3p5()),
                    )
            })
            .collect();

        // Outer wrapper — padding creates the "floating" look
        div()
            .flex()
            .flex_col()
            .w_full()
            .px(theme.space_6)
            .pb(theme.space_4)
            .pt(theme.space_2)
            // Floating card
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(px(900.0))
                    .mx_auto()
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(theme.radius_lg)
                    .overflow_hidden()
                    // Attachment chips
                    .when(has_attachments, |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(theme.space_1)
                                .px(theme.space_3)
                                .pt(theme.space_2)
                                .children(attachment_chips),
                        )
                    })
                    // Input row
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(theme.space_2)
                            .px(theme.space_3)
                            .py(theme.space_2)
                            // Attach button
                            .child(
                                div()
                                    .id("attach-btn")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded(theme.radius_sm)
                                    .cursor_pointer()
                                    .text_color(theme.text_secondary)
                                    .hover(|el| el.bg(theme.bg_tertiary))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, window, cx| {
                                            this.handle_attach(window, cx);
                                        }),
                                    )
                                    .child(Icon::new(IconName::Plus).size_4()),
                            )
                            // Text input — appearance(false) since the card provides
                            // the visual boundary (border + rounded corners).
                            .child(
                                div().flex_1().child(
                                    Input::new(&self.input_state)
                                        .appearance(false)
                                        .disabled(self.is_sending)
                                        .cleanable(false),
                                ),
                            )
                            // Send button
                            .child(
                                div()
                                    .id("send-btn")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded(theme.radius_sm)
                                    .bg(send_bg)
                                    .cursor_pointer()
                                    .text_color(send_text_color)
                                    .when(send_enabled, |el| {
                                        el.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, window, cx| {
                                                this.submit(window, cx);
                                            }),
                                        )
                                    })
                                    .child(Icon::new(IconName::ArrowRight).size_4()),
                            ),
                    )
                    // Cost bar (only when cost estimate is present)
                    .when(!cost_text.is_empty(), |el| {
                        el.child(
                            div()
                                .w_full()
                                .px(theme.space_3)
                                .pb(theme.space_1)
                                .flex()
                                .justify_end()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(cost_text),
                        )
                    }),
            )
    }
}
