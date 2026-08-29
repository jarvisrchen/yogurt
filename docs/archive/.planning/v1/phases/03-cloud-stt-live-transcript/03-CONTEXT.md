# Phase 3: Cloud STT + Live Transcript - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

This is the **first phase that wires the full audio → STT → WebSocket → browser pipeline end-to-end.** A `SttEngine` trait abstracts cloud and (future) local transcription; the Deepgram streaming adapter is its first implementation; the right-edge live transcript dock UI renders incoming transcript events with < 2s lag and visible "Me"/"Them" channel labels.

By the end of Phase 3 the user can run `cargo run -p yogurt -- start`, open a meeting, hit Start, talk into their mic, and watch transcript lines stream into the browser dock in real time.

**Depends on:** Phase 2 (audio capture → `tokio::sync::broadcast::Sender<AudioChunk>`), Phase 1 (design tokens), Phase 0 (axum server skeleton + embedded web assets + `/api/health`).

**Explicitly NOT in this phase:**
- Persistence of meetings/transcripts to SQLite — Phase 7. Meetings live in `RwLock<HashMap<MeetingId, Arc<Meeting>>>` and vanish on server restart.
- Local `whisper.cpp` adapter — Phase 8. Only the cloud Deepgram adapter ships here, but the trait must already exist as the extension point.
- Settings UI for the Deepgram API key — Phase 5. Key reads from `YOGURT_DEEPGRAM_API_KEY` env var only.
- Notes augmentation (`aiGrey` mark, transcript deep links) — Phase 4.
- In-meeting chat — Phase 6.

</domain>

<decisions>
## Implementation Decisions

### STT trait shape
- **D-01:** `Stt` trait (the per-PRD §10 wire-format name; PLAN uses this; ROADMAP calls it `SttEngine` colloquially — same thing) is defined in a new `yogurt-stt` crate. Single method: `async fn start(&self, audio_rx: broadcast::Receiver<AudioChunk>, txn: broadcast::Sender<TranscriptEvent>) -> anyhow::Result<()>`. Runs for the lifetime of the audio stream; returns when audio closes or the engine ends.
- **D-02:** `yogurt-stt` has **zero dependency** on `yogurt-audio`. It defines its own `AudioChunk { channel: Channel, samples: Vec<i16>, ts_ms: u64 }` type. The server crate is the wirer that sends `yogurt-audio` output into the STT's receiver. Keeps the trait crate dependency-light so the Phase 8 `whisper.cpp` adapter can swap in without pulling audio.
- **D-03:** `Channel` enum is `Mic | System` (serialized lowercase). "Me"/"Them" labeling happens at the UI layer; the trait carries the raw source channel. Granola itself only does two-way labeling (PRD §5.2) — diarization is an explicit v1 anti-goal.

### Deepgram adapter
- **D-04:** Implementation uses hand-rolled `tokio-tungstenite 0.24` (with `rustls-tls-webpki-roots` feature, NOT native-tls — single-binary distribution requires no OpenSSL link). The community `deepgram` Rust crate is pre-1.0 and pulls extra surface area; ~200 LOC of hand-rolled WS is leaner and keeps the trait clean for swap-out. PROJECT.md flagged this swap as a known risk; this is the mitigation.
- **D-05:** Two WebSocket sessions per meeting — one per `Channel` (mic + system) — running in parallel. Costs 2× Deepgram seconds but is the only correct way to preserve channel label without speaker diarization. Each session: `wss://api.deepgram.com/v1/listen?model=nova-2&encoding=linear16&sample_rate=16000&channels=1&interim_results=true&endpointing=300&smart_format=true` with `Authorization: Token <key>` header.
- **D-06:** Test override: `DeepgramStt.base_url` field defaults to `wss://api.deepgram.com` but is publicly settable so mock-WS integration tests can dial `ws://127.0.0.1:<port>` against `tokio_tungstenite::accept_async`.

