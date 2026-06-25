# Requirements: Yogurt

**Defined:** 2026-06-25
**Core Value:** The black-user / grey-AI in-place augmented-notes UX, running fully local on macOS without a meeting bot.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Foundation

- [ ] **FOUND-01**: Cargo workspace with all 8 crates compiles (`cargo build --release`)
- [ ] **FOUND-02**: `yogurt start` CLI command launches axum server on `localhost:7878`
- [x] **FOUND-03**: Server serves a "Hello yogurt" React page via `rust-embed`
- [x] **FOUND-04**: SQLite database initializes at `~/.yogurt/db.sqlite` with WAL + read-pool / single-writer model
- [x] **FOUND-05**: WebSocket endpoint validates `Origin` header and session token
- [x] **FOUND-06**: Port `7878` conflict surfaces a clear CLI error with `--port` override

### Design System

- [x] **DESIGN-01**: Color tokens implemented (paper / ink / grey / blueberry / strawberry / matcha) per PRD §16.2
- [x] **DESIGN-02**: Typography tokens implemented (Instrument Serif / Hanken Grotesk / JetBrains Mono) per PRD §16.3
- [x] **DESIGN-03**: Spacing / radius / elevation scales applied per PRD §16.4
- [x] **DESIGN-04**: Motion tokens implemented (260ms popUp, 340ms slideInRight, 600ms staggered reveal, 1.4s recpulse, 1.0s blink, 1.25s shimmer, 1.0s wave, 3.5s float) per PRD §16.5
- [x] **DESIGN-05**: Core component primitives shipped (Primary / Secondary / Ghost button, recording badge, tab group, provider chip, browser-chrome mockup wrapper) per PRD §16.6 — Logo + Button (primary/secondary/ghost) + Pill family (RecordingBadge + ProviderChip) + Card + BrowserChrome landed in Plan 01-02; tab group + Button 'ink' variant explicitly deferred to Phase 4 per plan scope
- [ ] **DESIGN-06**: Icon system selected and applied (Lucide or Phosphor)

### Audio

- [ ] **AUDIO-01**: macOS Screen Recording permission prompt triggers on first record via ScreenCaptureKit
- [x] **AUDIO-02**: Captures mic (default input device) as mono 16 kHz / 16-bit PCM stream
- [x] **AUDIO-03**: Captures system audio (loopback via SCK) as mono 16 kHz / 16-bit PCM stream with `excludesCurrentProcessAudio = true`
- [x] **AUDIO-04**: Both streams pushed to in-process Tokio broadcast channel with capacity ≥ 256 frames
- [x] **AUDIO-05**: Meeting-relative clock established from `Instant::now()` at start, drift between mic/system < 50ms
- [ ] **AUDIO-06**: Recording stops cleanly on "End meeting"; per-meeting task supervisor terminates cleanly
- [ ] **AUDIO-07**: User can list and pick mic input device from `/api/audio/devices`

### Transcript (Cloud STT)

- [ ] **TRANS-01**: `SttEngine` trait defined with `open_session(channel) → SttSession` shape
- [ ] **TRANS-02**: Deepgram streaming adapter implements `SttEngine`
- [ ] **TRANS-03**: Live transcript dock panel collapsed by default as right-edge tab with 3-bar animated wave icon
- [ ] **TRANS-04**: Click expands panel — slides in from right at 340ms `cubic-bezier(.2,.7,.2,1)`, 330px wide, notes column stays editable (not dimmed)
- [ ] **TRANS-05**: Each transcript line shows channel label ("Me" ink / "Them" grey) + JetBrains-Mono timestamp from meeting start (e.g. `00:11:02`)
- [ ] **TRANS-06**: Auto-scrolls to bottom; pauses auto-scroll if user scrolls up
- [ ] **TRANS-07**: Cursor blink on most-recent partial transcript indicates "still listening"
- [ ] **TRANS-08**: Transcript appears with < 2s lag using Deepgram

### Augmented Notes (Hero Feature)

