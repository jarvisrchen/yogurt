# Roadmap: Yogurt

## Overview

Yogurt ships in 10 phases that progressively deliver Granola's signature augmented-notes UX as a single-binary, local-first macOS app. The roadmap starts with a foundation that bakes pitfall mitigations into the skeleton (Phase 0), establishes the design system before any screens (Phase 1), retires the highest project-killer risk early through an audio capture spike (Phase 2), and then layers user-visible capability slices on top: cloud transcript (Phase 3) → augmented-notes hero (Phase 4) → BYO LLM + settings (Phase 5) → in-meeting chat (Phase 6) → library + onboarding + table-stakes adds (Phase 7) → local STT escape hatch (Phase 8) → distribution polish (Phase 9). Every v1 requirement maps to exactly one phase; the project-killer pitfalls catalogued in research are mapped to specific phase deliverables (not assumptions).

## Phases

**Phase Numbering:**

- Integer phases (0, 1, 2…): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 0: Skeleton & Foundations** - Cargo workspace, axum server, embedded SPA, SQLite WAL + dual pool, localhost-only binding, WS Origin check + session token, port-conflict UX (completed 2026-06-25)
- [ ] **Phase 1: Design System** - Tokens (color/typography/spacing/motion) + core component primitives applied before any screen is built
- [ ] **Phase 2: Audio Capture (HIGHEST RISK)** - ScreenCaptureKit mic + system loopback gated behind a dual-channel PCM ear-test spike, meeting-relative clock, Swift sidecar fallback documented
- [ ] **Phase 3: Cloud STT + Live Transcript** - SttEngine trait + Deepgram adapter, right-edge live transcript dock with Me/Them channel labels
- [ ] **Phase 4: Augmented Notes Hero (HIGHEST PAYOFF)** - TipTap aiGrey + transcriptTs marks, server-side AST diff, enriched_doc_json schema migration, minimal hardcoded LLM client, bundled enhance.md
- [ ] **Phase 5: LLM Client + Settings + Keychain** - LlmClient trait, settings UI (Model/Transcription/Audio/General), Keychain eager-loaded at startup
- [ ] **Phase 6: In-Meeting Chat** - Floating Ask-this-meeting pill + chat window streaming against transcript context
- [ ] **Phase 7: Library + Onboarding + States** - Library home view, /welcome flow, empty/error states, FTS5 keyword search, copy/reveal-in-finder, inline-editable title, delete-card UI
- [ ] **Phase 8: Local STT (whisper.cpp)** - whisper-rs adapter gated behind local-stt Cargo feature, VAD + dual whisper_state, M1 Air benchmark
- [ ] **Phase 9: Distribution Polish** - Per-arch tarballs, notarized release, Homebrew tap, cargo publish, `yogurt doctor` subcommand, README

## Phase Details

### Phase 0: Skeleton & Foundations

**Goal**: Cargo workspace builds, `yogurt start` serves a "Hello yogurt" SPA from a single static binary, with the foundational pitfall mitigations (SQLite WAL + dual pool, embedded SPA fallback, localhost-only bind, WS Origin check + session token, port-conflict UX) baked in from day one.
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: FOUND-01, FOUND-02, FOUND-03, FOUND-04, FOUND-05, FOUND-06, STORE-01 (schema scaffold), STORE-02, STORE-05
**Success Criteria** (what must be TRUE):

  1. `cargo build --release` succeeds for all 8 workspace crates with zero warnings on a clean clone
  2. `yogurt start` launches axum on `localhost:7878` and the embedded React SPA renders "Hello yogurt" — including `/library/anything` SPA fallback returning the embedded `index.html`
  3. SQLite database initializes at `~/.yogurt/db.sqlite` with WAL mode, separate read pool + single-writer `Mutex<Connection>`, and runs the v1 schema migration (meetings + chat_messages tables + indexes)
  4. WebSocket endpoint rejects connections with non-`localhost:7878` Origin headers and requires the session token written to `~/.yogurt/session-token` (mode 0600)
  5. Running `yogurt start` while port 7878 is occupied prints a friendly error (`Port 7878 is already in use. Try --port 7879 or run lsof -i :7878`) and exits cleanly

