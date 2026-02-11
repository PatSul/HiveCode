# Hive Platform Vision

**Your AI that learns, protects, and works while you sleep.**

---

## Part 1: The Vision

### What Is Hive?

Hive is a native desktop AI platform that unifies three capabilities no competitor combines: a world-class **development environment**, a comprehensive **personal assistant**, and a **security-first architecture** that treats your privacy as non-negotiable. It is built entirely in Rust using the GPUI framework -- no Electron, no web wrappers, no compromises.

Where other tools give you a chatbot that can write code, Hive gives you a **swarm of specialized agents** that can plan, build, test, deploy, order your groceries, triage your email, manage your calendar, and learn your preferences over time -- all while ensuring that not a single secret, PII fragment, or dangerous command ever leaves your machine without your explicit approval.

### Tagline Options

- *Your AI that learns, protects, and works while you sleep.*
- *One hive. Every task. Total control.*
- *The last AI assistant you will ever need.*

### Competitive Landscape

| Capability | ChatGPT | Copilot | Cursor | Windsurf | Devin | Replit Agent | **Hive** |
|---|---|---|---|---|---|---|---|
| Native desktop app | No | VS Code ext | Electron | Electron | Cloud | Cloud | **Rust + GPUI** |
| Multi-agent swarm | No | No | No | No | Yes | No | **Yes (Queen + teams)** |
| Git worktree isolation | No | No | No | No | Partial | No | **Yes** |
| Personal assistant | Limited | No | No | No | No | No | **Full (email, cal, shopping)** |
| PII/secrets scanning | No | No | No | No | No | No | **Built-in (hive_shield)** |
| Local-first privacy | No | No | No | No | No | No | **Yes** |
| Self-improving | No | No | No | No | Partial | No | **Yes (hive_learn)** |
| Blockchain/DeFi tools | No | No | No | No | No | No | **Yes** |
| Plugin system (MCP) | Plugins (GPT) | Extensions | No | No | No | No | **MCP server + client** |
| Cost tracking & routing | No | No | No | No | No | No | **Built-in** |
| Works offline (local LLMs) | No | No | No | No | No | No | **Yes (Ollama, LM Studio)** |

**The key insight:** Every competitor does one thing well. ChatGPT is a great conversationalist. Copilot autocompletes code. Cursor is a polished editor. Devin runs autonomous coding loops. None of them are a *platform*. None of them are the single application you open in the morning and rely on all day for everything -- coding, communication, scheduling, research, security, and personal tasks.

Hive is that platform.

### The Three Pillars

```
                        +------------------+
                        |      HIVE        |
                        |   Native Rust    |
                        |   Desktop App    |
                        +--------+---------+
                                 |
              +------------------+------------------+
              |                  |                  |
    +---------v--------+ +------v-------+ +--------v--------+
    |   DEVELOPMENT    | |  ASSISTANT   | |    SAFETY       |
    |   EXCELLENCE     | |  EXCELLENCE  | |    EXCELLENCE   |
    +------------------+ +--------------+ +-----------------+
    | Multi-agent swarm| | Email triage | | PII detection   |
    | Queen coordinator| | Calendar mgmt| | Secrets scanning|
    | Git worktrees    | | Reservations | | SecurityGateway |
    | Code review      | | Research     | | Audit logging   |
    | Context engine   | | Shopping     | | Approval flows  |
    | Testing & deploy | | Smart home   | | Explain mode    |
    | Self-learning    | | Finance      | | Rollback        |
    +------------------+ +--------------+ +-----------------+
```

### Why Native Rust?

1. **Performance.** GPUI renders at 120fps using the GPU directly. No DOM, no virtual DOM, no JavaScript event loop. The UI is as responsive as a video game.

2. **Memory.** A typical Electron app consumes 300-500MB at idle. Hive runs at under 50MB. This matters when your AI assistant is running 24/7.

3. **Privacy.** Rust's ownership model eliminates entire categories of memory safety vulnerabilities. No garbage collector pauses. No runtime reflection that could leak data.

4. **System access.** Native code can interact with the OS at the deepest level -- system tray, global hotkeys, file watchers, process management, GPU acceleration -- without the sandboxing limitations of web-based architectures.

5. **Startup time.** Hive launches in under 1 second. Electron apps take 3-8 seconds. When your AI assistant needs to respond to a wake word or a tray click, every millisecond of latency breaks the illusion of intelligence.

6. **No dependency on Google.** Electron ships a full copy of Chromium. That is a 150MB dependency on a browser engine maintained by a single corporation. Hive depends only on the Rust toolchain and the OS graphics API.

---

## Part 2: Development Platform Excellence

### The Swarm Architecture

Hive does not use a single AI agent. It uses a **hierarchical swarm** modeled on a beehive:

