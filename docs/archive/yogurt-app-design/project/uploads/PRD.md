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
| Q4 | **v1 Scope = 9 IN features, 9 deferred** | See §5 / §6. |
| Q5 | **Stack = Rust (axum + tokio) for backend, React + Vite + TipTap for frontend, embedded web assets** | Single static binary distribution is the killer feature for a local-first OSS tool. Rust handles audio FFI, WebSocket fan-out, and whisper.cpp embedding without paying the GIL or Node memory tax. |
| Q6 | **LLM provider = OpenAI-compatible only** | One adapter covers Minimax, OpenAI, Ollama, LM Studio, OpenRouter, Groq, vLLM, llama.cpp server, Together, Fireworks, and anything else with an OpenAI-compatible endpoint. Anthropic and Google models reachable via OpenRouter. |
| Q7 | **Audio retention = delete after transcription** | Granola model. Smallest privacy footprint, smallest disk usage. If a user wants to keep audio, they can flip a per-meeting toggle in v1.1 (trivial schema addition). |
| Q8 | **Distribution = Homebrew + Cargo + GitHub Release binaries** | Standard Rust CLI distribution. One source of truth (the GitHub release), three install channels. Source build via `git clone` is fully supported and documented. |

## 5. v1 feature list

The nine features in v1 scope, with concrete acceptance criteria.

### 5.1 Record meeting (mic + system audio)
- User starts a meeting by clicking "New meeting" in the browser UI.
- First-run: macOS Screen Recording permission dialog appears (triggered by the Rust binary via ScreenCaptureKit init). Yogurt waits for grant.
- Captures two mono 16 kHz / 16-bit PCM streams: `mic` (default input device) and `system` (loopback via ScreenCaptureKit).
- Streams are pushed to the in-process STT engine via a Tokio broadcast channel.
- **Done when:** user can record a 30-minute meeting on macOS 13+, both channels are captured cleanly, recording stops cleanly on "End meeting."

### 5.2 Live transcript panel
- Right-side panel in the meeting UI shows scrolling transcript as it arrives.
- Channel-labeled: mic transcript labeled "Me" (black), system audio transcript labeled "Them" (grey).
- Auto-scrolls to bottom unless the user has scrolled up (then sticky to position).
- Each transcript line has a millisecond timestamp from meeting start.
- **Done when:** transcript appears with < 2s lag using Deepgram, <3s lag using local `whisper.cpp small.en`.

### 5.3 Augmented markdown notes editor
- Left side of meeting UI: a markdown editor (TipTap-based) for the user's notes.
- During the meeting: pure markdown editing, no AI involvement.
- On "End meeting": LLM is called with `(user notes + full transcript + selected template prompt)`. Returns enriched markdown.
- **Display contract:** user-authored content renders black; AI-added content renders in a muted grey; the diff is computed structurally (by markdown AST), not character-by-character.
- **Edit contract:** editing a grey range promotes it to black (it is now "the user's"). Black ranges are never overwritten by re-enhancement.
- AI-added bullets carry a `data-transcript-ts` attribute referencing the millisecond timestamp; hovering shows a tooltip with the transcript excerpt; clicking jumps the transcript panel to that moment.
- **Done when:** user can type 5 sparse bullets during a 30-min meeting, hit "End meeting," and within 30s see a clean enriched-notes document where their bullets are black, AI bullets are grey, and clicking a grey bullet jumps the transcript.

### 5.4 In-meeting AI chat
- Collapsible right-side chat panel (separate from transcript), available during the meeting.
- Each user message is sent to the LLM with the transcript-so-far as context. System prompt: "You are watching this meeting live. Answer the user's question using only the transcript content available so far."
- Streaming response renders inline.
- **Common queries the prompt is tuned for:** "What's the current topic?", "What did <speaker> say about X?", "Summarize the last 5 minutes."
- **Done when:** chat response begins streaming < 2s after sending, and references content from the transcript accurately.

### 5.5 Post-meeting summary (regenerable)
- After a meeting ends and augmented notes are computed, a "Summary" tab appears.
- User can pick a template (1:1, standup, interview debrief, generic), click "Generate," and the LLM produces a full-document summary.
- Regenerate button re-runs with a different template at any time without losing prior runs (each is saved as a version).
- **Done when:** user can switch between templates and re-generate within 5s for a 30-min meeting.

