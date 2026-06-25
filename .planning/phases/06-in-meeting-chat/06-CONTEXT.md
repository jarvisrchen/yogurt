# Phase 6: In-Meeting Chat - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

The floating "Ask this meeting…" pill anchored bottom-center expands into a chat window on `⌘K`, streams responses from the LLM client against `chat-system.md` + transcript-so-far as context, and persists chat messages per meeting in SQLite. Pill persists into the post-meeting view. Notes column behind the chat remains live and editable — no dim, no z-index trap, no outside-click dismissal.

Scope is limited to single-meeting context. Cross-meeting chat, semantic search, markdown rendering in bubbles, regenerate/retry buttons, and chat history pagination are explicitly out of scope.

</domain>

<decisions>
## Implementation Decisions

### Floating pill (collapsed)
- **D-01:** Pill is 480px wide, fixed bottom-center, anchored 24px from the bottom edge (`fixed bottom-6 left-1/2 -translate-x-1/2`)
- **D-02:** Placeholder text reads "Ask this meeting…"; right-aligned controls show a `⌘K` keyboard-hint badge (mono `--blsoft` chip) and a purple send-arrow glyph in a `--blue` filled circle
- **D-03:** Pill renders on BOTH the live meeting view (`Meeting.tsx`) and the post-meeting view (`MeetingPost.tsx`)

### Expansion behaviour
- **D-04:** Either clicking the pill OR pressing `⌘K` (or Ctrl+K on non-Mac) expands it into the chat window
- **D-05:** Expansion uses a hand-rolled CSS keyframe `popUp` — exactly 260ms ease-out, scale `0.96 → 1` + translateY `8px → 0` + opacity `0 → 1`. NO Framer Motion
- **D-06:** Chat window is "sticky" — clicking outside does NOT collapse it. Only the collapse caret in the header dismisses

### Chat window UI
- **D-07:** 480px wide, max-height 60vh, same bottom-center anchor; rounded-2xl on `--paper` background with `--line` border and `shadow-pop` elevation
- **D-08:** Header shows yogurt swirl logo + "Ask the meeting" label on the left, collapse-caret button on the right
- **D-09:** User messages right-aligned, `--blue` (blueberry) bubble, white text, `rounded-2xl rounded-br-md`
- **D-10:** Assistant messages left-aligned, `--card` (cream) bubble with `--line` (grey) border, `--ink` text, `rounded-2xl rounded-bl-md`
- **D-11:** Streaming assistant bubble shows a pulsing caret while `streamingId === message.id`

### Server-side streaming contract
- **D-12:** `POST /api/meetings/:id/chat` accepts `{ content }`, returns `{ message_id }` synchronously (< 100ms), and spawns an async streaming task
- **D-13:** Streaming tokens fan out over the existing per-meeting `/ws/meetings/:id` WebSocket as a new `WsEvent::ChatChunk { message_id, delta, done }` variant
- **D-14:** First chunk MUST arrive within 2s of POST when the LLM is reachable
- **D-15:** Chunks for one `message_id` arrive in order; stream terminates with `done: true` (with empty `delta` allowed on the terminal chunk)
- **D-16:** On stream error, emit a `chat_chunk` with `delta = "\n\n[stream error: <message>]"` and `done = true` so failure is visible to the user

### LLM prompt assembly
- **D-17:** System prompt is `chat-system.md` loaded from the Phase 4 `yogurt-prompts` crate (no in-line prompt strings in the server)
- **D-18:** A second system turn injects "TRANSCRIPT SO FAR (most recent at bottom):\n\n{transcript}" — transcript pulled via Phase 5's `db.get_meeting_transcript(id)` (or equivalent)
- **D-19:** Prior chat history (filtered to drop the empty placeholder assistant row for the in-flight message) is appended as alternating user/assistant turns
- **D-20:** LLM call goes through Phase 5's `LlmClient::stream_chat` — do NOT re-instantiate a hardcoded client

