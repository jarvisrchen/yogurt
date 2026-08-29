# Architecture Research

**Domain:** Local-first macOS meeting copilot (single Rust binary + browser UI)
**Researched:** 2026-06-25
**Confidence:** HIGH for the overall topology and component boundaries (validated against Meetily, Hyprnote, axum/tokio idioms, and `screencapturekit-rs` docs); MEDIUM for fan-out scaling specifics; LOW only on `screencapturekit` audio-only-loopback edge cases (requires a Phase 2 spike — already flagged in PRD §13).

---

## Verdict on the Proposed Architecture (PRD §7 + §8)

**The proposed architecture is sound and aligned with how comparable tools are actually built in 2026.** No structural changes recommended. The 8-crate split, single-process model, in-process audio, embedded-assets distribution, and trait-bounded STT/LLM swap points all match patterns proven by Meetily, Hyprnote, the axum chat example, and the `whisper-cpp-plus` streaming layout.

What this document adds on top of PRD §7/§8:

1. Concrete component-boundary rules ("X talks to Y, but Y never talks back to X").
2. Three explicit data-flow pipelines (audio, notes, chat) drawn end-to-end.
3. A build-order with hard dependencies between crates — used directly by the roadmap to order phases.
4. Trait shapes and where exactly the swap-out boundary should sit so a future "AssemblyAI" or "Claude-via-Anthropic-native" provider doesn't reshape the codebase.
5. Risks with concrete mitigations (some delta on PRD §13).

---

## Standard Architecture (in this domain)

### The "single Rust process + browser UI + embedded assets" pattern

Meetily and Hyprnote both ship as Tauri apps with a Rust core that owns audio capture, STT inference, SQLite, and LLM calls; the frontend is a JS framework (Next.js for Meetily, TanStack Start for Hyprnote) loaded by Tauri's WebView. Yogurt's variation — axum-served browser UI at `localhost:7878` instead of a Tauri WebView — is **strictly simpler**: no Tauri build chain, no IPC bridge, no separate menu-bar process. The cost is no native menu bar / no dock icon, which PRD §2 explicitly accepts as a v2 path.

The dominant pattern across all three:

