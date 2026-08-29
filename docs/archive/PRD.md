# Yogurt — Product Requirements Document

**Status:** Draft v1 · **Date:** 2026-06-24 · **Author:** Brainstorm output

> Yogurt is an open-source, local-first meeting copilot you launch from the command
> line. It captures both your microphone and your Mac's system audio without joining
> the call as a bot, transcribes live, and produces "augmented notes" — your sparse
> markdown notes fused in-place with what was actually said. Bring your own LLM API
> key. Your audio never leaves your machine unless you opt into a cloud transcription
> provider. Inspired by [Granola.ai](https://www.granola.ai/) — the goal is a faithful
> recreation of the signature UX, with the privacy posture inverted.

---

## 1. Vision

Granola is the best meeting-notes UX shipped in years. Two ideas make it work:

1. **System-audio capture instead of a meeting bot.** No "Granola AI has joined the call." Just turn it on and it hears everything.
2. **Augmented notes instead of summaries.** You type sparse markdown bullets during the meeting. After the meeting, the LLM expands *your* bullets in-place — your text stays black, AI-added text renders grey, and editing a grey bullet promotes it to black. Each AI bullet is hyperlinked to the transcript moment it came from.

Granola is, however, a commercial SaaS that streams your audio to Deepgram and your transcripts to OpenAI/Anthropic. For people who can't or won't ship meeting audio to third parties — legal, finance, security, anyone under NDA, anyone who simply prefers local-first software — there is no comparable tool today. The closest open-source projects (Meetily, Hyprnote) lack the augmented-notes UX entirely.

**Yogurt is that tool.** A single Rust binary, served at `http://localhost:7878`, that ships with Granola-quality UX and lets you swap any OpenAI-compatible LLM in via a settings page.

## 2. Goals & Non-Goals

### Goals (v1)

- **Faithfully reproduce Granola's "augmented notes" UX**, including the black-user / grey-AI in-place merge and the transcript-link affordance on every AI bullet.
- **Capture both mic and system audio on macOS** without a meeting bot, using Apple's native ScreenCaptureKit.
- **Pluggable transcription**: ship with a high-quality cloud default (Deepgram or AssemblyAI) and a fully-local fallback (`whisper.cpp` on Metal).
- **Pluggable LLM via any OpenAI-compatible endpoint.** Minimax, OpenAI, Ollama, LM Studio, OpenRouter, Groq, vLLM, llama.cpp server — paste a base URL and an API key, you're done.
- **Privacy-first defaults.** Audio is deleted as soon as transcription completes. Notes and transcripts live in `~/.yogurt/` on the user's machine. Nothing is uploaded unless the user picks a cloud STT provider, and even then only audio (no notes).
- **One-command install for non-developers**: `brew install yogurt && yogurt start`.
- **One-command source build for developers**: `git clone && cargo run` (or `cargo run` + `pnpm dev` in `/web` for hot-reload).
- **Run entirely from the CLI.** No app bundle, no menu bar — just `yogurt start` opens the browser. (Wrapping in Tauri for menu-bar integration is an explicit v2 path.)

### Non-Goals (v1)

These are deliberately out of scope. Each is a defensible v2+ feature; including any in v1 would push ship date.

- **Calendar integration.** No Google/Outlook OAuth. User clicks "New meeting" manually.
- **Cross-meeting chat or search.** No embeddings, no vector store, no "ask anything across your meeting history."
- **Integrations to Slack / Notion / CRM.** Markdown export covers 80% of the value.
- **Mobile or web-hosted version.** macOS only; the browser UI is local-only.
- **Multi-user, sync, or authentication.** Single-user, single-machine. No login screen.
- **Per-speaker diarization beyond mic/system split.** Granola itself only does "Me"/"Them" on desktop — same here.
- **MCP server or external API.** Cool, but only useful once meeting history is worth querying.
- **Windows or Linux support.** Architecture leaves the door open (separate `audio-helper-windows`, `audio-helper-linux` binaries), but no porting in v1.
- **Custom user-defined templates UI.** Templates are just markdown prompt files in the repo; power users can edit them. A first-class authoring UI defers.

### Anti-Goals (things we *commit* to not doing)

- **Joining meetings as a participant bot.** This is the single thing that makes Granola feel magical. We don't compromise on it.
- **Sending audio to a Yogurt cloud.** There is no Yogurt cloud. Period.
- **Subscription billing or telemetry by default.** Open source, MIT licensed, opt-in only for any usage data.

## 3. Users & use cases

| User | Use case | Why Yogurt (vs Granola SaaS) |
|---|---|---|
| **Solo IC with back-to-back meetings** | Wants notes without cognitive load. | Same UX, but doesn't trust SaaS with sensitive meeting content. |
| **Compliance-bound team** (legal, security, finance, regulated industries) | Cannot transmit meeting audio to third parties. | Local STT + local notes is contractually viable; Granola is not. |
| **Engineering team using a self-hosted LLM** (Ollama, vLLM) | Wants meeting notes that flow through their internal LLM. | OpenAI-compatible provider config covers this with zero extra plumbing. |
| **OSS contributor** | Wants to hack on the editor / add a new STT backend. | Single Rust binary + small React app, no Electron build chain. |

## 4. Locked design decisions

These are the eight decisions reached during brainstorming. They are load-bearing for everything below.

| # | Decision | Rationale |
|---|---|---|
| Q1 | **Core UX = Granola-style augmented notes + visible live transcript + in-meeting AI chat** | The augmented-notes UX is the entire reason Granola is loved. Visible transcript and in-meeting "what's happening?" chat are the two real wins from the live experience. |
| Q2 | **Architecture = single Rust binary + browser UI + in-process audio capture** | Browser is a familiar, hackable UI surface. macOS system audio (ScreenCaptureKit) requires native code, which we get for free in Rust via the `screencapturekit` crate. |
| Q3 | **Transcription = pluggable, cloud default, local fallback** | Cloud STT (Deepgram) gives best quality and easy streaming partials out of the box. `whisper.cpp` on Metal gives full local-only mode for privacy-sensitive users (M3 Max runs `large-v3` at ~5–7x real-time). |
| Q4 | **v1 Scope = 11 IN features, 9 deferred** | See §5 / §6. (Originally 9, then 8 after template-picker cut, then 11 after Claude Design handoff added library / onboarding / empty-error states.) |
| Q5 | **Stack = Rust (axum + tokio) for backend, React + Vite + TipTap for frontend, embedded web assets** | Single static binary distribution is the killer feature for a local-first OSS tool. Rust handles audio FFI, WebSocket fan-out, and whisper.cpp embedding without paying the GIL or Node memory tax. |
| Q6 | **LLM provider = OpenAI-compatible only** | One adapter covers Minimax, OpenAI, Ollama, LM Studio, OpenRouter, Groq, vLLM, llama.cpp server, Together, Fireworks, and anything else with an OpenAI-compatible endpoint. Anthropic and Google models reachable via OpenRouter. |
| Q7 | **Audio retention = delete after transcription** | Granola model. Smallest privacy footprint, smallest disk usage. If a user wants to keep audio, they can flip a per-meeting toggle in v1.1 (trivial schema addition). |
| Q8 | **Distribution = Homebrew + Cargo + GitHub Release binaries** | Standard Rust CLI distribution. One source of truth (the GitHub release), three install channels. Source build via `git clone` is fully supported and documented. |

## 5. v1 feature list

The eleven features in v1 scope, with concrete acceptance criteria. Updated 2026-06-24 (late) from the Claude Design board handoff: §§5.9–5.11 (library, onboarding, empty/error states) added to make the feature set match the designed screens. Template picker remains cut — see §5.5 for the open question.

### 5.1 Record meeting (mic + system audio)
- User starts a meeting by clicking "New meeting" in the browser UI.
- First-run: macOS Screen Recording permission dialog appears (triggered by the Rust binary via ScreenCaptureKit init). Yogurt waits for grant.
- Captures two mono 16 kHz / 16-bit PCM streams: `mic` (default input device) and `system` (loopback via ScreenCaptureKit).
- Streams are pushed to the in-process STT engine via a Tokio broadcast channel.
- **Done when:** user can record a 30-minute meeting on macOS 13+, both channels are captured cleanly, recording stops cleanly on "End meeting."

### 5.2 Live transcript panel
- **Collapsed by default** as a right-edge tab labeled "Live transcript" with a 3-bar animated wave icon. Tab is visible during the entire meeting.
- Click expands the panel: docks beside the notes column (330px wide), slides in from the right at 340ms with `cubic-bezier(.2,.7,.2,1)`. **Notes stay fully editable while the panel is open** — nothing is dimmed.
- Each transcript line: channel label ("Me" in ink black for mic; "Them" in grey for system audio), JetBrains-Mono timestamp from meeting start (e.g. `00:11:02`).
- Auto-scrolls to bottom; pauses auto-scroll if the user has scrolled up.
- Cursor blink on the most-recent partial transcript indicates "still listening."
- **Done when:** transcript appears with < 2s lag using Deepgram, < 3s lag using local `whisper.cpp small.en`. Panel open/close animates cleanly without re-flowing the notes column.

### 5.3 Augmented markdown notes editor — the hero feature
- **During the meeting:** a single markdown editor (TipTap-based) centered in the meeting view, max-width ~660px. Pure markdown editing. A small lilac pill underneath reads "✨ AI enhances these when you hit End." Cursor blinks on the active line.
- **On "End meeting":** the editor transitions into an *enhancing* state — progress bar across the top reads "Weaving your notes into the transcript…" with streaming character count. AI-generated lines appear as shimmer skeletons that resolve into grey markdown bullets, staggered (140ms / 340ms / 560ms / 760ms — see motion tokens in §16).
- **Post-meeting view = one combined document.** Not a separate summary tab. Your bullets sit black under their headings; AI-added bullets render in muted grey (`#A89F90`) under the same headings. The whole thing reads as one coherent document.
- **Display contract:** user-authored content stays **ink black (`#211D18`)**; AI-added content renders **grey (`#A89F90`)**; the diff is computed structurally (by markdown AST), not character-by-character.
- **Edit contract:** editing a grey range promotes it to black (it is now "the user's"). Black ranges are never overwritten by re-enhancement.
- **Transcript deep-links.** Each AI-added bullet ends with a small `↳ HH:MM` link (dotted-underline lilac, e.g. `↳ 11:02`). Clicking the link opens the transcript panel (if closed) and scrolls it to that timestamp. Hovering shows a tooltip with the transcript excerpt.
- **Re-enhance button** in the top-right of the post-meeting view ("Re-enhance"). v1: re-runs the same bundled `enhance.md` prompt against the current notes + transcript. No template picker, no versions rail — those are v2 (see §5.5 and §6).
- **Live legend** in the top-right shows the swatch contract: small black square = "your notes", small grey square = "AI".
- **Done when:** user can type 5 sparse bullets during a 30-min meeting, hit "End meeting," and within 30s see a clean enriched-notes document where their bullets are black, AI bullets are grey, and clicking a `↳ HH:MM` link opens the transcript at that moment.

### 5.4 In-meeting AI chat — "Ask this meeting…"
- **Floating pill, anchored bottom-center** of the meeting view (480px wide, 24px from bottom). Reads "Ask this meeting…" in placeholder text, with a `⌘K` keyboard hint badge and a small purple send arrow. Always present during a meeting; persists into the post-meeting view too.
- **Click or ⌘K** expands the pill into a chat window (260ms ease-out, `popUp` animation — see motion tokens). Window is the same width as the pill, anchored from the same point. Notes stay live behind it (no dim).
- Chat header shows the yogurt swirl logo + "Ask the meeting" + a collapse caret. User messages right-aligned in blueberry; AI messages left-aligned in cream with grey border.
- Each user message is sent to the LLM with the transcript-so-far as context. System prompt comes from `chat-system.md`: "You are watching this meeting live. Answer the user's question using only the transcript content available so far."
- Streaming response renders inline.
- **Common queries the prompt is tuned for:** "What's the current topic?", "What did <speaker> say about X?", "Summarize the last 5 minutes.", "What did we decide on X?"
- **Done when:** first response chunk streams < 2s after sending; references transcript content accurately; pill/window animations feel snappy (no jank).

### 5.5 Bundled prompts (internal, two files)
- `crates/yogurt-prompts/` ships exactly two prompt files in v1:
  - `enhance.md` — the augmented-notes prompt; takes `{{NOTES}}` + `{{TRANSCRIPT}}` and returns the merged black/grey markdown document (§5.3 depends on this).
  - `chat-system.md` — the system prompt for in-meeting chat (§5.4).
- These are not "templates" in the user-facing sense — there is no template picker, no per-meeting prompt switching. Power users can edit either file in the repo; reloading the binary picks up changes.
- **Why no picker:** brainstorm decision (2026-06-24) — user reported they never used Granola's template picker, and informal evidence suggests most users don't either.
- **The "Re-enhance" button (§5.3) still exists in v1**, but it simply re-runs the same `enhance.md` against the current notes + transcript. The template-picker popover and the versions rail shown on the design board are **deferred to v2 / v1.1** — see §6.

### 5.6 Settings UI
- Settings page at `/settings`. Two-column layout: 212px left sidebar listing sections (**Model** · Transcription · Audio · General), main content right.
- Sidebar footer shows a green "Local-only · on" pill when no cloud providers are active, and a JetBrains-Mono caption: `keys → macOS Keychain` / `data → ~/.yogurt/`.
- Config persisted in `~/.yogurt/config.toml`. API keys → macOS Keychain via `keyring` crate, never in plaintext.

**Model section.** "Model · LLM provider" header with `OpenAI-compatible` mono-caption. Subhead: "Paste a base URL and key. Anthropic & Gemini reachable via OpenRouter."
- Active provider rendered as a 1.5px blueberry-bordered card with subtle shadow: name + "Active" badge + Edit link. Two-column inline view: `BASE URL` and `MODEL` displayed in mono, then `API KEY · in Keychain` showing masked key with last-4 + green "✓ stored".
- Inactive providers stack below as plain rows with "Set active" link.
- Bottom row: `CLONE A PRESET →` with dashed-border preset chips (Ollama, LM Studio, OpenRouter) + `+ Add` link to add a custom provider.

**Transcription section.** Side-by-side card pair:
- Left: Cloud (selected, blueberry border). Radio button + provider chips (Deepgram active in lilac, AssemblyAI / Groq inactive). Masked key field with ✓.
- Right: Local · whisper.cpp (matcha-soft chrome). Pill row of model sizes (`tiny.en` · `small.en ✓` · `medium.en` · `large-v3 ↓`). Caption: "Models download on first use · stored in `~/.yogurt/models`".

**Audio + General** rendered side-by-side at the bottom.
- Audio: input device dropdown ("MacBook Pro Mic ⌄"). Caption: "System audio is captured via ScreenCaptureKit — no extra setup."
- General: Port row (`7878`), "Open browser on start" toggle (default on).

**Dev convenience — env-var bootstrap.** On startup, yogurt loads `.env.local` from the repo root (via the `dotenvy` crate) and inspects the following env vars. If found AND the provider isn't already configured in the DB, the provider entry is auto-seeded and the key is stored in the Keychain. This lets contributors avoid manually re-entering keys after every `~/.yogurt/` reset.

| Env var | Becomes |
|---|---|
| `YOGURT_MINIMAX_API_KEY` | Minimax provider (active if no other provider is) |
| `YOGURT_OPENAI_API_KEY` | OpenAI provider |
| `YOGURT_OPENROUTER_API_KEY` | OpenRouter provider |
| `YOGURT_DEEPGRAM_API_KEY` | Cloud STT — Deepgram provider |
| `YOGURT_ASSEMBLYAI_API_KEY` | Cloud STT — AssemblyAI provider |
| `YOGURT_GROQ_API_KEY` | Cloud STT — Groq Whisper provider |

`.env.local` is gitignored (`.env*.local` pattern). Env vars are read once at startup and copied into the Keychain — the file itself is never re-read at runtime. For production / `brew install yogurt`, the Settings UI is the only way to enter keys; the env-var bootstrap is a dev-only convenience.

### 5.7 Local storage (SQLite + markdown export)
- All persistent data in `~/.yogurt/db.sqlite` (rusqlite).
- Each meeting also written to `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` as the canonical exportable file (front-matter for metadata, then enriched notes, then transcript appendix).
- Markdown files are the source of truth for "user wants to grep their meetings" — SQLite is the structured/queryable mirror.
- Schema sketch (§8).

### 5.8 macOS only (Apple Silicon primary, Intel best-effort)
- v1 ships universal binary (arm64 + x86_64).
- Apple Silicon: tested on M1 / M1 Pro / M3 Max baseline.
- Intel: builds and runs; `whisper.cpp` performance limited to `small.en` real-time. Documented as best-effort.
- macOS 13+ required (ScreenCaptureKit).
- Windows/Linux: out of scope. Architecture isolates the platform code in `crates/yogurt-audio`, so future ports are additive.

### 5.9 Meeting library (home view) · added via design 2026-06-24
- The default page at `localhost:7878` is the library — sidebar + meeting list.
- **Left sidebar (212px):**
  - Yogurt swirl logo + wordmark at the top.
  - "+ New meeting" primary button (blueberry, white text).
  - Nav rows: "All meetings" (active = lilac background, blueberry text), "Starred".
  - **Folders section** with `+` affordance — each folder = color dot + name + count (e.g. `Work · 8`, `Hiring · 3`, `1:1s · 5`). Folder colors come from the brand palette (blueberry / strawberry / matcha).
  - Bottom: green "Local-only · on" pill (when no cloud providers active), `⚙ Settings` row.
- **Main pane:**
  - Greeting header in Instrument Serif: "Good afternoon, Dana" + meta caption "14 meetings · all on this Mac".
  - Search affordance top-right: pill with magnifier icon, "Search notes & transcripts".
  - Meeting list grouped by date with mono-caption labels (`TODAY`, `YESTERDAY`, etc.).
  - Each meeting card: 42px colored-tinted square avatar (initials in Instrument Serif), title (Hanken 700), meta line ("2:00 PM · 38 min · enhanced"), small template/local badges on the right.
- **Done when:** user can land on library, see 0-N meetings grouped by day, click into any meeting, create a new one, and the "Local-only · on" pill reflects whether any cloud providers are configured.

### 5.10 Onboarding · first-run flow · added via design 2026-06-24
- Route: `localhost:7878/welcome`. Two-column layout on cream paper.
- **Left column:** Yogurt logo, Instrument-Serif welcome ("Welcome to yogurt."), one-liner ("Two streams, one set of notes, zero bots in the call. Everything below happens on this Mac."), and a small terminal mockup showing the boot sequence (`$ yogurt start` → `✓ server live on :7878` → `✓ opening your browser…` → `→ waiting for screen-recording grant`).
- **Right column:** "ONE-TIME SETUP" caption + a vertical step list (3 cards):
  1. **Screen Recording** — green "✓" badge once granted, caption explaining "this is how yogurt hears the other side of the call — no meeting bot required."
  2. **Connect your model** — current-step card (blueberry border), description "Bring your own key — OpenAI-compatible. Nothing is built in.", row of provider chips (Minimax active, Ollama / OpenAI / OpenRouter as presets).
  3. **Pick transcription** — pending card, "Cloud Deepgram for speed, or fully-local whisper.cpp."
- Primary button: "Take me to my meetings →" (blueberry, full-width).
- Footer note: "Restart once after granting — a macOS quirk, not us."
- **Done when:** a fresh user can complete the three steps in order and reach the empty library (5.11).

### 5.11 Empty & error states · added via design 2026-06-24
- **Empty library** (no meetings yet): centered, soft-floating Yogurt logo (3.5s float animation), Instrument-Serif "No meetings yet" headline, supporting line ("Start one and Yogurt listens to both sides of the call — no bot joins. Your notes and audio stay on this Mac."), primary "Start your first meeting" CTA with `⌘N` keyboard hint, mono caption "notes saved to `~/.yogurt/notes/*.md`".
- **Permission not granted** (Screen Recording off): full-screen warning card. Strawberry-tinted alert icon, Instrument-Serif headline "Yogurt can't hear the call yet", explanation paragraph, **numbered 3-step recovery list** (1. open System Settings → Privacy → Screen Recording, 2. toggle Yogurt on, 3. restart Yogurt once with note "a macOS requirement, not us"), CTA pair: "Open System Settings" (primary) + "Restart Yogurt" (outline).
- **Enhancing state** (just after End meeting): lilac progress banner across top of the meeting view with active dot pulse, "Weaving your notes into the transcript…", animated progress bar, character-streaming count. Existing notes stay in place; new bullets appear as shimmer skeleton rectangles that resolve into grey text, staggered.
- **First-time `whisper.cpp` model download** (only if user picks Local STT): modal-like card. Matcha down-arrow icon, "Downloading small.en" / mono caption "whisper.cpp · 487 MB", matcha progress bar with bytes/speed/ETA, body copy "Most users stay on cloud STT and never see this", button pair: Cancel / Run in background.

## 6. v2+ deferred features

For clarity; not built in v1. Order suggests priority.

1. Calendar integration (Google + Outlook OAuth, auto-detect upcoming meetings).
2. **Re-enhance template picker + versions rail.** The popover designed in the Claude Design board (Standup / Generic / 1:1 / Interview debrief templates + v1/v2/v3 versions sidebar). Cut from v1 (confirmed 2026-06-24 after design handoff). v1 keeps the Re-enhance button as a single-prompt regenerate. Add back when there's signal users actually want format switching.
3. Cross-meeting chat / semantic search (embeddings + sqlite-vss or LanceDB).
4. Slack / Notion / Linear / Granola-style integration exports.
5. Custom template authoring UI.
6. Per-speaker diarization (pyannote via a sidecar Python service, opt-in).
7. Menu-bar / global-hotkey UI (Tauri wrap of the existing backend).
8. Windows + Linux support.
9. MCP server.
10. Optional sync / multi-device (encrypted, user-hosted only — no Yogurt cloud).

## 7. Architecture

```
                ┌─────────────────────────────────────────────────────┐
                │                       USER MAC                      │
                │                                                     │
  ┌──────────┐  │   ┌─────────────────────────────────────────────┐   │
  │ Browser  │◄─┼──►│            yogurt (single binary)           │   │
  │ (Chrome, │  │   │                                             │   │
  │  Safari) │  │   │  ┌──────────┐   ┌──────────┐  ┌──────────┐  │   │
  └──────────┘  │   │  │   axum   │   │  audio   │  │   STT    │  │   │
                │   │  │  HTTP +  │◄─►│  capture │─►│  engine  │  │   │
   HTTP+WS      │   │  │    WS    │   │  (SCK)   │  │ (trait)  │  │   │
   localhost    │   │  └────┬─────┘   └──────────┘  └────┬─────┘  │   │
   :7878        │   │       │                            │        │   │
                │   │       ▼                            ▼        │   │
                │   │  ┌──────────┐                ┌──────────┐   │   │
                │   │  │   LLM    │                │  SQLite  │   │   │
                │   │  │  client  │                │  + .md   │   │   │
                │   │  │(OAI-cmpt)│                │  files   │   │   │
                │   │  └────┬─────┘                └──────────┘   │   │
                │   │       │                                     │   │
                │   └───────┼─────────────────────────────────────┘   │
                └───────────┼─────────────────────────────────────────┘
                            │
                            ▼  HTTPS (optional, only if cloud STT/LLM selected)
                   ┌────────────────────────┐
                   │   STT:  Deepgram /     │
                   │         AssemblyAI /   │
                   │         Groq           │
                   │   LLM:  Minimax /      │
                   │         OpenAI /       │
                   │         OpenRouter /   │
                   │         user-chosen    │
                   └────────────────────────┘
```

Key properties:
- **One process.** No subprocesses, no IPC, no sidecar binaries. Audio capture, transcription, LLM calls, web serving, and SQLite all in one Rust process.
- **In-process audio.** ScreenCaptureKit is called directly from Rust via the `screencapturekit` crate. The OS Screen Recording permission dialog is triggered by the binary on first run.
- **Cloud is opt-in for STT/LLM.** Defaults are cloud-STT and BYO-LLM, but a fully-local configuration (whisper.cpp + Ollama) is supported and documented.
- **Browser is the UI.** Static `web/dist` is embedded into the Rust binary via `rust-embed`; served by axum on the configured port. No CORS issues, no separate frontend deploy.

## 8. Component breakdown (Cargo workspace)

```
yogurt/
├── Cargo.toml                     # workspace root
├── crates/
│   ├── yogurt-cli/                # binary entrypoint: `yogurt start`, `yogurt config`
│   ├── yogurt-server/             # axum HTTP + WS, embeds web/dist
│   ├── yogurt-audio/              # ScreenCaptureKit + CoreAudio wrapper, streams PCM
│   ├── yogurt-stt/                # STT trait + Deepgram + AssemblyAI + whisper.cpp adapters
│   ├── yogurt-llm/                # OpenAI-compat client (async-openai under the hood)
│   ├── yogurt-db/                 # SQLite via rusqlite; migrations; data access
│   ├── yogurt-notes/              # markdown <-> AST <-> augmented-merge logic
│   └── yogurt-prompts/            # 2 files: enhance.md + chat-system.md
├── web/
│   ├── package.json
│   ├── vite.config.ts
│   └── src/                       # React + TipTap + Tailwind
├── docs/
│   └── PRD.md                     # this document
└── .github/workflows/
    └── release.yml                # cross-compile, publish to Homebrew, attach to GH Release
```

Why split crates: each is independently testable, each has one clear purpose, and the platform-isolated parts (`yogurt-audio`) sit behind a trait so future Windows/Linux ports are additive.

## 9. Data model (SQLite)

Concise sketch — full migrations live in `crates/yogurt-db/migrations/`.

```sql
CREATE TABLE meetings (
  id              TEXT PRIMARY KEY,         -- ulid
  title           TEXT NOT NULL,
  started_at      INTEGER NOT NULL,         -- unix millis
  ended_at        INTEGER,                  -- null while in progress
  notes_md        TEXT NOT NULL DEFAULT '', -- raw markdown, source of truth
  enriched_md     TEXT,                     -- post-enhance markdown (black+grey)
  transcript_json TEXT NOT NULL DEFAULT '[]'  -- [{ts_ms, channel, text}, ...]
);

CREATE TABLE chat_messages (
  id          TEXT PRIMARY KEY,
  meeting_id  TEXT NOT NULL REFERENCES meetings(id),
  role        TEXT NOT NULL,                -- 'user' | 'assistant'
  content     TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE INDEX idx_meetings_started ON meetings(started_at DESC);
CREATE INDEX idx_chat_meeting ON chat_messages(meeting_id, created_at);
```

Markdown export: `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` is written on every `notes_md` or `enriched_md` mutation. The markdown file's YAML front-matter mirrors the row.

## 10. API surface

### REST (axum)

| Method | Path | Purpose |
|---|---|---|
| GET    | `/api/meetings`                          | List meetings, newest first |
| POST   | `/api/meetings`                          | Create new meeting (returns id) |
| GET    | `/api/meetings/:id`                      | Full meeting (notes, enriched_md, transcript, chat) |
| PATCH  | `/api/meetings/:id`                      | Update title or notes_md |
| POST   | `/api/meetings/:id/start`                | Begin recording (triggers audio capture + STT) |
| POST   | `/api/meetings/:id/stop`                 | End recording, kick off enhance |
| POST   | `/api/meetings/:id/enhance`              | Re-run augmented-notes enhance |
| POST   | `/api/meetings/:id/chat`                 | Send chat message; streaming response |
| DELETE | `/api/meetings/:id`                      | Delete meeting + markdown file |
| GET    | `/api/settings`                          | Current config (no API keys exposed) |
| PATCH  | `/api/settings`                          | Update config |
| POST   | `/api/settings/providers`                | Add/update LLM or STT provider |
| GET    | `/api/audio/devices`                     | List input devices |

### WebSocket

Single endpoint: `GET /ws/meetings/:id`. Bidirectional JSON messages:

| Direction | Type | Payload |
|---|---|---|
| S→C | `transcript`    | `{ts_ms, channel: 'mic'|'system', text, is_final}` |
| S→C | `notes_synced`  | `{rev, md}` (server-side autosaves) |
| S→C | `enhance_progress` | `{phase: 'sending'|'streaming'|'done', chars?}` |
| S→C | `chat_chunk`    | `{message_id, delta}` |
| C→S | `notes_edit`    | `{rev, md}` (debounced from editor) |
| C→S | `chat_send`     | `{content}` |

## 11. Distribution & dev workflow

### End-user install (the answer to "would `brew install yogurt` install everything?")

Yes — a single command. Yogurt is one static Rust binary with web assets embedded
via `rust-embed`, whisper.cpp linked statically, ScreenCaptureKit bindings linked
against the system framework. The Homebrew formula downloads the pre-built binary
from the matching GitHub Release for the user's architecture and drops it in
`/opt/homebrew/bin/yogurt`. There is no Node, no Python, no separate audio helper
to install.

```bash
# end-user, non-technical
brew install yogurt
yogurt start           # opens http://localhost:7878 in default browser
                       # macOS prompts for Screen Recording on first record
```

### Alternative install channels

```bash
# Rust developers
cargo install yogurt

# Direct binary (no package manager)
curl -L https://github.com/<org>/yogurt/releases/latest/download/yogurt-aarch64-apple-darwin.tar.gz | tar xz
./yogurt start
```

### Source / contributor workflow

The dev experience runs the Rust backend and the Vite frontend as two processes during development so frontend changes hot-reload, but bundles to one binary for release.

```bash
git clone https://github.com/jarvisrchen/yogurt.git
cd yogurt

# one-time setup
brew install rust pnpm
pnpm --dir web install

# optional: seed your API keys via .env.local (gitignored; see §5.6)
cat > .env.local <<'EOF'
YOGURT_MINIMAX_API_KEY=sk-...
# YOGURT_DEEPGRAM_API_KEY=...   # optional, for cloud STT
EOF

# dev: two terminals
cargo run -p yogurt-cli -- start --dev   # backend at :7878, no embedded assets
pnpm --dir web dev                       # vite at :5173, proxies /api + /ws to :7878

# release build (single binary with embedded assets)
pnpm --dir web build                     # outputs web/dist
cargo build --release                    # rust-embed picks up web/dist
./target/release/yogurt start
```

`--dev` flag tells the server to proxy `/` to the Vite dev server instead of serving embedded assets. In release builds, `web/dist` is compiled into the binary.

`.env.local` is read once at startup; on first run any `YOGURT_*_API_KEY` vars are copied into the macOS Keychain and a matching provider entry is seeded in the DB. After that, the file can be deleted without losing the keys. See §5.6 for the full env-var → provider mapping.

### Release pipeline (GitHub Actions)

On tag push (`v*`):
1. Matrix build: `aarch64-apple-darwin` + `x86_64-apple-darwin`.
2. Each builds frontend, then `cargo build --release`.
3. Tarballs uploaded to GitHub Release.
4. Workflow opens a PR against `homebrew-yogurt` tap with updated SHA256.
5. `cargo publish` for the binary crate.

## 12. Implementation roadmap

Estimates are working-day rough orders of magnitude, not commitments.

| Phase | Deliverable | Est. |
|---|---|---|
| **0. Skeleton**     | Cargo workspace, axum server serving a "Hello yogurt" page, CLI with `start` command, web app scaffold (React + Vite + Tailwind + TipTap). | 1d |
| **1. Design system** | Set up the design tokens from §16 (paper/ink/grey/blueberry/strawberry/matcha palette, Instrument Serif + Hanken Grotesk + JetBrains Mono fonts, spacing/radius/elevation/motion scales). Build core components (button, badge, card, pill, mockup-window chrome) per the design board. | 1-2d |
| **2. Audio capture**| `yogurt-audio` crate: ScreenCaptureKit init, mic capture, two channels piped to a Tokio broadcast. Permission prompt UX with the design's "Permission not granted" recovery screen. | 2d |
| **3. Cloud STT**    | `yogurt-stt` trait + Deepgram streaming adapter. Live transcript dock panel (right-edge tab + 340ms slide-in). | 2d |
| **4. Augmented notes hero** | TipTap setup with custom marks for `ai-grey` and `transcript-link`. Markdown round-trip. Enhance endpoint + streaming enhancing state with shimmer skeletons + staggered reveal (motion tokens). | 3-4d |
| **5. LLM client + settings** | `yogurt-llm` crate, OpenAI-compat client, full settings UI per design (Model / Transcription / Audio / General sidebar), Keychain storage, provider preset cards. | 2d |
| **6. In-meeting chat** | Floating ⌘K "Ask this meeting" pill + chat window (popUp animation). Streaming chat endpoint. Prompt-tune for transcript-aware Q&A. | 1d |
| **7. Library + onboarding** | Library home (sidebar, folders, date-grouped meeting cards, search). Onboarding flow (3-step welcome). Empty / first-run states. | 2d |
| **8. Local STT**    | `whisper.cpp` adapter via `whisper-rs`. Model-download UX card (per design). Streaming chunked decode with VAD. | 2d |
| **9. Polish + distribution** | Markdown export, README, Homebrew tap, GH Actions release workflow, universal binary. | 2-3d |

Total rough estimate: **~18 working days** (~3.5 weeks at one engineer focused). Phases 0-3 are the highest-risk because they validate the audio + transcription path; if those work, the rest is straightforward. (History: 15d original → 14d after template cut → 18d after design board added library / onboarding / states / design-system phase.)

## 13. Open risks

| Risk | Mitigation |
|---|---|
| `screencapturekit` Rust crate may have gaps for audio-only loopback (it's mainly designed for screen capture). | Phase 1 spike: if the crate falls short, ship a thin Swift binary (~150 lines) invoked as a subprocess. Pure additive change, doesn't reshape architecture. |
| TipTap's mark system may struggle with structural diffing of LLM-rewritten markdown. | Phase 3 prototype: validate the black/grey merge against three real meeting transcripts before committing to TipTap. Fallback: ProseMirror directly. |
| `whisper.cpp` streaming partials are noticeably worse than Deepgram's polished streaming. | Acceptable — local mode is positioned as the privacy escape hatch, not the daily-driver. Document expected lag. |
| Browser permission dialog for microphone (`getUserMedia`) may confuse users if the *backend* is also capturing mic. | We don't use `getUserMedia` — all audio is captured by the Rust binary. Browser only renders the UI. |
| First-meeting permission UX (macOS Screen Recording prompt) is a friction wall. | Onboarding screen explicitly explains *why* the permission is needed, with a screenshot. After grant, user must restart Yogurt once (macOS limitation). |
| Whisper.cpp model download (~500MB for `small.en`, ~3GB for `large-v3`) is a first-run gotcha. | Settings UI shows download progress; default cloud STT means most users never trigger it. |

## 14. Success criteria

V1 is "done" when:

1. A new user on an M-series Mac can run `brew install yogurt && yogurt start`, grant Screen Recording permission, paste a Minimax API key in Settings, and record + transcribe + get augmented notes for a real meeting end-to-end.
2. The black-user / grey-AI in-place notes merge is visually indistinguishable in feel from Granola's.
3. A privacy-focused user can switch to `whisper.cpp` + a local Ollama endpoint and the full pipeline still works with no internet.
4. A developer can `git clone && cargo run` and have a working dev environment within 5 minutes.

## 15. Open questions for future rounds

All v1-relevant questions are now resolved.

Closed 2026-06-24:
- **Naming** — committed to **"yogurt"** lowercase with the purple "spoon &amp; swirl" mark.
- **Template picker** — deferred to v2 (see §6 item 2). v1 ships a Re-enhance button that re-runs the single bundled `enhance.md` prompt.
- **License** — **MIT** (matches Meetily and Hyprnote).
- **Org / repo** — `github.com/jarvisrchen/yogurt` (personal account).
- **Telemetry** — **no phone-home of any kind** in v1. Not even opt-in Sentry. If we want crash reporting later, it gets its own design pass.
- **Design themes** — **Blueberry only in v1**. Strawberry and Matcha-dark are documented in §16 but deferred. Single theme keeps the design-system phase tighter.

No remaining open questions for v1. Next step: writing-plans skill produces the phased implementation plan.

## 16. Brand &amp; Visual Design System

Source of truth: `docs/archive/yogurt-app-design/project/Yogurt Design Board.dc.html`. This section is the implementation reference — keep it in sync if the design board changes.

### 16.1 Brand identity

- **Wordmark:** `yogurt` — Instrument Serif, lowercase, letter-spacing `-0.01em`.
- **Logo mark:** "spoon &amp; swirl" — a 44×44 blueberry circle (`#5B4FC7`), with a white spoon-curve stroke and a strawberry dot (`#E07A66`) at the spoon's tip. Renders cleanly at 19px and at 60px.
- **Personality line:** *"A local-first, open-source meeting copilot. Granola's augmented-notes UX, the privacy posture inverted — your audio never leaves the machine."*
- **Voice:** warm + editorial + a touch dev-tooly. Mono captions ("100% on-device", "localhost:7878", "$ yogurt start") set the technical tone; Instrument Serif headlines set the editorial tone.

### 16.2 Color palette (Blueberry — default theme)

| Token | Hex | Use |
|---|---|---|
| `--paper` | `#FBF7EF` | App background, hero surfaces |
| `--card`  | `#FFFFFF` | Cards, surfaces over paper |
| `--ink`   | `#211D18` | User notes, headings, primary text |
| `--grey`  | `#A89F90` | AI-added text, secondary captions |
| `--line`  | `#EBE3D5` | Borders, dividers |
| `--blue`  | `#5B4FC7` | Primary (blueberry) — buttons, active states, transcript links |
| `--blsoft`| `#ECE9FB` | Soft blueberry — active-nav backgrounds, pill backgrounds |
| `--straw` | `#E07A66` | Recording indicator, error/warning accent |
| `--matcha`| `#5E9E73` | Local-only / privacy / success state |
| `--mtsoft`| `#E7F0E8` | Soft matcha — local-mode badges |
| `--mut`   | `#8A8174` | Muted text on cards |

Alternative themes ship behind a setting:
- **Strawberry** — warmer paper (`#FCF4F1`), coral accent (`#E0564B`). Consumer-leaning marketing variant.
- **Matcha (dark)** — dark green-grey paper (`#1B1F1A`), white-warm text (`#EDEAE2`), green accent (`#7FBE8C`). For late-night meetings. *Open question §15 — ship just default or all three?*

### 16.3 Typography

| Family | Use | Weights |
|---|---|---|
| **Instrument Serif** | Display, wordmark, screen titles, hero headlines | 400, 400 italic |
| **Hanken Grotesk**   | Note body, labels, buttons, UI text | 400, 500, 600, 700, 800 |
| **JetBrains Mono**   | Timestamps, CLI output, technical captions, terminal mockups | 400, 500, 600 |

Type scale (approx, based on design board): hero 52px Serif, section title 30-38px Serif, card title 16-21px Hanken 700, body 13-17px Hanken 400-500, caption 11-13px Hanken 500, mono caption 10.5-12px.

### 16.4 Spacing, radius, elevation

- **Spacing (4-base):** `4 · 8 · 12 · 16 · 24 · 32 · 48`. Use these exclusively — no off-scale paddings.
- **Border radius:** `6` (small chip), `9` (button, input), `14` (card), `999` (pill, recording badge).
- **Elevation:**
    - **Card:** `0 2px 6px rgba(40,30,15,.08)`
    - **Pop (chat window, popover):** `0 12px 30px -10px rgba(40,30,15,.22)`
    - **Window (modal, mockup chrome):** `0 26px 60px -28px rgba(40,30,15,.4)`

### 16.5 Motion

| Duration | Easing | Use |
|---|---|---|
| 260ms | ease-out (`popUp`) | Chat window expanding from the Ask pill |
| 340ms | `cubic-bezier(.2,.7,.2,1)` (`slideInRight`) | Live transcript dock opening from right edge |
| 600ms | per-item ease (staggered 140/340/560/760ms) | Augmented-notes merge — AI bullets fading + sliding in under user bullets after enhance |
| 1.4s  | ease-in-out infinite (`recpulse`) | Recording dot pulse, enhancing dot pulse |
| 1.0s  | step-end (`blink`) | Cursor blink for active editor / streaming partial |
| 1.25s | linear infinite (`shimmer`) | Skeleton placeholders during enhance streaming |
| 1.0s  | ease-in-out (`wave`) | 3-bar audio wave on live transcript tab |
| 3.5s  | ease-in-out (`float`) | Empty-state logo gentle float |

### 16.6 Component primitives (referenced across screens)

- **Buttons:**
    - Primary — blueberry bg, white text, 13.5px Hanken 600, `9px` radius, `0 2px 8px rgba(91,79,199,.3)` shadow. Used for "New meeting", "End meeting" (ink version), "Take me to my meetings →", "Re-enhance ⌄", "Generate" etc.
    - Secondary — white bg, ink text, `1px solid #D9D0C0` border, same dimensions. Used for "Enhance", "Restart Yogurt", "Cancel".
    - Ghost — transparent, muted-grey text. Used for "Cancel" in subtle contexts.
- **Recording badge:** white pill with `1px solid var(--straw)` border, ink text, pulsing strawberry dot + mono timer (`12:04`). Pill radius (`999`).
- **Tab group (Notes / Summary / Transcript):** `#F2EBDD` track, `4px` padding, white-card active tab with subtle shadow. Used wherever tab nav appears.
- **Provider chip:** rounded `8px` pill, soft theme color (blueberry for active, paper-border for inactive, matcha-soft for local providers). Small dot indicator + provider name.
- **Browser-chrome mockup wrapper:** every full-screen mock uses a fake-Safari header (`42px` height, `#F4EEE3` bg, 3-color traffic-light dots, centered URL pill showing `localhost:7878/...`). Use this wrapper in screenshots / marketing.

### 16.7 Black-you / grey-AI signature treatment

This is the single most important visual contract in the product (§5.3 hero). The design board evaluated three variants:

| Variant | What it does | Verdict |
|---|---|---|
| **A. Grey text** | Plain `color: #A89F90` on AI lines. Reads as one document. Faithful to Granola. | **✓ picked** |
| **B. Violet rail** | A 2px lilac left-border marks AI runs. More scannable, more "feature-y". | Considered, not picked |
| **C. Soft highlight** | Cream wash background on AI bullets. Most obvious; risks busy. | Rejected — too loud |

Implementation: a TipTap mark called `aiGrey` applied to LLM-added runs, with `transcriptTs` data attribute for the deep-link affordance. Edits to an `aiGrey` mark strip the mark (promote-to-black). The deep-link suffix `↳ HH:MM` is rendered via a separate inline node with dotted-underline `1.5px dotted #C9B8F0`.

### 16.8 Layout invariants

- Library / settings: 212px sidebar + flexible main column.
- Live meeting / post-meeting: centered notes column max-width `660px` with `42px 24px 130px` padding. Transcript panel docks to the right at `330px` width. Ask pill anchored bottom-center at `480px` width.
- Onboarding: 1.05fr / 0.95fr split, left column on paper, right column on white.

### 16.9 What's NOT in the visual system (yet)

- No real icon system — design board uses inline SVGs and emoji glyphs (⚙, ⌕, ✨, ↳, ↑, ✓, +). Pick an icon set (Lucide / Phosphor / custom SVGs) in phase 1.
- No data viz components — none needed for v1.
- No drag-and-drop (folder reorder etc.) — defer.

---

## Appendix A — Research summary

(Condensed from the Granola deep-dive done during brainstorm.)

- **Granola itself**: macOS desktop app, captures mic + system audio via ScreenCaptureKit, streams to Deepgram for transcription, calls OpenAI/Anthropic (likely Claude Sonnet) for the augmented-notes enhance. Audio deleted immediately, transcripts + notes stored in their AWS US VPC. Pricing: Free / $14 / $35 per user/month. Series C, $1.5B valuation as of March 2026.
- **Signature UX**: black-user/grey-AI markdown merge, AI bullets hyperlinked to transcript timestamps, markdown headings act as inline section prompts to the LLM, "Enhanced notes" regenerate button.
- **Closest OSS analogs**: Meetily (Tauri/Rust, ~13k stars — no augmented-notes UX) and Hyprnote (Tauri/Rust + Swift — produces summaries, not in-place augmentation). Yogurt's wedge is the augmented-notes UX.
- **Minimax API**: OpenAI-compatible for chat completions only. No public ASR endpoint. Suitable as the LLM, not as the STT provider.
- **Audio**: ScreenCaptureKit is the right macOS path; no kext, no BlackHole. Rust bindings via the `screencapturekit` crate.
- **Local STT**: `whisper.cpp` on Metal is the pragmatic default for Apple Silicon. M3 Max runs `large-v3` at 5–7x real-time; M1 Air runs `small.en` at 10–15x real-time.

## Appendix B — Glossary

- **Augmented notes** — the merging of user-written markdown bullets with LLM-added bullets, where user content stays black and LLM content renders grey, with grey-to-black promotion on edit.
- **ScreenCaptureKit (SCK)** — Apple's native macOS API (13+) for capturing screen and system audio without a kernel extension. The mechanism Yogurt uses to hear meeting audio without joining as a bot.
- **STT** — speech-to-text (transcription).
- **OpenAI-compatible endpoint** — any HTTP server that implements the OpenAI `/v1/chat/completions` API shape. Used as the universal interface for LLM providers in Yogurt.
- **Augmented-notes enhance** — the post-meeting LLM call that takes `(user notes + transcript + the bundled enhance prompt)` and returns the merged markdown document.