### API key sourcing (Phase 3 only — Keychain lands in Phase 5)
- **D-07:** Deepgram API key reads from `std::env::var("YOGURT_DEEPGRAM_API_KEY")` at `Registry::start(&id)` time. If missing, `start_meeting` returns HTTP 400 with `{"error": "YOGURT_DEEPGRAM_API_KEY not set ..."}`. Phase 3 does NOT wire `dotenvy`; user either `export`s the var or uses `direnv`. Phase 5 replaces this read with a Keychain-eager-loaded `Arc<RwLock<Secrets>>` lookup, with `.env.local` as a `--dev` fallback.
- **D-08:** No Keychain stub or shim ships in Phase 3 — direct env var read keeps the phase scope tight and matches the superpowers plan exactly. The swap to Keychain in Phase 5 is a single-function-body change in `meetings::Registry::start`.

### WebSocket transcript protocol
- **D-09:** Server route `GET /ws/meetings/:id` upgrades to WebSocket via `axum::extract::ws::WebSocketUpgrade` (axum 0.8 `ws` feature). On connect: look up meeting in `Registry`, `.subscribe()` to its `broadcast::Sender<TranscriptEvent>`. If meeting doesn't exist, close with code 4404 + reason `"meeting not found"`.
- **D-10:** Frame envelope (S→C): `{"type":"transcript","payload":{ts_ms,channel,text,is_final}}`. `payload` matches PRD §10 verbatim (snake_case fields, lowercase channel). C→S frames are read-and-discarded in v1 (Phase 4 will add `notes_edit`, Phase 6 will add `chat_send`).
- **D-11:** Subscribers see events from the moment of subscription onward — no replay of history. Late joiners see the live tail only (matches PRD §5.2 explicit behavior).
- **D-12:** Three new REST endpoints gate lifecycle: `POST /api/meetings` (create, returns `{id: Uuid v7, created_at_ms}`), `POST /api/meetings/:id/start` (spawn audio + STT supervisor), `POST /api/meetings/:id/stop` (abort supervisor; idempotent).

### Right-edge transcript dock UI
- **D-13:** Collapsed by default — a vertical tab pinned to the right edge (28px wide, `writingMode: vertical-rl`, rounded-left, cream-bordered) with the label "Live transcript". Click toggles `open` state.
- **D-14:** Expanded panel: 330px wide, full-height, `position: fixed` right-edge, white background, cream left-border. Tailwind 4 `@keyframes slideInRight` defined in `web/src/index.css` (Phase 1 will later move tokens into a proper file; Phase 3 keeps them inline to bound scope). Animation: `340ms cubic-bezier(.2, .7, .2, 1)` exactly. Class `.dock-open` triggers it.
- **D-15:** Notes column is NOT dimmed and NOT reflowed when dock opens. Layout uses `pr-7` on the notes wrapper to reserve the tab gutter; the 330px panel overlays on top via `z-30` + `position: fixed` (no layout shift, no z-index war with editor).
- **D-16:** Channel labels: "Me" (mic, ink `#211D18`) / "Them" (system, grey `#A89F90`). Timestamp in JetBrains Mono, formatted `HH:MM:SS` from meeting start. Inline hex values used in Phase 3; Phase 1's CSS-variable refactor is a sed-and-replace later.
- **D-17:** Auto-scroll: list scrolls to bottom on each new event UNLESS `stickyRef.current === false`. `onScroll` handler flips `stickyRef` based on `scrollHeight - scrollTop - clientHeight < 24` (within 24px of bottom = sticky; user scrolled up = paused).
- **D-18:** Cursor blink ("still listening" signal per TRANS-07): non-final transcript events render at `opacity: 0.7` while finals render at `opacity: 1`. The latest partial per channel replaces the previous partial in-place (handled by `mergeEvent` in `useTranscriptWs`).

