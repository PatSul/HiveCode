# Hive Personal Assistant Capabilities Plan

## Table of Contents

1. [Current State Audit](#current-state-audit)
2. [Core Assistant Features (Must-Have)](#core-assistant-features-must-have)
3. [Extended Assistant Features (Differentiators)](#extended-assistant-features-differentiators)
4. [Integration Architecture](#integration-architecture)
5. [Safety and Privacy](#safety-and-privacy)
6. [Implementation Roadmap](#implementation-roadmap)
7. [UI Mockup Descriptions](#ui-mockup-descriptions)
8. [New Crates and Modules](#new-crates-and-modules)

---

## Current State Audit

### What Exists in the Rust Codebase

The Rust app already has substantial assistant infrastructure ported (or redesigned)
from the Electron predecessor. Here is a module-by-module breakdown.

#### hive_integrations (Fully Ported)

| Module | Status | What It Does |
|--------|--------|--------------|
| `oauth.rs` | **Complete** | OAuth 2.0 + PKCE flow. Generates auth URLs, exchanges codes, refreshes tokens. Ready for Google and Microsoft. |
| `google/email.rs` | **Complete** | Gmail API v1 client. List, get, send, create drafts, modify labels. Full REST wrapper with bearer-token auth. |
| `google/email_classifier.rs` | **Complete** | 5-layer email classification (label, sender, subject, body, combined score). Categories: Important, Personal, Work, Newsletter, Marketing, Spam, Transactional, Social. |
| `google/subscription_manager.rs` | **Complete** | Track recurring senders, parse RFC 8058/2369 unsubscribe headers, bulk unsubscribe. |
| `google/calendar.rs` | **Complete** | Google Calendar v3 client. List calendars, list/create/update/delete events, FreeBusy queries for conflict detection. |
| `google/contacts.rs` | **Complete** | Google People API client. List contacts, get by ID. |
| `google/docs.rs` | **Complete** | Google Docs API client. Get/create documents. |
| `google/drive.rs` | **Complete** | Google Drive v3 client. List files, search, get metadata, upload. |
| `google/sheets.rs` | **Complete** | Google Sheets v4 client. Read/write cell ranges. |
| `google/tasks.rs` | **Complete** | Google Tasks v1 client. List task lists, list/create/update/delete tasks. |
| `microsoft/outlook_email.rs` | **Complete** | Microsoft Graph email client. List/get/send messages. |
| `microsoft/outlook_calendar.rs` | **Complete** | Microsoft Graph calendar client. List/create events. |
| `messaging/hub.rs` | **Complete** | Unified MessagingHub facade with platform-keyed provider registry. Send/receive/list channels across platforms. |
| `messaging/provider.rs` | **Complete** | Trait definition for MessagingProvider. Platform enum: Slack, Discord, Teams, Telegram, Matrix, WebChat. |
| `messaging/slack.rs` | **Complete** | Slack provider via Web API (chat.postMessage, conversations.list, conversations.history). |
| `messaging/discord.rs` | **Complete** | Discord provider via REST API (channels, messages). |
| `messaging/teams.rs` | **Complete** | Microsoft Teams provider via Graph API (chats, messages). |
| `messaging/telegram.rs` | **Complete** | Telegram provider via Bot API (sendMessage, getUpdates, getChat). |
| `messaging/matrix.rs` | **Complete** | Matrix provider via client-server API (rooms, messages). |
| `messaging/webchat.rs` | **Complete** | Local webchat provider for in-app messaging. |
| `messaging/cross_channel.rs` | **Complete** | Cross-channel memory and message routing between platforms. |
| `smart_home.rs` | **Complete** | Philips Hue client. Bridge discovery, list/control lights, activate scenes. |
| `webhooks.rs` | **Complete** | Webhook registry. Register, dispatch events to URLs, manage subscriptions. |
| `ide.rs` | **Complete** | IDE integration service (workspace info, diagnostics, commands). |
| `cloud/cloudflare.rs` | **Complete** | Cloudflare Workers/Pages deployment. |
| `cloud/vercel.rs` | **Complete** | Vercel deployment client. |
| `cloud/supabase.rs` | **Complete** | Supabase client for database/auth. |
| `github.rs` | **Complete** | GitHub REST API client (repos, PRs, issues). |

#### hive_agents (Fully Ported + Extended)

| Module | Status | What It Does |
|--------|--------|--------------|
| `voice.rs` | **Complete** | Voice assistant with intent classification (SendMessage, SearchFiles, RunCommand, OpenPanel, CreateTask, ReadNotifications, CheckSchedule), wake word detection ("hey hive", "ok hive"), command history. |
| `automation.rs` | **Complete** | Workflow engine with triggers (Schedule, FileChange, Webhook, Manual, OnMessage, OnError), conditional steps, lifecycle management, simulated execution, run history. |
| `mcp_server.rs` | **Complete** | Built-in MCP server exposing local tools (file I/O, shell, git, search) via JSON-RPC 2.0. |
| `mcp_client.rs` | **Complete** | MCP client for connecting to external tool servers via stdio or SSE transports. |
| `hivemind.rs` | **Complete** | Multi-agent orchestration with configurable agent count and AI executor. |
| `coordinator.rs` | **Complete** | Task dispatch with dependency ordering, cost/time limits, specialist persona routing. |
| `queen.rs` | **Complete** | Meta-coordinator for swarm orchestration across multiple teams. Decomposes goals, dispatches teams, synthesizes results, records learnings. |
| `swarm.rs` | **Complete** | Swarm configuration, team objectives, orchestration modes. |
| `personas.rs` | **Complete** | Persona registry with prompt overrides per specialist type (Coder, Reviewer, Architect, etc.). |
| `skills.rs` | **Complete** | Skill engine for registerable command handlers. |
| `skill_marketplace.rs` | **Complete** | Skill directory with security scanning, categories, install/uninstall. |
| `guardian.rs` | **Complete** | Safety agent that validates actions before execution. |
| `persistence.rs` | **Complete** | Agent state persistence (snapshots, completed tasks) via SQLite. |
| `heartbeat.rs` | **Complete** | Agent health monitoring with heartbeat tracking. |
| `standup.rs` | **Complete** | Daily standup reports from agents (what was done, blockers, plans). |
| `collective_memory.rs` | **Complete** | Shared memory across agents with categories and search. |
| `tool_use.rs` | **Complete** | Tool invocation framework for agents. |
| `hiveloop.rs` | **Complete** | Continuous feedback loop for agent improvement. |
| `specs.rs` | **Complete** | Specification management for multi-agent task planning. |
| `auto_commit.rs` | **Complete** | Automatic git commit with configurable triggers. |
| `worktree.rs` | **Complete** | Git worktree management for parallel agent work. |

#### hive_core (Fully Ported)

| Module | Status | What It Does |
|--------|--------|--------------|
| `scheduler.rs` | **Complete** | Cron-like scheduler. Parses 5-field cron expressions, tick-based execution, job lifecycle management. |
| `background.rs` | **Complete** | Background task service with concurrency limits, promotion from pending to running, lifecycle tracking. |
| `notifications.rs` | **Complete** | In-memory notification store (Info, Success, Warning, Error), read/unread tracking, max-capacity truncation. |
| `secure_storage.rs` | **Complete** | AES-256-GCM encryption with Argon2id key derivation. Stores API keys and secrets encrypted at `~/.hive/`. |
| `kanban.rs` | **Complete** | Full Kanban board with columns (Todo, InProgress, Review, Done, Blocked), priorities, subtasks, comments, due dates, metrics. |
| `config.rs` | **Complete** | App configuration management at `~/.hive/`. |
| `conversations.rs` | **Complete** | Conversation persistence and history. |
| `persistence.rs` | **Complete** | SQLite database for conversations, messages, model costs, memory entries. |
| `security.rs` | **Complete** | SecurityGateway for command validation and path sanitization. |
| `context.rs` | **Complete** | Context window management with token estimation. |
| `session.rs` | **Complete** | Session state management. |
| `enterprise.rs` | **Complete** | Team management, audit logging, usage metrics. |
| `code_review.rs` | **Complete** | Code review workflows with comments and status tracking. |
| `canvas.rs` | **Complete** | Live canvas for visual collaboration. |

#### hive_shield (New -- No Electron Equivalent)

| Module | Status | What It Does |
|--------|--------|--------------|
| `pii.rs` | **Complete** | PII detection for 11+ types (Email, Phone, SSN, CreditCard, IP, Name, Address, DOB, Passport, DriversLicense, BankAccount). Cloaking formats: Placeholder, Hash, Redact. |
| `secrets.rs` | **Complete** | Secret scanning for API keys, tokens, passwords in text. Risk levels and match reporting. |
| `vulnerability.rs` | **Complete** | Vulnerability assessment for prompt injection, jailbreak attempts, and other threats. Threat levels with detailed assessments. |
| `access_control.rs` | **Complete** | Policy-based access control with data classification and provider trust levels. |
| `shield.rs` | **Complete** | Unified pipeline combining PII, secrets, vulnerability, and access control into a single scan with actions: Allow, CloakAndAllow, Block, Warn. |

#### hive_learn (New -- No Electron Equivalent)

| Module | Status | What It Does |
|--------|--------|--------------|
| `outcome_tracker.rs` | **Complete** | Records and queries AI response quality by model, task type, time window. |
| `routing_learner.rs` | **Complete** | Learns optimal model routing from outcome history. Periodic analysis at 50-interaction intervals. |
| `preference_model.rs` | **Complete** | Observes and infers user preferences (tone, detail level, etc.) with confidence scoring. |
| `prompt_evolver.rs` | **Complete** | Versioned prompt management with quality tracking, refinement, and rollback. |
| `pattern_library.rs` | **Complete** | Extracts and stores high-quality code patterns from accepted outputs. |
| `self_evaluator.rs` | **Complete** | Periodic self-assessment of overall system quality. Triggers at 200-interaction intervals. |

#### hive_assistant (New -- Phase 1 Foundation)

| Module | Status | What It Does |
|--------|--------|--------------|
| `lib.rs` | **Complete** | `AssistantService` coordinator: owns EmailService, CalendarService, ReminderService, ApprovalService. `open(db_path)` / `in_memory()` constructors. `daily_briefing()` and `tick_reminders()` methods. |
| `storage.rs` | **Complete** | `AssistantStorage` with SQLite WAL mode. Tables: reminders, email_poll_state, email_digests, approval_log. Parameterized queries throughout. |
| `approval.rs` | **Complete** | `ApprovalService` with `ApprovalRequest`, `ApprovalLevel` (Low/Medium/High/Critical), `ApprovalStatus` (Pending/Approved/Rejected). Submit/approve/reject/list_pending. |
| `plugin.rs` | **Complete** | `AssistantPlugin` async trait with `AssistantCapability` enum. Mock implementation for testing. |
| `email/mod.rs` | **Complete** | `EmailService`: `UnifiedEmail`, `EmailProvider`, `EmailDigest`. Methods: `fetch_gmail_inbox()`, `fetch_outlook_inbox()`, `build_digest()`, `send_email()` (with Shield scan), `classify()`. |
| `email/inbox_agent.rs` | **Complete** | `InboxAgent`: background polling with `poll()` returning notifications for important emails. |
| `email/compose_agent.rs` | **Complete** | `ComposeAgent`: `DraftedEmail` struct, `draft_from_instruction()` and `draft_reply()` methods. |
| `calendar/mod.rs` | **Complete** | `CalendarService`: `UnifiedEvent`, `CalendarProvider`. Methods: `today_events()`, `events_in_range()`, `create_event()`. |
| `calendar/conflict_detector.rs` | **Complete** | `ConflictReport` with severity levels. Pure `detect_conflicts()` function with proper overlap detection. |
| `calendar/smart_scheduler.rs` | **Complete** | `SchedulingSuggestion` struct. `find_available_slots()` scanning gaps between sorted events. |
| `calendar/daily_brief.rs` | **Complete** | `DailyBriefing` combining events + email digest + reminders + auto-generated action items. |
| `reminders/mod.rs` | **Complete** | `ReminderService`: `Reminder`, `ReminderTrigger` (At/Recurring/OnEvent), `ReminderStatus`. CRUD + tick + snooze + complete + dismiss. |
| `reminders/os_notifications.rs` | **Complete** | `show_toast()` via `winrt-notification` on Windows, no-op elsewhere. cfg-gated. |

**105 tests, 3,196 lines of Rust, 0 compiler warnings.**

#### hive_ui (18 Panels)

Existing panels: Chat, History, Files, Specs, Agents, Kanban, Monitor, Logs, Costs, Review, Skills, Routing, Learning, Shield, **Assistant**, TokenLaunch, Settings, Help.

**The Assistant panel is live** with briefing card, events timeline, email digest, reminders, research progress, and recent actions sections.

### Electron Features NOT Yet in Rust

The following Electron modules have no direct Rust equivalent yet:

| Electron Module | Status | Notes |
|----------------|--------|-------|
| `assistant/background-service.ts` | **Partial** | `hive_core::background` covers task management. `hive_assistant::reminders` adds tick-based reminder service. Still lacks proactive "run while user is away" logic. |
| `assistant/notification-service.ts` | **Complete** | `hive_core::notifications` for in-app. `hive_assistant::reminders::os_notifications` delivers OS-level toast notifications via `winrt-notification` on Windows. |
| `assistant/proactive-outreach.ts` | **Missing** | No proactive assistant that reaches out based on context (e.g., "I noticed you have a meeting in 15 minutes"). |
| `assistant/scheduler.ts` | **Ported** | `hive_core::scheduler` is a full cron implementation. |
| `assistant/task-executor.ts` | **Partial** | `hive_core::background` + `hive_agents::automation` cover this, but need glue. |
| `browser/browser-automation.ts` | **Missing** | No headless browser automation (Playwright/Puppeteer equivalent). |
| `browser/browser-pool.ts` | **Missing** | No browser pool management. |
| `browser/system-control.ts` | **Missing** | No OS-level automation (mouse, keyboard, window management). |
| `companion/companion-service.ts` | **Missing** | No always-on companion personality that maintains conversation state across sessions. |
| `messaging/signal-provider.ts` | **Missing** | Signal integration not ported (requires libsignal). |
| `messaging/whatsapp-provider.ts` | **Missing** | WhatsApp integration not ported (requires WhatsApp Business API). |
| `messaging/imessage-provider.ts` | **Missing** | iMessage integration not ported (macOS-only, requires AppleScript). |
| `voice/voice-assistant.ts` | **Ported** | `hive_agents::voice` covers intent classification and wake words. Missing: actual audio capture/STT/TTS pipeline. |
| `voice/wake-word-service.ts` | **Ported** | Text-based wake word detection in `hive_agents::voice`. Missing: audio-level wake word (e.g., Porcupine). |
| `integrations/location-service.ts` | **Missing** | No location awareness (GPS, IP geolocation). |
| `integrations/screen-service.ts` | **Missing** | No screen capture or OCR. |

### Summary Scorecard

| Category | Electron Modules | Rust Modules | Coverage |
|----------|-----------------|--------------|----------|
| Email/Calendar/Tasks | 7 | 7 | **100%** |
| Messaging | 10 | 7 | **70%** (missing Signal, WhatsApp, iMessage) |
| Smart Home | 1 | 1 | **100%** |
| Voice | 3 | 1 (partial) | **33%** (text classification only, no audio) |
| Browser/System Automation | 3 | 0 | **0%** |
| Background Assistant | 5 | 5 (partial) | **80%** (hive_assistant adds reminders, briefings, email polling) |
| OAuth/Webhooks | 3 | 3 | **100%** |
| Security/Shield | 2 | 6 | **300%** (exceeds Electron) |
| Learning/Adaptation | 0 | 8 | **New** (no Electron equivalent) |
| Multi-Agent Orchestration | 3 | 12 | **400%** (far exceeds Electron) |

---

## Core Assistant Features (Must-Have)

### 1. Email Management

**Current state:** Gmail and Outlook REST clients are fully implemented in
`hive_integrations::google::email` and `hive_integrations::microsoft::outlook_email`.
The email classifier and subscription manager are also complete.

**What needs building:**

```
hive/crates/hive_assistant/src/email/
  mod.rs              -- Unified email service wrapping Gmail + Outlook
  inbox_agent.rs      -- Background agent that polls for new mail, classifies, and surfaces summaries
  compose_agent.rs    -- AI-powered email composition from natural language ("email John about the project delay")
  smart_reply.rs      -- Generate contextual reply suggestions based on thread history
  batch_actions.rs    -- Bulk archive, label, unsubscribe operations driven by classifier output
```

**Key behaviors:**
- Poll for new emails every N minutes (configurable via `hive_core::scheduler`).
- Classify incoming emails using `EmailClassifier`, surface important ones as notifications.
- Support natural language composition: "Draft an email to sarah@example.com declining the meeting politely."
- Route email drafts through `hive_shield` PII detection before sending.
- Maintain an email digest: group and summarize unread emails by category.
- Track and execute unsubscribe actions via `SubscriptionManager`.

**Dependencies already satisfied:**
- OAuth2 flow (`hive_integrations::oauth`)
- Gmail/Outlook clients (full REST wrappers)
- Email classifier (5-layer, pattern-based)
- Secure token storage (`hive_core::secure_storage`)
- Background task scheduling (`hive_core::scheduler` + `hive_core::background`)

### 2. Calendar and Scheduling

**Current state:** Google Calendar and Outlook Calendar clients are complete,
including FreeBusy queries for conflict detection.

**What needs building:**

```
hive/crates/hive_assistant/src/calendar/
  mod.rs              -- Unified calendar service wrapping Google + Outlook
  conflict_detector.rs -- Analyze upcoming events, detect double-bookings, suggest resolutions
  smart_scheduler.rs  -- Find optimal meeting times across multiple calendars
  event_agent.rs      -- Create events from natural language ("Schedule a 1:1 with Alex next Tuesday at 2pm")
  daily_brief.rs      -- Generate morning briefing of today's schedule with travel time estimates
```

**Key behaviors:**
- Morning briefing: at user-configured time, generate a summary of today's events.
- Conflict detection: when a new event is proposed, run FreeBusy queries against all calendars.
- Smart scheduling: given attendees and duration, find the first available slot using FreeBusy.
- Natural language event creation: parse "lunch with Sarah at noon on Friday" into a `CreateEventRequest`.
- Reminder integration: push reminders N minutes before events via `hive_core::notifications`.
- Cross-calendar view: merge Google + Outlook events into a single timeline.

**Dependencies already satisfied:**
- Google Calendar client with FreeBusy queries
- Outlook Calendar client with event CRUD
- OAuth2 flow for both providers
- Cron scheduler for periodic briefings
- Notification store for reminders

### 3. Reminders and Notifications

**Current state:** `hive_core::notifications::NotificationStore` handles in-app
notifications. `hive_core::scheduler::Scheduler` supports cron-based scheduling.
No OS-level notification delivery exists yet.

**What needs building:**

```
hive/crates/hive_assistant/src/reminders/
  mod.rs              -- Reminder service with time-based, location-based, and context-aware triggers
  os_notifications.rs -- Bridge to OS notification APIs (Win32 toast, macOS UNUserNotification)
  context_triggers.rs -- Fire reminders based on context (e.g., "remind me when I open the project")
  recurring.rs        -- Recurring reminder management with snooze/dismiss
```

**Key behaviors:**
- Time-based reminders: "Remind me to call the dentist at 3pm."
- Recurring reminders: "Every Monday morning, remind me to submit the weekly report."
- Context-aware: "When I next open the hive-ui project, remind me to fix the layout bug."
- OS-level delivery: Windows toast notifications via the `winrt-notification` crate, system tray balloon fallback.
- Snooze and dismiss actions directly from the notification.
- All reminders persisted to SQLite so they survive app restarts.

**New dependencies needed:**
- `winrt-notification` or `notify-rust` for OS notifications
- Location APIs are deferred to Phase 4

### 4. Research Agent

**Current state:** The multi-agent orchestration system (Queen, HiveMind,
Coordinator) is fully built. MCP client/server support exists. No dedicated
research persona or background research loop exists.

**What needs building:**

```
hive/crates/hive_assistant/src/research/
  mod.rs              -- Research agent coordinator
  web_search.rs       -- Web search integration (via MCP tool servers or direct API: Brave Search, SerpAPI)
  summarizer.rs       -- Document/page summarization using AI models
  topic_monitor.rs    -- Track topics of interest and surface new developments
  clip_collector.rs   -- Save research findings to a local knowledge base
```

**Key behaviors:**
- Background research: "Research the latest Rust async runtime developments and have a summary ready for me tomorrow."
- Topic monitoring: Watch RSS feeds, HackerNews, specific GitHub repos for updates on tracked topics.
- Document summarization: Given a URL or file, produce a concise summary.
- Knowledge base: Store research findings in `hive_core::persistence` with full-text search.
- Integration with existing RAG pipeline (`hive_ai::rag`) for semantic search over collected research.
- Cost-aware: Use `hive_ai::routing::model_router` to pick cheap models for summarization, expensive models for synthesis.

**Dependencies already satisfied:**
- Multi-agent orchestration (HiveMind, Coordinator, Queen)
- AI model routing with cost optimization
- RAG and semantic search (`hive_ai::rag`, `hive_ai::semantic_search`)
- Background task service
- Persistent storage (SQLite)
- MCP client for connecting to external search tools

### 5. Task Management

**Current state:** `hive_core::kanban` is a full Kanban board with columns,
priorities, subtasks, comments, due dates, and metrics. Google Tasks client exists
in `hive_integrations::google::tasks`. A UI panel exists at `hive_ui::panels::kanban`.

**What needs building:**

```
hive/crates/hive_assistant/src/tasks/
  mod.rs              -- Unified task service bridging local Kanban + Google Tasks
  nl_parser.rs        -- Natural language task creation ("Add a high-priority task to fix the login bug by Friday")
  deadline_tracker.rs -- Monitor approaching deadlines and escalate via notifications
  daily_planner.rs    -- AI-generated daily plan based on task priorities, calendar, and context
  sync.rs             -- Two-way sync between local Kanban board and Google Tasks
```

**Key behaviors:**
- Natural language task creation with priority, assignee, due date extraction.
- Daily planner: each morning, suggest which tasks to focus on based on deadlines, priorities, and calendar load.
- Deadline alerts: escalating reminders as due dates approach (7 days, 3 days, 1 day, overdue).
- Two-way sync with Google Tasks so tasks created in Hive appear in Google and vice versa.
- Integration with the Kanban UI panel for drag-and-drop management.
- Weekly review: summarize what was completed, what slipped, and suggest re-prioritization.

---

## Extended Assistant Features (Differentiators)

### 6. Phone and Reservations

**New module:**

```
hive/crates/hive_assistant/src/reservations/
  mod.rs              -- Reservation service coordinator
  restaurant.rs       -- OpenTable/Resy API integration for restaurant bookings
  appointments.rs     -- Generic appointment booking (dentist, doctor, salon)
  confirmation.rs     -- Track confirmation emails, extract booking details, add to calendar
```

**Key behaviors:**
- "Book a table for 2 at an Italian restaurant near downtown for Friday at 7pm."
- Search available restaurants via OpenTable/Resy APIs, present options, confirm booking.
- After booking, automatically create a calendar event with the restaurant details.
- Monitor email for booking confirmations and extract structured data (time, address, confirmation number).
- Cancellation and modification support.

**New dependencies:**
- OpenTable API client (REST, requires partner access)
- Resy API client (REST)
- Address/geocoding service for "near downtown" resolution (deferred if no location service)

### 7. Shopping and Groceries

**New module:**

```
hive/crates/hive_assistant/src/shopping/
  mod.rs              -- Shopping coordinator
  grocery_list.rs     -- Manage a persistent grocery list with categories
  instacart.rs        -- Instacart API integration for grocery delivery
  amazon.rs           -- Amazon product search and order tracking
  price_tracker.rs    -- Track prices of items and alert on drops
```

**Key behaviors:**
- "Add milk, eggs, and bread to my grocery list."
- "Order my grocery list from Instacart for delivery tomorrow."
- "Track the price of the Sony WH-1000XM5 headphones on Amazon."
- Maintain persistent shopping lists in SQLite, organized by category (Produce, Dairy, etc.).
- All purchase actions require user confirmation (see Safety section).

### 8. Smart Home (Extended)

**Current state:** Philips Hue client is complete.

**What needs building:**

```
hive/crates/hive_assistant/src/smart_home/
  mod.rs              -- Unified smart home coordinator
  scenes.rs           -- Define custom multi-device scenes ("Movie mode": dim lights, set warm tone)
  thermostat.rs       -- Ecobee/Nest thermostat integration
  routines.rs         -- Time-based and event-based routines ("When I say goodnight, turn off all lights")
  device_registry.rs  -- Discover and track all smart home devices across platforms
```

**Key behaviors:**
- "Turn off all lights in the living room."
- "Set the thermostat to 72 degrees."
- "Activate movie mode." (custom scene: dim lights to 20%, set color to warm white)
- Scheduled routines via `hive_core::scheduler`.
- Device state dashboard in the UI.

**New dependencies:**
- Ecobee API client (REST + OAuth2)
- Nest/Google Home API (REST + OAuth2)
- MQTT client for generic IoT devices (future)

### 9. Document Creation

**Current state:** Workspace already includes `rust_xlsxwriter`, `docx-rs`, and
`zip` crates for document generation.

**What needs building:**

```
hive/crates/hive_assistant/src/documents/
  mod.rs              -- Document generation coordinator
  report_builder.rs   -- Generate structured reports (weekly status, project summary)
  email_template.rs   -- Template-based email composition for recurring messages
  presentation.rs     -- Generate slide outlines (export to Google Slides or PPTX)
  formatter.rs        -- Convert between formats (Markdown to DOCX, CSV to XLSX)
```

**Key behaviors:**
- "Generate a weekly status report from my completed tasks and git commits."
- "Create a project proposal document from this spec."
- "Convert this CSV to a formatted Excel spreadsheet."
- Use AI to draft content, then render to DOCX/XLSX using existing crates.
- Export to Google Docs/Drive via existing integration clients.

### 10. Financial Tracking

**New module:**

```
hive/crates/hive_assistant/src/finance/
  mod.rs              -- Financial tracking coordinator
  expenses.rs         -- Manual expense entry and categorization
  budget.rs           -- Budget management with category limits and alerts
  receipt_parser.rs   -- Extract vendor, amount, date from receipt images/emails
  reports.rs          -- Monthly/yearly spending reports and charts data
```

**Key behaviors:**
- "Log a $45 expense for lunch with the team, category: meals."
- "What's my spending this month in the 'software subscriptions' category?"
- Monitor email for purchase receipts, auto-extract and categorize.
- Budget alerts when category spending approaches limits.
- All financial data encrypted at rest using `hive_core::secure_storage`.

**Note:** No bank account integration (Plaid) in initial phases due to security/compliance complexity.
Start with manual entry + email receipt parsing.

### 11. Travel Planning

**New module:**

```
hive/crates/hive_assistant/src/travel/
  mod.rs              -- Travel planning coordinator
  flight_search.rs    -- Flight search via Amadeus/Skyscanner API
  hotel_search.rs     -- Hotel search via Booking.com/Hotels.com API
  itinerary.rs        -- Build and manage trip itineraries
  travel_alerts.rs    -- Flight status monitoring, gate changes, delays
```

**Key behaviors:**
- "Find flights from SFO to JFK next Friday, returning Sunday."
- "Build an itinerary for a 3-day trip to Tokyo."
- After booking, monitor flight status and alert on delays/gate changes.
- Create calendar events for each leg of the trip.
- Store trip documents (boarding passes, hotel confirmations) in an organized local folder.

### 12. Health and Wellness

**New module:**

```
hive/crates/hive_assistant/src/health/
  mod.rs              -- Health tracking coordinator
  medication.rs       -- Medication schedule with recurring reminders
  appointments.rs     -- Medical appointment tracking with reminder integration
  health_log.rs       -- Daily health data logging (sleep, exercise, mood)
  wellness_check.rs   -- Periodic check-in prompts ("How are you feeling today?")
```

**Key behaviors:**
- "Remind me to take my medication at 8am and 8pm daily."
- "Schedule a dentist appointment for next month and remind me 1 week before."
- Health data is strictly local-only (never sent to cloud APIs).
- All health data encrypted with `hive_core::secure_storage`.

---

## Integration Architecture

### OAuth2 Flow for Cloud Services

```
+------------------+     +------------------+     +-------------------+
|   Hive UI        |     | hive_integrations|     | Cloud Provider    |
|  (Settings Panel)|     |  ::oauth         |     | (Google, MSFT)    |
+--------+---------+     +--------+---------+     +---------+---------+
         |                         |                         |
         | 1. User clicks          |                         |
         |    "Connect Google"     |                         |
         +------------------------>|                         |
         |                         | 2. Generate auth URL    |
         |                         |    with PKCE challenge  |
         |                         +------------------------>|
         |                         |                         | 3. User consents
         |                         |                         |    in browser
         |                         | 4. Redirect with code   |
         |                         |<------------------------+
         |                         | 5. Exchange code        |
         |                         |    for tokens           |
         |                         +------------------------>|
         |                         | 6. Receive tokens       |
         |                         |<------------------------+
         |                         | 7. Encrypt & store      |
         |                         |    via SecureStorage     |
         | 8. Connected!           |                         |
         |<------------------------+                         |
```

**Token lifecycle:**
- Tokens stored encrypted at `~/.hive/tokens/` using `hive_core::secure_storage`.
- `OAuthClient::is_expired()` checks 30-second safety margin before each API call.
- Auto-refresh via `OAuthClient::refresh_token()` when access token expires.
- Revocation on user disconnect request.

### Webhook Receivers for Real-Time Updates

The `WebhookRegistry` in `hive_integrations::webhooks` is the foundation. For
real-time assistant features, we need a local HTTP listener.

```
hive/crates/hive_assistant/src/webhooks/
  listener.rs         -- Lightweight HTTP server (axum or warp) on localhost:PORT
  gmail_push.rs       -- Gmail push notification handler (via Google Pub/Sub)
  calendar_push.rs    -- Calendar change notification handler
  dispatcher.rs       -- Route incoming webhooks to the appropriate assistant module
```

**New workspace dependency:** `axum` (lightweight, tokio-native HTTP framework).

**How it works:**
1. When the user connects Gmail, register a watch on the inbox via Gmail Push API.
2. Google sends notifications to a configured endpoint (ngrok tunnel for dev, cloud relay for prod).
3. Local listener receives the notification, verifies it, and dispatches to the email agent.
4. Email agent fetches new messages, classifies them, and surfaces notifications.

### Local-First Data Storage with Encrypted Sync

```
~/.hive/
  config.toml           -- App configuration (hive_core::config)
  storage.salt          -- Argon2id salt for SecureStorage (hive_core::secure_storage)
  tokens/               -- Encrypted OAuth tokens per provider
    google.enc
    microsoft.enc
  db/
    conversations.db    -- Chat history (hive_core::persistence)
    learning.db         -- Learning subsystem data (hive_learn)
    assistant.db        -- NEW: Assistant-specific data (reminders, tasks, research, finances)
    agents.db           -- Agent persistence (hive_agents::persistence)
  research/             -- NEW: Saved research documents and summaries
  documents/            -- NEW: Generated documents
  receipts/             -- NEW: Scanned receipts and financial records
```

**Encryption at rest:** All `.db` files containing user data should use SQLite
encryption via `sqlcipher` or application-level encryption for sensitive fields
using `SecureStorage::encrypt()`.

**Sync (future):** End-to-end encrypted sync between devices via user's own
cloud storage (Google Drive, iCloud, OneDrive). The app never has access to
unencrypted data on the wire.

### MCP Server for Extensibility

The existing `hive_agents::mcp_server` exposes workspace tools. For assistant
features, we extend it with assistant-specific tools.

```
New MCP tools to register:
  assistant/send_email        -- Compose and send an email
  assistant/create_event      -- Create a calendar event
  assistant/set_reminder      -- Set a time-based reminder
  assistant/search_emails     -- Search email by query
  assistant/check_calendar    -- Query calendar for a time range
  assistant/manage_tasks      -- CRUD operations on Kanban tasks
  assistant/control_lights    -- Smart home light control
  assistant/web_search        -- Perform a web search
  assistant/create_document   -- Generate a document from template
```

This allows external AI agents (Claude, GPT, etc.) to interact with the user's
assistant features via the standard MCP protocol.

### Plugin System for Third-Party Integrations

The `hive_agents::skill_marketplace` already provides:
- Skill discovery and installation
- Security scanning before install
- Category-based organization
- Enable/disable per skill

For assistant plugins, define a `AssistantPlugin` trait:

```rust
// hive_assistant/src/plugin.rs
#[async_trait]
pub trait AssistantPlugin: Send + Sync {
    /// Unique identifier for this plugin.
    fn id(&self) -> &str;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// What capabilities this plugin provides.
    fn capabilities(&self) -> Vec<AssistantCapability>;

    /// Handle a natural language request relevant to this plugin.
    async fn handle_request(&self, request: &AssistantRequest) -> Result<AssistantResponse>;

    /// Called periodically for background work.
    async fn tick(&self) -> Result<()>;
}

pub enum AssistantCapability {
    EmailProvider,
    CalendarProvider,
    TaskProvider,
    SmartHomeDevice,
    SearchEngine,
    ShoppingService,
    Custom(String),
}
```

---

## Safety and Privacy

### PII Detection Before Cloud API Calls

The `hive_shield` crate is the most advanced safety subsystem in the codebase. Every
outbound API call must pass through the shield pipeline.

**Integration points:**

```
User says: "Email John about the meeting. My SSN is 123-45-6789."
                          |
                          v
               +-------------------+
               |   HiveShield      |
               |   .scan(message)  |
               +--------+----------+
                        |
          +-------------+-------------+
          |             |             |
    PII Detector   Secret Scanner  Vuln Assessor
    (finds SSN)    (no secrets)    (no threats)
          |             |             |
          v             v             v
    ShieldResult {
      action: CloakAndAllow(CloakedText { text: "Email John about the meeting. My SSN is [SSN_1]." }),
      pii_found: [PiiMatch { pii_type: SSN, value: "123-45-6789", ... }],
      ...
    }
                          |
                          v
         User sees: "I detected an SSN in your message.
                     Should I remove it before sending?"
                          |
                    [Yes] / [No, send anyway]
```

**Mandatory scan points:**
1. Before sending any email (compose, reply, forward).
2. Before posting to any messaging platform.
3. Before sending context to AI model APIs.
4. Before storing research findings that might contain others' PII.
5. Before generating documents that include user data.

### Secrets Management

**Current:** `hive_core::secure_storage` provides AES-256-GCM encryption.
`hive_shield::secrets` scans for leaked API keys.

**Extended for assistant:**
- All OAuth tokens encrypted at rest.
- API keys for third-party services (OpenTable, Instacart, etc.) stored via `SecureStorage`.
- Periodic secret rotation reminders.
- Never log tokens or keys (tracing filter).
- Memory wipe: `SecureStorage` keys zeroed on drop.

### User Approval Workflows

**Sensitive actions that ALWAYS require user confirmation:**

| Action | Risk Level | Approval Flow |
|--------|-----------|---------------|
| Send email | Medium | Show preview in UI, require click to send |
| Create calendar event with attendees | Medium | Show event details, require confirmation |
| Make a purchase | High | Show item, price, payment method. Require explicit "Buy" click |
| Post to external messaging platform | Medium | Show message preview, require send confirmation |
| Share documents | Medium | Show what will be shared and with whom |
| Execute shell command | High | Show command, require approval (existing SecurityGateway) |
| Delete data | High | Show what will be deleted, require typed confirmation |
| Connect new OAuth service | Medium | Show requested permissions, require consent |

**Implementation:**

```rust
// hive_assistant/src/approval.rs

pub enum ApprovalLevel {
    /// No approval needed (reading data, local operations).
    Auto,
    /// Show a notification, proceed unless user objects within N seconds.
    NotifyAndProceed { timeout_secs: u32 },
    /// Require explicit user click to proceed.
    RequireConfirmation,
    /// Require typed confirmation phrase for destructive actions.
    RequireTypedConfirmation { phrase: String },
}

pub struct ApprovalRequest {
    pub id: String,
    pub action_description: String,
    pub level: ApprovalLevel,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub struct ApprovalService {
    pending: Vec<ApprovalRequest>,
    // Connected to UI via channels
}
```

### Audit Log

Every action taken on behalf of the user is recorded:

```rust
// Lives in hive_core::enterprise (AuditEntry already exists)
pub struct AssistantAuditEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub action: String,           // "email.send", "calendar.create", "lights.toggle"
    pub target: String,           // "john@example.com", "Team Meeting", "Living Room"
    pub approved_by: String,      // "user_click", "auto", "timeout"
    pub shield_result: Option<ShieldResult>,
    pub success: bool,
    pub error: Option<String>,
}
```

Stored in `~/.hive/db/assistant.db` and viewable in the Shield panel.

### Dry Run Mode

```rust
// hive_assistant/src/dry_run.rs

pub struct DryRunResult {
    pub action_description: String,
    pub would_affect: Vec<String>,    // "Would send email to john@example.com"
    pub estimated_cost: Option<f64>,  // API cost estimate
    pub shield_scan: ShieldResult,    // What the shield found
    pub reversible: bool,             // Can this action be undone?
}
```

When dry run mode is enabled (toggle in Settings panel):
- All actions are simulated but not executed.
- User sees exactly what would happen.
- Useful for testing automation workflows before activating them.

---

## Implementation Roadmap

### Phase 1: Email + Calendar + Reminders (Core Productivity)

**Timeline:** 4-6 weeks
**Priority:** Highest -- these are the daily-driver features.

**Tasks:**

- [x] Create `hive_assistant` crate in workspace (13 crates total, 13 source files, 105 tests)
- [x] Implement unified email service wrapping Gmail + Outlook clients (`hive_assistant::email`)
- [x] Build inbox polling agent with `Scheduler` integration (`hive_assistant::email::inbox_agent`)
- [x] Implement AI-powered email composition via chat input (`hive_assistant::email::compose_agent`)
- [ ] Build smart reply suggestions using AI models (`smart_reply.rs` -- deferred)
- [x] Implement unified calendar service wrapping Google + Outlook (`hive_assistant::calendar`)
- [x] Build conflict detection using FreeBusy queries (`hive_assistant::calendar::conflict_detector`)
- [x] Implement smart scheduling (find available slots) (`hive_assistant::calendar::smart_scheduler`)
- [ ] Build natural language event creation parser (`event_agent.rs` -- deferred)
- [x] Create morning briefing generator (schedule + email digest) (`hive_assistant::calendar::daily_brief`)
- [x] Implement OS notification delivery (`winrt-notification` on Windows) (`hive_assistant::reminders::os_notifications`)
- [x] Build reminder service with time-based triggers (`hive_assistant::reminders`)
- [x] Add recurring reminder support with snooze/dismiss (`ReminderTrigger::Recurring`)
- [x] Persist reminders to SQLite (`hive_assistant::storage`)
- [x] Create "Assistant" UI panel in `hive_ui` (18th panel, Bell icon, full dashboard)
- [x] Wire approval workflow for email sending (`hive_assistant::approval`)
- [x] Add Shield scanning to all outbound messages (wired in `workspace.rs::handle_send_text`)
- [ ] Add OAuth connection UI to Settings panel

**New crate:**

```toml
# hive/crates/hive_assistant/Cargo.toml
[package]
name = "hive_assistant"
version = "0.1.0"
edition = "2024"

[dependencies]
hive_core = { path = "../hive_core" }
hive_ai = { path = "../hive_ai" }
hive_integrations = { path = "../hive_integrations" }
hive_shield = { path = "../hive_shield" }
hive_agents = { path = "../hive_agents" }

tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
chrono.workspace = true
uuid.workspace = true
tracing.workspace = true
rusqlite.workspace = true
async-trait.workspace = true
```

**Workspace Cargo.toml addition:**

```toml
members = [
    # ... existing members ...
    "crates/hive_assistant",
]
```

### Phase 2: Research Agent + Task Management (Background Intelligence)

**Timeline:** 3-4 weeks (after Phase 1)
**Priority:** High -- makes the assistant genuinely useful while user is away.

**Tasks:**

- [ ] Build web search integration via MCP tool server (Brave Search API)
- [ ] Implement document summarization using AI models
- [ ] Build topic monitoring with configurable watch list
- [ ] Create research knowledge base with full-text search in SQLite
- [ ] Integrate research with existing RAG pipeline (`hive_ai::rag`)
- [ ] Build natural language task creation parser
- [ ] Implement deadline tracker with escalating notifications
- [ ] Build daily planner (AI-generated task prioritization)
- [ ] Implement two-way sync between Kanban and Google Tasks
- [ ] Build weekly review summarizer
- [ ] Add research results to Assistant panel
- [ ] Add task dashboard widget to Assistant panel

### Phase 3: Shopping + Reservations + Documents (Lifestyle)

**Timeline:** 4-5 weeks (after Phase 2)
**Priority:** Medium -- lifestyle convenience features.

**Tasks:**

- [ ] Build grocery list management with categories
- [ ] Implement restaurant search and booking (OpenTable/Resy APIs)
- [ ] Build booking confirmation parser (extract from emails)
- [ ] Create automatic calendar event creation from bookings
- [ ] Build report generator using `docx-rs` and `rust_xlsxwriter`
- [ ] Implement email template system for recurring messages
- [ ] Build format converter (Markdown to DOCX, CSV to XLSX)
- [ ] Create expense tracker with manual entry
- [ ] Build receipt parser for email-based receipts
- [ ] Implement budget management with category alerts
- [ ] Add shopping/reservations/documents sections to Assistant panel

### Phase 4: Voice + Smart Home + Financial (Ambient Assistant)

**Timeline:** 5-6 weeks (after Phase 3)
**Priority:** Lower -- ambient/passive features.

**Tasks:**

- [ ] Integrate audio capture for voice input (platform-specific: WASAPI on Windows)
- [ ] Add speech-to-text via Whisper API or local model
- [ ] Add text-to-speech for voice responses
- [ ] Integrate wake word detection (Porcupine SDK or custom)
- [ ] Extend smart home support: Ecobee/Nest thermostat
- [ ] Build custom scene management across devices
- [ ] Implement time-based and event-based smart home routines
- [ ] Build flight search integration (Amadeus API)
- [ ] Build hotel search integration (Booking.com API)
- [ ] Create itinerary builder with calendar integration
- [ ] Build flight status monitoring with delay alerts
- [ ] Implement medication reminder system
- [ ] Build medical appointment tracker
- [ ] Create health data logging (local-only, encrypted)
- [ ] Add voice indicator and smart home controls to Assistant panel

---

## UI Mockup Descriptions

### New Panel: "Assistant" (Panel #18)

Add to `hive_ui::sidebar::Panel`:

```rust
pub enum Panel {
    // ... existing 17 panels ...
    Assistant,  // NEW -- 18th panel
}
```

**Icon:** Bell or sparkle icon (IconName::Bell or a custom assistant icon).
**Keyboard shortcut:** The first available unbound key.

### Assistant Panel Layout

The Assistant panel is divided into a **feed view** (primary) and a **sidebar
quick-actions area** (secondary). It is the hub for all personal assistant
interactions that aren't coding-related.

```
+------------------------------------------------------------------+
|  ASSISTANT                                      [Dry Run: OFF] [G]|
+------------------------------------------------------------------+
|                                                                    |
|  TODAY - Monday, Feb 10                                           |
|  +--------------------------+  +-------------------------------+  |
|  | MORNING BRIEFING         |  | QUICK ACTIONS                |  |
|  | 3 meetings today         |  |                               |  |
|  | 12 unread emails         |  | [+ New Reminder]             |  |
|  | (2 important)            |  | [+ Compose Email]            |  |
|  | 5 tasks due this week    |  | [+ Schedule Meeting]         |  |
|  +--------------------------+  | [+ Add Task]                 |  |
|                                | [+ Start Research]           |  |
|  UPCOMING                      |                               |  |
|  +----------------------------+-------------------------------+  |
|  | 10:00  Team Standup       |  Google Meet                   |  |
|  | 12:00  Lunch with Sarah   |  Cafe Roma                     |  |
|  | 14:00  1:1 with Alex      |  Zoom                          |  |
|  +----------------------------+-------------------------------+  |
|                                                                    |
|  EMAIL DIGEST                                                     |
|  +--------------------------------------------------------------+|
|  | [!] Important: 2 messages                                     ||
|  |   - John D: "Re: Q1 Budget Review" (30m ago)                ||
|  |   - HR Team: "Benefits enrollment deadline" (2h ago)         ||
|  |                                                               ||
|  | [i] Work: 5 messages                                         ||
|  |   - GitHub: 3 PR reviews requested                           ||
|  |   - Jira: 2 issue updates                                    ||
|  |                                                               ||
|  | [~] Newsletters: 5 (auto-archived)                           ||
|  +--------------------------------------------------------------+|
|                                                                    |
|  ACTIVE REMINDERS                                                 |
|  +--------------------------------------------------------------+|
|  | [bell] Call dentist (today at 3:00 PM)          [Snooze][Done]||
|  | [bell] Submit weekly report (Mon at 9:00 AM)    [Snooze][Done]||
|  +--------------------------------------------------------------+|
|                                                                    |
|  RESEARCH IN PROGRESS                                             |
|  +--------------------------------------------------------------+|
|  | [~] "Rust async patterns 2026" -- 3/5 sources analyzed       ||
|  |     ETA: ~10 minutes remaining                                ||
|  +--------------------------------------------------------------+|
|                                                                    |
|  RECENT ACTIONS                                                   |
|  +--------------------------------------------------------------+|
|  | [check] Email sent to john@example.com (5m ago)              ||
|  | [check] Event "1:1 with Alex" created (1h ago)               ||
|  | [check] Lights set to 50% brightness (2h ago)                ||
|  +--------------------------------------------------------------+|
+------------------------------------------------------------------+
```

### Key UI Interactions

**1. Morning Briefing Card:**
- Displayed at the top each day, generated via `daily_brief.rs`.
- Shows count of meetings, unread emails, due tasks.
- Click to expand into full daily schedule view.
- Accent color: #00D4FF (cyan) for the card header.

**2. Quick Actions Panel:**
- Fixed sidebar on the right (or collapsible).
- Each button opens a modal or inline form.
- "Compose Email" opens a mini composer with To/Subject/Body fields.
- "Schedule Meeting" opens a time picker with attendee input.
- "Start Research" opens a text field for the research topic.

**3. Email Digest:**
- Grouped by category (Important, Work, Newsletters, etc.).
- Important emails highlighted with accent color.
- Click an email to expand inline with action buttons (Reply, Archive, Forward).
- "Reply" opens the AI-powered reply composer.

**4. Calendar Timeline:**
- Horizontal timeline of today's events.
- Color-coded by calendar source (Google = blue, Outlook = green).
- Click an event to see details and join link.
- Conflicts highlighted in red.

**5. Approval Notifications:**
When an action needs approval, an inline card appears at the top:

```
+--------------------------------------------------------------+
| [!] APPROVAL NEEDED                                          |
|                                                              |
| Send email to john@example.com                               |
| Subject: "Re: Q1 Budget Review"                             |
|                                                              |
| Shield scan: PASS (no PII detected)                         |
|                                                              |
| [Preview Full Email]  [Approve & Send]  [Edit]  [Cancel]    |
+--------------------------------------------------------------+
```

**6. Dry Run Mode Toggle:**
- Toggle button in the panel header.
- When enabled, all actions show "DRY RUN" badge and display what would happen without executing.
- The panel header changes to a subtle yellow tint to indicate dry run mode.

**7. Settings Integration:**
The Settings panel (`hive_ui::panels::settings`) needs new sections:

```
CONNECTED ACCOUNTS
  [G] Google (john@gmail.com)              [Disconnect]
  [M] Microsoft (john@outlook.com)         [Disconnect]
  [+] Connect new account...

ASSISTANT PREFERENCES
  Morning briefing time: [8:00 AM]
  Email check frequency: [Every 5 minutes]
  Research model tier: [Standard (cost-optimized)]
  Dry run mode: [OFF]
  Voice wake word: [hey hive]

NOTIFICATION PREFERENCES
  OS notifications: [ON]
  Email digest: [Grouped by category]
  Calendar reminders: [15 minutes before]
  Task deadline alerts: [ON]

PRIVACY
  Shield scan level: [Standard]
  PII auto-cloak: [ON]
  Audit log retention: [30 days]
```

---

## New Crates and Modules

### New Crate: `hive_assistant`

This is the primary new crate that ties everything together. It depends on
existing crates but does not modify them.

```
hive/crates/hive_assistant/
  Cargo.toml
  src/
    lib.rs                -- Public API and module declarations
    plugin.rs             -- AssistantPlugin trait for extensibility
    approval.rs           -- User approval workflow service
    dry_run.rs            -- Dry run simulation engine
    audit.rs              -- Assistant-specific audit logging
    daily_brief.rs        -- Morning briefing generator
    storage.rs            -- SQLite persistence for assistant data

    email/
      mod.rs              -- Unified email service
      inbox_agent.rs      -- Background inbox monitoring
      compose_agent.rs    -- AI-powered composition
      smart_reply.rs      -- Reply suggestions
      batch_actions.rs    -- Bulk operations

    calendar/
      mod.rs              -- Unified calendar service
      conflict_detector.rs
      smart_scheduler.rs
      event_agent.rs
      daily_brief.rs

    reminders/
      mod.rs              -- Reminder service
      os_notifications.rs -- OS-level notification delivery
      context_triggers.rs
      recurring.rs

    research/
      mod.rs              -- Research agent
      web_search.rs
      summarizer.rs
      topic_monitor.rs
      clip_collector.rs

    tasks/
      mod.rs              -- Task management service
      nl_parser.rs
      deadline_tracker.rs
      daily_planner.rs
      sync.rs

    reservations/
      mod.rs
      restaurant.rs
      appointments.rs
      confirmation.rs

    shopping/
      mod.rs
      grocery_list.rs
      instacart.rs
      amazon.rs
      price_tracker.rs

    smart_home/
      mod.rs
      scenes.rs
      thermostat.rs
      routines.rs
      device_registry.rs

    documents/
      mod.rs
      report_builder.rs
      email_template.rs
      presentation.rs
      formatter.rs

    finance/
      mod.rs
      expenses.rs
      budget.rs
      receipt_parser.rs
      reports.rs

    travel/
      mod.rs
      flight_search.rs
      hotel_search.rs
      itinerary.rs
      travel_alerts.rs

    health/
      mod.rs
      medication.rs
      appointments.rs
      health_log.rs
      wellness_check.rs
```

### New Dependencies to Add to Workspace

```toml
# In hive/Cargo.toml [workspace.dependencies]

# OS notifications (Windows toast)
winrt-notification = "0.7"

# HTTP server for webhook listener
axum = { version = "0.8", features = ["tokio"] }

# Audio capture (Phase 4)
# cpal = "0.15"

# Speech-to-text (Phase 4)
# whisper-rs = "0.12"
```

### UI Changes

Add to `hive_ui::sidebar::Panel`:

```rust
// In hive/crates/hive_ui/src/sidebar.rs
pub enum Panel {
    Chat,
    History,
    Files,
    Specs,
    Agents,
    Kanban,
    Monitor,
    Logs,
    Costs,
    Review,
    Skills,
    Routing,
    Learning,
    Shield,
    TokenLaunch,
    Assistant,  // NEW
    Settings,
    Help,
}
```

Add new panel module:

```
hive/crates/hive_ui/src/panels/assistant.rs  -- Main assistant panel view
```

Add `hive_assistant` to `hive_ui` dependencies:

```toml
# In hive/crates/hive_ui/Cargo.toml
[dependencies]
hive_assistant = { path = "../hive_assistant" }
```

---

## Summary

The Rust codebase is remarkably well-positioned for personal assistant features.
The integration layer (`hive_integrations`) already has complete API clients for
Gmail, Google Calendar, Google Tasks, Google Docs, Google Drive, Google Sheets,
Google Contacts, Outlook Email, Outlook Calendar, Slack, Discord, Teams,
Telegram, Matrix, and Philips Hue. The security layer (`hive_shield`) exceeds
what the Electron app had. The learning subsystem (`hive_learn`) is entirely new.

The `hive_assistant` crate now provides the **orchestration glue** that ties
individual API clients into coherent assistant behaviors (polling, classifying,
briefing, composing, scheduling, reminding). **OS-level notification delivery**
is implemented via `winrt-notification` on Windows, and the **Assistant UI panel**
(18th panel, Bell icon) surfaces all of this to the user.

**Phase 1 is substantially complete.** The remaining Phase 1 gaps are:
- Smart reply suggestions (AI-powered reply drafting from thread context)
- Natural language event creation parser ("lunch with Sarah at noon on Friday")
- OAuth connection UI in Settings panel (backend flows exist, UI does not)

Additionally, the system wiring is live:
- `LearningService` initialized at startup from `~/.hive/learning.db`
- `HiveShield` initialized at startup, scanning outgoing messages before AI calls
- `LearnerTierAdjuster` wired into `ModelRouter` for learned routing adjustments
- `StreamCompleted` events feed learning outcomes from chat completions
- Learning and Shield UI panels refresh from live global data

**Test count: 2,013 tests, 0 failures, 0 compiler warnings** (commit on rust/main).