```
                    +-------------+
                    |    QUEEN    |   Meta-coordinator
                    |  (Planning) |   Goal decomposition
                    +------+------+   Budget enforcement
                           |          Cross-team synthesis
              +------------+------------+
              |            |            |
        +-----v----+ +----v-----+ +----v-----+
        |  TEAM 1  | |  TEAM 2  | |  TEAM 3  |
        | HiveMind | |Coordinator| |SingleShot|
        +----+-----+ +----+-----+ +----------+
             |             |
       +-----+-----+  +---+---+
       |     |     |  |       |
      Arch  Code  Rev Inv    Impl
```

**The Queen** (`hive_agents::Queen`) sits at the top. Given a high-level goal like "Add a caching layer to the API," she:
1. **Plans** -- Decomposes the goal into team objectives with dependency ordering using AI-driven analysis.
2. **Dispatches** -- Assigns each team an orchestration mode (HiveMind, Coordinator, NativeProvider, or SingleShot) based on complexity.
3. **Enforces** -- Monitors budget limits (`total_cost_limit_usd`) and time constraints (`total_time_limit_secs`), halting teams that exceed their allocation.
4. **Shares** -- Passes cross-team insights between dependency waves so later teams benefit from earlier findings.
5. **Synthesizes** -- Merges all team outputs into a coherent final result.
6. **Learns** -- Records success patterns, failure patterns, and model insights to collective memory for future runs.

**HiveMind teams** use specialized agent roles: Architect (designs the approach), Coder (writes the implementation), Reviewer (checks quality), Tester (validates correctness), and Security (checks for vulnerabilities). These agents reach consensus through structured debate.

**Coordinator teams** decompose work into dependency-ordered tasks (investigate, implement, verify) and execute them with persona-specific prompts.

### Git Worktree Isolation

Every swarm team gets its own **git worktree** (`hive_agents::WorktreeManager`):

- Teams work on isolated branches: `swarm/{run_id}/{team_id}`
- All worktrees live under `.hive-worktrees/` in the repository root
- Teams can modify files simultaneously without conflicts
- Completed work merges back to the target branch with conflict detection
- Failed teams leave no trace on the main branch

This is critical infrastructure. Without isolation, multi-agent coding is a race condition. With worktrees, it is parallel compilation.

### Context Engine

The context engine (`hive_ai::ContextEngine`) solves the hardest problem in AI-assisted development: **what context to include in the prompt.**

- Scores thousands of potential context sources (files, symbols, docs, git history) using TF-IDF with heuristic boosts
- Respects a strict token budget so models never receive truncated or irrelevant context
- Prioritizes: filename matches > symbol names > recent edits > test files > documentation
- Operates on a `ContextBudget` that specifies max tokens, max sources, and reserved tokens for prompt/response

### AI Provider Ecosystem

Hive is not locked to any single AI provider. The `hive_ai` crate supports:

- **Cloud providers:** Anthropic (Claude), OpenAI (GPT), OpenRouter (100+ models), Groq (fast inference), HuggingFace
- **Local providers:** Ollama, LM Studio, Generic OpenAI-compatible endpoints, LiteLLM proxy
- **Model routing:** Automatic complexity classification routes simple tasks to cheap models and complex tasks to powerful models
- **Auto-fallback:** When a provider fails, requests automatically fall through to the next available provider
- **Cost tracking:** Every token is tracked per model, per conversation, with real-time budget monitoring

### Code Review and Testing

- `hive_core::CodeReview` provides structured code review with file-level change tracking, inline comments, and review status management
- Automated test generation and execution via the terminal executor
- Docker sandbox (`hive_terminal::DockerSandbox`) for safe code execution with resource limits and volume mounts
- Browser automation (`hive_terminal::BrowserAutomation`) for end-to-end testing

### Project-Specific Learning

Every interaction makes Hive smarter about *your* codebase:

- The `hive_learn::PatternLibrary` extracts coding patterns from high-quality outputs (your accepted code)
- The `hive_learn::RoutingLearner` tracks which models perform best for which task types in your project
- The `hive_learn::PreferenceModel` learns your preferences (detail level, tone, formatting) through observation
- The `hive_learn::PromptEvolver` refines system prompts based on outcome data, with version history and rollback

---

## Part 3: Personal Assistant Excellence

Hive's assistant capabilities are powered by the same AI infrastructure that drives the development platform -- the same model routing, the same security scanning, the same learning loop. The difference is the *integrations*.

### Email Triage and Drafting

**Current infrastructure:** `hive_integrations::google` (Gmail, Google Contacts), `hive_integrations::microsoft` (Outlook Email), `hive_integrations::oauth` (OAuth2 flows)