- [ ] **NOTES-01**: TipTap-based markdown editor centered in meeting view, max-width ~660px
- [ ] **NOTES-02**: Live legend in top-right shows the swatch contract (black = your notes / grey = AI)
- [ ] **NOTES-03**: Custom `aiGrey` TipTap mark applied to LLM-added runs
- [ ] **NOTES-04**: Custom `transcriptTs` data attribute on AI runs; renders `↳ HH:MM` link with dotted-underline lilac
- [ ] **NOTES-05**: Server-side AST diff in `yogurt-notes`; computed structurally over markdown (not character diff)
- [ ] **NOTES-06**: Schema persists both `notes_md` (pure markdown) AND `enriched_doc_json` (ProseMirror JSON with marks)
- [ ] **NOTES-07**: On "End meeting" — enhancing state: lilac progress banner with active dot pulse, "Weaving your notes into the transcript…", animated progress bar + character-streaming count
- [ ] **NOTES-08**: AI bullets appear as shimmer skeletons (1.25s linear infinite), resolve into grey markdown, staggered at 140/340/560/760ms
- [ ] **NOTES-09**: User-authored content stays ink black (`#211D18`); AI-added content renders grey (`#A89F90`)
- [ ] **NOTES-10**: Editing a grey range promotes it to black (`aiGrey` mark stripped); black ranges never overwritten on re-enhance
- [ ] **NOTES-11**: Clicking `↳ HH:MM` link opens transcript panel (if closed) and scrolls to timestamp; hover shows tooltip with transcript excerpt
- [ ] **NOTES-12**: "Re-enhance" button in top-right re-runs the same bundled `enhance.md` against current notes + transcript
- [ ] **NOTES-13**: Within 30s of "End meeting", user sees clean enriched document where their bullets are black and AI bullets are grey

### LLM Client + Settings

- [ ] **LLM-01**: `LlmClient` trait with single `complete_streaming(ChatRequest) → BoxStream<ChatDelta>` method
- [ ] **LLM-02**: `async-openai`-backed adapter accepts arbitrary base URL via `OpenAIConfig::with_api_base()`
- [ ] **LLM-03**: SSE streaming works end-to-end against an OpenAI-compatible endpoint
- [ ] **SET-01**: Settings page at `/settings` — 212px left sidebar + main content right
- [ ] **SET-02**: Sidebar lists sections: Model · Transcription · Audio · General
- [ ] **SET-03**: Sidebar footer shows green "Local-only · on" pill when no cloud providers active; JetBrains-Mono caption: `keys → macOS Keychain` / `data → ~/.yogurt/`
- [ ] **SET-04**: Model section — active provider rendered as 1.5px blueberry-bordered card; BASE URL + MODEL in mono; API KEY shown masked with last-4 + green "✓ stored"
- [ ] **SET-05**: Inactive providers stack as plain rows with "Set active" link
- [ ] **SET-06**: Preset chips for Ollama, LM Studio, OpenRouter; "+ Add" link for custom provider
- [ ] **SET-07**: Transcription section — Cloud (selected) / Local card pair with whisper.cpp model sizes
- [ ] **SET-08**: Audio + General rendered side-by-side: input device dropdown; port row (`7878`); "Open browser on start" toggle
- [ ] **SET-09**: Config persisted in `~/.yogurt/config.toml`
- [ ] **SET-10**: API keys stored in macOS Keychain via `keyring` crate, never plaintext; eager-loaded at startup
- [ ] **SET-11**: Dev-mode (`--dev` flag) loads keys from `.env.local` at repo root (gitignored) as a developer convenience; release builds ignore `.env.local` and only read Keychain. Default dev key is `MINIMAX_API_KEY` against base URL `https://api.minimaxi.chat/v1`.

### In-Meeting Chat

- [ ] **CHAT-01**: Floating "Ask this meeting…" pill anchored bottom-center of meeting view (480px wide, 24px from bottom)
- [ ] **CHAT-02**: Pill shows `⌘K` keyboard hint badge and purple send arrow; persists into post-meeting view
- [ ] **CHAT-03**: Click or `⌘K` expands pill into chat window with 260ms `popUp` ease-out
- [ ] **CHAT-04**: Chat window shows yogurt swirl logo + "Ask the meeting" + collapse caret; user msgs right-aligned blueberry, AI msgs left-aligned cream with grey border
- [ ] **CHAT-05**: Each user message sent to LLM with transcript-so-far as context, using `chat-system.md` system prompt
- [ ] **CHAT-06**: Streaming response renders inline
- [ ] **CHAT-07**: First response chunk streams < 2s after sending