### 5.6 Starter templates
Four prompt templates ship in `crates/yogurt-prompts/templates/`:
- `generic.md` — "Summarize this meeting clearly with key decisions, action items, and open questions."
- `one-on-one.md` — "Format as a 1:1: what's going well, what's blocking, action items for both parties."
- `standup.md` — "What did each person discuss as 'in progress' or 'blocked'? List action items."
- `interview.md` — "Capture the candidate's responses, signals on each interview rubric dimension, and a recommend/no-recommend judgment."
- Each is plain markdown with `{{NOTES}}` and `{{TRANSCRIPT}}` placeholders.
- Power users can edit these in place; reloading the binary picks up changes.

### 5.7 Settings UI
- Settings page at `/settings`. Stores config in `~/.yogurt/config.toml`.
- **LLM section:** list of saved providers (name + base URL + API key + default model). User can add/edit/delete. Pick one as active. Ships with greyed-out preset entries for Minimax, OpenAI, Ollama, LM Studio, OpenRouter that the user can clone-and-fill.
- **STT section:** radio: Cloud (Deepgram | AssemblyAI | Groq Whisper) with API key field; or Local (`whisper.cpp` with model picker: tiny.en / small.en / medium.en / large-v3). First-time local pick triggers model download.
- **Audio section:** input device picker (defaults to system default mic).
- **General:** port (default 7878), data directory (default `~/.yogurt/`), auto-open browser on `yogurt start` (default true).
- API keys are stored in the macOS Keychain via `keyring` crate, not in plaintext config.

### 5.8 Local storage (SQLite + markdown export)
- All persistent data in `~/.yogurt/db.sqlite` (rusqlite).
- Each meeting also written to `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` as the canonical exportable file (front-matter for metadata, then enriched notes, then transcript appendix).
- Markdown files are the source of truth for "user wants to grep their meetings" — SQLite is the structured/queryable mirror.
- Schema sketch (§8).

### 5.9 macOS only (Apple Silicon primary, Intel best-effort)
- v1 ships universal binary (arm64 + x86_64).
- Apple Silicon: tested on M1 / M1 Pro / M3 Max baseline.
- Intel: builds and runs; `whisper.cpp` performance limited to `small.en` real-time. Documented as best-effort.
- macOS 13+ required (ScreenCaptureKit).
- Windows/Linux: out of scope. Architecture isolates the platform code in `crates/yogurt-audio`, so future ports are additive.

## 6. v2+ deferred features

For clarity; not built in v1. Order suggests priority.

1. Calendar integration (Google + Outlook OAuth, auto-detect upcoming meetings).
2. Cross-meeting chat / semantic search (embeddings + sqlite-vss or LanceDB).
3. Slack / Notion / Linear / Granola-style integration exports.
4. Custom template authoring UI.
5. Per-speaker diarization (pyannote via a sidecar Python service, opt-in).
6. Menu-bar / global-hotkey UI (Tauri wrap of the existing backend).
7. Windows + Linux support.
8. MCP server.
9. Optional sync / multi-device (encrypted, user-hosted only — no Yogurt cloud).

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
│   └── yogurt-prompts/            # bundled templates + system prompts
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
  template        TEXT,                     -- which template was used for enhance
  notes_md        TEXT NOT NULL DEFAULT '', -- raw markdown, source of truth
  enriched_md     TEXT,                     -- post-enhance markdown (black+grey)
  transcript_json TEXT NOT NULL DEFAULT '[]'  -- [{ts_ms, channel, text}, ...]
);

CREATE TABLE summaries (
  id          TEXT PRIMARY KEY,
  meeting_id  TEXT NOT NULL REFERENCES meetings(id),
  template    TEXT NOT NULL,
  body_md     TEXT NOT NULL,
  generated_at INTEGER NOT NULL
);