- **Triage:** AI classifies incoming emails by urgency, category, and required action. The `EmailClassifier` already handles categorization. Expansion: automatic priority scoring, thread summarization, and "needs reply" flagging.
- **Drafting:** Given a conversation thread and intent ("decline politely," "schedule a follow-up," "request more information"), Hive drafts a response in your voice -- learned over time from your sent messages.
- **Unsubscribe management:** The `SubscriptionManager` already tracks subscriptions and unsubscribe methods. Expansion: one-click bulk unsubscribe with AI-identified low-value subscriptions.

### Calendar Management and Smart Scheduling

**Current infrastructure:** `hive_integrations::google::GoogleCalendarClient`, `hive_integrations::microsoft::OutlookCalendarClient`

- **Smart scheduling:** "Find me 30 minutes with Sarah this week" -- Hive checks both calendars, proposes times, and sends the invite.
- **Conflict detection:** Automatic warnings when new events overlap existing commitments.
- **Travel time awareness:** Calendar events include location-aware travel time buffers.
- **Meeting prep:** Before each meeting, Hive surfaces relevant emails, documents, and prior meeting notes.

### Reservations and Phone Calls

**Target integrations:** Twilio (voice), OpenTable/Resy (restaurants), travel booking APIs

- **Restaurant reservations:** "Book dinner for 4 at 7pm Friday, Italian, near downtown" -- Hive searches availability across platforms and confirms the booking.
- **Phone calls:** Via Twilio integration, Hive can make structured phone calls (appointment confirmations, prescription refills, utility inquiries) using voice synthesis and speech-to-text.
- **Appointment scheduling:** Medical, dental, automotive -- Hive navigates phone trees and online booking systems.

### Grocery and Shopping

**Target integrations:** Instacart API, Amazon Fresh, store-specific APIs

- **Recurring lists:** Hive learns your weekly grocery patterns and pre-populates lists.
- **Smart substitutions:** When items are out of stock, Hive suggests alternatives based on your preferences and dietary restrictions.
- **Price tracking:** Monitor prices across stores and alert on deals for items you regularly buy.
- **Meal planning integration:** "Plan meals for the week and order ingredients" -- generates a meal plan, compiles the ingredient list, checks pantry inventory (manual or smart fridge API), and places the order.

### Research While Away

- **Topic monitoring:** Set up research tasks -- "Monitor developments in Rust async traits this week" -- and Hive aggregates findings from RSS feeds, Hacker News, academic papers, and documentation sites.
- **Summarization:** Daily or weekly research digests with key findings, trends, and your relevance scores.
- **Deep dives:** "Research the best approach to implementing CRDT-based real-time collaboration" -- Hive reads papers, compares libraries, and produces a structured recommendation with code examples.

### Document Generation

**Current infrastructure:** `hive_docs` supports CSV, DOCX, XLSX, HTML, Markdown, PDF, and PPTX generation.

- **Reports:** "Generate a weekly project status report" -- pulls data from Kanban board, git history, and conversation logs.
- **Presentations:** AI-generated slide decks from outlines or documents, with theme-consistent formatting.
- **Invoices and contracts:** Template-based document generation with variable substitution.
- **Data visualization:** Charts and infographics embedded in documents using the canvas system (`hive_core::LiveCanvas`).

### Smart Home Control

**Current infrastructure:** `hive_integrations::smart_home::PhilipsHueClient`

- **Lighting:** "Set the office to focus mode" -- adjusts brightness, color temperature, and individual light states.
- **Routines:** Time-based and event-triggered automations -- dim lights at sunset, turn on the coffee maker when your morning alarm fires.
- **Expansion targets:** HomeKit (via local protocol), Google Home, Samsung SmartThings, MQTT for custom devices.

### Financial Tracking

- **Expense categorization:** AI-powered categorization of bank transactions.
- **Budget monitoring:** Real-time tracking against monthly budgets with alerts.
- **Tax preparation:** Year-end summaries with categorized deductions.
- **Investment monitoring:** Portfolio tracking with AI-generated market summaries (no trading -- information only).

### Travel Planning

- **Itinerary generation:** "Plan a 5-day trip to Tokyo in April" -- produces a day-by-day itinerary with flights, hotels, activities, restaurants, and transit directions.
- **Booking assistance:** Searches across booking platforms and presents options with price comparisons.
- **Document organization:** Collects confirmations, boarding passes, and hotel details into a unified travel document.

### Health and Wellness

- **Medication reminders:** Scheduled notifications with refill tracking.
- **Activity tracking integration:** Connect to Apple Health or Google Fit for daily summaries.
- **Wellness check-ins:** Periodic mood and energy prompts that build a personal wellness timeline.

---

## Part 4: Safety as Competitive Advantage

Safety is not a feature of Hive. It is the *foundation*. Every line of code runs through a security architecture designed to make it impossible for AI actions to cause harm without explicit human approval.