### Latency
- **D-19:** Server-side budget (from `transcript_tx.send(...)` to WS client frame received) must be < 200ms — asserted by the synthetic-audio E2E test. The full < 2s lag spec (TRANS-08) includes Deepgram's network + processing budget, verified by manual three-terminal smoke against the real API.

### Frontend routing
- **D-20:** No `react-router` yet — Phase 3 uses a simple `useState<"library" | "meeting">` switch in `App.tsx`. Phase 7 (library + onboarding) revisits the router decision. `web/src/routes/` folder is introduced here for forward compatibility.

### Claude's Discretion
- Exact log level wording inside spawned STT tasks (`tracing::warn` vs `tracing::info`).
- mpsc channel sizing for the per-channel writer pump (plan suggests 64; any 32-128 is acceptable).
- Tab glyph between "▶"/"◀" and a Lucide chevron icon (Phase 1's icon system isn't fully wired yet).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Source of truth
- `docs/superpowers/plans/2026-06-25-yogurt-phase-3-cloud-stt.md` — Full task-by-task plan with concrete code, file paths, test ports, and acceptance criteria. **Authoritative for every implementation detail.**

### Product spec
- `docs/PRD.md` §3 — User personas (compliance, OSS, self-hosted LLM users all care about transcript latency)
- `docs/PRD.md` §5.2 — Live transcript panel: collapsed tab, 330px, "Me"/"Them" labels, JetBrains Mono timestamps, `< 2s` lag spec, notes-stay-editable constraint
- `docs/PRD.md` §10 — WebSocket protocol: `S→C transcript {ts_ms, channel, text, is_final}` wire format
- `docs/PRD.md` §16.2 — Palette tokens: `--ink #211D18`, `--grey #A89F90`, `--line #EBE3D5`
- `docs/PRD.md` §16.5 — Motion: 340ms `cubic-bezier(.2,.7,.2,1)` `slideInRight` for the dock

### Phase requirements
- `.planning/REQUIREMENTS.md` "Transcript (Cloud STT)" section — TRANS-01 through TRANS-08 in full
- `.planning/ROADMAP.md` "### Phase 3: Cloud STT + Live Transcript" — phase goal + 4 success criteria

### Project posture
- `CLAUDE.md` (project root) "Recommended Stack" — version pins for `tokio-tungstenite 0.24`, `axum 0.8`, React 19, Tailwind 4
- Phase 1 design tokens — produced in Phase 1; Phase 3 references their hex values inline and Phase 1 will refactor to CSS variables in a later pass

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets (consumed from prior phases)
- **Phase 2 `yogurt_audio::capture_into(tx: broadcast::Sender<AudioChunk>) -> Result<()>`**: Loops on real ScreenCaptureKit audio + mic and pushes 16kHz mono i16 PCM chunks into the supplied sender until it errors or the sender drops. Phase 3 `meetings::Registry::start` spawns this as the audio-side half of the supervisor.
- **Phase 0 `yogurt_server::run(addr, mode)`**: Already binds axum at `7878` and serves embedded web assets in Release mode / proxies to Vite in Dev mode. Phase 3 extends `routes::router` to mount `/api/meetings*` + `/ws/meetings/:id` on top of the existing `/api/health` route.
- **Phase 0 `web/src/App.tsx`**: Replaced (not extended) with a tiny `View` enum switch. Phase 0 test is updated to assert on the new library-stub heading.
- **Phase 1 design tokens**: Hex values (`#211D18`, `#A89F90`, `#EBE3D5`, `#5B4FC7`) are referenced inline in Phase 3 components. Phase 1's CSS-variable layer is the long-term home; Phase 3 ships before that refactor.

### Established Patterns
- **`tokio::sync::broadcast` for fan-out**: Phase 2 used it for audio; Phase 3 reuses the same pattern for the transcript stream. Capacity 256 is standard. Lagged subscribers `warn!` + continue; closed sender = clean exit.
- **`#[cfg(test)] mod tests` inline + `crates/<crate>/tests/` integration**: Established in Phase 0; Phase 3 adds mock-WS integration tests using `tokio_tungstenite::accept_async` on `TcpListener::bind("127.0.0.1:0")` (OS-assigned ephemeral port).
- **Test ports reserved**: `17890`, `17891`, `17892`, `17893` for `yogurt-server` integration tests across this phase (must not collide across files).