```
┌─────────────────────────────────────────────────────────────┐
│                      Process Boundary                        │
│                                                              │
│  ┌─────────────────────┐         ┌───────────────────────┐   │
│  │     UI Surface      │   HTTP  │   HTTP/WS Server      │   │
│  │  (Browser / WebView)│◄──+WS──►│  (axum / Tauri IPC)   │   │
│  └─────────────────────┘         └──────────┬────────────┘   │
│                                             │                │
│                              ┌──────────────┼─────────────┐  │
│                              │              │             │  │
│                              ▼              ▼             ▼  │
│                       ┌──────────┐   ┌──────────┐  ┌────────┐│
│                       │  Audio   │   │   STT    │  │  LLM   ││
│                       │ (SCK+mic)│──►│  engine  │  │ client ││
│                       └──────────┘   └────┬─────┘  └───┬────┘│
│                                           │            │     │
│                                           ▼            ▼     │
│                                      ┌─────────────────────┐ │
│                                      │  Persistence layer  │ │
│                                      │  (SQLite + .md)     │ │
│                                      └─────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

Yogurt's PRD §7 diagram is this pattern.

### Component Responsibilities (Yogurt-specific)

| Crate                | Responsibility                                             | Talks to                       | Never talks to                       |
|----------------------|------------------------------------------------------------|--------------------------------|--------------------------------------|
| `yogurt-cli`         | Binary entry, arg parsing (`start`, `config`), browser-open hook, sets up tracing | `yogurt-server` (run)          | Audio, STT, LLM, DB directly         |
| `yogurt-server`      | axum routes, WebSocket handler, embedded asset serve, state container (`AppState`) | All other crates               | ScreenCaptureKit FFI directly        |
| `yogurt-audio`       | SCK init, permission check, mic capture, dual-channel PCM emit on `broadcast::Sender<AudioFrame>` | None upward (pure producer)    | STT, DB, server (knows nothing of them) |
| `yogurt-stt`         | `SttEngine` trait + Deepgram, AssemblyAI, whisper.cpp impls | `yogurt-audio` (subscribes to broadcast), emits `TranscriptEvent` on its own broadcast | LLM, DB, server routes               |
| `yogurt-llm`         | OpenAI-compat client; `LlmClient` trait; streaming chat + one-shot `enhance` | Called by `yogurt-server`/`yogurt-notes` | Audio, STT directly                  |
| `yogurt-db`          | rusqlite pool, migrations, typed repos (`MeetingsRepo`, `ChatRepo`), markdown-file mirror writer | Called by `yogurt-server` and `yogurt-notes` | Audio, STT, LLM directly             |
| `yogurt-notes`       | Markdown ↔ AST, augmented-merge logic (mark `aiGrey` ranges, `transcriptTs` injection), enhance orchestration | `yogurt-llm` (calls), `yogurt-prompts` (reads), `yogurt-db` (writes) | Audio, STT directly                  |
| `yogurt-prompts`     | Embeds `enhance.md` + `chat-system.md` via `include_str!` | Read by `yogurt-notes` and `yogurt-server` chat route | Everything else                      |
| `web/` (React)       | TipTap editor with `aiGrey` mark, library, settings, chat pill, transcript dock | `yogurt-server` over HTTP/WS only | The Rust crates directly             |

**Boundary rules made explicit:**

- **Audio is a pure source.** `yogurt-audio` knows nothing about who consumes frames. It hands out a `broadcast::Receiver<AudioFrame>` and that's the entire upward API. Hyprnote and Meetily both arrived at this shape independently. This is what makes Windows/Linux ports additive (PRD §8 claim is valid).
- **STT subscribes to audio, publishes transcripts.** Two unrelated broadcast channels (`audio_tx` and `transcript_tx`), one downstream of the other. Don't tempt yourself to fold them into one channel with an enum — the lifecycle differs (audio is firehose, transcript is rate-limited by STT).
- **LLM is called, never calls.** `yogurt-llm` exposes a `LlmClient` trait. The server route or `yogurt-notes` calls it. The LLM crate never reaches into DB or audio.
- **`yogurt-server` is the only crate that knows about WebSockets.** All other crates speak in channels and traits. This means tests can drive the pipeline without ever opening a socket.
- **`yogurt-cli` is a thin shell.** It exists so `yogurt start`, `yogurt config`, `yogurt --version` work without dragging axum into the dependency graph of small utility subcommands. Don't put business logic here.

---

## Three Pipelines, Drawn

The PRD describes the components but doesn't draw the runtime data flow. Roadmap phasing depends on understanding these three pipelines as distinct:

### Pipeline 1: Audio → Live Transcript (firehose, ~real-time)

```
ScreenCaptureKit ─┐
                  ├──► yogurt-audio ──► broadcast::Sender<AudioFrame>
CoreAudio (mic) ──┘                       │
                                          ├──► STT task (mic channel)
                                          │      └──► Deepgram WS / whisper.cpp
                                          │                │
                                          └──► STT task (system channel)
                                                 └──► Deepgram WS / whisper.cpp
                                                               │
                                                               ▼
                                                broadcast::Sender<TranscriptEvent>
                                                               │
                                  ┌────────────────────────────┼─────────┐
                                  ▼                            ▼         ▼
                          WS subscribers              transcript_json  (future:
                          (browser clients)           append to DB     diarization,
                                                                       search index)
```

Why two STT tasks per meeting (one per channel): channel labeling ("Me" / "Them" in PRD §5.2) requires the STT to know which stream it's transcribing. Multiplexing into one task and demuxing post-transcript is fragile because partial-result ordering across channels is undefined.

**Key property:** the broadcast channels are the contract. `yogurt-stt` doesn't import from `yogurt-server`; the WS handler subscribes to the transcript broadcast in the server crate.

### Pipeline 2: Notes ⇄ Enhance (interactive + one-shot)

```
Browser (TipTap)
   │
   │ debounced notes_edit over WS
   ▼
yogurt-server route ──► yogurt-db.MeetingsRepo.update_notes_md()
                                 │
                                 └──► writes .md file mirror
                                       (~/.yogurt/notes/<slug>.md)

On "End meeting" / Re-enhance:
   │
   ▼