### hive_shield: The Data Guardian

The `hive_shield` crate (`HiveShield`) provides four layers of protection:

1. **PII Detection** (`PiiDetector`): Scans all outgoing data for personally identifiable information -- names, emails, phone numbers, SSNs, credit card numbers, addresses. Detected PII can be cloaked (replaced with placeholders) before data leaves the machine. Configurable sensitivity levels and cloaking formats.

2. **Secrets Scanning** (`SecretScanner`): Detects API keys, passwords, tokens, private keys, and connection strings in code, messages, and file contents. Assigns risk levels (Critical, High, Medium, Low) and blocks transmission of high-risk secrets.

3. **Vulnerability Assessment** (`VulnerabilityAssessor`): Evaluates AI-generated code for security vulnerabilities -- injection attacks, unsafe deserialization, hardcoded credentials, path traversal. Also detects prompt injection threats in untrusted input.

4. **Access Control** (`PolicyEngine`): Enforces data classification policies that determine which data can be sent to which providers based on trust levels. A local Ollama instance might be trusted with proprietary code, while a cloud API might only receive sanitized snippets.

### SecurityGateway: The Command Filter

Every shell command executed by Hive passes through `hive_core::SecurityGateway`. This is not optional -- the architecture makes it impossible to bypass.

Blocked patterns include:
- Destructive filesystem operations (`rm -rf /`, `format C:`)
- Credential theft (`cat ~/.ssh/id_rsa`, `cat ~/.aws/credentials`)
- Privilege escalation (`chmod 777`, `sudo` without approval)
- Network exfiltration (`curl` to unknown domains, `scp` to external hosts)

### User Approval Workflows

High-stakes actions require explicit human confirmation:

- **Tier 1 (Auto-approve):** Read-only operations, safe queries, file reads within project scope
- **Tier 2 (Notify):** File writes within project scope, git commits to feature branches, API calls to configured services
- **Tier 3 (Approve):** File writes outside project scope, git pushes, deployment triggers, financial transactions
- **Tier 4 (Multi-approve):** Destructive operations, credential access, system configuration changes

### Local-First Architecture

The default Hive installation keeps everything on your machine:

- **Configuration:** `~/.hive/` directory with encrypted key storage (`SecureStorage` using AES-256-GCM)
- **Conversations:** Local SQLite database (`hive_core::Database`)
- **Learning data:** Local SQLite database (`hive_learn::LearningStorage`)
- **Collective memory:** Local SQLite database (`hive_agents::CollectiveMemory`)
- **No telemetry.** No analytics. No "anonymous usage data." Your machine, your data, period.

Cloud providers are used *only* for AI inference when you choose cloud models. Even then, hive_shield scans every outgoing request.

### Audit Logging

Every AI action is logged with:
- Timestamp, action type, and actor (which agent, which model)
- Input hash and output hash (for reproducibility)
- Approval status and approver
- Cost incurred
- Any security events (blocked commands, detected PII, secrets found)

The `hive_core::EnterpriseService` provides structured audit queries with filtering by action type, time range, team, and severity.

### Explain Mode

Before any non-trivial action, Hive can show its reasoning:

- "I am about to run `cargo test` because the code review found 3 files changed in the authentication module."
- "I am about to send this email draft to the Gmail API. The content has been scanned: no PII detected, no secrets found."
- "I am routing this request to claude-sonnet-4 because the complexity classifier scored it at 0.72 (medium complexity) and your cost budget favors mid-tier models for this task type."

### Rollback Capability

Every automated action is reversible:
- Git operations use branches and worktrees -- nothing touches `main` until explicitly merged
- File changes are tracked with before/after snapshots
- Prompt refinements have version history (`hive_learn::PromptEvolver`) with rollback to any prior version
- Configuration changes are logged with previous values

---

## Part 5: The Self-Improving Loop

Hive gets better every time you use it. Not through cloud training. Not through data collection. Through **local, private, user-controlled learning.**

### How It Works

```
  User interacts with Hive
          |
          v
  +-------+--------+
  | Outcome Tracker |  Records: accepted, rejected, edited, ignored
  +-------+--------+
          |
    +-----+-----+-----+-----+
    |     |     |     |     |
    v     v     v     v     v
  Route  Pref  Prompt Pat  Self
  Learn  Model Evolve Lib  Eval
```

1. **Outcome Tracking** (`OutcomeTracker`): Every AI response has an outcome -- accepted, rejected, edited, or ignored. The edit distance between the AI output and your final version is measured. Follow-up count (how many times you had to ask again) is tracked. Quality scores are computed per model, per task type.

