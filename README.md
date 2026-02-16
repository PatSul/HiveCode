<p align="center">
  <img src="hive/assets/hive_bee.png" width="80" alt="Hive logo" />
</p>

<h1 align="center">Hive</h1>

<p align="center">
  <strong>Your AI that learns, protects, and works while you sleep.</strong>
</p>

<p align="center">
  <a href="https://github.com/PatSul/Hive/releases"><img src="https://img.shields.io/github/v/release/PatSul/Hive?label=download&color=brightgreen&cache=1" alt="Download" /></a>
  <img src="https://img.shields.io/badge/version-0.2.0-blue" alt="Version" />
  <img src="https://img.shields.io/badge/language-Rust-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/tests-2%2C531-brightgreen" alt="Tests" />
  <img src="https://img.shields.io/badge/crates-16-blue" alt="Crates" />
  <img src="https://img.shields.io/badge/warnings-0-brightgreen" alt="Warnings" />
  <img src="https://img.shields.io/badge/lines-127k%2B-informational" alt="Lines of Rust" />
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20(Apple%20Silicon)%20%7C%20Linux-informational" alt="Windows | macOS (Apple Silicon) | Linux" />
  <img src="https://img.shields.io/badge/UI-GPUI-blueviolet" alt="GPUI" />
</p>

---

## What Is Hive?