yogurt-server POST /api/meetings/:id/enhance
   │
   ▼
yogurt-notes.enhance(meeting)
   ├── read notes_md + transcript_json from yogurt-db
   ├── read yogurt-prompts::ENHANCE_MD (compile-time include_str!)
   ├── render prompt with {{NOTES}} + {{TRANSCRIPT}}
   ├── call yogurt-llm.complete_streaming(...)
   │     └── stream chunks ──► WS broadcast (enhance_progress)
   ├── parse final markdown → AST diff → mark aiGrey runs → inject transcriptTs
   └── yogurt-db.MeetingsRepo.update_enriched_md(...)
                                 │
                                 └──► writes .md file mirror
```

The `aiGrey` diff is computed structurally over the markdown AST (PRD §5.3 + §16.7), not over plain text. This lives in `yogurt-notes` and is the single most important piece of logic in the whole codebase — it is the hero feature.

**Key property:** `yogurt-notes` is the orchestrator for enhance. The HTTP route is dumb; it kicks off a `tokio::spawn` and returns 202. Progress comes back over the existing meeting WebSocket. This pattern matches how Meetily structures its summarization endpoint.

### Pipeline 3: In-meeting Chat (live, transcript-aware)

```
Browser sends `chat_send` over WS  (or POST /api/meetings/:id/chat)
   │
   ▼
yogurt-server chat route
   ├── read transcript_so_far from yogurt-db (in-memory cache OK)
   ├── read yogurt-prompts::CHAT_SYSTEM_MD
   ├── load prior chat_messages
   ├── yogurt-llm.complete_streaming(system + history + transcript + new msg)
   │     └── stream chunks ──► WS `chat_chunk` to the requesting client
   └── on final: yogurt-db.ChatRepo.insert(user_msg, assistant_msg)
```

**Key property:** chat is independent from the audio pipeline at the call-graph level. The only thing it shares with audio is *reading* the transcript that the audio pipeline has been writing. This means chat can be built and tested without audio capture working — useful for the build order.

---

## Trait Boundaries (the swap-out points)

These are the only two places the codebase tolerates polymorphism. Getting them right means "add Groq" / "add AssemblyAI" / "swap to Anthropic-native" is a 1-file change.

### `SttEngine` trait (in `yogurt-stt`)

The trait should sit *above* both cloud-streaming and local-batch engines. The right shape:

```rust
#[async_trait::async_trait]
pub trait SttEngine: Send + Sync {
    /// Start a session for one channel of a meeting.
    /// Returns immediately; transcripts flow out on the returned stream.
    async fn open_session(
        &self,
        channel: Channel,                          // Mic | System
    ) -> Result<Box<dyn SttSession>, SttError>;
}

#[async_trait::async_trait]
pub trait SttSession: Send {
    /// Push a PCM frame in. Cheap; engine buffers internally.
    async fn push_frame(&mut self, frame: AudioFrame) -> Result<(), SttError>;

    /// Drain transcript events (partial + final). One stream per session.
    fn events(&mut self) -> &mut (dyn Stream<Item = TranscriptEvent> + Send + Unpin);

    /// Flush any final partials and close the session cleanly.
    async fn close(self: Box<Self>) -> Result<(), SttError>;
}
```

**Why this shape:**

- Deepgram is a WebSocket — `push_frame` writes to its socket, `events()` reads from it. Natural fit.
- whisper.cpp is local batch with VAD-driven chunking (per `whisper-cpp-plus` docs). `push_frame` appends to a ring buffer; the VAD task emits to `events()` when a segment closes. Same trait, different internals.
- AssemblyAI / Groq are again WebSocket cloud STT — same shape as Deepgram.
- The trait carries `Channel` so each engine knows whether it's labeling "Me" or "Them".

**What NOT to put in the trait:**

- Don't include "rewind 30 seconds" or "get historical transcript." Storage is `yogurt-db`'s job. The STT trait is purely streaming.
- Don't include model selection. That's config the constructor reads. `Deepgram::new(api_key, model: "nova-3")`.

### `LlmClient` trait (in `yogurt-llm`)

```rust
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Streaming chat completion. Returns a stream of content deltas.
    async fn complete_streaming(
        &self,
        req: ChatRequest,        // { system, messages, model, temperature, ... }
    ) -> Result<BoxStream<'static, Result<ChatDelta, LlmError>>, LlmError>;
}
```

**Why one trait and one method:**

The "OpenAI-compatible only" decision in PRD §4 Q6 means there is functionally one implementation — `OpenAiCompatClient` parameterized by `(base_url, api_key, model)`. The trait exists so tests can stub it and so a future Anthropic-native client (if OpenRouter ever feels too lossy) is additive.

**Don't model "enhance" and "chat" as separate methods.** Both are chat completions. The differences (system prompt, message structure) live in `yogurt-notes` and the chat route — they prepare the `ChatRequest`. The LLM crate just streams.

---

## Build Order (the answer to "what blocks what")

This is the single most important thing for roadmap construction. The crates have a strict dependency DAG:

```
                yogurt-prompts (just files, blocks nothing)
                       │
                       ▼
