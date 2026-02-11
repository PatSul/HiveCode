# Hive — Comprehensive Feature Documentation

> **Updated**: 2026-02-11
> **Branch**: rust/main
> **Codebase**: ~97,000 lines of Rust across 13 crates
> **Tests**: 2,013 passing, 0 compiler warnings

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [hive_app — Application Shell](#2-hive_app--application-shell)
3. [hive_core — Configuration & Security](#3-hive_core--configuration--security)
4. [hive_ai — AI Provider Orchestration](#4-hive_ai--ai-provider-orchestration)
5. [hive_ui — User Interface (18 Panels)](#5-hive_ui--user-interface-18-panels)
6. [hive_agents — Multi-Agent Orchestration](#6-hive_agents--multi-agent-orchestration)
7. [hive_terminal — Terminal & Execution](#7-hive_terminal--terminal--execution)
8. [hive_fs — File System Operations](#8-hive_fs--file-system-operations)
9. [hive_shield — Security Shield](#9-hive_shield--security-shield)
10. [hive_learn — Self-Improvement System](#10-hive_learn--self-improvement-system)
11. [hive_assistant — Personal Assistant](#11-hive_assistant--personal-assistant)
12. [hive_docs — Document Generation](#12-hive_docs--document-generation)
13. [hive_blockchain — Wallet & Token Launch](#13-hive_blockchain--wallet--token-launch)
14. [hive_integrations — External Services](#14-hive_integrations--external-services)

---

## 1. Architecture Overview

Hive is a native desktop AI coding assistant built in Rust with GPUI (Zed's GPU-accelerated UI framework). It runs as a single binary (~50MB) with a system tray icon. Memory footprint is <50MB at idle with <1s startup time and 120fps GPU-accelerated rendering.

### Crate Layout

```
hive/crates/
  hive_app/           — Binary crate (entry point, window, tray)
  hive_core/          — Config, security gateway, database, shared types
  hive_ui/            — GPUI workspace, 18 panels, theme system
  hive_ai/            — 11 AI providers, routing, cost tracking, RAG, TTS
  hive_agents/        — Multi-agent orchestration, personas, skills, MCP
  hive_terminal/      — Command execution, shell, browser, Docker
  hive_fs/            — File operations, git, search, watching
  hive_shield/        — PII detection, secret scanning, prompt injection
  hive_learn/         — Outcome tracking, routing learning, self-evaluation
  hive_assistant/     — Email, calendar, reminders, approval workflows
  hive_docs/          — Document generation (7 formats)
  hive_blockchain/    — EVM/Solana wallets, token deployment
  hive_integrations/  — 30+ external service integrations
```

### Key Technologies

| Technology | Purpose |
|------------|---------|
| GPUI 0.2.2 | GPU-accelerated reactive UI framework |
| Tokio | Full-featured async runtime |
| rusqlite (bundled) | SQLite persistence |
| reqwest (rustls-tls) | HTTP client (no OpenSSL dependency) |
| git2 | Native git operations |
| aes-gcm + argon2 | AES-256-GCM encryption with Argon2id key derivation |
| tray-icon | System tray integration |
| rust-embed | Asset embedding in binary |
| pulldown-cmark | Markdown parsing and rendering |

### Data Flow

```
User Input → ChatInputView → HiveWorkspace → AiService → Provider → SSE Stream
                                                  ↓
                                          ModelRouter (auto-routing)
                                          CostTracker (budget)
                                          HiveShield (PII/secrets scan)
                                          SecurityGateway (validation)
                                          LearningService (outcome tracking)
```

---

## 2. hive_app — Application Shell

### Boot Sequence

1. Load `HiveConfig` from `~/.hive/config.json` (migrate from `~/.hivecode/` if needed)
2. Initialize GPUI `Application`
3. Register all actions and key bindings
4. Open main window (1280x800, min 800x600)
5. Set GPUI globals (`AppAiService`, `AppConfig`, `AppDatabase`, `AppNotifications`, `AppSecurity`, `AppConversationStore`)
6. Initialize `LearningService` and `HiveShield` pipeline
7. Wire `LearnerTierAdjuster` into `ModelRouter` for learned routing
8. Embed assets (background image, bee icon)
9. Create `HiveWorkspace` view
10. Spawn system tray (32x32 pixel-art bee, Nearest-neighbor filter)

### Window Configuration

| Property | Value |
|----------|-------|
| Default size | 1280 x 800 |
| Minimum size | 800 x 600 |
| Title | "Hive" |
| Custom titlebar | Yes (Win32 NC behavior for drag/minimize/maximize/close) |
| Background | Embedded `hive_bg.jpg` |

### System Tray

- **Icon**: 32x32 pixel-art bee (upscaled from 16x16 with Nearest filter)
- **Menu items**: Show Window, New Chat, Toggle Privacy, Settings, Quit
- **Platform**: Windows (`hive_bee.ico` multi-size 16/32/48/256), macOS/Linux (PNG)

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+N | New conversation |
| Ctrl+L | Clear chat |
| Ctrl+1..9 | Switch to panel 1-9 |
| Ctrl+0 | Switch to panel 10 (Costs) |
| Ctrl+, | Settings |
| Ctrl+/ | Help |
| Ctrl+P | Toggle privacy mode |

### GPUI Globals

| Global | Wraps | Purpose |
|--------|-------|---------|
| `AppAiService` | `hive_ai::AiService` | Provider routing, streaming |
| `AppConfig` | `hive_core::ConfigManager` | Hot-reload config |
| `AppDatabase` | `hive_core::Database` | SQLite persistence |
| `AppNotifications` | `hive_core::NotificationStore` | In-app notifications |
| `AppSecurity` | `hive_core::SecurityGateway` | Command/URL/path validation |
| `AppConversationStore` | `hive_core::ConversationStore` | Chat persistence |

---

## 3. hive_core — Configuration & Security

### HiveConfig (`~/.hive/config.json`)

| Field | Type | Default |
|-------|------|---------|
| `default_model` | String | `"claude-sonnet-4-5"` |
| `auto_routing` | bool | `true` |
| `privacy_mode` | bool | `false` |
| `theme` | String | `"dark"` |
| `auto_update` | bool | `true` |
| `notifications_enabled` | bool | `true` |
| `ollama_url` | String | `"http://localhost:11434"` |
| `lmstudio_url` | String | `"http://localhost:1234"` |
| `local_provider_url` | Option | `None` |
| `litellm_url` | Option | `None` |
| `daily_budget` | Option<f64> | `None` |
| `monthly_budget` | Option<f64> | `None` |
| API keys | `#[serde(skip)]` | Loaded from SecureStorage |

API keys (`anthropic_api_key`, `openai_api_key`, `openrouter_api_key`, `groq_api_key`, `huggingface_api_key`, `litellm_api_key`) are marked `#[serde(skip)]` — never written to the JSON config file. Stored in encrypted SecureStorage.

### ConfigManager

- `get() -> HiveConfig` — Current config
- `update(fn)` — Modify + auto-save
- `reload()` — Re-read from disk
- `base_dir()` — `~/.hive/`
- Migration from `~/.hivecode/` on first run

### SecurityGateway

Multi-layer security validation for all external operations.

**Command Validation** (`check_command`):
- Blocks: `rm -rf /`, `format C:`, `shutdown`, `reboot`, `mkfs`, `dd if=`, `:(){ :|:& };:` (fork bomb)
- Blocks: `curl | bash`, `wget | sh`, `eval`, `exec` patterns
- Blocks environment variable manipulation (`export`, `unset` on PATH/HOME/USER)
- Returns `CommandVerdict`: `Allow`, `Block(reason)`, `Sanitize(modified)`

**URL Validation** (`check_url`):
- HTTPS required for all external URLs
- Blocks private IPs: `127.0.0.1`, `10.*`, `172.16-31.*`, `192.168.*`, `169.254.169.254`
- Blocks: `localhost`, `*.local`, `0.0.0.0`
- Domain allowlist support

**Path Validation** (`validate_path`):
- Canonicalizes path
- Blocks system roots: `/`, `C:\`, `/System`, `/Windows`
- Blocks sensitive dirs: `.ssh`, `.aws`, `.gnupg`, `.config/gcloud`
- Blocks traversal: `../`

**Injection Detection** (`check_injection`):
- SQL patterns: `' OR 1=1`, `; DROP TABLE`, `UNION SELECT`
- Shell patterns: `$(...)`, `` `...` ``, `; rm`, `| cat /etc/passwd`

### SecureStorage

AES-256-GCM encrypted key-value store.

- **Encryption**: AES-256-GCM with random 12-byte nonce
- **Key derivation**: Argon2id (memory: 64MB, iterations: 3, parallelism: 4)
- **Storage**: `~/.hive/secure_storage.enc`
- **Salt**: Random 32-byte, stored alongside ciphertext
- **Methods**: `store(key, value)`, `retrieve(key) -> Option<String>`, `delete(key)`, `list_keys()`

### Database (SQLite)

Tables: `conversations`, `messages`, `cost_records`, `memories`

**ConversationStore**: CRUD operations for conversations with message storage, title/model updates, listing with filters, search by content.

**NotificationStore**: In-app notifications with read/unread status, priority levels (Low, Normal, High, Urgent), bulk operations.

### Other Modules

| Module | Purpose |
|--------|---------|
| `SessionState` | Active panel, conversation ID, window geometry |
| `CronScheduler` | Cron-expression task scheduling |
| `BackgroundService` | Periodic background task runner |
| `Canvas` | Collaborative editing data structures |
| `CodeReview` | Review request/comment data types (711 lines) |
| `ContextWindow` | Token budget management |
| `Enterprise` | Team/organization features, RBAC, audit logging (835 lines) |
| `ErrorHandler` | Structured error types + recovery suggestions |
| `Kanban` | Board/column/card task management (884 lines) |

---

## 4. hive_ai — AI Provider Orchestration

### AiProvider Trait

```rust
trait AiProvider: Send + Sync {
    fn provider_type(&self) -> ProviderType;
    fn name(&self) -> &str;
    async fn is_available(&self) -> bool;
    async fn get_models(&self) -> Vec<ModelInfo>;
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse>;
    async fn stream_chat(&self, request: &ChatRequest) -> Result<Receiver<StreamChunk>>;
}
```

### 11 Providers

| Provider | Endpoint | Auth | SSE Format | Models |
|----------|----------|------|------------|--------|
| **Anthropic** | `api.anthropic.com/v1/messages` | `x-api-key` header | Custom SSE (text + thinking) | Opus 4.6, Sonnet 4.5, Haiku 4.5 |
| **OpenAI** | `api.openai.com/v1/chat/completions` | Bearer token | OpenAI SSE | GPT-4o, GPT-4o-mini, o1, o3 |
| **OpenRouter** | `openrouter.ai/api/v1/chat/completions` | Bearer token | OpenAI SSE | 200+ models (gateway) |
| **Google Gemini** | `generativelanguage.googleapis.com/v1beta/openai/chat/completions` | Bearer token | OpenAI SSE | Gemini 2.5 Pro, 2.5 Flash, 2.0 Flash |
| **Groq** | `api.groq.com/openai/v1/chat/completions` | Bearer token | OpenAI SSE | Llama 3.3 70B, Llama 3.1 8B, Mixtral 8x7B, Gemma 2 9B |
| **HuggingFace** | `api-inference.huggingface.co/v1/chat/completions` | Bearer token | OpenAI SSE | Llama 3.3 70B, Mixtral 8x7B, Phi-3 Mini |
| **LiteLLM** | Configurable (default `localhost:4000`) | Optional Bearer | OpenAI SSE | Dynamic (proxy) |
| **Ollama** | `localhost:11434/api/chat` | None | NDJSON | Dynamic (local) |
| **LM Studio** | `localhost:1234/v1/chat/completions` | None | OpenAI SSE | Dynamic (local) |
| **GenericLocal** | Configurable | None | OpenAI SSE | Configurable |

### Model Routing

**ComplexityClassifier** — 12-factor scoring system:
1. Token count, Context size, File count, Has errors
2. Reasoning depth, Domain specificity, Task type
3. Code complexity, High/Medium complexity patterns
4. Deep reasoning indicators, Expert domain patterns

**Score to Tier**: <20 Free, 20-40 Budget, 40-60 Mid, >=60 Premium

**Hard overrides**: Architecture/Security/Debugging tasks always route to Premium

**ModelRouter** — prefix-based model-to-provider resolution:
- `claude-*` -> Anthropic, `gpt-*` -> OpenAI, `gemini-*` -> Google, `groq/*` -> Groq, `hf/*` -> HuggingFace
- Contains `/` -> OpenRouter, Unknown -> Ollama

**TierAdjuster integration**: `hive_learn::LearnerTierAdjuster` can override classified tiers based on historical quality data.

### Auto-Fallback System

Tracks provider health and provides intelligent fallback chains per tier.

**ProviderStatus**: `available`, `rate_limited_until`, `consecutive_failures`, `budget_exhausted`

**Default Fallback Chains**:
- **Premium**: Anthropic -> OpenAI -> OpenRouter
- **Mid**: Anthropic Sonnet -> OpenAI Mini -> OpenRouter
- **Budget**: Haiku -> DeepSeek -> Llama -> Qwen -> Groq
- **Free**: Ollama (llama3.2 -> codellama -> mistral)

**Config**: max 3 consecutive failures, 60s rate-limit cooldown, 30s failure cooldown

### Cost Tracking

- Token estimation: ~4 chars per token
- Per-request cost calculation from model registry (30+ models with pricing)
- Daily/monthly budget limits with enforcement
- Cost breakdown by model
- CSV export

### Context Engine

TF-IDF scoring with heuristic boosts for filename/symbol/recency/test matches. Greedy token budget packing. (~866 lines)

### RAG Service

Document chunking with configurable overlap, TF-IDF similarity ranking, cosine similarity, context assembly with file/line headers.

### Semantic Search

File-content search with term overlap + exact match + bigram scoring, contextual snippets, search history.

### Fleet Learning

Distributed AI instance coordination — pattern discovery, model performance tracking (latency, cost, quality), fleet insights, instance metrics.

### Privacy Mode

When enabled, all cloud providers (Anthropic, OpenAI, OpenRouter, Groq, HuggingFace) are disabled. Only local providers (Ollama, LM Studio, GenericLocal) remain available.

### TTS (Text-to-Speech)

New module for text-to-speech integration (in development).

---

## 5. hive_ui — User Interface (18 Panels)

### Workspace Layout

```
+----------------------------------------------+
| Titlebar (34px) - Brand, version, controls   |
+--------+-------------------------------------+
|        |                                     |
| Sidebar|      Active Panel Content           |
| (52px) |                                     |
|        |                                     |
|        +-------------------------------------+
|        | Chat Input (Chat panel only)        |
+--------+-------------------------------------+
| Status Bar (26px) - Model, cost, privacy     |
+----------------------------------------------+
```

### 18 Navigable Panels

| # | Panel | Icon | Key Features |
|---|-------|------|--------------|
| 1 | **Chat** | Bot | Markdown rendering, code blocks with copy, streaming with cyan border, thinking sections (collapsible), model/cost badges per message |
| 2 | **History** | Calendar | Search, conversation cards, relative timestamps, delete, refresh |
| 3 | **Files** | Folder | Breadcrumb nav, file tree, search, new file/folder, open/delete |
| 4 | **Specs** | File | Specification management with List/Detail/Edit modes, spec count badges, new spec creation (~570 lines) |
| 5 | **Agents** | Bot | Multi-agent management with 6 persona displays (Investigator, Implementer, Verifier, Critic, Debugger, Code Reviewer), orchestration run tracking with progress/cost/elapsed (~700 lines) |
| 6 | **Kanban** | LayoutDashboard | Todo/InProgress/Done columns, task cards |
| 7 | **Monitor** | Loader | Agent activity monitoring with 9 roles (Architect through TaskVerifier), system status tracking (Idle/Running/Paused/Error), run history, color-coded status dots (~881 lines) |
| 8 | **Logs** | File | Filterable log viewer, auto-scroll, clear |
| 9 | **Costs** | ChartPie | Today/monthly costs, budget progress bars, per-model breakdown, CSV export |
| 10 | **Review** | Eye | Git diff viewer, stage/unstage, commit, file-level review with inline comments (~1,424 lines) |
| 11 | **Skills** | Star | Skill marketplace with tabs (Installed/Directory/Create/AddSource), 8 categories, search filtering, integrity hash validation, ratings, skill authoring (~1,678 lines) |
| 12 | **Routing** | Map | Model routing rule editor with provider status, tier configuration (~1,320 lines) |
| 13 | **Learning** | TrendingUp | Self-improvement dashboard showing quality metrics/trends, learning log, user preferences with confidence scores, prompt suggestions per persona, routing insights, best/worst models (~617 lines) |
| 14 | **Shield** | EyeOff | Security monitoring with event timeline (severity-colored), PII detection counts, secrets blocked, threats caught, provider access policies with trust levels (~676 lines) |
| 15 | **Assistant** | Bell | Personal assistant dashboard with daily briefing (greeting, events, emails, reminders), upcoming events with conflict detection, email groups/previews, active reminders with overdue tracking, research progress, action history (~892 lines) |
| 16 | **Token Launch** | Globe | Multi-step ERC-20/SPL token deployment wizard (~1,039 lines) |
| 17 | **Settings** | Settings | API keys (masked input, status dots), local AI URLs, model routing, budget limits, general toggles, auto-save on blur (~1,145 lines) |
| 18 | **Help** | Info | Keyboard shortcuts reference, getting started guide, troubleshooting |

### Chat Panel Features

- **Markdown**: Headings, bold, italic, lists, inline code, code blocks, horizontal rules
- **Code blocks**: Language badge, copy button, monospace font (JetBrains Mono)
- **Message bubbles**: User (right-aligned), Assistant (left-aligned), Error (red)
- **Streaming**: Cyan border, "Generating..." label, throttled updates (15fps)
- **Thinking sections**: Collapsible reasoning display
- **Badges**: Model name + tier, per-message cost + tokens

### Settings Panel

- **API Keys**: Anthropic, OpenAI, OpenRouter, Groq, HuggingFace (masked input, status dots)
- **Local AI**: Ollama URL, LM Studio URL, LiteLLM URL, Custom URL, Privacy Mode toggle
- **Model Routing**: Default model dropdown (grouped by provider, tier badges), auto-routing toggle
- **Budget**: Daily/monthly limits (USD)
- **General**: Auto-update, notifications toggles
- **Auto-save**: All inputs save on blur/toggle

### Chat Input

- Auto-grow textarea (1-8 lines)
- File attachment with native picker
- Cost estimate before send
- Disabled during streaming

### Model Selector

- Search input with real-time filtering
- Provider grouping with API key gating
- Tier badges (Free/Budget/Mid/Premium, color-coded)
- Pricing display (input/output per million tokens)
- OpenRouter catalog fetch (100+ models)

### Theme System

| Token | Value |
|-------|-------|
| `bg_primary` | #1A1A2E |
| `bg_secondary` | #16213E |
| `accent_aqua` | #00FFF0 |
| `accent_cyan` | #00D4FF |
| `font_ui` | Inter |
| `font_mono` | JetBrains Mono |
| Spacing grid | 4px base |

### Session Persistence

Auto-saves to `~/.hive/session.json`: active panel, conversation ID. Restores on startup.

---

## 6. hive_agents — Multi-Agent Orchestration

### Queen (Meta-Coordinator) — ~1,844 lines

Top-level orchestrator that manages entire swarm lifecycles.

- Goal decomposition into team-level tasks
- Swarm provisioning across git worktrees (`swarm/{run_id}/{team_id}`)
- Team progress monitoring with cost/time limits
- Merge-on-completion with conflict resolution
- Budget enforcement ($5 default limit)

### HiveMind (9-Role Orchestrator) — ~1,647 lines

| Role | Tier | Order | Purpose |
|------|------|-------|---------|
| Architect | Premium | 0 | System design, task decomposition |
| Coder | Mid | 1 | Code generation |
| Reviewer | Mid | 2 | Code review |
| Tester | Mid | 3 | Test writing |
| Debugger | Mid | 4 | Root cause analysis |
| Security | Premium | 5 | OWASP auditing |
| Documenter | Budget | 6 | Documentation |
| OutputReviewer | Budget | 7 | Output verification |
| TaskVerifier | Budget | 8 | Requirements check |

**Config**: max 5 agents, $5 cost limit, 300s time limit, 0.7 consensus threshold

**Flow**: Decompose -> Execute sequentially -> Track cost/time -> Consensus check -> Synthesize

### Coordinator (Dependency-Aware Planning)

- AI-powered task decomposition from specifications
- Dependency-ordered execution in waves
- Persona-based task delegation (Investigate, Implement, Verify, Critique, Debug, CodeReview)
- Cycle detection and validation

### Persona System (6 Built-in + Custom) — ~693 lines

| Persona | Purpose | Tier |
|---------|---------|------|
| Investigate | Deep codebase analysis | Mid |
| Implement | Code generation | Mid |
| Verify | Testing and validation | Mid |
| Critique | Code review | Mid |
| Debug | Root cause analysis | Premium |
| CodeReview | Style/security/perf review | Mid |

### HiveLoop (Autonomous Iteration)

- Max 20 iterations, $2 cost limit, 600s time limit
- Automatic completion detection via keyword phrases
- Checkpoint/restore for crash recovery
- Pause/resume support

### Guardian Agent (AI Output Validation)

11 issue categories scanned via regex (no API calls):
OffTopic, HarmfulContent, DataLeak, Hallucination, SecurityRisk, PromptInjection, CodeQuality, Incomplete, Incorrect, FormattingError, Confidentiality

### Automation Workflows — ~911 lines

**Triggers**: Schedule (cron), FileChange, Webhook, Manual, OnMessage, OnError
**Actions**: RunCommand, SendMessage, CallApi, CreateTask, SendNotification, ExecuteSkill

### Skills System — ~670 lines (marketplace)

- Skill registry with injection scanning (SHA-256 integrity verification)
- Skill marketplace (installation, security scanning, 8 categories)
- Built-in tools: ReadFile, WriteFile, ListDirectory, SearchFiles, ExecuteCommand

### MCP Integration

- **Client** (~1,316 lines): JSON-RPC 2.0 protocol, Stdio and SSE transport, tool discovery and invocation
- **Server** (~1,108 lines): Expose Hive tools via MCP protocol to external clients

### Voice Assistant — ~688 lines

- Wake word detection ("hey hive", "ok hive", "hive")
- Intent classification (code, search, run, explain, fix, test, deploy, status)
- Command history with success/failure tracking
- Microphone management

### Tool Use Framework — ~1,687 lines

Agent tool invocation framework supporting ReadFile, WriteFile, ListDirectory, SearchFiles, ExecuteCommand with security validation.

### Other Modules

| Module | Purpose |
|--------|---------|
| `auto_commit` | Automatic git commits after task completion |
| `persistence` | Agent snapshot save/restore to `~/.hive/agents/` |
| `heartbeat` | Agent liveness monitoring |
| `standup` | Daily standup report generation |
| `specs` | Live specification management with auto-update (~586 lines) |
| `collective_memory` | Shared memory across agent instances |
| `worktree` | Git worktree management for parallel agent work |

---

## 7. hive_terminal — Terminal & Execution

### CommandExecutor

- SecurityGateway validation on every command
- 30-second default timeout, 1MB output truncation
- Platform-appropriate shell (`cmd /c` Windows, `sh -c` Unix)
- Rejects system roots and sensitive directories as working dir

### Interactive Shell

- Async read/write via mpsc channels
- Platform shell detection (`cmd.exe` / `bash` / `sh`)
- Default 80x24 dimensions
- Process group termination on Unix

### CLI Service

5 built-in commands: `doctor`, `config`, `chat`, `version`, `help`
5 health checks: config file, data directory, git available, disk space, network

### Browser Automation

Data structures for browser automation (Navigate, Click, Type, Screenshot, etc.). Pool management (max 3 instances, 300s idle timeout).

### Docker Sandbox

Container lifecycle management (Create -> Running -> Paused -> Stopped -> Removed). Resource limits (memory, CPU, disk, timeout).

### Local AI Detection

Probes 7 localhost ports for AI providers:
Ollama (:11434), LM Studio (:1234), vLLM (:8000), LocalAI (:8080), llama.cpp (:8081), text-gen-webui (:5000), Custom (:8090)

Ollama-specific: model pull with progress, delete, show, list.

---

## 8. hive_fs — File System Operations

### FileService

- Read (10MB limit), write (with parent dir creation), delete, rename, list, stats
- All paths canonicalized and validated
- System roots and sensitive directories blocked
- `.hive/config.json` write blocked

### GitService (via git2)

- Open/init repositories
- Status (includes untracked, excludes ignored)
- Diff (patch format against HEAD)
- Stage/unstage files
- Commit with message

### SearchService

- Regex pattern search across files
- `.gitignore`-aware traversal (via `ignore` crate)
- Glob-based file filtering
- Binary file detection
- Max 100 results default

### FileWatcher

- Uses `notify` crate's `RecommendedWatcher`
- Recursive watching
- Simplified events: Created, Modified, Deleted, Renamed

---

## 9. hive_shield — Security Shield

### PII Detection & Cloaking

**Detected types**: Email, Phone, SSN, CreditCard, IpAddress, Name, Address, DateOfBirth, Passport, DriversLicense, BankAccount

**Cloaking formats**: Placeholder (`[EMAIL_1]`), Hash (SHA-256 prefix), Redact (`****`)

### Secret Scanning

**Detected secrets**: AWS Access Key, GitHub Token, GitLab Token, Slack Token, JWT, Private Key, Database URL, Generic Secret

**Risk levels**: None, Low, Medium, High, Critical

### Prompt Injection & Jailbreak Detection

**Threat types**: Injection, Jailbreak, DataExfiltration, SystemPromptLeak, TokenSmuggling, IndirectInjection

9+ detection patterns for common injection and jailbreak attempts.

### Access Control

**Data classification**: Public, Internal, Confidential, Restricted
**Provider trust**: Local, Trusted, Standard, Untrusted

Policy engine enforces data sharing rules per provider trust level.

### Unified Shield Pipeline

4-stage pipeline for outgoing messages:
1. **Secret Scanning** — Block if secrets found
2. **Vulnerability Assessment** — Block if high/critical threat
3. **PII Detection** — Find PII matches
4. **Access Control** — Apply provider-specific policies

**Decisions**: Allow, CloakAndAllow, Block, Warn

---

## 10. hive_learn — Self-Improvement System

### LearningService (Entry Point)

Central coordinator for all learning subsystems. Opens a dedicated SQLite database at `~/.hive/learning.db` with 6 tables.

**Key methods**:
- `on_outcome(record)` — Records interaction results; triggers analysis at 50 and 200 interaction milestones
- `learning_log(limit)` — Transparent log of all learning events for UI
- `reject_preference(key)` — User can delete any learned preference
- `accept_prompt_refinement(persona, prompt)` — User approves a refined prompt
- `rollback_prompt(persona, version)` — Rollback to a previous prompt version
- `reset_all()` — Nuclear option: clear all learned data
- `all_preferences()` — Returns all (key, value, confidence) triples

### Outcome Tracker

Records every AI interaction outcome: `Accepted`, `Corrected`, `Regenerated`, `Ignored`, `Unknown`.

Each `OutcomeRecord` captures: conversation_id, message_id, model_id, task_type, tier, persona, outcome, edit_distance, follow_up_count, quality_score, cost, latency_ms, timestamp.

### Routing Learner

Learns when the ComplexityClassifier's tier assignment was wrong and adjusts future routing.

- Tracks quality outcomes per task_type + tier combination
- Uses exponential moving average (EMA) for recency weighting
- Generates `RoutingAdjustment`: from_tier -> to_tier with confidence and reason
- Integrates with `ModelRouter` via `LearnerTierAdjuster` adapter (implements `hive_ai::TierAdjuster` trait)
- Reanalyzes every 50 interactions

### Preference Model

Bayesian confidence tracking for user preferences (tone, detail level, formatting).

- `observe(key, value, confidence)` — Record an observation
- `get(key, min_confidence)` — Only return preference if confidence exceeds threshold
- Tracks observation count per preference
- Users can delete individual preferences

### Prompt Evolver

Version-controlled prompts per persona with quality-gated refinements.

- Each persona has a version history with quality metrics (avg_quality, sample_count)
- New versions only promoted when quality exceeds threshold
- Supports rollback to any previous version
- Generates refinement suggestions based on quality analysis

### Pattern Library

Extracts coding patterns from accepted responses across 6 languages (Rust, Python, JavaScript, TypeScript, Go, Java).

- Categorizes patterns using keyword detection (e.g., `pub fn`, `impl`, `class`, `def`)
- Tracks usage count and quality score per pattern
- Retrieves patterns by language and category
- Surfaces popular patterns for reuse

### Self-Evaluator

Generates comprehensive quality reports every 200 interactions.

**Report includes**:
- `overall_quality` — Average quality across all interactions
- `trend` — Improving, Declining, or Stable
- `best_model` / `worst_model` — Performance ranking
- `misroute_rate` — Percentage of suboptimal tier assignments
- `cost_per_quality_point` — Cost efficiency metric
- `weak_areas` — Task types with low quality
- `correction_rate` / `regeneration_rate` — User satisfaction indicators

### Storage (SQLite)

6 tables with ~40 query methods across ~1,551 lines:

| Table | Purpose |
|-------|---------|
| `outcomes` | All recorded interaction outcomes |
| `routing_history` | Routing decisions and quality results |
| `user_preferences` | Key-value preferences with confidence |
| `prompt_versions` | Versioned prompts per persona |
| `code_patterns` | Extracted code patterns |
| `learning_log` | Transparent audit log of all learning events |

---

## 11. hive_assistant — Personal Assistant

### AssistantService (Entry Point)

Coordinates email, calendar, reminders, and approval subsystems. Opens a dedicated SQLite database.

**Key methods**:
- `daily_briefing()` — Generates combined briefing from calendar events, email digest, and active reminders
- `tick_reminders()` — Checks all active reminders and returns those that should trigger now

### Email Service

- `fetch_gmail_inbox()` / `fetch_outlook_inbox()` — Fetch emails (Phase 2: full OAuth API integration)
- `build_digest(emails, provider)` — Create summarized digest
- `send_email(to, subject, body, shield)` — Send email with HiveShield PII/secrets scanning
- `classify(email)` — Classify as Important, Normal, Spam, or Newsletter
- Sub-modules for inbox agent and compose agent (Phase 2)

### Calendar Service

- `today_events()` — Get today's events
- `events_in_range(start, end)` — Query events in date range
- `create_event(event)` — Create calendar event
- `UnifiedEvent` type with conflict detection
- Sub-modules for daily briefings, conflict detection, smart scheduling (Phase 2)

### Reminder Service

**Trigger types**:
- `At(DateTime)` — One-time reminder at specific time
- `Recurring(String)` — Cron expression for recurring schedule
- `OnEvent(String)` — Trigger on named event

**Operations**:
- `create(title, description, trigger_at)` — Create one-time reminder
- `create_recurring(title, description, cron_expr)` — Create recurring reminder
- `tick()` — Check all active reminders, return triggered ones
- `snooze(id)` / `complete(id)` / `dismiss(id)` — Status management
- `list_active()` — Get all active reminders

**OS Notifications**: Windows toast notifications via `winrt-notification` crate.

### Approval Service

Multi-tier approval workflows for sensitive operations.

**Approval levels**: Low, Medium, High, Critical

**Operations**:
- `submit(action, resource, level, requested_by)` — Create approval request
- `approve(id, decided_by)` / `reject(id, decided_by)` — Decide on request
- `list_pending()` — Get all pending approvals

**Use cases**: Deployment approvals, resource deletion, sensitive email sending, agent-initiated actions requiring human sign-off.

### Plugin System

`AssistantPlugin` trait for extending assistant capabilities:
- `name()` — Plugin name
- `capabilities()` — Email, Calendar, Reminders, Research, Approvals
- `initialize()` / `shutdown()` — Lifecycle hooks

### Storage (SQLite)

| Table | Purpose |
|-------|---------|
| `reminders` | All reminders with status and trigger info |
| `approvals` | Approval requests with status |
| `email_digests` | Email digest history |
| `approval_log` | Approval audit log |

---

## 12. hive_docs — Document Generation

7 output formats from a single generation interface:

| Format | Module | Use Case |
|--------|--------|----------|
| CSV | `csv.rs` | Data export, spreadsheet import |
| DOCX | `docx.rs` | Word documents (via `docx-rs`) |
| HTML | `html.rs` | Web content, email bodies |
| Markdown | `markdown.rs` | Documentation, READMEs |
| PDF | `pdf.rs` | Reports, formal documents |
| PPTX | `pptx.rs` | Presentations (via `zip` crate) |
| XLSX | `xlsx.rs` | Spreadsheets (via `rust_xlsxwriter`) |

---

## 13. hive_blockchain — Wallet & Token Launch

### EVM Support

- `EvmWallet` — Wallet creation and management
- `Erc20Contract` — Pre-compiled ERC-20 bytecode templates
- `TokenDeployParams` — Name, symbol, supply, decimals
- `DeployResult` — Contract address, transaction hash
- 7 chains: Ethereum, Polygon, Arbitrum, BSC, Avalanche, Optimism, Base

### Solana Support

- `SolanaWallet` — Wallet operations
- `SplTokenParams` — SPL token configuration
- `SplDeployResult` — Deployment result

### Wallet Storage

- `WalletStore` — Encrypted wallet persistence (AES-256-GCM)
- `encrypt_key()` / `decrypt_key()` — Key protection
- `Chain` enum — Multi-chain support
- `WalletEntry` — Chain + address + encrypted key

### RPC Configuration

- `RpcConfig` / `RpcConfigStore` — Endpoint management per chain
- `validate_url()` — URL validation

---

## 14. hive_integrations — External Services

### Google Workspace (8 services)

| Service | Client | Key Features |
|---------|--------|-------------|
| Gmail | `GmailClient` | List/get/send/draft/search/delete/modify labels, email classifier, subscription manager (~986 lines) |
| Calendar | `GoogleCalendarClient` | Events CRUD, list calendars, free/busy, attendees (~723 lines) |
| Drive | `GoogleDriveClient` | File upload/download/list/delete |
| Docs | `GoogleDocsClient` | Create/read/update documents (~610 lines) |
| Sheets | `GoogleSheetsClient` | Read/write cell ranges |
| Tasks | `GoogleTasksClient` | Task lists and tasks (read-only) |
| Contacts | `GoogleContactsClient` | Contact CRUD via People API v1 (~637 lines) |
| OAuth | `GoogleOAuth` | PKCE flow, token refresh |

### Microsoft Suite

| Service | Client | Key Features |
|---------|--------|-------------|
| Outlook Email | `OutlookEmailClient` | Full email CRUD via Graph API |
| Outlook Calendar | `OutlookCalendarClient` | Calendar management via Graph API |

### Messaging Platforms (6)

`MessagingHub` — Unified interface with `MessagingProvider` trait (~4,300 lines total).

| Platform | Provider | Transport |
|----------|----------|-----------|
| Discord | `DiscordProvider` | Bot API |
| Matrix | `MatrixProvider` | Matrix protocol |
| Slack | `SlackProvider` | Web API |
| Teams | `TeamsProvider` | Graph API |
| Telegram | `TelegramProvider` | Bot API |
| WebChat | `WebChatProvider` | WebSocket |

**CrossChannelService** — Cross-platform message routing with shared memory.

### Cloud Platforms

| Service | Client | Capabilities |
|---------|--------|-------------|
| Cloudflare | `CloudflareClient` | DNS, Workers, Pages |
| Supabase | `SupabaseClient` | Database, Auth, Storage |
| Vercel | `VercelClient` | Deployments, Domains |

### Other Integrations

| Service | Client | Purpose |
|---------|--------|---------|
| GitHub | `GitHubClient` | Repos, PRs, Issues |
| OAuth | `OAuthClient` | OAuth 2.0 PKCE token management |
| Philips Hue | `PhilipsHueClient` | Smart home lighting (scenes, individual lights) |
| IDE | `IdeIntegrationService` | LSP-style symbols, diagnostics, editor commands |
| Webhooks | `WebhookRegistry` | Incoming webhook registration and dispatch |

---

## Appendix A: Test Coverage

| Crate | Tests | Notes |
|-------|-------|-------|
| hive_ai | 271 | All providers, routing, complexity, cost |
| hive_terminal | 191 | Executor, CLI, shell, browser, docker, local_ai |
| hive_agents | ~869 | All orchestration, skills, guardian, MCP, automation |
| hive_core | ~200 | Config, security, database, storage, kanban, review |
| hive_assistant | 105 | Email, calendar, reminders, approval, plugin |
| hive_learn | ~100 | Outcome, routing, preference, prompt evolution, patterns |
| hive_ui | ~100 | Component tests |
| hive_shield | ~80 | PII, secrets, vulnerability, access control |
| hive_fs | ~50 | Files, git, search, watcher |
| hive_docs | ~30 | Document generation formats |
| hive_blockchain | ~20 | Wallet, token deploy, RPC config |
| **Total** | **~2,013** | **0 warnings** |

## Appendix B: Local Storage (`~/.hive/`)

| File | Purpose |
|------|---------|
| `config.json` | Application configuration (JSON) |
| `conversations.db` | Chat history and messages (SQLite) |
| `learning.db` | Self-improvement data (SQLite, 6 tables) |
| `assistant.db` | Reminders, approvals, email state (SQLite) |
| `secure_storage.enc` | Encrypted API keys and secrets (AES-256-GCM) |
| `session.json` | Active panel, conversation ID |
| `hive.log` | Application log (tracing output) |
| `agents/` | Agent snapshots for persistence/restore |

## Appendix C: Build & Deploy

```
# Requirements
- Rust (edition 2024)
- VS Build Tools with C++ workload (Windows)
- Or: VS Developer Command Prompt / INCLUDE + LIB env vars

# Commands
cd hive
cargo build          # Debug build
cargo build --release # Release (opt-level=3, thin LTO, stripped)
cargo test           # Run all 2,013 tests
cargo run            # Launch application

# Release Profile
[profile.release]
opt-level = 3        # Maximum optimization
lto = "thin"         # Thin LTO for compilation speed
strip = true         # Strip symbols from binary
```

**Binary output**: ~50MB self-contained executable with embedded assets.
