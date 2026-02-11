//! Assistant panel — Personal AI assistant dashboard.
//!
//! Displays daily briefing, upcoming events, email digest, active reminders,
//! research progress, and recent actions.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{Icon, IconName};

use crate::theme::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A summary for the daily briefing card.
#[derive(Debug, Clone)]
pub struct BriefingSummary {
    pub greeting: String,
    pub date: String,
    pub event_count: usize,
    pub unread_emails: usize,
    pub active_reminders: usize,
    pub top_priority: Option<String>,
}

/// An upcoming calendar event.
#[derive(Debug, Clone)]
pub struct UpcomingEvent {
    pub title: String,
    pub time: String,
    pub location: Option<String>,
    pub is_conflict: bool,
}

/// A group of emails from a provider.
#[derive(Debug, Clone)]
pub struct EmailGroup {
    pub provider: String,
    pub previews: Vec<EmailPreview>,
}

/// Preview of a single email.
#[derive(Debug, Clone)]
pub struct EmailPreview {
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub time: String,
    pub important: bool,
}

/// An active reminder.
#[derive(Debug, Clone)]
pub struct ActiveReminder {
    pub title: String,
    pub due: String,
    pub is_overdue: bool,
}

/// A background research task's progress.
#[derive(Debug, Clone)]
pub struct ResearchProgress {
    pub topic: String,
    pub status: String,
    pub progress_pct: u8,
}

/// A recent assistant action.
#[derive(Debug, Clone)]
pub struct RecentAction {
    pub description: String,
    pub timestamp: String,
    pub action_type: String,
}

/// All data needed to render the assistant panel.
#[derive(Debug, Clone)]
pub struct AssistantPanelData {
    pub briefing: Option<BriefingSummary>,
    pub events: Vec<UpcomingEvent>,
    pub email_groups: Vec<EmailGroup>,
    pub reminders: Vec<ActiveReminder>,
    pub research: Vec<ResearchProgress>,
    pub recent_actions: Vec<RecentAction>,
}

impl AssistantPanelData {
    pub fn empty() -> Self {
        Self {
            briefing: None,
            events: Vec::new(),
            email_groups: Vec::new(),
            reminders: Vec::new(),
            research: Vec::new(),
            recent_actions: Vec::new(),
        }
    }