### Local Storage

- [ ] **STORE-01**: SQLite schema includes `meetings` (with `notes_md` + `enriched_md` + `enriched_doc_json` + `transcript_json`) and `chat_messages` *(Phase 0 scaffold complete; `enriched_doc_json` column deferred to Phase 4)*
- [x] **STORE-02**: Indexes on `meetings(started_at DESC)` and `chat_messages(meeting_id, created_at)`
- [ ] **STORE-03**: Each meeting also written to `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` with YAML front-matter
- [ ] **STORE-04**: Markdown file rewritten on every `notes_md` or `enriched_md` mutation via single `MarkdownExporter`
- [x] **STORE-05**: SQLite uses WAL mode; separate read pool + `Mutex<Connection>` writer

### Library (Home View)

- [ ] **LIB-01**: Default page at `localhost:7878` is the library — sidebar + meeting list
- [ ] **LIB-02**: Left sidebar shows Yogurt swirl logo + wordmark + "+ New meeting" primary button
- [ ] **LIB-03**: Sidebar nav rows: "All meetings" (active = lilac bg, blueberry text), "Starred"
- [ ] **LIB-04**: Sidebar footer: green "Local-only · on" pill when no cloud providers + `⚙ Settings` row
- [ ] **LIB-05**: Main pane greeting in Instrument Serif ("Good afternoon, Dana") + caption ("N meetings · all on this Mac")
- [ ] **LIB-06**: Search affordance top-right: pill with magnifier icon, "Search notes & transcripts"
- [ ] **LIB-07**: SQLite FTS5 keyword search across notes + transcripts (added per research findings)
- [ ] **LIB-08**: Meeting list grouped by date with mono-caption labels (`TODAY`, `YESTERDAY`, etc.)
- [ ] **LIB-09**: Each meeting card: 42px colored-tinted avatar (Instrument Serif initials) + title (Hanken 700) + meta line + local badges on right
- [ ] **LIB-10**: User can click into any meeting, create a new one, delete a meeting from card
- [ ] **LIB-11**: Meeting card supports inline-editable title with default fallback
- [ ] **LIB-12**: Per-meeting "Copy markdown" and "Reveal in Finder" affordance

### Onboarding

- [ ] **ONB-01**: Route `/welcome` shows two-column layout on cream paper
- [ ] **ONB-02**: Left column: Yogurt logo + Instrument-Serif welcome ("Welcome to yogurt.") + one-liner + terminal mockup showing boot sequence
- [ ] **ONB-03**: Right column: "ONE-TIME SETUP" caption + vertical step list with 3 cards (Screen Recording, Connect your model, Pick transcription)
- [ ] **ONB-04**: Screen Recording step shows green "✓" badge once granted
- [ ] **ONB-05**: Connect-your-model step is the current-step card (blueberry border) with provider chips
- [ ] **ONB-06**: Pick-transcription step explains Cloud Deepgram vs Local whisper.cpp
- [ ] **ONB-07**: Primary button "Take me to my meetings →" navigates to library
- [ ] **ONB-08**: Footer note: "Restart once after granting — a macOS quirk, not us."

### Empty & Error States

- [ ] **STATE-01**: Empty library — centered soft-floating Yogurt logo (3.5s float), "No meetings yet" headline, supporting line, primary "Start your first meeting" CTA with `⌘N` hint, mono caption with file path
- [ ] **STATE-02**: Permission-not-granted — full-screen warning card, strawberry alert icon, headline "Yogurt can't hear the call yet", numbered 3-step recovery, CTA pair: "Open System Settings" + "Restart Yogurt"
- [ ] **STATE-03**: Enhancing state — lilac progress banner across top of meeting view (covered by NOTES-07/08)
- [ ] **STATE-04**: First-time whisper.cpp model download — modal-like card with matcha down-arrow, "Downloading small.en", mono caption, matcha progress bar, body copy, Cancel / Run-in-background button pair