**Plans**: TBD
**UI hint**: no

### Phase 1: Design System

**Goal**: Every design token from PRD §16 (color/typography/spacing/radius/elevation/motion) and every core component primitive (buttons, recording badge, tab group, provider chip, browser-chrome mockup wrapper) is implemented in `/web` and rendered on a single token-showcase screen before any user-facing screen is built.
**Mode:** mvp
**Depends on**: Phase 0
**Requirements**: DESIGN-01, DESIGN-02, DESIGN-03, DESIGN-04, DESIGN-05, DESIGN-06
**Success Criteria** (what must be TRUE):

  1. A `/design-system` developer route renders all color tokens (paper/ink/grey/blueberry/strawberry/matcha), all three font families (Instrument Serif / Hanken Grotesk / JetBrains Mono), and the full spacing scale (4·8·12·16·24·32·48) visibly applied
  2. All 8 motion tokens (260ms popUp, 340ms slideInRight, 600ms staggered reveal, 1.4s recpulse, 1.0s blink, 1.25s shimmer, 1.0s wave, 3.5s float) are available as Tailwind/CSS utilities and demonstrable on the showcase screen
  3. Core primitives — Primary/Secondary/Ghost button, recording badge with pulsing strawberry dot + mono timer, tab group, provider chip, browser-chrome mockup wrapper — render correctly with documented props
  4. A Lucide (or Phosphor) icon set is wired and at least 5 icons used in the showcase render at expected sizes

**Plans**: TBD
**UI hint**: yes

### Phase 2: Audio Capture (HIGHEST RISK)