CREATE TABLE chat_messages (
  id          TEXT PRIMARY KEY,
  meeting_id  TEXT NOT NULL REFERENCES meetings(id),
  role        TEXT NOT NULL,                -- 'user' | 'assistant'
  content     TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE INDEX idx_meetings_started ON meetings(started_at DESC);
CREATE INDEX idx_summaries_meeting ON summaries(meeting_id);
CREATE INDEX idx_chat_meeting ON chat_messages(meeting_id, created_at);
```

Markdown export: `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` is written on every `notes_md` or `enriched_md` mutation. The markdown file's YAML front-matter mirrors the row.

## 10. API surface

### REST (axum)

| Method | Path | Purpose |
|---|---|---|
| GET    | `/api/meetings`                          | List meetings, newest first |
| POST   | `/api/meetings`                          | Create new meeting (returns id) |
| GET    | `/api/meetings/:id`                      | Full meeting (notes, transcript, summaries) |
| PATCH  | `/api/meetings/:id`                      | Update title or notes_md |
| POST   | `/api/meetings/:id/start`                | Begin recording (triggers audio capture + STT) |
| POST   | `/api/meetings/:id/stop`                 | End recording, kick off enhance |
| POST   | `/api/meetings/:id/enhance`              | Re-run augmented-notes enhance |
| POST   | `/api/meetings/:id/summarize`            | Generate summary with given template |
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
git clone https://github.com/<org>/yogurt.git
cd yogurt

# one-time setup
brew install rust pnpm
pnpm --dir web install

# dev: two terminals
cargo run -p yogurt-cli -- start --dev   # backend at :7878, no embedded assets
pnpm --dir web dev                       # vite at :5173, proxies /api + /ws to :7878

# release build (single binary with embedded assets)
pnpm --dir web build                     # outputs web/dist
cargo build --release                    # rust-embed picks up web/dist
./target/release/yogurt start
```

`--dev` flag tells the server to proxy `/` to the Vite dev server instead of serving embedded assets. In release builds, `web/dist` is compiled into the binary.

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
| **1. Audio capture**| `yogurt-audio` crate: ScreenCaptureKit init, mic capture, two channels piped to a Tokio broadcast. Permission prompt UX. | 2d |
| **2. Cloud STT**    | `yogurt-stt` trait + Deepgram streaming adapter. Live transcript panel renders in the browser via WS. | 2d |
| **3. Augmented notes** | TipTap setup with custom marks for `ai-grey` and `transcript-link`. Markdown round-trip. Enhance endpoint that calls the LLM and produces the merged document. | 3-4d |
| **4. LLM client**   | `yogurt-llm` crate, OpenAI-compat client, settings UI for provider management, Keychain storage. | 1-2d |
| **5. In-meeting chat** | Chat panel + streaming chat endpoint. Prompt-tune for transcript-aware Q&A. | 1d |
| **6. Local STT**    | `whisper.cpp` adapter via `whisper-rs`. Model download UX. Streaming chunked decode with VAD. | 2d |
| **7. Templates + summary** | Summary tab, 4 starter templates, regenerate flow. | 1d |
| **8. Polish + distribution** | Markdown export, README, Homebrew tap, GH Actions release workflow, universal binary. | 2-3d |

Total rough estimate: **~15 working days** (3 weeks at one engineer focused). Phases 0-2 are the highest-risk because they validate the audio + transcription path; if those work, the rest is straightforward.

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

1. A new user on an M-series Mac can run `brew install yogurt && yogurt start`, grant Screen Recording permission, paste a Minimax API key in Settings, and record + transcribe + summarize a real meeting end-to-end.
2. The black-user / grey-AI in-place notes merge is visually indistinguishable in feel from Granola's.
3. A privacy-focused user can switch to `whisper.cpp` + a local Ollama endpoint and the full pipeline still works with no internet.
4. A developer can `git clone && cargo run` and have a working dev environment within 5 minutes.

## 15. Open questions for future rounds

Deliberately not blocking v1 design, but worth surfacing:

- **Naming.** Is "Yogurt" the keeper, or a working name? (Granola → Yogurt is fine playfully, but the repo will be public.)
- **License.** Default assumption: MIT, matching Meetily and Hyprnote. Confirm.
- **Org / repo location.** Personal GitHub or a new org?
- **Telemetry policy for the future.** Even fully opt-in, do we want any phone-home (for crash reports, e.g. Sentry self-hosted)?

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
- **Augmented-notes enhance** — the post-meeting LLM call that takes `(user notes + transcript + template prompt)` and returns the merged markdown document.
