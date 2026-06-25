# yogurt

## What This Is

Yogurt is an open-source, local-first meeting copilot launched from the command line. It captures the user's microphone and Mac system audio without joining the call as a bot, transcribes live, and produces "augmented notes" — sparse markdown bullets fused in-place with what was actually said. It's built for users who love Granola's UX but can't or won't ship meeting audio to third-party SaaS (compliance-bound teams, security-conscious ICs, OSS contributors, self-hosted-LLM users). Single Rust binary, browser UI at `localhost:7878`, MIT licensed.

## Core Value

**The black-user / grey-AI in-place augmented-notes UX, running fully local on macOS without a meeting bot.** Every other feature exists to make this hero experience possible — if augmented notes don't feel indistinguishable from Granola's, the product fails.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Record meeting (mic + system audio via ScreenCaptureKit)
- [ ] Live transcript panel (right-edge dock, < 2s lag cloud / < 3s local)
- [ ] Augmented markdown notes editor (black-user / grey-AI merge, transcript deep-links)
- [ ] In-meeting AI chat ("Ask this meeting…" ⌘K floating pill)
- [ ] Bundled prompts (`enhance.md` + `chat-system.md`, no template picker in v1)
- [ ] Settings UI (Model · Transcription · Audio · General, Keychain key storage)
- [ ] Local storage (SQLite + markdown export to `~/.yogurt/notes/`)
- [ ] macOS only (Apple Silicon primary, Intel best-effort, macOS 13+)
- [ ] Meeting library home view (sidebar + date-grouped meeting cards + folders)
- [ ] Onboarding first-run flow (3-step welcome at `/welcome`)
- [ ] Empty & error states (empty library, permission-denied recovery, enhancing, model download)

### Out of Scope

- **Calendar integration** — Google/Outlook OAuth deferred to v2; user clicks "New meeting" manually.
- **Cross-meeting chat / semantic search** — embeddings/vector store deferred until meeting history is worth querying.
- **Slack / Notion / CRM integrations** — markdown export covers 80% of value; explicit v2+.
- **Mobile or web-hosted version** — macOS-only, browser UI is local-only.
- **Multi-user, sync, or authentication** — single-user, single-machine, no login screen.
- **Per-speaker diarization beyond mic/system split** — Granola itself only does Me/Them; same here.
- **MCP server or external API** — useful only once meeting history is worth querying.
- **Windows or Linux support** — architecture isolates platform code, but no porting in v1.
- **Custom user-defined templates UI** — templates are markdown files in repo; first-class authoring UI deferred.
- **Template picker + versions rail** — designed but cut from v1 (2026-06-24 brainstorm decision); Re-enhance button stays as single-prompt regenerate.
- **Joining meetings as a participant bot** — *anti-goal*; this is the single thing that makes the product feel magical vs Granola.
- **Sending audio to a Yogurt cloud** — *anti-goal*; there is no Yogurt cloud, period.
- **Subscription billing or default telemetry** — *anti-goal*; MIT open source, opt-in only.

## Context

- **Domain:** macOS meeting-notes copilot. Closest commercial reference: Granola.ai (Series C, $1.5B valuation). Closest OSS analogs: Meetily (~13k stars) and Hyprnote — both Tauri/Rust, neither has augmented-notes UX. Yogurt's wedge is augmented notes + local-first.
- **System audio capture:** ScreenCaptureKit (macOS 13+) via the `screencapturekit` Rust crate. No kext, no BlackHole, no meeting bot.
- **Augmented-notes UX:** computed structurally over markdown AST, not character diff. TipTap mark `aiGrey` applied to LLM-added runs; edits strip the mark (promote-to-black). AI bullets carry a `transcriptTs` data attribute for the `↳ HH:MM` deep-link affordance.
- **Transcription strategy:** pluggable trait. Cloud default = Deepgram (best streaming partials). Local fallback = `whisper.cpp` on Metal (`small.en` baseline, `large-v3` available; M3 Max runs `large-v3` at 5–7x real-time).
- **LLM strategy:** OpenAI-compatible only. One adapter covers Minimax, OpenAI, Ollama, LM Studio, OpenRouter, Groq, vLLM, llama.cpp server, Together, Fireworks. Anthropic + Gemini reachable via OpenRouter.
- **Distribution:** Homebrew + Cargo + GitHub Release binaries. Single static binary with web assets embedded via `rust-embed`; whisper.cpp linked statically; ScreenCaptureKit linked against system framework.
- **Storage:** `~/.yogurt/db.sqlite` is the structured/queryable mirror; `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` is the canonical exportable file. Markdown is source of truth for grep; SQLite is the queryable mirror.
- **Brand & visual design:** complete design system documented in PRD §16 — paper/ink/blueberry/strawberry/matcha palette, Instrument Serif + Hanken Grotesk + JetBrains Mono, 4-base spacing, defined motion tokens (260/340/600/1.4s). Reference design board: `yogurt-app-design/project/Yogurt Design Board.dc.html`. v1 ships **Blueberry theme only**.
- **Prior planning work:** Phase 0 plan review (earlier today) identified 5 blockers before implementation — those should be re-surfaced when planning Phase 1.

## Constraints

- **Tech stack:** Rust (axum + tokio) backend, React + Vite + TipTap + Tailwind frontend, embedded web assets — load-bearing for "single static binary" distribution.
- **Platform:** macOS 13+ only (ScreenCaptureKit requirement). Universal binary (arm64 + x86_64); Intel best-effort.
- **Privacy posture:** audio never leaves machine unless user opts into cloud STT, and even then only audio (no notes). Audio deleted after transcription. API keys → macOS Keychain via `keyring` crate, never plaintext.
- **Single process:** no subprocesses, no IPC, no sidecar binaries. Audio, STT, LLM, web serving, SQLite all in one Rust process.
- **No telemetry:** zero phone-home in v1, not even opt-in Sentry. If we add crash reporting later, it gets its own design pass.
- **License:** MIT (matches Meetily and Hyprnote).
- **Repo:** `github.com/jarvisrchen/yogurt` (already created).
- **Timeline:** rough estimate ~18 working days at one focused engineer (project-wide order of magnitude, not a commitment).

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Core UX = augmented notes + visible live transcript + in-meeting AI chat | Augmented notes is the hero; transcript and chat are the two real wins from "live" | — Pending |
| Single Rust binary + browser UI + in-process audio capture | Browser is hackable UI; Rust gives free FFI for ScreenCaptureKit; single static binary is the killer distribution feature | — Pending |
| Pluggable STT — cloud default (Deepgram), local fallback (whisper.cpp) | Best quality + best privacy escape hatch; one trait covers both | — Pending |
| LLM = OpenAI-compatible only | One adapter covers ~10 providers including local Ollama/LM Studio | — Pending |
| Delete audio after transcription | Granola model; smallest privacy and disk footprint | — Pending |
| Distribution = Homebrew + Cargo + GitHub Release | Standard Rust CLI distribution; one source of truth, three channels | — Pending |
| Template picker cut from v1 | Brainstorm 2026-06-24: user reported never using Granola's picker | — Pending |
| Blueberry theme only in v1 (Strawberry + Matcha-dark deferred) | Single theme keeps the design-system phase tighter | — Pending |
| MIT license | Matches Meetily and Hyprnote (closest OSS analogs) | — Pending |
| No telemetry, no phone-home — even opt-in | Trust posture for the privacy-focused user base | — Pending |
| Wordmark: "yogurt" lowercase + purple "spoon & swirl" mark | Brainstorm decision 2026-06-24 | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-06-25 after initialization*