### Integration Points
- **`AppState { meetings: Arc<Registry> }`** is the new router state, threaded via `Router::with_state(state)`. All new handlers consume `State(state): State<AppState>`.
- **`yogurt-server/src/lib.rs`** exposes `pub mod meetings`, `pub mod ws`, plus `#[doc(hidden)] pub fn __test_router(state) -> Router` so integration tests can reach into the registry without going through `run()`.
- **Cargo workspace**: Adds `crates/yogurt-stt` to `members`. New workspace deps: `async-trait 0.1`, `tokio-tungstenite 0.24` (rustls-tls-webpki-roots), `futures-util 0.3`, `url 2`, `uuid 1` (v7 + serde).

</code_context>

<specifics>
## Specific Ideas

- **"Me" / "Them" channel labels are visible and persistent** — not just a color difference. PRD §5.2 explicitly calls out the textual labels because Granola does the same. Don't try to be clever and drop them in favor of color-only.
- **Cursor blink as "still listening" signal** — non-final partials render dimmer (opacity 0.7) and the latest partial per channel replaces in-place. This is the user's signal that audio is still flowing even when no new finals have landed.
- **Single binary, no Deepgram SDK** — hand-rolled `tokio-tungstenite` is the explicit choice. The community `deepgram` crate is fine but pre-1.0 churn risk is real; the hand-rolled adapter is ~200 LOC and lives behind the `Stt` trait for trivial swap-out later if it ever becomes worth it.
- **The 340ms motion is load-bearing for the brand** — PRD §16.5 spec'd it exactly. The compiled CSS must contain `340ms` and `cubic-bezier(.2, .7, .2, 1)` (verified via grep on `web/dist/assets/*.css` in Task 3.10 step 4).
- **No persistence in this phase** — meetings vanish on server restart. This is intentional and acceptable for the v1 milestone. Phase 7 swaps the in-memory `HashMap` for SQLite behind the same `Registry` API.

</specifics>

<deferred>
## Deferred Ideas

- **Local `whisper.cpp` STT adapter** — Phase 8. Same `Stt` trait, second implementation.
- **Settings UI for Deepgram API key + Keychain storage** — Phase 5. Phase 3 reads `YOGURT_DEEPGRAM_API_KEY` env var only; missing key → HTTP 400.
- **`.env.local` loading via `dotenvy`** — Phase 5 (with the full env-var bootstrap pattern). For Phase 3, user `export`s the var or uses `direnv`.
- **Speaker diarization beyond mic/system labels** — explicit v1 anti-goal (PRD §2). Granola itself only does "Me"/"Them".
- **Audio level metering on the dock tab** — only the static "Live transcript" label + arrow icon ship here; the 3-bar animated wave glyph from PRD §5.2 lands in Phase 1 / Phase 7.
- **Persistence of meetings or transcripts to SQLite** — Phase 7. Meetings live in `RwLock<HashMap>` and vanish on restart.
- **`aiGrey` TipTap mark + `↳ HH:MM` transcript deep links** — Phase 4.
- **In-meeting chat pill** — Phase 6.
- **Pause/resume of recording mid-meeting** — v1.1. Start/Stop only in v1.
- **AssemblyAI / Groq STT adapters** — not in v1 at all. Trait is the extension point if anyone ever wants them.
- **CSS-variable design tokens** — Phase 1's refactor. Phase 3 inlines hex values matching PRD §16.2 so Phase 1's pass is a mechanical sed-and-replace.

</deferred>

---

*Phase: 03-cloud-stt-live-transcript*
*Context gathered: 2026-06-25*