yogurt-audio ──► yogurt-stt ──┐
                              │
yogurt-db ────────────────────┼──► yogurt-notes ──► yogurt-llm
                              │           │              │
                              └───────────┴──────────────┘
                                          │
                                          ▼
                                   yogurt-server
                                          │
                                          ▼
                                       yogurt-cli
                                          │
                                          ▼
                                       web/ UI
```

### Recommended phase ordering (mirrors PRD §12, with rationale)

1. **Skeleton + workspace** (matches PRD Phase 0). Empty crates, `cargo build` passes, `yogurt start` serves "hello." Validates the embed-assets toolchain *before* it has to embed anything real.

2. **Design system in `/web`** (matches PRD Phase 1). This is critical and the PRD orders it correctly. Reason: every screen that comes later (transcript dock, enhancing skeleton, settings cards, library, onboarding) is composed from these primitives. Building screens *first* and then "extracting" a design system later is the classic mistake. Tokens + Button/Badge/Card/Pill/MockupWindow + motion utility classes — then screens.

3. **`yogurt-audio` capture + permission UX** (matches PRD Phase 2). This is the hardest and riskiest crate (SCK loopback maturity is the open question — see Risks below). It blocks STT entirely. Get a working `AudioFrame` stream out of two channels before touching STT. Acceptance: write 60 seconds of dual-channel PCM to a WAV file and verify it sounds right.

4. **`yogurt-stt` Deepgram adapter + live transcript** (matches PRD Phase 3). Cloud STT first because (a) Deepgram's streaming partials are the gold-standard UX target and (b) it doesn't depend on bundling/loading models. Live transcript dock UI lands here too — it's the first place the audio→STT→WS path is wired end-to-end.

5. **`yogurt-db` + minimal CRUD + `yogurt-notes` AST plumbing + augmented-notes hero** (matches PRD Phase 4). This is the hero feature and the highest-payoff phase. Needs DB persistence so notes survive a reload. Needs the AST diff logic. Needs an LLM call for enhance — so a stub `yogurt-llm` (just one impl, no settings UI) goes in here too. Don't ship the settings UI yet; hardcode the provider in dev.

6. **`yogurt-llm` real client + settings UI + Keychain** (matches PRD Phase 5). Once enhance works against a hardcoded provider, generalize the LLM client and build the settings page. Order matters: prove the LLM contract is right *before* building UI around it.

7. **In-meeting chat** (matches PRD Phase 6). Trivially additive after Phase 5 — same LLM client, different prompt, different route.

8. **Library + onboarding + empty/error states** (matches PRD Phase 7). These compose existing components from the design system + existing API endpoints. Pure UI work.

9. **`yogurt-stt` whisper.cpp adapter** (matches PRD Phase 8). Local STT comes last because it's an additive trait impl. Doing it earlier delays the moment you can prove the full happy path works.

10. **Distribution polish** (matches PRD Phase 9).

**The PRD's phase order is correct.** This is independent validation. The one tension to watch: Phase 4 (augmented notes) needs *some* LLM client to call. The PRD's Phase 5 ships the LLM crate. Reading both carefully: Phase 4 should ship a minimal hardcoded OpenAI-compat client (~50 lines) and Phase 5 promotes it to a configurable, trait-bounded, settings-managed one. That's how I'd interpret it; worth confirming in roadmap planning.

---

## Architectural Patterns to Follow

### Pattern 1: Channels as crate boundaries

**What:** When crate A produces data for crate B, A exposes a `broadcast::Sender<T>` (or hands B a `broadcast::Receiver<T>`); B subscribes. The two crates never name each other's types beyond `T`.

**Why:** Lets you test `yogurt-stt` by sending fake `AudioFrame`s into a broadcast in a unit test. Lets you swap `yogurt-audio` for `MockAudioSource` in integration tests. Matches the axum chat example's idiom exactly.

**Where in Yogurt:**

- `audio_tx: broadcast::Sender<AudioFrame>` — owned by `yogurt-server` (in `AppState`), filled by `yogurt-audio`.
- `transcript_tx: broadcast::Sender<TranscriptEvent>` — owned by `AppState`, filled by per-meeting STT tasks, consumed by WS handler.

### Pattern 2: Per-meeting task supervisor

**What:** When a meeting starts, spawn a supervisor task that owns the audio capture handle, two STT sessions, and the broadcast senders. When the meeting ends, the supervisor closes the STT sessions and drops the audio handle. All cleanup is structural.

**Why:** Without a supervisor, you'll leak audio capture handles when the user closes the browser tab without clicking "End meeting" (the WS will disconnect but the audio task keeps writing into a broadcast nobody reads). The supervisor pattern means meeting lifecycle == task lifecycle.

```rust
let meeting_handle = MeetingSupervisor::start(meeting_id, app_state.clone()).await?;
// Stored in AppState::active_meetings: DashMap<MeetingId, MeetingSupervisor>
// On stop or shutdown, .stop() closes everything in order.
```

### Pattern 3: WebSocket = read-only projection of internal state

**What:** The WS handler subscribes to broadcast channels and forwards events to the client. It never *decides* anything. All decisions (start meeting, stop meeting, send chat) come in over HTTP POST and mutate `AppState`; the WS is the change feed.

**Why:** Keeps the WS handler tiny and testable. Means the HTTP REST API is the canonical surface (PRD §10 already takes this shape). Browser tab close = WS close = nothing breaks because state lives in `AppState`.

### Pattern 4: `rust-embed` with dev/prod split

**What:** In release builds, `web/dist` is compiled into the binary via `rust-embed`. In `cargo run -- start --dev`, the server proxies `/` to `http://localhost:5173` (Vite). PRD §11 already calls this out.