    pub fn sample() -> Self {
        Self {
            briefing: Some(BriefingSummary {
                greeting: "Good morning".into(),
                date: "Monday, Feb 10".into(),
                event_count: 3,
                unread_emails: 12,
                active_reminders: 2,
                top_priority: Some("Sprint planning at 10:00 AM".into()),
            }),
            events: vec![
                UpcomingEvent {
                    title: "Sprint Planning".into(),
                    time: "10:00 AM".into(),
                    location: Some("Conf Room A".into()),
                    is_conflict: false,
                },
                UpcomingEvent {
                    title: "1:1 with Manager".into(),
                    time: "11:30 AM".into(),
                    location: None,
                    is_conflict: false,
                },
                UpcomingEvent {
                    title: "Code Review Session".into(),
                    time: "2:00 PM".into(),
                    location: Some("Virtual".into()),
                    is_conflict: true,
                },
            ],
            email_groups: vec![EmailGroup {
                provider: "Gmail".into(),
                previews: vec![
                    EmailPreview {
                        from: "alice@example.com".into(),
                        subject: "Q1 Budget Review".into(),
                        snippet: "Please review the attached budget proposal...".into(),
                        time: "8:30 AM".into(),
                        important: true,
                    },
                    EmailPreview {
                        from: "bob@example.com".into(),
                        subject: "Deployment Update".into(),
                        snippet: "The v2.1 deployment completed successfully...".into(),
                        time: "7:45 AM".into(),
                        important: false,
                    },
                ],
            }],
            reminders: vec![
                ActiveReminder {
                    title: "Submit expense report".into(),
                    due: "Today, 5:00 PM".into(),
                    is_overdue: false,
                },
                ActiveReminder {
                    title: "Review PR #423".into(),
                    due: "Yesterday".into(),
                    is_overdue: true,
                },
            ],
            research: vec![ResearchProgress {
                topic: "Rust async patterns".into(),
                status: "Gathering sources".into(),
                progress_pct: 35,
            }],
            recent_actions: vec![
                RecentAction {
                    description: "Drafted reply to alice@example.com".into(),
                    timestamp: "5 min ago".into(),
                    action_type: "email".into(),
                },
                RecentAction {
                    description: "Created reminder: Submit expense report".into(),
                    timestamp: "1 hour ago".into(),
                    action_type: "reminder".into(),
                },
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct AssistantPanel;

impl AssistantPanel {
    pub fn render(data: &AssistantPanelData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("assistant-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(theme))
            .child(render_briefing_card(&data.briefing, theme))
            .child(render_events_section(&data.events, theme))
            .child(render_email_section(&data.email_groups, theme))
            .child(render_reminders_section(&data.reminders, theme))
            .child(render_research_section(&data.research, theme))
            .child(render_recent_actions(&data.recent_actions, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(40.0))
                .h(px(40.0))
                .rounded(theme.radius_lg)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .child(Icon::new(IconName::Bell).size_4()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(theme.font_size_xl)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child("Assistant"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Your daily briefing, emails, calendar, and reminders"),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Briefing card
// ---------------------------------------------------------------------------

fn render_briefing_card(briefing: &Option<BriefingSummary>, theme: &HiveTheme) -> AnyElement {
    match briefing {
        None => div()
            .flex()
            .items_center()
            .justify_center()
            .py(theme.space_4)
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child("No briefing available. Connect email and calendar to get started."),
            )
            .into_any_element(),
        Some(b) => {
            let mut card = div()
                .flex()
                .flex_col()
                .p(theme.space_4)
                .gap(theme.space_3)
                .rounded(theme.radius_md)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.accent_cyan)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(theme.font_size_lg)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::BOLD)
                                .child(format!("{}, User", b.greeting)),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_muted)
                                .child(b.date.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(theme.space_4)
                        .child(briefing_stat("Events", &b.event_count.to_string(), theme.accent_aqua, theme))
                        .child(briefing_stat("Emails", &b.unread_emails.to_string(), theme.accent_cyan, theme))
                        .child(briefing_stat("Reminders", &b.active_reminders.to_string(), theme.accent_yellow, theme)),
                );

            if let Some(ref priority) = b.top_priority {
                card = card.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            Icon::new(IconName::ArrowRight)
                                .size_3()
                                .text_color(theme.accent_cyan),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_sm)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::MEDIUM)
                                .child(format!("Next up: {priority}")),
                        ),
                );
            }

            card.into_any_element()
        }
    }
}

fn briefing_stat(label: &str, value: &str, color: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(2.0))
        .child(
            div()
                .text_size(theme.font_size_xl)
                .text_color(color)
                .font_weight(FontWeight::BOLD)
                .child(value.to_string()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
}

// ---------------------------------------------------------------------------
// Events timeline
// ---------------------------------------------------------------------------

fn render_events_section(events: &[UpcomingEvent], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Upcoming Events", theme));

    if events.is_empty() {
        section = section.child(empty_state("No upcoming events", theme));
    } else {
        for event in events {
            section = section.child(render_event_row(event, theme));
        }
    }

    section.into_any_element()
}

fn render_event_row(event: &UpcomingEvent, theme: &HiveTheme) -> AnyElement {
    let time_color = if event.is_conflict {
        theme.accent_red
    } else {
        theme.accent_cyan
    };

    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .p(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(if event.is_conflict { theme.accent_red } else { theme.border })
        .child(
            div()
                .w(px(70.0))
                .text_size(theme.font_size_sm)
                .text_color(time_color)
                .font_weight(FontWeight::MEDIUM)
                .child(event.time.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(event.title.clone()),
        );

    if let Some(ref loc) = event.location {
        row = row.child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(loc.clone()),
        );
    }

    if event.is_conflict {
        row = row.child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_red)
                .child("Conflict"),
        );
    }

    row.into_any_element()
}

// ---------------------------------------------------------------------------
// Email digest
// ---------------------------------------------------------------------------

fn render_email_section(groups: &[EmailGroup], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Email Digest", theme));

    if groups.is_empty() {
        section = section.child(empty_state("No email accounts connected", theme));
    } else {
        for group in groups {
            section = section.child(render_email_group(group, theme));
        }
    }

    section.into_any_element()
}

fn render_email_group(group: &EmailGroup, theme: &HiveTheme) -> AnyElement {
    let mut container = div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(group.provider.clone()),
                )
                .child(
                    div()
                        .px(theme.space_1)
                        .py(px(1.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_tertiary)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child(format!("{}", group.previews.len())),
                ),
        );

    for preview in &group.previews {
        container = container.child(render_email_preview(preview, theme));
    }

    container.into_any_element()
}

fn render_email_preview(preview: &EmailPreview, theme: &HiveTheme) -> AnyElement {
    let subject_color = if preview.important {
        theme.accent_cyan
    } else {
        theme.text_primary
    };

    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .py(theme.space_1)
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(subject_color)
                        .font_weight(FontWeight::MEDIUM)
                        .child(preview.subject.clone()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(preview.time.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child(preview.from.clone()),
                )
                .when(preview.important, |el: Div| {
                    el.child(
                        div()
                            .px(theme.space_1)
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_tertiary)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_yellow)
                            .child("Important"),
                    )
                }),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .overflow_hidden()
                .child(preview.snippet.clone()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Reminders
// ---------------------------------------------------------------------------

fn render_reminders_section(reminders: &[ActiveReminder], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Active Reminders", theme));

    if reminders.is_empty() {
        section = section.child(empty_state("No active reminders", theme));
    } else {
        for reminder in reminders {
            let color = if reminder.is_overdue {
                theme.accent_red
            } else {
                theme.text_primary
            };
            let due_color = if reminder.is_overdue {
                theme.accent_red
            } else {
                theme.text_muted
            };

            section = section.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .p(theme.space_2)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(if reminder.is_overdue { theme.accent_red } else { theme.border })
                    .child(
                        div()
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded(theme.radius_full)
                            .bg(color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme.font_size_sm)
                            .text_color(color)
                            .child(reminder.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(due_color)
                            .child(reminder.due.clone()),
                    ),
            );
        }
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Research progress
// ---------------------------------------------------------------------------

fn render_research_section(research: &[ResearchProgress], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Research", theme));

    if research.is_empty() {
        section = section.child(empty_state("No active research tasks", theme));
    } else {
        for item in research {
            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_1)
                    .p(theme.space_3)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(theme.font_size_sm)
                                    .text_color(theme.text_primary)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(item.topic.clone()),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.accent_cyan)
                                    .child(format!("{}%", item.progress_pct)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(item.status.clone()),
                    )
                    // Progress bar
                    .child(
                        div()
                            .w_full()
                            .h(px(4.0))
                            .rounded(theme.radius_full)
                            .bg(theme.bg_tertiary)
                            .child(
                                div()
                                    .h(px(4.0))
                                    .rounded(theme.radius_full)
                                    .bg(theme.accent_cyan)
                                    .w(relative(item.progress_pct as f32 / 100.0)),
                            ),
                    ),
            );
        }
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Recent actions
// ---------------------------------------------------------------------------

fn render_recent_actions(actions: &[RecentAction], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .child(section_title("Recent Actions", theme));

    if actions.is_empty() {
        section = section.child(empty_state("No recent actions", theme));
    } else {
        for action in actions {
            let type_color = match action.action_type.as_str() {
                "email" => theme.accent_cyan,
                "reminder" => theme.accent_yellow,
                "calendar" => theme.accent_aqua,
                "research" => theme.accent_green,
                _ => theme.text_secondary,
            };

            section = section.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme.space_2)
                    .py(theme.space_1)
                    .child(
                        div()
                            .px(theme.space_1)
                            .py(px(1.0))
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_tertiary)
                            .text_size(theme.font_size_xs)
                            .text_color(type_color)
                            .min_w(px(60.0))
                            .child(action.action_type.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .child(action.description.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(action.timestamp.clone()),
                    ),
            );
        }
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn section_title(title: &str, theme: &HiveTheme) -> Div {
    div()
        .text_size(theme.font_size_lg)
        .text_color(theme.text_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .child(title.to_string())
}

fn empty_state(message: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(theme.space_4)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(message.to_string()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_panel_data_empty() {
        let data = AssistantPanelData::empty();
        assert!(data.briefing.is_none());
        assert!(data.events.is_empty());
        assert!(data.email_groups.is_empty());
        assert!(data.reminders.is_empty());
        assert!(data.research.is_empty());
        assert!(data.recent_actions.is_empty());
    }

    #[test]
    fn assistant_panel_data_sample() {
        let data = AssistantPanelData::sample();
        assert!(data.briefing.is_some());
        assert_eq!(data.events.len(), 3);
        assert_eq!(data.email_groups.len(), 1);
        assert_eq!(data.reminders.len(), 2);
        assert_eq!(data.research.len(), 1);
        assert_eq!(data.recent_actions.len(), 2);
    }

    #[test]
    fn briefing_summary_fields() {
        let b = BriefingSummary {
            greeting: "Good morning".into(),
            date: "Monday".into(),
            event_count: 3,
            unread_emails: 5,
            active_reminders: 1,
            top_priority: Some("Meeting".into()),
        };
        assert_eq!(b.event_count, 3);
        assert_eq!(b.top_priority.as_deref(), Some("Meeting"));
    }

    #[test]
    fn upcoming_event_conflict() {
        let event = UpcomingEvent {
            title: "Meeting".into(),
            time: "10:00".into(),
            location: None,
            is_conflict: true,
        };
        assert!(event.is_conflict);
    }

    #[test]
    fn email_preview_importance() {
        let preview = EmailPreview {
            from: "test@test.com".into(),
            subject: "Test".into(),
            snippet: "...".into(),
            time: "9:00".into(),
            important: true,
        };
        assert!(preview.important);
    }

    #[test]
    fn active_reminder_overdue() {
        let r = ActiveReminder {
            title: "Task".into(),
            due: "Yesterday".into(),
            is_overdue: true,
        };
        assert!(r.is_overdue);
    }

    #[test]
    fn research_progress_bounds() {
        let rp = ResearchProgress {
            topic: "Test".into(),
            status: "In progress".into(),
            progress_pct: 50,
        };
        assert!(rp.progress_pct <= 100);
    }

    #[test]
    fn recent_action_types() {
        let a = RecentAction {
            description: "Drafted email".into(),
            timestamp: "5m ago".into(),
            action_type: "email".into(),
        };
        assert_eq!(a.action_type, "email");
    }
}