### Local STT

- [ ] **LOCAL-01**: `whisper.cpp` adapter via `whisper-rs` implements `SttEngine`, gated behind `local-stt` Cargo feature
- [ ] **LOCAL-02**: VAD-driven chunking with dual `whisper_state` (preview + settled) running on Metal
- [ ] **LOCAL-03**: Models download on first use, stored in `~/.yogurt/models`
- [ ] **LOCAL-04**: `small.en` baseline produces transcript with < 3s lag on M1 Air
- [ ] **LOCAL-05**: All `whisper.cpp` calls run on `spawn_blocking` (never block tokio scheduler)

### Bundled Prompts

- [ ] **PROMPT-01**: `crates/yogurt-prompts/` ships exactly two files: `enhance.md` and `chat-system.md`
- [ ] **PROMPT-02**: `enhance.md` takes `{{NOTES}}` and `{{TRANSCRIPT}}` placeholders
- [ ] **PROMPT-03**: `chat-system.md` is the in-meeting chat system prompt
- [ ] **PROMPT-04**: Reloading binary picks up edits to either file (no compile step required for power users)

### Distribution

- [ ] **DIST-01**: GitHub Actions release workflow runs on `v*` tag push
- [ ] **DIST-02**: Matrix build produces `aarch64-apple-darwin` + `x86_64-apple-darwin` per-arch tarballs
- [ ] **DIST-03**: Each release notarized via `notarytool` + `staple` with stable Developer ID
- [ ] **DIST-04**: Bundle ID pinned to `ai.yogurt.app`
- [ ] **DIST-05**: Tarballs attached to GitHub Release
- [ ] **DIST-06**: Homebrew tap PR opened against `homebrew-yogurt` with updated SHA256s (after release visible)
- [ ] **DIST-07**: `cargo publish` for the binary crate
- [ ] **DIST-08**: `yogurt doctor` subcommand for TCC reset / model re-download / port diagnostics
- [ ] **DIST-09**: `brew install yogurt && yogurt start` works end-to-end for a non-technical user
- [ ] **DIST-10**: README documents install, dev workflow, configuration

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Calendar & Integrations

- **CAL-01**: Google Calendar OAuth + auto-detect upcoming meetings
- **CAL-02**: Outlook Calendar OAuth + auto-detect upcoming meetings
- **INTG-01**: Slack export integration
- **INTG-02**: Notion export integration
- **INTG-03**: Linear export integration

### Templates

- **TPL-01**: Re-enhance template picker popover (Standup / Generic / 1:1 / Interview debrief)
- **TPL-02**: Versions rail showing v1/v2/v3 enhance outputs
- **TPL-03**: Custom template authoring UI

### Cross-Meeting Intelligence

- **CROSS-01**: Embeddings + sqlite-vss or LanceDB for cross-meeting search
- **CROSS-02**: "Ask anything across your meeting history" chat

### Platform Expansion

- **PLAT-01**: Per-speaker diarization (pyannote sidecar, opt-in)
- **PLAT-02**: Menu-bar / global-hotkey UI (Tauri wrap)
- **PLAT-03**: Windows support
- **PLAT-04**: Linux support
- **PLAT-05**: MCP server
- **PLAT-06**: Optional encrypted sync / multi-device (user-hosted only)

### Library Enhancements

- **LIB-V2-01**: Folders with color dots + counts (requires `folders` table schema addition)
- **LIB-V2-02**: Per-meeting "keep audio" retention toggle
- **LIB-V2-03**: Auto-save "Saved · 2s ago" indicator
- **LIB-V2-04**: Strawberry theme
- **LIB-V2-05**: Matcha-dark theme

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Meeting participant bot | Anti-goal — the single thing that makes Yogurt feel magical vs Granola |
| Yogurt cloud service | Anti-goal — there is no Yogurt cloud, period |
| Subscription billing | Anti-goal — MIT open source forever |
| Default telemetry / phone-home | Anti-goal — zero phone-home in v1, not even opt-in Sentry |
| Multi-user / authentication | Single-user single-machine; no login screen |
| Mobile / web-hosted version | macOS only; browser UI is local-only |
| `getUserMedia` browser audio | Double-permission UX trap; all audio captured in Rust |
| Tauri / Electron wrapper | Defeats single-static-binary distribution; killer feature is `brew install` |
| BlackHole-style virtual audio | Defeats privacy posture; ScreenCaptureKit is the right path |