**Why:** This is the standard pattern (see `axum-embed`, `static-serve`, `axum-embed-files`). Don't reinvent. Use one of the existing crates; `axum-embed` is the most direct fit with axum's routing.

### Pattern 5: Markdown as source of truth, SQLite as queryable mirror

**What:** Every notes/enriched mutation writes both the SQLite row *and* the markdown file in `~/.yogurt/notes/`. PRD §5.7 / §9 already specifies this.

**Why:** Users can `grep` their meetings even if the app is uninstalled. Users can `git init ~/.yogurt/notes && git commit` if they want versioning. This is a real differentiator vs. Granola.

**Implementation hint:** make `yogurt-db::MeetingsRepo::update_notes_md` and `update_enriched_md` the only paths that write either; have them call into a `MarkdownExporter` helper. Don't let two different code paths write the .md file.

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: One trait for STT + LLM
**Why bad:** They have nothing in common. The "Provider" abstraction is a junk-drawer trait by another name. PRD already keeps them separate — keep it that way.

### Anti-Pattern 2: A `Provider` trait that erases "is this local or cloud?"
**Why bad:** The UI needs to know. The "Local-only · on" pill (PRD §5.6, §5.9) is a privacy contract surface. Each engine should expose `fn is_local(&self) -> bool` (or sit behind a config that records it). Hiding it behind a trait makes the privacy posture invisible to the code that surfaces it to the user.

### Anti-Pattern 3: WebSocket message routing via stringly-typed `type` field with no enum
**Why bad:** PRD §10 already lists the message types. Define a `WsMessageOut` and `WsMessageIn` enum with `#[serde(tag = "type")]` in `yogurt-server` and use that everywhere. The browser side has the equivalent TS union. No `if msg.type == "transcript"` branching.