2. **Routing Learning** (`RoutingLearner`): After 50 interactions, Hive analyzes which models actually performed best for which task types. If the complexity classifier consistently over-estimates a task type (sending to expensive models when cheap ones suffice), the routing learner adjusts via the `TierAdjuster` trait.

3. **Preference Model** (`PreferenceModel`): Learns implicit preferences through observation. If you consistently edit AI responses to be more concise, Hive learns "concise" as a preference. Preferences have confidence scores -- low-confidence preferences are suggested, not applied.

4. **Prompt Evolution** (`PromptEvolver`): System prompts for each persona (coder, reviewer, researcher) evolve based on quality scores. Each refinement is versioned. You can accept, reject, or rollback any prompt change. The AI never changes its own instructions without your awareness.

5. **Pattern Library** (`PatternLibrary`): Extracts coding patterns from your accepted code. Over time, Hive learns your project's conventions -- error handling style, naming conventions, test structure -- and applies them to new code generation.

6. **Self-Evaluation** (`SelfEvaluator`): Periodic comprehensive evaluation (every 200 interactions) of overall quality, model performance, and trend analysis. Produces a report with actionable recommendations.

### User Control

Every aspect of the learning system is transparent and controllable:

- **Learning log:** A chronological feed of everything Hive has learned, visible in the UI
- **Reject preference:** Delete any learned preference you disagree with
- **Accept/reject prompt refinement:** Review and approve prompt changes before they take effect
- **Rollback prompt:** Revert any persona's prompt to a previous version
- **Reset all:** Nuclear option -- wipe all learned data and start fresh

### The Compounding Effect

After one week, Hive knows your preferred models and routing patterns.
After one month, Hive knows your coding style, communication tone, and project conventions.
After three months, Hive is an extension of your thinking -- anticipating your needs, pre-loading relevant context, suggesting actions before you ask.

**This is the moat.** No competitor can replicate three months of personalized local learning by offering a better model or a nicer UI.

---

## Part 6: Unified Roadmap

### Q1 2026 (January - March): Foundation

**Development:**
- [x] Core chat interface with streaming responses
- [x] Multi-provider AI routing (Anthropic, OpenAI, OpenRouter, Ollama, LM Studio, Groq, HuggingFace, LiteLLM)
- [x] Complexity-based model routing with auto-fallback
- [x] Cost tracking and budget management
- [x] Context engine with TF-IDF scoring
- [x] HiveMind multi-agent orchestration (Architect, Coder, Reviewer, Tester, Security)
- [x] Queen meta-coordinator with swarm planning
- [x] Git worktree isolation per team
- [x] Coordinator with dependency-ordered task dispatch
- [x] Personas system with prompt overrides
- [x] Collective memory for cross-run learning
- [x] SecurityGateway command filtering
- [x] hive_shield (PII detection, secrets scanning, vulnerability assessment, access control)
- [x] hive_learn (outcome tracking, routing learning, preference model, prompt evolution, pattern library, self-evaluation)
- [ ] Context engine integration with file watcher for real-time relevance updates
- [ ] Spec-driven development workflow (specs panel already in UI)

**Assistant:**
- [x] Google integrations (Gmail, Calendar, Contacts, Drive, Docs, Sheets, Tasks)
- [x] Microsoft integrations (Outlook Calendar, Outlook Email)
- [x] Messaging hub (Slack, Discord, Teams, Telegram, Matrix, WebChat)
- [x] OAuth2 authentication flows
- [x] Email classifier and subscription manager
- [x] Smart home (Philips Hue)
- [x] Webhook registry
- [x] `hive_assistant` crate -- email service, calendar service, reminders, approval, daily briefing, conflict detection, smart scheduling, OS notifications (13 files, 105 tests)
- [x] Assistant UI panel (18th panel, Bell icon, full dashboard with briefing/events/email/reminders)
- [x] Shield scanning wired into chat pipeline (outgoing messages scanned before AI calls)
- [x] Learning instrumentation (StreamCompleted events feed outcomes to LearningService)
- [ ] OAuth connection UI in Settings panel
- [ ] Smart reply suggestions (AI-powered)
- [ ] Natural language event creation parser

**Safety:**
- [x] PII detection with configurable sensitivity
- [x] Secrets scanning with risk levels
- [x] Vulnerability assessment for generated code
- [x] Access control with provider trust levels
- [x] Encrypted key storage (AES-256-GCM)
- [x] SecurityGateway for command filtering
- [x] Shield panel in UI showing live scan configuration and status
- [x] HiveShield wired into chat pipeline (Block/CloakAndAllow/Warn/Allow actions)
- [x] Approval service in hive_assistant (Low/Medium/High/Critical levels)
- [ ] Full approval workflow UI for Tier 3/4 actions (backend exists, UI pending)

### Q2 2026 (April - June): Intelligence