## Traceability

Finalized 2026-06-25 during roadmap creation. Every v1 requirement maps to exactly one phase.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUND-01 | Phase 0 | Pending |
| FOUND-02 | Phase 0 | Pending |
| FOUND-03 | Phase 0 | Complete |
| FOUND-04 | Phase 0 | Complete |
| FOUND-05 | Phase 0 | Complete |
| FOUND-06 | Phase 0 | Complete |
| DESIGN-01 | Phase 1 (Plan 01-01) | Complete (2026-06-25) |
| DESIGN-02 | Phase 1 (Plan 01-01) | Complete (2026-06-25) |
| DESIGN-03 | Phase 1 (Plan 01-01) | Complete (2026-06-25) |
| DESIGN-04 | Phase 1 (Plan 01-01) | Complete (2026-06-25) |
| DESIGN-05 | Phase 1 (Plan 01-02) | Complete (2026-06-25) — tab group + Button 'ink' variant deferred to Phase 4 per plan scope |
| DESIGN-06 | Phase 1 | Pending |
| AUDIO-01 | Phase 2 (Plan 02-01) | API ready (2026-06-25) — `has_screen_recording_permission()` + `request_screen_recording_permission()` exposed by `yogurt-audio`; end-to-end "prompt fires on first record" verification deferred to Plan 02-XX once `start_capture()` exists |
| AUDIO-02 | Phase 2 (Plan 02-02) | Complete (2026-06-25) — `spawn_mic_capture()` opens cpal default input, resamples via `Downmix` to 16 kHz mono i16, chunks into 320-sample `Frame`s. Hardware-verified: 249 frames in 5s on Apple Silicon. |
| AUDIO-03 | Phase 2 (Plan 02-02) | Complete (2026-06-25) — `spawn_system_capture()` builds an audio-only SCStream with `with_excludes_current_process_audio(true)` set from first commit. Hardware-verified: 248 frames in 5s with Glass.aiff loop, peak −5221. |
| AUDIO-04 | Phase 2 (Plan 02-02) | Complete (2026-06-25) — `BROADCAST_CAPACITY = 256` const; both `mic_tx` and `system_tx` created with `broadcast::channel::<Frame>(BROADCAST_CAPACITY)`. |
| AUDIO-05 | Phase 2 (Plan 02-02) | Complete (2026-06-25) — each `FrameChunker` captures `Instant::now()` at construction; both chunkers seeded synchronously inside `start_capture()` (spawn-order skew microseconds, trivially < 50ms drift budget). Long-run 60-min drift assertion deferred to Phase 3 once STT timestamps land. |
| AUDIO-06 | Phase 2 | Pending |
| AUDIO-07 | Phase 2 | Pending |
| TRANS-01 | Phase 3 | Pending |
| TRANS-02 | Phase 3 | Pending |
| TRANS-03 | Phase 3 | Pending |
| TRANS-04 | Phase 3 | Pending |
| TRANS-05 | Phase 3 | Pending |
| TRANS-06 | Phase 3 | Pending |
| TRANS-07 | Phase 3 | Pending |
| TRANS-08 | Phase 3 | Pending |
| NOTES-01 | Phase 4 | Pending |
| NOTES-02 | Phase 4 | Pending |
| NOTES-03 | Phase 4 | Pending |
| NOTES-04 | Phase 4 | Pending |
| NOTES-05 | Phase 4 | Pending |
| NOTES-06 | Phase 4 | Pending |
| NOTES-07 | Phase 4 | Pending |
| NOTES-08 | Phase 4 | Pending |
| NOTES-09 | Phase 4 | Pending |
| NOTES-10 | Phase 4 | Pending |
| NOTES-11 | Phase 4 | Pending |
| NOTES-12 | Phase 4 | Pending |
| NOTES-13 | Phase 4 | Pending |
| LLM-01 | Phase 5 | Pending |
| LLM-02 | Phase 5 | Pending |
| LLM-03 | Phase 5 | Pending |
| SET-01 | Phase 5 | Pending |
| SET-02 | Phase 5 | Pending |
| SET-03 | Phase 5 | Pending |
| SET-04 | Phase 5 | Pending |
| SET-05 | Phase 5 | Pending |
| SET-06 | Phase 5 | Pending |
| SET-07 | Phase 5 | Pending |
| SET-08 | Phase 5 | Pending |
| SET-09 | Phase 5 | Pending |
| SET-10 | Phase 5 | Pending |
| CHAT-01 | Phase 6 | Pending |
| CHAT-02 | Phase 6 | Pending |
| CHAT-03 | Phase 6 | Pending |
| CHAT-04 | Phase 6 | Pending |
| CHAT-05 | Phase 6 | Pending |
| CHAT-06 | Phase 6 | Pending |
| CHAT-07 | Phase 6 | Pending |
| STORE-01 | Phase 0 (schema scaffold) / Phase 4 (enriched_doc_json migration) | Phase 0 scaffold complete; Phase 4 migration pending |
| STORE-02 | Phase 0 | Complete |
| STORE-03 | Phase 4 | Pending |
| STORE-04 | Phase 4 | Pending |
| STORE-05 | Phase 0 | Complete |
| LIB-01 | Phase 7 | Pending |
| LIB-02 | Phase 7 | Pending |
| LIB-03 | Phase 7 | Pending |
| LIB-04 | Phase 7 | Pending |
| LIB-05 | Phase 7 | Pending |
| LIB-06 | Phase 7 | Pending |
| LIB-07 | Phase 7 | Pending |
| LIB-08 | Phase 7 | Pending |
| LIB-09 | Phase 7 | Pending |
| LIB-10 | Phase 7 | Pending |
| LIB-11 | Phase 7 | Pending |
| LIB-12 | Phase 7 | Pending |
| ONB-01 | Phase 7 | Pending |
| ONB-02 | Phase 7 | Pending |
| ONB-03 | Phase 7 | Pending |
| ONB-04 | Phase 7 | Pending |
| ONB-05 | Phase 7 | Pending |
| ONB-06 | Phase 7 | Pending |
| ONB-07 | Phase 7 | Pending |
| ONB-08 | Phase 7 | Pending |
| STATE-01 | Phase 7 | Pending |
| STATE-02 | Phase 7 | Pending |
| STATE-03 | Phase 7 | Pending |
| STATE-04 | Phase 7 | Pending |
| LOCAL-01 | Phase 8 | Pending |
| LOCAL-02 | Phase 8 | Pending |
| LOCAL-03 | Phase 8 | Pending |
| LOCAL-04 | Phase 8 | Pending |
| LOCAL-05 | Phase 8 | Pending |
| PROMPT-01 | Phase 4 | Pending |
| PROMPT-02 | Phase 4 | Pending |
| PROMPT-03 | Phase 4 | Pending |
| PROMPT-04 | Phase 4 | Pending |
| DIST-01 | Phase 9 | Pending |
| DIST-02 | Phase 9 | Pending |
| DIST-03 | Phase 9 | Pending |
| DIST-04 | Phase 9 | Pending |
| DIST-05 | Phase 9 | Pending |
| DIST-06 | Phase 9 | Pending |
| DIST-07 | Phase 9 | Pending |
| DIST-08 | Phase 9 | Pending |
| DIST-09 | Phase 9 | Pending |
| DIST-10 | Phase 9 | Pending |

**Coverage:**

- v1 requirements: 96 total across 14 categories
- Mapped to phases: 96/96 (100%)
- Orphaned: 0
- Split mappings: STORE-01 (Phase 0 schema scaffold + Phase 4 `enriched_doc_json` column migration — single requirement, two phases of work)

---
*Requirements defined: 2026-06-25*
*Traceability finalized: 2026-06-25 after roadmap creation*
