<p align="center">
  <img src="hive/assets/hive_bee.png" width="80" alt="Hive logo" />
</p>

<h1 align="center">Hive</h1>

<p align="center">
  <strong>Your AI that learns, protects, and works while you sleep.</strong>
</p>

<p align="center">
  <a href="https://github.com/PatSul/Hive/releases"><img src="https://img.shields.io/github/v/release/PatSul/Hive?label=download&color=brightgreen&cache=1" alt="Download" /></a>
  <img src="https://img.shields.io/badge/language-Rust-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/tests-2k%2B-brightgreen" alt="Tests" />
  <img src="https://img.shields.io/badge/crates-13-blue" alt="Crates" />
  <img src="https://img.shields.io/badge/warnings-0-brightgreen" alt="Warnings" />
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20(Apple%20Silicon)%20%7C%20Linux-informational" alt="Windows | macOS (Apple Silicon) | Linux" />
  <img src="https://img.shields.io/badge/UI-GPUI-blueviolet" alt="GPUI" />
</p>

---

## What Is Hive?

Hive is a **native Rust desktop AI platform** built on [GPUI](https://gpui.rs) — no Electron, no web wrappers. It unifies a development environment, a personal assistant framework, and a security-first architecture into a single application. Instead of one chatbot, Hive runs a **multi-agent swarm** that can plan, build, test, and orchestrate workflows while learning your preferences over time — all while ensuring no secret or PII ever leaves your machine without approval.

---

## The Three Pillars

<table>
<tr>
<td width="33%" valign="top">

### Development Excellence
- Multi-agent swarm (Queen + teams)
- 11 AI providers with auto-routing
- Git worktree isolation per team
- Context engine (TF-IDF scoring)
- Cost tracking & budget enforcement
- Code review & testing automation
- MCP client + server

</td>
<td width="33%" valign="top">

### Assistant Excellence
- Email triage & drafting workflows
- Calendar planning workflows
- Smart scheduling & daily briefings
- Reminders with OS notifications
- Approval workflows
- Document generation (7 formats)
- Smart home control

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

**Coordinator teams** decompose work into dependency-ordered tasks (investigate → implement → verify) with persona-specific prompts.

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

## Personal Assistant

The assistant uses the same AI infrastructure as the development platform — same model routing, same security scanning, same learning loop.

| Capability | Details |
|---|---|
| **Email** | Assistant email workflows with shield-scanned outbound content. Gmail/Outlook integration clients are included in `hive_integrations`; direct provider wiring in `hive_assistant` is in progress. |
| **Calendar** | Assistant calendar workflows with conflict detection and scheduling logic. Google/Outlook integration clients are included in `hive_integrations`; direct provider wiring in `hive_assistant` is in progress. |
| **Reminders** | Time-based and recurring. Snooze/dismiss. Native OS notifications. SQLite persistence. |
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

## Integrations

<table>
<tr><td><strong>Google</strong></td><td>Gmail, Calendar, Contacts, Drive, Docs, Sheets, Tasks</td></tr>
<tr><td><strong>Microsoft</strong></td><td>Outlook Email, Outlook Calendar</td></tr>
<tr><td><strong>Messaging</strong></td><td>Slack, Discord, Teams, Telegram, Matrix, WebChat</td></tr>
<tr><td><strong>Cloud</strong></td><td>GitHub, Cloudflare, Vercel, Supabase</td></tr>
<tr><td><strong>Smart Home</strong></td><td>Philips Hue</td></tr>
<tr><td><strong>Protocol</strong></td><td>MCP client + server, OAuth2 (PKCE), Webhooks</td></tr>
</table>

---

## Blockchain / Web3

| Chain | Features |
|---|---|
| **EVM** (Ethereum, Polygon, Arbitrum, BSC, Avalanche, Optimism, Base) | Wallet management and RPC configuration; ERC-20 deployment backend is scaffolded and currently disabled in this build |
| **Solana** | Wallet management; SPL token deployment backend is scaffolded and currently disabled in this build |
| **Security** | Encrypted private key storage (AES-256-GCM), no keys ever sent to AI providers |

---

## Architecture — 13-Crate Workspace

```
hive/crates/
├── hive_app           Binary entry point — window, tray, build.rs (winres)
├── hive_core          Config, SecurityGateway, persistence (SQLite), Kanban, secure storage
├── hive_ui            GPUI views — 18 panels, components, theme, workspace orchestration
├── hive_ai            11 AI providers, model router, complexity classifier, context engine, RAG
├── hive_agents        Queen, HiveMind, Coordinator, collective memory, MCP, skills, personas
├── hive_shield        PII detection, secrets scanning, vulnerability assessment, access control
├── hive_learn         Outcome tracking, routing learner, preference model, prompt evolution
├── hive_assistant     Email, calendar, reminders, approval workflows, daily briefings
├── hive_fs            File operations, git integration, file watchers, search
├── hive_terminal      Command execution, Docker sandbox, browser automation, local AI detection
├── hive_docs          Document generation — CSV, DOCX, XLSX, HTML, Markdown, PDF, PPTX
├── hive_blockchain    EVM + Solana wallets, RPC config, deployment scaffolding
└── hive_integrations  Google, Microsoft, GitHub, messaging, OAuth2, smart home, cloud, webhooks
```

---

## UI — 18 Panels

| Panel | Description |
|---|---|
| Chat | Main AI conversation interface |
| History | Conversation history browser |
| Files | Project file browser |
| Specs | Specification management |
| Agents | Multi-agent swarm orchestration |
| Kanban | Task board with drag-and-drop |
| Monitor | System and agent monitoring |
| Logs | Application logs viewer |
| Costs | AI cost tracking and budget |
| Review | Code review interface |
| Skills | Skill marketplace management |
| Routing | Model routing configuration |
| Learning | Self-improvement dashboard |
| Shield | Security scanning status |
| Assistant | Personal assistant dashboard |
| Token Launch | Token launch workflow and deployment validation |
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
cargo test
```

---

## Project Stats

| Metric | Value |
|---|---|
| Crates | 13 |
| Rust source files | 210+ |
| Lines of Rust | 100,000+ |
| Tests | 2,300+ |
| Compiler warnings | 0 |
| Memory footprint | < 50 MB |
| Startup time | < 1 second |
| UI rendering | 120fps (GPU-accelerated via GPUI) |

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