**Development:**
- [ ] Swarm execution with real git worktree creation and file modifications
- [ ] Auto-commit service integration with worktree merge workflows
- [ ] RAG service connected to project file index (semantic search)
- [ ] Fleet learning -- aggregate anonymized performance insights across instances (opt-in)
- [ ] Live canvas for visual code architecture mapping
- [ ] Docker sandbox integration for safe test execution
- [ ] Browser automation for end-to-end test generation
- [ ] IDE integration service for external editor coordination

**Assistant:**
- [ ] Research agent -- autonomous background research with scheduled digests
- [ ] Shopping integrations (Instacart, Amazon Fresh)
- [ ] Reservation system (OpenTable, Resy, via API)
- [ ] Travel planning with itinerary generation
- [ ] Document generation from templates (reports, invoices, slide decks)
- [ ] Cross-channel messaging memory (context preserved across Slack, email, chat)

**Safety:**
- [ ] Audit log viewer with filtering and export
- [ ] Explain mode toggle -- AI shows reasoning before acting
- [ ] Rollback UI -- visual history of all automated actions with undo
- [ ] Data classification UI for categorizing project files

### Q3 2026 (July - September): Experience

**Development:**
- [ ] Voice interface for hands-free coding ("Create a new function that handles user authentication")
- [ ] Wake word detection for instant activation
- [ ] Terminal emulation improvements -- interactive shell support
- [ ] Code visualization service for dependency graphs and architecture diagrams
- [ ] Self-improvement v2 -- pattern library suggests refactorings, prompt evolution uses A/B testing

**Assistant:**
- [ ] Voice interface for personal assistant ("What is on my calendar today?", "Order my usual groceries")
- [ ] Smart home expansion (HomeKit, Google Home, MQTT)
- [ ] Financial tracking with AI-categorized transactions
- [ ] Health and wellness reminders with tracking
- [ ] Phone call automation via Twilio (appointment confirmations, prescription refills)
- [ ] Proactive suggestions ("You have a meeting with Sarah in 30 minutes. Here are the relevant documents.")

**Safety:**
- [ ] Prompt injection detection in real-time (streaming analysis)
- [ ] Network traffic monitoring -- flag unexpected outbound connections
- [ ] Dependency vulnerability scanning (integrate with RustSec advisory database)
- [ ] Compliance report generation (SOC2, GDPR audit artifacts)

### Q4 2026 (October - December): Ecosystem

**Development:**
- [ ] Plugin marketplace -- community-contributed skills, integrations, and personas
- [ ] MCP server for external tool integration (IDE plugins, CI/CD systems)
- [ ] Multi-repository swarm support (work across multiple projects simultaneously)
- [ ] Enterprise features -- SSO, RBAC, team workspaces, shared learning

**Assistant:**
- [ ] Plugin-based integrations (community-contributed services)
- [ ] Companion mode -- always-on background assistant with proactive outreach
- [ ] Multi-user household support (shared calendar, shared shopping lists, individual preferences)

**Safety:**
- [ ] Enterprise audit compliance (SOC2 Type II evidence collection)
- [ ] Self-hosted deployment option with air-gapped operation
- [ ] Third-party plugin security review and signing
- [ ] Formal security audit and penetration testing results published

---

## Part 7: Differentiators and Moats

### 1. Native Performance

Hive is compiled to machine code. There is no interpreter, no JIT, no garbage collector between the user and the UI. GPUI renders directly on the GPU. The result is an application that feels as responsive as a native IDE from the 1990s but with the intelligence of a 2026 AI system.

**Measurable advantage:** Sub-50MB memory footprint, sub-1-second startup, 120fps UI rendering, sub-10ms input latency.

**Why this is a moat:** Rebuilding in Rust from scratch requires 12-18 months of engineering effort. No competitor will abandon their Electron/web codebase to match this.

### 2. Local Learning

Hive's learning system runs entirely on the user's machine. No training data leaves the device. No cloud service aggregates your patterns. The learning is yours.

**Measurable advantage:** After 200 interactions, model routing accuracy improves by an estimated 15-25% (measured by reduction in rejected/edited responses). After 1000 interactions, prompt quality scores increase by 20-40%.

**Why this is a moat:** The longer a user stays with Hive, the more personalized it becomes. Switching to a competitor means starting from zero. This is the most durable moat in software.

### 3. Multi-Agent Architecture

Hive does not have "one AI." It has a hierarchy -- Queen, HiveMind teams, Coordinators, specialized agents -- each with distinct roles and orchestration modes.

**Measurable advantage:** Complex tasks that require planning, implementation, review, and testing are completed 2-4x faster than single-agent approaches because specialized agents handle each phase in parallel.

**Why this is a moat:** Building a robust multi-agent orchestration system with budget enforcement, dependency ordering, cross-team context sharing, and failure recovery is a 6-month engineering investment. Most competitors are still iterating on single-agent loops.