### Persistence
- **D-21:** New table `chat_messages` via migration `V002__chat_messages.sql`: `id TEXT PRIMARY KEY (ulid)`, `meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE`, `role TEXT CHECK IN ('user','assistant')`, `content TEXT NOT NULL`, `created_at INTEGER NOT NULL` (unix millis); index on `(meeting_id, created_at)`
- **D-22:** Both the user message and the (initially empty) assistant message rows are inserted synchronously inside the POST handler before streaming starts — guarantees the client + DB agree on `message_id`
- **D-23:** Once streaming completes, the assistant row's `content` is updated to the full concatenated text via `update_chat_message_content`
- **D-24:** On meeting view remount, full history loads via `GET /api/meetings/:id/chat` and renders in chronological order

### Server architecture
- **D-25:** Introduce `AppState { db, llm, ws_channels }` in `crates/yogurt-server/src/state.rs` to consolidate the three shared resources the chat handler needs; migrate existing Phase 3 / Phase 5 handlers to `State(AppState)` instead of per-`Extension`
- **D-26:** Per-meeting WS broadcast senders live in `AppState::ws_channels: Arc<RwLock<HashMap<String, broadcast::Sender<WsEvent>>>>`; lazy-create via `state.channel_for(meeting_id).await`

### Frontend wiring
- **D-27:** `useChat(meetingId)` hook owns send + stream + persistence; subscribes to `chat_chunk` events via the existing Phase 3 `useMeetingSocket(meetingId)` hook
- **D-28:** `useKeyboardShortcut({ key: "k", metaOrCtrl: true }, handler)` is a generic reusable hook (lives in `web/src/hooks/`)
- **D-29:** A tiny `<AskExperience meetingId={...} />` wrapper toggles between `<AskPill />` and `<ChatWindow />`; mounted at the route root on both `Meeting.tsx` and `MeetingPost.tsx`
- **D-30:** Optimistic user bubble appears immediately on send (`id: tmp-<timestamp>`); replaced when the POST returns and the assistant placeholder row is inserted

### Claude's Discretion
- Exact pulsing-caret colour intensity / animation duration (only requirement: visibly pulses during stream)
- Whether the swirl logo is the final brand asset or a placeholder dot (PRD §16 supplies the asset; use what exists in Phase 1 design system)
- Exact mock-LLM chunk count in tests (just enough to assert order + done)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Authoritative implementation plan
- `docs/superpowers/plans/2026-06-25-yogurt-phase-6-in-meeting-chat.md` — 11-task implementation plan with file paths, SQL, Rust, and TSX in full; treat as source of truth for all decisions captured above

### Product spec
- `docs/PRD.md` §5.4 — Ask-pill UX, sticky-no-outside-click behaviour, post-meeting persistence
- `docs/PRD.md` §5.5 — `chat-system.md` prompt requirements
- `docs/PRD.md` §9 — `chat_messages` schema
- `docs/PRD.md` §10 — `POST /api/meetings/:id/chat` + `WsEvent::ChatChunk` event shape
- `docs/PRD.md` §16.2 — blueberry / cream / grey / ink colour tokens
- `docs/PRD.md` §16.4 — `shadow-pop` elevation
- `docs/PRD.md` §16.5 — 260ms `popUp` ease-out motion token
- `docs/PRD.md` §16.8 — 480px pill width, 24px bottom anchor

### Requirements
- `.planning/REQUIREMENTS.md` "In-Meeting Chat" — CHAT-01 through CHAT-07
- `.planning/ROADMAP.md` "### Phase 6" — phase goal + 4-point success criteria

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Phase 5 `LlmClient` trait** (`crates/yogurt-llm/src/lib.rs`): `stream_chat(messages) -> Result<impl Stream<Item = Result<String>>>` — used directly by `spawn_stream`; do NOT bypass the trait
- **Phase 4 `yogurt-prompts` crate**: `chat-system.md` template bundled via `include_str!`; loader accessor (e.g. `ChatSystemPrompt::load()` or `yogurt_prompts::CHAT_SYSTEM`) — confirm exact symbol at implementation time
- **Phase 5 `yogurt-db` Db handle**: `Db::open(":memory:")`, `Db::conn()`, refinery migration loader picking up `V00*__*.sql` files
- **Phase 3 `/ws/meetings/:id` WebSocket handler**: per-meeting `broadcast::Sender<WsEvent>` fan-out; existing serializer pattern (`#[serde(tag = "type", rename_all = "snake_case")]`)
- **Phase 3 `useMeetingSocket(meetingId)` hook**: assumed `subscribe(eventType, handler)` API — if it only exposes raw `WebSocket`, wrap in a 5-line event-emitter in Task 6.9, do NOT refactor Phase 3
- **Phase 1 design tokens**: `--blue`, `--card`, `--line`, `--grey`, `--ink`, `--paper`, `--mut`, `--blsoft` already defined in `web/src/index.css` `@theme` block
- **Phase 0 SQLite scaffold (STORE-01)**: `chat_messages` table is scaffolded; this phase ships migration `V002` and the CRUD module