### Anti-Pattern 4: Putting business logic in `yogurt-cli`
**Why bad:** CLI argument parsing should be all `yogurt-cli` does. The moment you put "start a meeting" logic in there, you've made the same logic untestable from the server.

### Anti-Pattern 5: Tokio broadcast `audio_tx` with capacity=1
**Why bad:** Broadcast channels emit `Lagged` errors when slow consumers can't keep up, dropping audio frames. If you set capacity too low, the STT subscriber will lose audio whenever the OS de-schedules it. Set it generously (e.g., 256 frames at 16kHz × 20ms = ~5s buffer) and *log* `Lagged` errors as a recording-quality warning.

### Anti-Pattern 6: Calling whisper.cpp from a Tokio async task without `spawn_blocking`
**Why bad:** whisper inference is CPU-bound and blocks the runtime worker thread, starving every other task (audio capture, WS sending, HTTP routes). All `whisper-rs` calls must go inside `tokio::task::spawn_blocking`. This is the #1 production gotcha in the `whisper-rs` ecosystem.

---

## Risks in the Proposed Structure

These extend PRD §13 with architectural specifics.

### Risk 1: `screencapturekit-rs` audio-only loopback maturity (HIGH — confirmed in research)

`screencapturekit-rs` (the `doom-fish/screencapturekit-rs` crate) supports system audio + mic on macOS 13+, but the upstream `cpal` PR for SCK loopback is still being integrated, and there are reports of "audio samples not being received or empty" in some configurations. PRD §13 already calls this out; the mitigation (a thin Swift sidecar binary) is correct and **doesn't reshape the architecture** because `yogurt-audio` is already the sole owner of capture.

**Architectural insurance:** keep `yogurt-audio`'s public API to *exactly* `broadcast::Sender<AudioFrame>` + `start(channels: &[Channel])` + `stop()`. If the Swift sidecar path is needed, it lives entirely inside `yogurt-audio` — `tokio::process::Command` spawning a binary, reading raw PCM from its stdout, pushing into the same broadcast. Zero downstream changes.

**Mitigation phase ordering:** Phase 2 must include a "audio capture spike" gate — record 60s of dual-channel PCM and listen to it before committing to the crate-only path. The PRD plan review (referenced in PROJECT.md context, "5 blockers") likely flagged this.

### Risk 2: whisper.cpp linkage in a single static binary (MEDIUM)

`whisper-rs` (the `tazz4843/whisper-rs` bindings) compiles whisper.cpp as a static C++ library and links it via build-script. With Metal acceleration enabled (required for acceptable performance on Apple Silicon), the build pulls in Metal framework linkage. Universal binary builds (arm64 + x86_64) compound the build complexity because each arch builds its own static whisper.

**Mitigation:**

- Pin `whisper-rs` to a specific version with known-good build.rs.
- Build matrix in CI runs `aarch64-apple-darwin` and `x86_64-apple-darwin` separately and `lipo`s the binaries (not cross-arch unified). Standard Rust universal-binary practice.
- Defer to Phase 8 (matches PRD ordering). Don't block hero features on it.
- Consider `whisper-cpp-plus` as an alternative — it explicitly markets streaming + VAD + segment aggregation, which is closer to what Yogurt needs than `whisper-rs`'s lower-level bindings. Evaluate in Phase 8 spike.

### Risk 3: WebSocket fan-out under "many tabs open" (LOW for v1, watch in v2)

Single-user, single-machine, browser-UI scenario means realistic concurrent WS count is 1–3 (one main tab, maybe a second tab on the same meeting, dev-tools). Tokio's `broadcast::Sender` handles this trivially; the documented quadratic-slowdown issue (`tokio-rs/tokio#5923`) kicks in at hundreds of receivers each on its own single-threaded runtime — not Yogurt's situation.

**Mitigation:** none needed for v1. If v2 ever does multi-device sync, the WS fan-out will be by-meeting, not by-user, so still small.

### Risk 4: TipTap structural-diff complexity for `aiGrey` (MEDIUM — PRD §13 already flagged)