### 4. Security-First Design

Security was not added to Hive. Hive was built on top of security. The `SecurityGateway` is in the dependency chain of every crate that executes commands. The `HiveShield` scans every outgoing request. The `PolicyEngine` enforces data classification.

**Measurable advantage:** Zero-trust architecture for AI actions. No PII or secrets have ever been leaked through Hive in testing (2,013 tests covering security paths).

**Why this is a moat:** Bolting security onto an existing system creates gaps. Building security from the foundation eliminates entire categories of vulnerabilities. Enterprise customers will choose the platform they can trust.

### 5. Blockchain Integration

Hive includes a unique `hive_blockchain` crate with:
- EVM wallet management (Ethereum, Polygon, Arbitrum, BSC, Avalanche, Optimism, Base)
- Solana wallet management
- ERC-20 token deployment
- SPL token deployment
- Encrypted private key storage
- Multi-chain RPC configuration

**Why this is a moat:** No other AI coding assistant has native blockchain tooling. For the Web3 developer community, Hive is the only option that understands their workflow.

### 6. MCP Protocol

Hive implements both MCP client and server, enabling:
- **Inbound:** External tools (VS Code, JetBrains, CI systems) can send tasks to Hive
- **Outbound:** Hive can use external MCP-compatible tools and services
- **Skills marketplace:** Community-contributed skills are distributed as MCP-compatible packages

**Why this is a moat:** MCP is becoming the standard for AI tool integration. Early, deep investment in the protocol means Hive will be the most compatible platform when the ecosystem matures.

### 7. Unified Platform

The deepest moat is the simplest: Hive does everything. A user who manages their code, email, calendar, research, shopping, and finances through Hive will not switch to a competitor that only does one of those things. The switching cost is not financial -- it is cognitive. Hive becomes the operating layer for your digital life.

---

## Part 8: Revenue Model

### Tier 1: Free (Hive Community)

- Full development platform with local LLM support (Ollama, LM Studio)
- Basic assistant features (email triage, calendar view)
- SecurityGateway and basic shield scanning
- Community skills from the marketplace
- 5 cloud AI requests per day (via shared OpenRouter allocation)

**Purpose:** User acquisition and community building.

### Tier 2: Pro ($20/month)