### Established Patterns
- **Per-table module in `yogurt-db`**: `meetings.rs` (Phase 5) and now `chat.rs` (Phase 6) — keeps `lib.rs` focused on the `Db` handle
- **TDD for backend**: write failing integration test under `crates/<crate>/tests/<name>.rs` first; implement until green
- **Vitest + Testing Library on frontend**: tests live in `web/src/components/__tests__/*.test.tsx`
- **Atomic commits per task**: every task ends with a `git commit -m "<conventional>"` — preserve that cadence
- **No real LLM in CI**: `MockLlmClient` returning canned chunks via `yogurt_server::test_support` feature-gated module

### Integration Points
- `crates/yogurt-server/src/state.rs` (NEW): consolidates `db`, `llm`, and `ws_channels` into `AppState`; replaces per-handler `Extension(...)` from Phases 3 & 5
- `crates/yogurt-server/src/api/chat.rs` (NEW): mounts at `POST/GET /api/meetings/:id/chat`
- `crates/yogurt-server/src/ws.rs` (MODIFY): add `ChatChunk` variant to `WsEvent` enum; existing handler must forward generically (no per-variant special case)
- `web/src/components/AskExperience.tsx` (NEW): toggles between `<AskPill />` and `<ChatWindow />`; mounted on `Meeting.tsx` and `MeetingPost.tsx`
- `web/src/index.css` (MODIFY): append `@keyframes popUp`, `.anim-popUp`, `.shadow-pop`

</code_context>

<specifics>
## Specific Ideas

- `⌘K` keyboard hint badge as a `--blsoft` mono-font chip on the pill — same chip pattern Granola uses; reusable for future shortcuts
- Hand-rolled CSS keyframe (NOT Framer Motion) for the 260ms `popUp` ease-out — keeps bundle small and avoids a library for a single animation
- Floating-pill → expanded-window morph: both use `fixed bottom-6 left-1/2 -translate-x-1/2` so the bottom-center anchor stays stable through the transform
- "Sticky" chat: clicking outside the window intentionally does nothing — `ChatWindow.test.tsx` explicitly asserts this; do NOT add an outside-click listener
- Streaming caret: small `w-[6px] h-[14px]` `animate-pulse` block inside the streaming assistant bubble — appears only while `streamingId === message.id`
- Optimistic user bubble on send (`id: tmp-<Date.now()>`), rolled back on POST failure — keeps the UI responsive without waiting for the server round-trip
- Empty state inside the chat window: italic muted "Ask anything about what's been said so far." centred at the top

</specifics>

<deferred>
## Deferred Ideas

- **Cross-meeting chat / semantic search** — v2, tracked as CROSS-02 in REQUIREMENTS.md
- **Templates for common queries** — v2; not in PRD §5.4
- **Markdown rendering inside chat bubbles** — plain text only in v1; defer until users ask for it
- **Per-message regenerate / retry button** — on stream error, user resends manually; we DO surface `[stream error: ...]` chunk
- **Chat history pagination** — load all messages on open; acceptable until meetings routinely exceed 50 chat turns
- **Image / file attachments in chat** — text only in v1
- **Chat export to markdown** — out of scope; users can copy/paste
- **Full `meetings` table schema enforcement of the V002 FK** — Phase 7 lands the full schema; if Phase 5's stub doesn't ship `meetings(id)` in time, drop the FK and re-add it via `V003__chat_messages_fk.sql` in Phase 7

</deferred>

---

*Phase: 06-in-meeting-chat*
*Context gathered: 2026-06-25*