Hive is a **native Rust desktop AI platform** built on [GPUI](https://gpui.rs) — no Electron, no web wrappers. It unifies a development environment, a personal assistant framework, and a security-first architecture into a single application. Instead of one chatbot, Hive runs a **multi-agent swarm** that can plan, build, test, and orchestrate workflows while learning your preferences over time — all while ensuring no secret or PII ever leaves your machine without approval.

What makes Hive different: it **learns from every interaction** (locally, privately), it **detects its own knowledge gaps** and autonomously researches and acquires new skills, and it **federates** across instances for distributed swarm execution.

---

## The Three Pillars

<table>
<tr>
<td width="33%" valign="top">

### Development Excellence
- Multi-agent swarm (Queen + teams)
- 11 AI providers with auto-routing
- Git worktree isolation per team
- Full Git Ops (commits, PRs, branches, gitflow, LFS)
- Context engine (TF-IDF scoring)
- Cost tracking & budget enforcement
- Code review & testing automation
- ClawdHub skill marketplace
- Autonomous skill acquisition (self-teaching)
- Automation workflows (cron, event, webhook triggers)
- Docker sandbox with real CLI integration
- MCP client + server
- P2P federation across instances

</td>
<td width="33%" valign="top">

### Assistant Excellence
- Email triage & AI-powered drafting
- Calendar integration & daily briefings
- Reminders (time, recurring cron, event-triggered)
- Approval workflows with audit trails
- Document generation (7 formats)
- Smart home control
- Voice assistant (wake word + intent)

</td>
<td width="33%" valign="top">

### Safety Excellence
- PII detection (11+ types)
- Secrets scanning with risk levels
- Vulnerability assessment
- SecurityGateway command filtering
- Encrypted storage (AES-256-GCM)
- Provider trust-based access control
- Local-first — no telemetry

</td>
</tr>
</table>

---

## AI & Multi-Agent System

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

**Queen** decomposes high-level goals into team objectives with dependency ordering, dispatches teams with the appropriate orchestration mode, enforces budget and time limits, shares cross-team insights, synthesizes results, and records learnings to collective memory.

**HiveMind teams** use specialized agents — Architect, Coder, Reviewer, Tester, Security — that reach consensus through structured debate.

**Coordinator teams** decompose work into dependency-ordered tasks (investigate, implement, verify) with persona-specific prompts.

Every team gets its own **git worktree** (`swarm/{run_id}/{team_id}`) for conflict-free parallel execution, merging back on completion.

### AI Providers

11 providers with automatic complexity-based routing and fallback:

| Cloud | Local |
|---|---|
| Anthropic (Claude) | Ollama |
| OpenAI (GPT) | LM Studio |
| Google (Gemini) | Generic OpenAI-compatible |
| OpenRouter (100+ models) | LiteLLM proxy |
| Groq (fast inference) | |
| HuggingFace | |

Features: complexity classification, 14-entry fallback chain, per-model cost tracking, streaming support, budget enforcement.

---

## Autonomous Skill Acquisition

Hive doesn't just execute what it already knows — it **recognizes what it doesn't know** and teaches itself. This is the closed-loop system that lets Hive grow its own capabilities in real time:

```
User request
    |
    v
Competence Detection ─── "I know this" ───> Normal execution
    |
    "I don't know this"
    |
    v
Search ClawdHub / Sources ─── Found sufficient skill? ───> Install & use
    |
    Not found (or insufficient)
    |
    v
Knowledge Acquisition ───> Fetch docs, parse, synthesize
    |
    v
Skill Authoring Pipeline ───> Generate, security-scan, test, install
    |
    v
New skill available for future requests
```

### Competence Detection

The **CompetenceDetector** scores Hive's confidence on every incoming request using a weighted formula across four signals:

| Signal | Weight | Source |
|---|---|---|
| Skill match | 30% | Exact trigger/name match in skills registry |
| Pattern match | 20% | Keyword overlap with marketplace skill descriptions |
| Memory match | 15% | Relevant entries in collective memory |
| AI assessment | 35% | Lightweight model call rating confidence 0-10 |

When confidence drops below the learning threshold (default 0.4), the system identifies **competence gaps** — missing skills, missing knowledge, low-quality skills, or absent patterns — and triggers the acquisition pipeline automatically.

A **quick assessment** mode (no AI call) is available for low-latency checks using purely pattern-based matching.

### Knowledge Acquisition

The **KnowledgeAcquisitionAgent** is a research agent that autonomously:

1. **Identifies** the best documentation URLs for a topic (AI-orchestrated)
2. **Fetches** pages via HTTPS with domain allowlisting and private-IP blocking
3. **Parses** HTML to clean text — strips scripts, styles, nav, footers; extracts `<code>` blocks with language detection
4. **Caches** locally (`~/.hive/knowledge/`) with SHA-256 content hashing and configurable TTL (default 7 days)
5. **Synthesizes** knowledge via AI into structured summaries (key concepts, relevant commands, code examples)
6. **Injects** results into the ContextEngine as `Documentation` sources for future queries

Security: HTTPS-only, 23+ allowlisted documentation domains (docs.rs, kubernetes.io, react.dev, MDN, etc.), private IP rejection, content scanned for injection before storage, configurable page-size limits.

### Skill Authoring Pipeline

When no existing skill is found, the **SkillAuthoringPipeline** creates one:

1. **Search existing skills first** — Queries ClawdHub directory and remote sources. Each candidate is AI-scored for sufficiency (0-10). Skills scoring >= 7 are installed directly.
2. **Research** — Delegates to KnowledgeAcquisitionAgent if no sufficient existing skill is found
3. **Generate** — AI creates a skill definition (name, trigger, category, prompt template, test input)
4. **Security scan** — Runs the same 6-category injection scan used for community skills. Retries up to 2x on failure.
5. **Test** — Validates the skill produces relevant output for the sample input
6. **Install** — Adds to marketplace with `/hive-` trigger prefix, disabled by default until user enables

All auto-generated skills are logged to CollectiveMemory for auditability. The pipeline fails gracefully at every step — a failed scan or test never installs a broken skill.

---

## Personal Assistant

The assistant uses the same AI infrastructure as the development platform — same model routing, same security scanning, same learning loop.

| Capability | Details |
|---|---|
| **Email** | Gmail and Outlook inbox polling, email digest generation, AI-powered composition and reply drafting with shield-scanned outbound content. |
| **Calendar** | Google Calendar and Outlook event fetching, daily briefing generation, conflict detection and scheduling logic. |
| **Reminders** | Time-based, recurring (cron), and event-triggered. Snooze/dismiss. Project-scoped. Native OS notifications. SQLite persistence. |
| **Approvals** | Multi-level workflows (Low / Medium / High / Critical). Submit, approve, reject with severity tracking. |
| **Documents** | Generate CSV, DOCX, XLSX, HTML, Markdown, PDF, and PPTX from templates or AI. |
| **Smart Home** | Philips Hue control — lighting scenes, routines, individual light states. |
| **Plugins** | `AssistantPlugin` trait for community extensibility. |

---

## Security & Privacy

Security is the **foundation**, not a feature bolted on. Every outgoing message is scanned. Every command is validated.

### HiveShield — 4 Layers of Protection

| Layer | What It Does |
|---|---|
| **PII Detection** | 11+ types (email, phone, SSN, credit card, IP, name, address, DOB, passport, driver's license, bank account). Cloaking modes: Placeholder, Hash, Redact. |
| **Secrets Scanning** | API keys, tokens, passwords, private keys. Risk levels: Critical, High, Medium, Low. |
| **Vulnerability Assessment** | Prompt injection detection, jailbreak attempts, unsafe code patterns, threat scoring. |
| **Access Control** | Policy-based data classification. Provider trust levels: Local, Trusted, Standard, Untrusted. |

### SecurityGateway

Hive routes command execution paths through `SecurityGateway` checks and blocks destructive filesystem ops, credential theft, privilege escalation, and common exfiltration patterns.

### Local-First

- All data in `~/.hive/` — config, conversations, learning data, collective memory
- Encrypted key storage (AES-256-GCM + Argon2id key derivation)
- **No telemetry. No analytics. No cloud dependency.**
- Cloud providers used only for AI inference when you choose cloud models — and even then, HiveShield scans every request

---

## Self-Improvement Engine

Hive gets smarter every time you use it. Entirely local. No data leaves your machine.

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

| System | Function |
|---|---|
| **Outcome Tracker** | Quality scores per model and task type. Edit distance and follow-up penalties. |
| **Routing Learner** | EMA analysis adjusts model tier selection. Wired into `ModelRouter` via `TierAdjuster`. |
| **Preference Model** | Bayesian confidence tracking. Learns tone, detail level, formatting from observation. |
| **Prompt Evolver** | Versioned prompts per persona. Quality-gated refinements with rollback support. |
| **Pattern Library** | Extracts code patterns from accepted responses (6 languages: Rust, Python, JS/TS, Go, Java/Kotlin, C/C++). |
| **Self-Evaluator** | Comprehensive report every 200 interactions. Trend analysis, misroute rate, cost-per-quality-point. |

All learning data stored locally in SQLite (`~/.hive/learning.db`). Every preference is transparent, reviewable, and deletable.

---

## Automation & Skills

| Feature | Details |
|---|---|
| **Automation Workflows** | Multi-step workflows with triggers (manual, cron schedule, event, webhook) and 6 action types (run command, send message, call API, create task, send notification, execute skill). YAML-based definitions in `~/.hive/workflows/`. |
| **ClawdHub Marketplace** | Browse, install, remove, and toggle skills. Create custom skills. Add remote skill sources. Built-in directory of curated skills. Security scanning on install. |
| **Autonomous Skill Creation** | When Hive encounters an unfamiliar domain, it searches existing skill sources first, then researches documentation and authors a new skill if nothing sufficient exists. See [Autonomous Skill Acquisition](#autonomous-skill-acquisition). |
| **Personas** | Named agent personalities with custom system prompts, prompt overrides per task type, and configurable model preferences. |
| **Auto-Commit** | Watches for staged changes and generates AI-powered commit messages. |
| **Daily Standups** | Automated agent activity summaries across all teams and workflows. |
| **Voice Assistant** | Wake-word detection, natural-language voice commands, intent recognition, and state-aware responses. |

---

## Terminal & Execution

| Feature | Details |
|---|---|
| **Shell Execution** | Run commands with configurable timeout, output capture, working directory management, and exit code tracking. |
| **Docker Sandbox** | Full container lifecycle: create, start, stop, exec, pause, unpause, remove. Real Docker CLI integration with simulation fallback for testing. |
| **Browser Automation** | Chrome DevTools Protocol over WebSocket: navigation, screenshots, JavaScript evaluation, DOM manipulation. |
| **CLI Service** | Built-in commands (`/doctor`, `/clear`, etc.) and system health checks. |
| **Local AI Detection** | Auto-discovers Ollama, LM Studio, and llama.cpp running on localhost. |

---

## P2P Federation

Hive instances can discover and communicate with each other over the network, enabling distributed swarm execution and shared learning.

| Feature | Details |
|---|---|
| **Peer Discovery** | UDP broadcast for automatic LAN discovery, plus manual bootstrap peers |
| **WebSocket Transport** | Bidirectional P2P connections with split-sink/stream architecture |
| **Typed Protocol** | 12 built-in message kinds (Hello, Welcome, Heartbeat, TaskRequest, TaskResult, AgentRelay, ChannelSync, FleetLearn, StateSync, etc.) plus extensible custom types |
| **Channel Sync** | Synchronize agent channel messages across federated instances |
| **Fleet Learning** | Share learning outcomes across a distributed fleet of nodes |
| **Peer Registry** | Persistent tracking of known peers with connection state management |

---

## Integrations

<table>
<tr><td><strong>Google</strong></td><td>Gmail, Calendar, Contacts, Drive, Docs, Sheets, Tasks</td></tr>
<tr><td><strong>Microsoft</strong></td><td>Outlook Email, Outlook Calendar</td></tr>
<tr><td><strong>Messaging</strong></td><td>Slack, Discord, Teams, Telegram, Matrix, WebChat</td></tr>
<tr><td><strong>Cloud</strong></td><td>GitHub, Cloudflare, Vercel, Supabase</td></tr>
<tr><td><strong>Smart Home</strong></td><td>Philips Hue</td></tr>
<tr><td><strong>Voice</strong></td><td>ClawdTalk (voice-over-phone via Telnyx)</td></tr>
<tr><td><strong>Protocol</strong></td><td>MCP client + server, OAuth2 (PKCE), Webhooks, P2P federation</td></tr>
</table>

---

## Blockchain / Web3

| Chain | Features |
|---|---|
| **EVM** (Ethereum, Polygon, Arbitrum, BSC, Avalanche, Optimism, Base) | Wallet management, real JSON-RPC (`eth_getBalance`, `eth_gasPrice`), per-chain RPC configuration, ERC-20 token deployment with cost estimation |
| **Solana** | Wallet management, real JSON-RPC (`getBalance`, `getTokenAccountsByOwner`, `getMinimumBalanceForRentExemption`), SPL token deployment with rent cost estimation |
| **Security** | Encrypted private key storage (AES-256-GCM), no keys ever sent to AI providers |

---

## Architecture — 16-Crate Workspace

```
hive/crates/
├── hive_app           Binary entry point — window, tray, build.rs (winres)
│                      3 files · 965 lines
├── hive_ui            Workspace shell, chat service, learning bridge, title/status bars
│                      21 files · 10,751 lines
├── hive_ui_core       Theme, actions, globals, sidebar, welcome screen
│                      6 files · 889 lines
├── hive_ui_panels     All panel implementations (20+ panels)
│                      42 files · 26,258 lines
├── hive_core          Config, SecurityGateway, persistence (SQLite), Kanban, channels, scheduling
│                      18 files · 9,691 lines
├── hive_ai            11 AI providers, model router, complexity classifier, context engine, RAG
│                      39 files · 17,741 lines
├── hive_agents        Queen, HiveMind, Coordinator, collective memory, MCP, skills, personas,
│                      knowledge acquisition, competence detection, skill authoring
│                      25 files · 21,399 lines
├── hive_shield        PII detection, secrets scanning, vulnerability assessment, access control
│                      6 files · 2,008 lines
├── hive_learn         Outcome tracking, routing learner, preference model, prompt evolution
│                      10 files · 5,438 lines
├── hive_assistant     Email, calendar, reminders, approval workflows, daily briefings
│                      13 files · 4,424 lines
├── hive_fs            File operations, git integration, file watchers, search
│                      5 files · 1,150 lines
├── hive_terminal      Command execution, Docker sandbox, browser automation, local AI detection
│                      8 files · 5,877 lines
├── hive_docs          Document generation — CSV, DOCX, XLSX, HTML, Markdown, PDF, PPTX
│                      8 files · 1,478 lines
├── hive_blockchain    EVM + Solana wallets, RPC config, token deployment with real JSON-RPC
│                      6 files · 1,669 lines
├── hive_integrations  Google, Microsoft, GitHub, messaging, OAuth2, smart home, cloud, webhooks
│                      35 files · 14,501 lines
└── hive_network       P2P federation, WebSocket transport, UDP discovery, peer registry, sync
                       11 files · 2,762 lines
```

---

## UI — 20+ Panels

| Panel | Description |
|---|---|
| Chat | Main AI conversation interface |
| History | Conversation history browser |
| Files | Project file browser with create/delete/navigate |
| Specs | Specification management |
| Agents | Multi-agent swarm orchestration |
| Workflows | Visual workflow builder (drag-and-drop nodes) |
| Channels | Agent messaging channels (Telegram/Slack-style) |
| Kanban | Task board with drag-and-drop |
| Monitor | Real-time system resource monitoring (CPU, RAM, disk) |
| Logs | Application logs viewer with level filtering |
| Costs | AI cost tracking and budget with CSV export |
| Git Ops | Full git workflow: staging, commits, push, PRs, branches, gitflow, LFS |
| ClawdHub | Skill marketplace: browse, install, remove, toggle, create, sources |
| Routing | Model routing configuration |
| Models | Model registry browser |
| Learning | Self-improvement dashboard |
| Shield | Security scanning status |
| Assistant | Personal assistant: email, calendar, reminders |
| Token Launch | Token deployment wizard with chain selection |
| Settings | Application configuration |
| Help | Documentation and guides |

---

## Installation

### Option 1: Download Pre-Built Binary (Recommended)

Grab the latest release for your platform from [**GitHub Releases**](https://github.com/PatSul/Hive/releases).

| Platform | Download | Runtime Requirements |
|---|---|---|
| **Windows** (x64) | `hive-windows-x64.zip` | Windows 10/11, GPU with DirectX 12 |
| **macOS** (Apple Silicon) | `hive-macos-arm64.tar.gz` | macOS 12+, Metal-capable GPU |
| **Linux** (x64) | `hive-linux-x64.tar.gz` | Vulkan-capable GPU + drivers (see below) |

**Windows:** Extract the zip, run `hive.exe`. No installer needed.

**macOS:** Extract, then `chmod +x hive && ./hive` (or move to `/usr/local/bin/`).

**Linux:** Extract, then `chmod +x hive && ./hive`. You need Vulkan drivers installed:
```bash
# Ubuntu/Debian
sudo apt install mesa-vulkan-drivers vulkan-tools

# Fedora
sudo dnf install mesa-vulkan-drivers vulkan-tools

# Arch
sudo pacman -S vulkan-icd-loader vulkan-tools
```

### Option 2: Build from Source

#### Prerequisites

1. **Rust toolchain** — install from [rustup.rs](https://rustup.rs):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Platform-specific dependencies:**

   <details>
   <summary><strong>Windows</strong></summary>

   - [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) with C++ workload (`Microsoft.VisualStudio.Component.VC.Tools.x86.x64`)
   - Run from **VS Developer Command Prompt** or set `INCLUDE`/`LIB` environment variables
   </details>

   <details>
   <summary><strong>macOS</strong></summary>

   ```bash
   xcode-select --install
   ```
   </details>

   <details>
   <summary><strong>Linux</strong></summary>

   ```bash
   # Ubuntu/Debian
   sudo apt install build-essential libssl-dev pkg-config \
     libvulkan-dev libwayland-dev libxkbcommon-dev \
     libxcb-shape0-dev libxcb-xfixes0-dev \
     libglib2.0-dev libgtk-3-dev libxdo-dev

   # Fedora
   sudo dnf install gcc openssl-devel pkg-config \
     vulkan-devel wayland-devel libxkbcommon-devel

   # Arch
   sudo pacman -S base-devel openssl pkg-config \
     vulkan-icd-loader wayland libxkbcommon
   ```
   </details>

#### Build & Run

```bash
git clone https://github.com/PatSul/Hive.git
cd Hive/hive
cargo build --release
cargo run --release
```

#### Run Tests

```bash
cd hive
cargo test --workspace
```

---

## Project Stats

| Metric | Value |
|---|---|
| Version | 0.2.0 |
| Crates | 16 |
| Rust source files | 256 |
| Lines of Rust | 127,001 |
| Tests | 2,531 |
| Compiler warnings | 0 |
| Memory footprint | < 50 MB |
| Startup time | < 1 second |
| UI rendering | 120fps (GPU-accelerated via GPUI) |

---

## Changelog

### v0.2.0

**Autonomous Skill Acquisition** — Hive can now detect its own knowledge gaps, research documentation, and author new skills entirely on its own.

- **Knowledge Acquisition Agent** — Fetches documentation from 23+ allowlisted domains, parses HTML to clean text with code block extraction, caches locally with SHA-256 dedup and 7-day TTL, synthesizes via AI into structured summaries, and injects into the context engine for future queries.
- **Competence Detection** — Self-awareness layer that scores confidence (0.0-1.0) across skill match, pattern overlap, memory recall, and AI assessment. Identifies gap types (missing skill, missing knowledge, low quality, no patterns) and triggers the learning pipeline when confidence is low.
- **Skill Authoring Pipeline** — Search-first approach: queries ClawdHub directory and remote sources, AI-scores each candidate for sufficiency (>= 7/10 threshold). Only if no sufficient existing skill is found does it research, generate, security-scan, test, and install a new `/hive-` prefixed skill. All auto-generated skills are disabled by default until user enables them.
- **P2P Federation** (`hive_network`) — UDP broadcast peer discovery, WebSocket transport, 12 typed message kinds, channel sync, fleet learning, persistent peer registry.
- **Blockchain / Web3** (`hive_blockchain`) — EVM multi-chain (7 networks) and Solana wallet management with real JSON-RPC calls, token deployment with cost estimation, encrypted key storage.
- **Docker Sandbox** — Real Docker CLI integration with container lifecycle management (create, start, stop, exec, pause, unpause, remove) and simulation fallback.
- Over 45 new tests across knowledge acquisition, competence detection, and skill authoring modules.
- Increased total test count from 2,486 to 2,531.
- Updated to 256 source files and 127,001 lines of Rust.

### v0.1.0

- Initial release with 16-crate architecture, multi-agent swarm (Queen + HiveMind + Coordinator), 11 AI providers, HiveShield security (PII detection, secrets scanning, vulnerability assessment), self-improvement engine (5 feedback loops), ClawdHub skill marketplace, personal assistant (email, calendar, reminders), 20+ UI panels, automation workflows, and full Git Ops.

---

## License

This project is licensed under the **Business Source License 1.1**. It is free for personal and small-business use. For large-scale commercial use, please contact the author. The license will convert to Apache 2.0 on January 1, 2029. See [LICENSE](LICENSE) for details.

For organizations requiring commercial use or priority support, see our [Enterprise Documentation](docs/ENTERPRISE.md).

## Security

Hive is built on a local-first, zero-trust architecture with a 4-layer outbound firewall (HiveShield), command-level SecurityGateway, and AES-256-GCM encrypted storage. For the full technical deep-dive, see [SECURITY.md](SECURITY.md).

---

<p align="center">
  <sub>Built with Rust, GPUI, and an unreasonable amount of ambition.</sub>
</p>