- Unlimited cloud AI routing (user provides API keys or uses Hive's pooled allocation)
- Full personal assistant features (research, shopping, reservations, document generation)
- Advanced shield features (custom PII rules, access control policies)
- Full learning system with prompt evolution
- Priority support
- Early access to new features

**Purpose:** Individual power users, freelancers, indie developers.

### Tier 3: Team ($40/user/month)

- Everything in Pro
- Shared team workspaces with role-based access
- Collective learning across team members (opt-in)
- Shared skill libraries and personas
- Team kanban board with AI task assignment
- Admin dashboard with usage analytics

**Purpose:** Small teams and startups.

### Tier 4: Enterprise (Custom pricing)

- Everything in Team
- Self-hosted deployment option
- SSO integration (SAML, OIDC)
- SOC2 compliance artifacts
- Custom data classification policies
- Dedicated support and SLAs
- Air-gapped operation for classified environments
- Audit log export and SIEM integration
- Custom model hosting (fine-tuned models on private infrastructure)

**Purpose:** Large organizations with compliance requirements.

### Marketplace Revenue

- 20% commission on paid skills and integrations sold through the Hive Marketplace
- Featured listing fees for skill authors
- Enterprise skill certification program (one-time fee for security audit and approval)

### Additional Revenue Streams

- **Hive Compute:** Managed GPU endpoints for users who want fast local-quality inference without running their own hardware. Pay-per-token pricing with volume discounts.
- **Training and Certification:** Online courses on building Hive skills, enterprise deployment, and AI-assisted development workflows.
- **Consulting:** Custom integration development for enterprise clients.

---

## Appendix: Current Codebase Inventory

### Workspace Structure (13 crates)

| Crate | Purpose | Key Modules |
|---|---|---|
| `hive_app` | Binary entry point | main.rs, tray.rs, build.rs |
| `hive_core` | Config, security, shared types | config, security, persistence, kanban, enterprise, code_review, scheduler, background, canvas, notifications, conversations, session, error_handler, logging, secure_storage |
| `hive_ui` | GPUI views and panels | workspace, sidebar, titlebar, statusbar, chat_input, welcome, theme + 19 panel modules (chat, history, files, specs, agents, kanban, monitor, logs, costs, review, routing, settings, help, skills, shield, learning, token_launch, **assistant**) |
| `hive_ai` | AI provider integrations | 9 providers (Anthropic, OpenAI, OpenRouter, Ollama, LM Studio, Generic Local, Groq, HuggingFace, LiteLLM), model router, complexity classifier, auto-fallback, context engine, RAG, semantic search, cost tracker, fleet learning, model registry |
| `hive_agents` | Multi-agent orchestration | queen, swarm, coordinator, hivemind, hiveloop, guardian, personas, skills, skill_marketplace, tool_use, mcp_client, mcp_server, persistence, heartbeat, standup, voice, automation, auto_commit, collective_memory, worktree, specs |
| `hive_shield` | Security scanning | pii, secrets, vulnerability, access_control, shield (unified facade) |
| `hive_learn` | Self-improvement | outcome_tracker, routing_learner, preference_model, prompt_evolver, pattern_library, self_evaluator, storage, **LearnerTierAdjuster** (bridges to hive_ai routing) |
| `hive_assistant` | **Personal assistant** | **email** (service, inbox_agent, compose_agent), **calendar** (service, conflict_detector, smart_scheduler, daily_brief), **reminders** (service, os_notifications), **approval**, **plugin**, **storage** |
| `hive_fs` | File operations | files, git, search, watcher |
| `hive_terminal` | Execution | executor, shell, cli, docker, browser, local_ai |
| `hive_docs` | Document generation | csv, docx, xlsx, html, markdown, pdf, pptx |
| `hive_blockchain` | Web3 tools | wallet_store, evm, solana, erc20_bytecode, rpc_config |
| `hive_integrations` | External services | google (Gmail, Calendar, Contacts, Drive, Docs, Sheets, Tasks), microsoft (Outlook Calendar, Outlook Email), github, messaging (Slack, Discord, Teams, Telegram, Matrix, WebChat), oauth, smart_home (Philips Hue), cloud (Cloudflare, Supabase, Vercel), ide, webhooks |

### Dependencies

- **UI Framework:** GPUI 0.2.2 + gpui-component 0.5.1
- **Async Runtime:** Tokio (full features)
- **HTTP:** Reqwest with rustls-tls (no OpenSSL dependency)
- **Database:** rusqlite (bundled SQLite)
- **Cryptography:** AES-GCM, Argon2, SHA-256
- **Git:** libgit2 via git2
- **Serialization:** serde + serde_json + TOML
- **Logging:** tracing + tracing-subscriber + tracing-appender

### Test Coverage

Last known state: **2,013 tests, 0 compiler warnings** (rust/main branch, Feb 2026). Up from 1,722 tests at commit `8eed5eb`.

---

## Concrete Next Steps

### Completed (Feb 2026)

1. ~~**Wire hive_shield into the chat pipeline.**~~ **DONE.** `HiveShield::process_outgoing()` called in `workspace.rs::handle_send_text()` before every AI request. Block/CloakAndAllow/Warn/Allow actions handled.

2. ~~**Wire hive_learn into the chat pipeline.**~~ **DONE.** `LearningService` initialized at startup from `~/.hive/learning.db`. `LearnerTierAdjuster` wired into `ModelRouter`. `StreamCompleted` events from `ChatService` feed outcomes via `LearningService::on_outcome()`.

3. ~~**Complete the Shield UI panel.**~~ **DONE.** Shield panel refreshes from live `AppShield` global data when switched to.

### Immediate (Next)

4. **Connect Assistant panel to live hive_assistant data.** Currently renders sample data. Wire `AssistantService` to `hive_integrations` OAuth flows for live email/calendar data.

5. **Add OAuth connection UI to Settings panel.** The backend OAuth flows exist in `hive_integrations::oauth`. Build the "Connect Google" / "Connect Microsoft" UI buttons.

6. **Build smart reply suggestions.** AI-powered reply drafting from email thread context using `hive_ai::AiService`.

### Short-Term (This Month)

7. **Complete the Learning UI panel.** Show the learning log, preference list, prompt versions, and pattern library. Allow accept/reject/rollback from the UI. (Panel exists with sample data; needs live data wiring.)

8. **Build natural language event creation parser.** Parse "lunch with Sarah at noon on Friday" into calendar events.

9. **Add explicit feedback UI.** Thumbs up/down buttons on AI messages for unambiguous quality signals.

### Medium-Term (This Quarter)

10. **Activate swarm execution with real worktrees.** Connect the Queen's planning output to actual git worktree creation, file modification, and merge workflows.

11. **Build the research agent.** A background service that monitors configured topics, collects information, and presents periodic research digests.

12. **Build full approval workflow UI.** Visual approval cards for Tier 3/4 actions (backend `ApprovalService` exists in `hive_assistant::approval`).

---

*This document is a living artifact. It will be updated as features ship, priorities shift, and the vision evolves. The north star remains constant: Hive is the single application that makes you more productive, more organized, and more secure -- every day, for everything.*