The `aiGrey` mark + AST-level diff is the hero feature and the highest-risk piece of UI logic. PRD §13 already proposes a Phase 3 prototype against three real meeting transcripts; treat that as a gate, not a checkbox. Architecturally, isolate the diff in `yogurt-notes` (Rust-side, runs server-side during enhance) — the browser only consumes pre-marked markdown. This means the diff is testable with `cargo test` against fixtures and doesn't depend on a TipTap render to validate.

### Risk 5: Markdown↔SQLite drift (LOW, but trivially preventable)

If the .md file mirror writes are not gated through a single helper, two writers will eventually disagree. Mitigation in Pattern 5 above: single `MarkdownExporter`, called only from repo update methods.

### Risk 6: Keychain access from a CLI-launched process (LOW)

The `keyring` crate works fine from a non-bundled CLI binary, but the first Keychain prompt is sometimes mis-attributed to "yogurt" (the binary name) which looks unbranded. Standard practice: ship the binary with a `Info.plist`-equivalent `CFBundleName` set via linker args, or accept the look.

---

## Scalability Considerations (single-user, but worth thinking)

| Concern                            | At 1 meeting | At 1000 meetings in library | At hour-long meeting |
|------------------------------------|--------------|------------------------------|----------------------|
| SQLite query for library list      | Trivial      | `idx_meetings_started` covers it; <5ms | N/A |
| Transcript JSON column size        | KBs          | KBs/meeting × 1000 = MBs total | ~500KB-1MB for a 1h meeting at 1 line per ~3 seconds |
| Markdown file count                | 1            | 1000 .md files in one dir; macOS handles fine | N/A |
| Audio broadcast buffer             | 5s of frames | N/A                         | Same; no growth     |
| Active per-meeting tasks           | 1 supervisor + 2 STT + 1 WS | 1 (only 1 meeting at a time) | Same |
| whisper.cpp memory                 | ~500MB (small.en) | Same                    | Same — streaming, not accumulated |

**Conclusion:** none of these scale concerns are real for v1. If meeting count exceeds ~10k, consider `transcript.json` moving out of the row into a side file. Trivially additive.

---

## Sources

- [Hyprnote vs Meetily: Detailed Comparison (openalternative.co)](https://openalternative.co/compare/hyprnote/vs/meetily)
- [Meetily (Zackriya-Solutions/meetily) on GitHub](https://github.com/Zackriya-Solutions/meetily)
- [Launch HN: Hyprnote (YC S25) — meeting notetaker](https://news.ycombinator.com/item?id=44725306)
- [screencapturekit on crates.io](https://crates.io/crates/screencapturekit)
- [doom-fish/screencapturekit-rs (idiomatic bindings)](https://github.com/doom-fish/screencapturekit-rs)
- [cpal PR #894: Support ScreenCapture loopback](https://github.com/RustAudio/cpal/pull/894)
- [cpal Issue #876: Support ScreenCaptureKit loopback](https://github.com/RustAudio/cpal/issues/876)
- [aizcutei/ruhear: Capture system output audio in Rust](https://github.com/aizcutei/ruhear)
- [whisper-rs (tazz4843/whisper-rs)](https://github.com/tazz4843/whisper-rs)
- [whisper-cpp-plus (streaming + VAD bindings)](https://github.com/operator-kit/whisper-cpp-plus-rs)
- [whisper-cpp-plus on lib.rs](https://lib.rs/crates/whisper-cpp-plus)
- [axum chat example (broadcast fan-out reference)](https://github.com/tokio-rs/axum/blob/main/examples/chat/src/main.rs)
- [Axum WebSocket guide (WebSocket.org)](https://websocket.org/guides/languages/rust/)
- [tokio::sync::broadcast docs](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html)
- [tokio issue #5923: broadcast quadratic slowdown with many receivers](https://github.com/tokio-rs/tokio/issues/5923)
- [axum-embed crate](https://docs.rs/axum-embed/latest/axum_embed/)
- [static-serve crate (M4SS-Code/static-serve)](https://github.com/m4ss-code/static-serve)
- [rust-embed on lib.rs](https://lib.rs/crates/rust-embed)
- [async-trait crate](https://crates.io/crates/async-trait)
- [Rust Book Ch. 18 — Trait Objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)