**Goal**: `yogurt-audio` crate captures mic + system audio via ScreenCaptureKit into a Tokio broadcast channel, with the meeting-relative clock model designed in from day one and a documented Swift sidecar fallback path if the SCK crate's audio loopback surface proves insufficient. The 30-second dual-channel PCM → WAV → ear-test acceptance is a phase gate, not a checkbox.
**Mode:** mvp
**Depends on**: Phase 0
**Requirements**: AUDIO-01, AUDIO-02, AUDIO-03, AUDIO-04, AUDIO-05, AUDIO-06, AUDIO-07
**Success Criteria** (what must be TRUE):

  1. **Gate spike**: 30 seconds of mic + system audio captured during a real YouTube/Zoom playback writes a 2-channel WAV file that passes an ear-test (both channels audible, no silence, no clipping, no channel swap) — this gates the rest of the phase
  2. First-record triggers the macOS Screen Recording permission prompt via SCK init; `excludesCurrentProcessAudio = true` is set from the first commit (verified by playing audio from Yogurt's own UI and confirming it is NOT in the transcript)
  3. Meeting-relative clock established from `Instant::now()` at meeting start; mic timestamps via CoreAudio's `AudioTimeStamp`, system timestamps via `CMSampleBuffer.presentationTimeStamp`; 60-min synthetic test shows mic↔system drift < 250ms at end
  4. `broadcast::Sender<AudioFrame>` capacity ≥ 256 frames; "End meeting" cleanly terminates the per-meeting supervisor (no leaked SCK handles, no orphan tasks)
  5. `GET /api/audio/devices` returns the list of CoreAudio input devices; user can pick mic input device

**Plans**: TBD
**UI hint**: no

### Phase 3: Cloud STT + Live Transcript

**Goal**: A `SttEngine` trait abstracts cloud and (future) local transcription; the Deepgram streaming adapter implements it; the right-edge live transcript dock UI renders incoming transcript events end-to-end with < 2s lag and visible Me/Them channel labels. This is the first phase that wires the full audio → STT → WS → browser pipeline.
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: TRANS-01, TRANS-02, TRANS-03, TRANS-04, TRANS-05, TRANS-06, TRANS-07, TRANS-08
**Success Criteria** (what must be TRUE):

  1. User can start a meeting, speak, and see live transcript lines appear in the right-edge dock with < 2s lag (Deepgram), each line correctly labeled "Me" (ink) or "Them" (grey) with a JetBrains-Mono `HH:MM:SS` meeting-relative timestamp
  2. Live transcript tab is collapsed by default with the 3-bar animated wave icon; clicking it slides the 330px-wide panel in from the right at 340ms `cubic-bezier(.2,.7,.2,1)`; notes column remains fully editable (not dimmed)
  3. Panel auto-scrolls to bottom; scrolling up pauses auto-scroll; cursor blink appears on the most-recent partial transcript to signal "still listening"
  4. Two STT sessions run per meeting (one per channel); per-meeting supervisor closes both sessions cleanly on "End meeting"

**Plans**: TBD
**UI hint**: yes

### Phase 4: Augmented Notes Hero (HIGHEST PAYOFF)

**Goal**: The hero augmented-notes UX works end-to-end: user types markdown bullets, hits "End meeting", and within 30 seconds sees their black bullets sitting in a unified document with grey AI bullets carrying `↳ HH:MM` transcript deep-links. Schema migration adds `enriched_doc_json TEXT` so TipTap marks survive restart. A minimal hardcoded `OpenAiCompatClient` (~50 LOC) ships in this phase to unblock the hero — it will be promoted to a trait-bounded client in Phase 5.
**Mode:** mvp
**Depends on**: Phase 3
**Requirements**: NOTES-01, NOTES-02, NOTES-03, NOTES-04, NOTES-05, NOTES-06, NOTES-07, NOTES-08, NOTES-09, NOTES-10, NOTES-11, NOTES-12, NOTES-13, STORE-03, STORE-04, PROMPT-01, PROMPT-02, PROMPT-03, PROMPT-04
**Success Criteria** (what must be TRUE):

  1. User types 5 sparse markdown bullets during a 30-min meeting, hits "End meeting", and within 30 seconds sees a unified document where their bullets stay ink-black (`#211D18`) and AI-added bullets render grey (`#A89F90`) with `↳ HH:MM` lilac dotted-underline deep-links
  2. Clicking a `↳ HH:MM` link opens the transcript panel (if closed) and scrolls to that timestamp; hovering shows a tooltip with the transcript excerpt
  3. Closing Yogurt and reopening preserves the black/grey distinction (verified via `enriched_doc_json` TEXT column round-trip); editing a grey range promotes it to ink-black; black ranges survive a "Re-enhance" click intact
  4. Enhancing state: lilac progress banner with active dot pulse renders "Weaving your notes into the transcript…" with animated progress bar + character-streaming count; AI bullets appear as 1.25s shimmer skeletons, resolve into grey markdown, staggered at 140/340/560/760ms
  5. Each meeting writes both a SQLite row (with `notes_md`, `enriched_md`, `enriched_doc_json`, `transcript_json`) and a `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` file via the single `MarkdownExporter`; bundled `enhance.md` + `chat-system.md` ship in `yogurt-prompts` and reload picks up edits

**Plans**: TBD
**UI hint**: yes

### Phase 5: LLM Client + Settings + Keychain

**Goal**: Phase 4's hardcoded `OpenAiCompatClient` is promoted behind the `LlmClient` trait with `complete_streaming(ChatRequest) → BoxStream<ChatDelta>`. The full settings UI ships (Model / Transcription / Audio / General sidebar) with Keychain-backed API key storage that is eager-loaded at startup to prevent cold-boot hangs. User can paste any OpenAI-compatible base URL + API key + model and the same enhance pipeline runs against it.
**Mode:** mvp
**Depends on**: Phase 4
**Requirements**: LLM-01, LLM-02, LLM-03, SET-01, SET-02, SET-03, SET-04, SET-05, SET-06, SET-07, SET-08, SET-09, SET-10, SET-11
**Success Criteria** (what must be TRUE):

  1. User opens `/settings`, pastes a custom OpenAI-compatible base URL + model + API key (e.g., a self-hosted Ollama endpoint), saves, and the next "Re-enhance" runs against that provider with SSE streaming working end-to-end
  2. Active provider renders as a 1.5px blueberry-bordered card with BASE URL + MODEL in mono and API KEY masked with last-4 + green "✓ stored"; inactive providers stack below as plain rows with "Set active" link; preset chips for Ollama / LM Studio / OpenRouter appear with "+ Add" for custom
  3. Sidebar footer shows green "Local-only · on" pill when no cloud providers active and JetBrains-Mono caption `keys → macOS Keychain` / `data → ~/.yogurt/`
  4. All Keychain secrets are eager-loaded into `Arc<RwLock<Secrets>>` at server startup (within 5s timeout); request handlers never block on `keyring` calls; cold-boot test on a fresh macOS account completes within 5 seconds
  5. Config persists in `~/.yogurt/config.toml`; API keys persist only in macOS Keychain via `keyring` crate (verified via `security find-generic-password -s yogurt`); Audio + General render side-by-side with input device dropdown, port row, "Open browser on start" toggle
  6. `--dev` flag loads `.env.local` (e.g., `MINIMAX_API_KEY` against `https://api.minimaxi.chat/v1`) as a Keychain fallback for fast dev iteration; release builds ignore `.env.local` entirely

**Plans**: TBD
**UI hint**: yes

### Phase 6: In-Meeting Chat

**Goal**: The floating "Ask this meeting…" pill anchored bottom-center expands into a chat window on `⌘K`, streams responses from the LLM client against `chat-system.md` + transcript-so-far as context, and persists chat messages per meeting in SQLite.
**Mode:** mvp
**Depends on**: Phase 5
**Requirements**: CHAT-01, CHAT-02, CHAT-03, CHAT-04, CHAT-05, CHAT-06, CHAT-07
**Success Criteria** (what must be TRUE):

  1. During a meeting, the 480px floating "Ask this meeting…" pill is anchored bottom-center 24px from bottom with `⌘K` keyboard hint badge and purple send arrow; pressing `⌘K` or clicking expands it into the chat window with 260ms `popUp` ease-out
  2. User asks "What's the current topic?", first response chunk streams inline < 2s after send, and the answer references actual transcript content (verified against a meeting where two distinct topics were discussed)
  3. User messages right-aligned blueberry, AI messages left-aligned cream with grey border; chat window shows yogurt swirl logo + "Ask the meeting" header + collapse caret
  4. Chat messages persist across page reload via the `chat_messages` SQLite table; the pill persists into the post-meeting view

**Plans**: TBD
**UI hint**: yes

### Phase 7: Library + Onboarding + States

**Goal**: The library home view (sidebar + date-grouped meeting cards), `/welcome` onboarding flow, and all four empty/error states are shipped. Four research-flagged table-stakes adds are absorbed into this phase: SQLite FTS5 keyword search, copy-markdown / reveal-in-finder, inline-editable meeting title, and delete-card UI.
**Mode:** mvp
**Depends on**: Phase 6
**Requirements**: LIB-01, LIB-02, LIB-03, LIB-04, LIB-05, LIB-06, LIB-07, LIB-08, LIB-09, LIB-10, LIB-11, LIB-12, ONB-01, ONB-02, ONB-03, ONB-04, ONB-05, ONB-06, ONB-07, ONB-08, STATE-01, STATE-02, STATE-03, STATE-04
**Success Criteria** (what must be TRUE):

  1. Default page at `localhost:7878` renders the library: 212px left sidebar (logo + "+ New meeting" blueberry button + "All meetings" / "Starred" nav + footer pill + `⚙ Settings`), main pane with Instrument-Serif greeting + meta caption + search pill, date-grouped meeting cards (`TODAY` / `YESTERDAY`) with 42px tinted avatars
  2. Typing into the search pill queries SQLite FTS5 across notes + transcripts and shows matching meetings; clicking into any meeting opens its view; per-meeting cards expose "Copy markdown", "Reveal in Finder", inline-editable title, and delete affordances
  3. Fresh user lands at `/welcome`, sees the two-column onboarding (left: logo + Instrument-Serif welcome + terminal mockup; right: ONE-TIME SETUP with 3 step cards), completes Screen Recording → Connect model → Pick transcription in order, and clicks "Take me to my meetings →" to reach the library
  4. All four states render correctly: empty library (floating logo + "No meetings yet" + ⌘N CTA), permission-not-granted (strawberry alert + 3-step recovery + "Open System Settings" + "Restart Yogurt"), enhancing (lilac progress banner), whisper.cpp model download modal (matcha progress + Cancel / Run-in-background)

**Plans**: TBD
**UI hint**: yes

### Phase 8: Local STT (whisper.cpp)

**Goal**: A `whisper.cpp` adapter implementing `SttEngine` ships behind the `local-stt` Cargo feature flag, with VAD-driven chunking and dual `whisper_state` (preview + settled) on Metal. Models download on first use to `~/.yogurt/models` with SHA256 verification. Benchmarked on M1 Air, not just M3 Max.
**Mode:** mvp
**Depends on**: Phase 7
**Requirements**: LOCAL-01, LOCAL-02, LOCAL-03, LOCAL-04, LOCAL-05
**Success Criteria** (what must be TRUE):

  1. With `--features local-stt` enabled, user can select Local in Settings → Transcription, pick `small.en`, see the model-download modal with bytes/speed/ETA, and run a meeting end-to-end fully offline (no network calls verified via Little Snitch / equivalent)
  2. On an M1 Air, `small.en` local STT produces transcript with < 3s lag and does not exhibit growing latency drift over a 30-min meeting
  3. All `whisper-rs` calls run on `tokio::task::spawn_blocking` (verified by confirming axum routes and WS sends remain responsive during inference); dual `whisper_state` preview/settled pattern reduces user-visible latency
  4. Model files in `~/.yogurt/models` are SHA256-verified against a hardcoded list before load; corrupted/incomplete downloads trigger re-download instead of crash
  5. Contributors without `local-stt` feature can `cargo build` in under 30 seconds (whisper.cpp CMake build does not run)

**Plans**: TBD
**UI hint**: no

### Phase 9: Distribution Polish

**Goal**: GitHub Actions release workflow on `v*` tag push produces notarized per-arch tarballs (universal binary optional), opens a Homebrew tap PR with updated SHA256s, runs `cargo publish`, and ships a `yogurt doctor` subcommand for TCC reset / model re-download / port diagnostics. The end-to-end install path `brew install yogurt && yogurt start` works for a non-technical user on a fresh Mac.
**Mode:** mvp
**Depends on**: Phase 8
**Requirements**: DIST-01, DIST-02, DIST-03, DIST-04, DIST-05, DIST-06, DIST-07, DIST-08, DIST-09, DIST-10
**Success Criteria** (what must be TRUE):

  1. Pushing a `v*` tag triggers the matrix CI release: builds `aarch64-apple-darwin` and `x86_64-apple-darwin` tarballs in parallel, notarizes each via `notarytool` + `staple` with a stable Developer ID, pins bundle ID `ai.yogurt.app`, and attaches both tarballs to the GitHub Release
  2. After the GitHub Release URL is visible, CI opens a PR against the `homebrew-yogurt` tap with updated SHA256s (sleeps/polls until release is reachable to avoid 404s); `cargo publish` for the binary crate runs in the strict order tag → release binaries → cargo publish → tap PR
  3. On a clean macOS install, `brew install yogurt && yogurt start` opens the browser at `localhost:7878`, the binary passes `spctl -a -vv` ("accepted"), and the Screen Recording prompt fires on first record (no Gatekeeper "damaged" error)
  4. `yogurt doctor` subcommand runs TCC reset (`tccutil reset ScreenCapture ai.yogurt.app`), reports port-conflict diagnostics, and lets the user re-download whisper models
  5. README documents install, dev workflow (`cargo run --dev` + `pnpm dev`), configuration, threat model (localhost trust assumption), and the 5-minute contributor onboarding claim is verified on a clean Mac

**Plans**: TBD
**UI hint**: no

## Progress

**Execution Order:**
Phases execute in numeric order: 0 → 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 0. Skeleton & Foundations | 3/3 | Complete   | 2026-06-25 |
| 1. Design System | 0/TBD | Not started | - |
| 2. Audio Capture | 0/TBD | Not started | - |
| 3. Cloud STT + Live Transcript | 0/TBD | Not started | - |
| 4. Augmented Notes Hero | 0/TBD | Not started | - |
| 5. LLM Client + Settings + Keychain | 0/TBD | Not started | - |
| 6. In-Meeting Chat | 0/TBD | Not started | - |
| 7. Library + Onboarding + States | 0/TBD | Not started | - |
| 8. Local STT (whisper.cpp) | 0/TBD | Not started | - |
| 9. Distribution Polish | 0/TBD | Not started | - |
