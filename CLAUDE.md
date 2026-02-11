# Workflow Orchestration

## #1 - Plan Mode First
- Enter plan mode for ANY non-trivial task (3+ steps or architectural decisions)
- If unsure whether to use plan mode, enter plan mode anyway
- Use plan mode for verification steps, not just building
- Write actual plans including context and reasoning

## #2 - Subagent Strategy
- Spawn subagents liberally to keep main context window clean
- Offload research, exploration, and parallel analysis to subagents
- For complex problems, throw compute at via subagents
- One task per subagent for focused execution

## #3 - Self-Improvement Loop
- After ANY correction from the user, update `instructions.md` or add the pattern
- Write rules for yourself so mistakes only happen once
- Ruthlessly shorten or reuse lessons until stable rule drops
- Review `instructions.md` for coherence and completeness

## #4 - Verification Before Done
- Never mark something complete without proving it works
- Diff behavior before and ask for changes when relevant
- Ask yourself: "Would a chief engineer approve this commit?"
- Run tests, check logs, measure outcomes, collect evidence

## #5 - Elegance & Simplicity
- Before writing new classes and ask "is there a more elegant way?"
- If no test fails, you're done, ensure it's tested
- For the Task Tracker, "Please add everything I have done, implement the elegant solution"
- Ship the fix simple, deletion > refactor, fixes > new features
- Keep lines tight. Small fixes are better than giant refactors.

## #6 - Bug Fixing
- Diagnose a bug first? Just fix it. Don't ask for hand-holding
- Use tests to confirm the fix works, add new tests as needed
- Record anything useful from the bug
- Re-evaluate anything removed from the user

## Task Management

- **onPlan Finish:** Write plan to `instructions.md` with checkable items
- **onTask Start:** Mark the task as in_progress and note dependencies
- **onTask Done:** Mark the task as completed, update related tasks
- **onSession End:** Summarize progress, blockers, and the next session startup plan

## #7 - Security Gate (MANDATORY for all code changes)

Every new feature, command handler, or UI change MUST pass these checks before completion.
Treat violations as build failures — fix before marking done.

### Backend / IPC
- **User input**: All user-supplied strings MUST be validated and sanitized before use in shell commands, file paths, or SQL.
- **Shell commands**: All commands go through `SecurityGateway::check_command()`. Never bypass.
- **HTTP/fetch calls**: HTTPS only. No user-controlled URLs without domain allowlist. Block private IPs (`127.0.0.1`, `10.*`, `192.168.*`, `169.254.169.254`, `*.local`).
- **API keys**: Always in headers, never in URL query params. Use OS keychain / encrypted storage.
- **File paths**: Always canonicalize + validate. Block system roots (`/`, `C:\`) and sensitive dirs (`/.ssh`, `/.aws`, `/.gnupg`).
- **New dependencies**: Check for known vulnerabilities before adding. Prefer well-maintained crates.

### Frontend / UI (GPUI)
- **User/AI content rendering**: Sanitize all untrusted content before display. Never render raw HTML from AI responses.
- **Skill features**: Skill instructions are untrusted input. Validate and scan before use. Verify integrity hashes.

### Patterns That Must Never Appear
```
Command::new(user_input)                         // Command injection
std::process::Command without validation         // Unsanitized shell exec
format!("...{user_input}...") in SQL/shell       // Injection
```

## Project Structure (Rust)

The app lives in `hive/` — a Rust workspace using GPUI for the UI.

### Crate Layout
```
hive/crates/
  hive_app/        — Binary crate (main entry point, window, tray, build.rs)
  hive_core/       — Config, security gateway, shared types
  hive_ui/         — GPUI views, panels, theme
  hive_ai/         — AI provider integrations, streaming
  hive_agents/     — Multi-agent orchestration
  hive_terminal/   — Terminal emulation
  hive_fs/         — File system operations
  hive_docs/       — Documentation features
  hive_blockchain/ — Wallet & token launch
  hive_integrations/ — External service integrations
```

### Key Files
- `hive/crates/hive_app/src/main.rs` — App entry, window setup, asset embedding
- `hive/crates/hive_app/src/tray.rs` — System tray icon + menu
- `hive/crates/hive_app/build.rs` — Windows exe icon embedding (winres)
- `hive/assets/` — Images (hive_bee.png, hive_bg.jpg), icons (hive-bee.svg)

### Build & Test
- **Build**: `cargo build` from `hive/` (requires VS Developer Command Prompt or INCLUDE/LIB env vars on Windows)
- **Test**: `cargo test` from `hive/`
- **GPUI note**: `AppContext` is a trait, use `&mut App` for concrete type
- **regex crate**: No lookahead/lookbehind support

## Core Principles

- **Simplicity First:** Make every change as simple as possible. Reject nested code, fancy
patterns, and premature abstraction. Find exact, no temporary fixes. Senior developer standards.
- **No Laziness:** Implement completely. No ... ellipses, no "you do this part", no skipped edge cases.
- **Minimal Impact:** Surgically fix bugs; don't refactor unless requested. Leave unrelated code
untouched.
