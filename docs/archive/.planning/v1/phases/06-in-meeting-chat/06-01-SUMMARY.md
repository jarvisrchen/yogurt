---
phase: 06-in-meeting-chat
plan: 01
subsystem: chat backend (db + REST + WS streaming)
tags: [sqlite, llm, streaming, websocket, ulid, axum, mocking]
requires:
  - phase: 04
    artifact: yogurt-prompts (chat_system() accessor)
  - phase: 05
    artifact: yogurt-db (Db handle + migration runner)
  - phase: 05
    artifact: yogurt-llm (LlmClient trait + ChatChunk/ChatRequest/ChatMessage)
  - phase: 04
    artifact: AppState (prompts, storage, meetings::Registry, db)
provides:
  - "crates/yogurt-db V002 migration + chat::{Role, ChatMessage} + CRUD on Db"
  - "yogurt-server::AppState.llm: Arc<dyn LlmClient>"
  - "yogurt-server::ws::WsEvent::ChatChunk typed variant"
  - "POST/GET /api/meetings/{id}/chat + spawn_stream LLM→WS bridge"
  - "yogurt-server::test_support (feature-gated MockChunksLlm + run_with_mock_llm + seed_meeting)"
affects:
  - "crates/yogurt-db/src/lib.rs (pub mod chat;)"
  - "crates/yogurt-db/src/migrations.rs (V002 chained after V001)"
  - "crates/yogurt-server/src/state.rs (AppState += llm; production + in_memory constructors default to MockLlm)"
  - "crates/yogurt-server/src/ws.rs (WsEvent enum + ChatChunk variant + serialization test)"
  - "crates/yogurt-server/src/routes.rs (POST/GET /api/meetings/{id}/chat under session-token middleware)"
  - "crates/yogurt-server/src/api/mod.rs (pub mod chat;)"
  - "crates/yogurt-server/src/lib.rs (test re-exports + test_support module behind feature)"
  - "crates/yogurt-server/Cargo.toml (test-support feature + optional tempfile)"
  - "crates/yogurt-server/tests/{meeting_ws,meeting_ws_auth,e2e_synthetic_audio}.rs (extended AppState literals with llm field)"
tech-stack:
  added: []
  patterns:
    - "ULID ids minted via ChatMessage::new with std::time::SystemTime epoch millis (no chrono dep)"
    - "POST inserts empty assistant placeholder ULID up-front; spawn_stream mutates content in place — supports across-reload continuity"
    - "WsEvent enum (#[serde(tag=\"type\", rename_all=\"snake_case\")]) coexists with Phase 4 ad-hoc serde_json::Value frames over per-meeting events_tx — same WS, one discriminator convention"
    - "test-support feature-gated module pattern (cfg(any(test, feature=\"test-support\"))) keeps mock helpers out of release builds"
key-files:
  created:
    - .planning/phases/06-in-meeting-chat/06-01-SUMMARY.md
    - crates/yogurt-db/migrations/V002__chat_messages.sql
    - crates/yogurt-db/src/chat.rs
    - crates/yogurt-db/tests/chat.rs
    - crates/yogurt-server/src/api/chat.rs
    - crates/yogurt-server/src/test_support.rs
    - crates/yogurt-server/tests/chat_streaming.rs
  modified:
    - Cargo.lock
    - crates/yogurt-db/src/lib.rs
    - crates/yogurt-db/src/migrations.rs
    - crates/yogurt-server/Cargo.toml
    - crates/yogurt-server/src/api/mod.rs
    - crates/yogurt-server/src/lib.rs
    - crates/yogurt-server/src/routes.rs
    - crates/yogurt-server/src/state.rs
    - crates/yogurt-server/src/ws.rs
    - crates/yogurt-server/tests/e2e_synthetic_audio.rs
    - crates/yogurt-server/tests/meeting_ws.rs
    - crates/yogurt-server/tests/meeting_ws_auth.rs
decisions:
  - "Prompt accessor used: state.prompts.chat_system() (Phase 4 Prompts struct method), NOT a yogurt_prompts::CHAT_SYSTEM static. The Prompts struct already lives on AppState — the chat-system.md body is loaded through the same dev-vs-release path as enhance.md."
  - "db.get_meeting_transcript did NOT exist in yogurt-db. Implemented a local read_transcript helper in api/chat.rs that uses the Phase 0 storage read-pool to SELECT transcript_json directly. Phase 7 (library) will introduce a proper Db accessor; the gap is intentional."
  - "AppState already existed (Phase 5). Extended in place with llm: Arc<dyn LlmClient> defaulting to MockLlm. No ws_channels HashMap added — the existing per-meeting events_tx broadcast (Phase 4) is reused for ChatChunk fan-out, mirroring the enhance_progress flow."
  - "No WsEvent enum existed before (Phase 4 emits ad-hoc serde_json::Value). Created a typed WsEvent in ws.rs with only the ChatChunk variant; existing enhance_progress flow is untouched. Both serialize to the same {type, …} discriminator convention."
  - "chat_messages table is declared in BOTH yogurt-db (V002) and yogurt-server::storage::migrations (Phase 0). Both use CREATE TABLE IF NOT EXISTS — whichever runner fires first wins; schemas agree on column names + types. V002 adds the role CHECK constraint + ON DELETE CASCADE; the prior Phase 0 declaration lacked these but they are additive."
  - "MockLlm + MockChunksLlm are feature-gated under `test-support` (Cargo feature) and re-exported via test_support module. Release builds never compile them — tempfile is also gated."
  - "axum 0.8 path syntax: routes use {id} (not :id). Plan was written against axum 0.7."
metrics:
  duration_minutes: 23
  completed_date: 2026-06-25
  tasks_completed: 3
  files_created: 7
  files_modified: 12
  commits: 5
---

# Phase 6 Plan 06-01: chat_messages table + WS streaming + REST handler Summary

**One-liner:** Wires the server-side half of in-meeting chat: a V002 `chat_messages` migration + CRUD in `yogurt-db`, an `AppState.llm` field defaulting to `MockLlm`, a typed `WsEvent::ChatChunk` variant, and a `POST/GET /api/meetings/{id}/chat` endpoint whose `spawn_stream` task fans per-token deltas out over the existing per-meeting `events_tx` broadcast — all driven through the Phase 5 `LlmClient` trait and the Phase 4 `chat-system.md` prompt accessor.

## What Shipped

### Task 1 — V002 chat_messages migration + Db CRUD

- `crates/yogurt-db/migrations/V002__chat_messages.sql`: `chat_messages (id ULID PK, meeting_id FK → meetings(id) ON DELETE CASCADE, role TEXT CHECK IN ('user','assistant'), content TEXT, created_at INTEGER)` + `idx_chat_meeting(meeting_id, created_at)` index.
- `crates/yogurt-db/src/chat.rs`: `Role::{User, Assistant}` with `as_str()` + `FromStr`, `ChatMessage { id, meeting_id, role, content, created_at }` with `::new(meeting_id, role, content)` minting a ULID + `SystemTime::now()` epoch millis.
- CRUD on `Db`: `insert_chat_message`, `list_chat_messages` (ordered by `created_at ASC, id ASC` tiebreaker), `get_chat_message`, `update_chat_message_content`.
- Two integration tests in `tests/chat.rs`: `it_inserts_and_lists_messages_in_chronological_order` (incl. update_content round-trip) + `it_scopes_messages_by_meeting`.

**Verify:** `cargo test -p yogurt-db --test chat` — 2 passed. Full `cargo test -p yogurt-db` — 15 passed.

**Commit:** `e47a610` — `feat(db,06-01 Task 1): V002 chat_messages migration + CRUD on Db`

### Task 2 — AppState.llm + WsEvent::ChatChunk

- `crates/yogurt-server/src/state.rs`: AppState gains `llm: Arc<dyn LlmClient>`. `production()` defaults to `MockLlm` (Phase 6 settings follow-up will hot-swap to `OpenAiCompatClient`); `in_memory()` also defaults to `MockLlm`.
- `crates/yogurt-server/src/ws.rs`: new `pub enum WsEvent { ChatChunk { message_id, delta, #[serde(default)] done } }` with `#[serde(tag = "type", rename_all = "snake_case")]`. Existing Phase 4 `events_tx` serde_json::Value flow is unchanged — both share the WS with one `{type, …}` convention.
- Wire-shape unit test `it_serializes_chat_chunk_with_expected_keys` asserts the four exact JSON keys the browser reads.
- Three integration test files (`meeting_ws`, `meeting_ws_auth`, `e2e_synthetic_audio`) extended with the new `llm` field via the new test-only re-export `__test_only_llm_mock::MockLlm`.

**Verify:** `cargo test -p yogurt-server` — 75 passed.

**Commit:** `5d2028a` — `feat(server,06-01 Task 2): AppState.llm + WsEvent::ChatChunk variant`

### Task 3 — POST/GET /api/meetings/{id}/chat + spawn_stream

- `crates/yogurt-server/src/api/chat.rs`:
  - `post_chat` → 400 on empty content, inserts user row + empty assistant placeholder, kicks off `spawn_stream`, returns `{ message_id: <ULID> }`.
  - `get_chat_history` → `{ messages: [...] }` chronologically.
  - `spawn_stream(state, meeting_id, message_id)` runs on a detached `tokio::spawn`:
    1. `state.prompts.chat_system()` — Phase 4 prompt accessor (NOT inline).
    2. `read_transcript(state, meeting_id)` — reads `meetings.transcript_json` via Phase 0 storage read pool (no `Db::get_meeting_transcript` yet; documented Phase 7 follow-up).
    3. `state.db.list_chat_messages(meeting_id)` for history; skip the empty placeholder row.
    4. Build `Vec<yogurt_llm::ChatMessage>` (system + transcript + history) and call `state.llm.stream(ChatRequest { messages, stream: true })`. The LLM trait is the SOLE call path — no hardcoded `OpenAiCompatClient` anywhere in chat.rs.
    5. For each chunk: accumulate delta + broadcast `WsEvent::ChatChunk { … done: false }`. Stream errors yield a single `delta: "[stream error: …]" + done: true` chunk.
    6. Persist accumulated text via `Db::update_chat_message_content`; emit terminal `done: true` chunk.
- Route wired in `routes.rs` under session-token middleware: `.route("/api/meetings/{id}/chat", post(...).get(...))`.
- `crates/yogurt-server/src/test_support.rs` (feature `test-support`):
  - `MockChunksLlm::new(&[&str])` — replays canned chunks then `done: true`.
  - `run_with_mock_llm(chunks)` — boots a tempdir-isolated axum server with the mock client and returns `{addr, token, state, _tmp}` + the server task handle.
  - `seed_meeting(state)` — creates a meeting in the registry AND seeds the row in BOTH storage's `meetings` table (so transcript read works) and yogurt-db's in-memory db (so the chat FK has a parent).
- Two integration tests in `tests/chat_streaming.rs`: `it_returns_message_id_on_post_chat` (asserts 26-char ULID) and `it_streams_chat_chunks_in_order_over_ws` (concatenated deltas equal `"hello world."`, terminal `done: true` observed, all chunks share the same `message_id`).

**Verify:** `cargo test -p yogurt-server --test chat_streaming` — 2 passed. Full `cargo test -p yogurt-server` — 77 passed.

**Commits:** `01fd54c` — `feat(server,06-01 Task 3): POST/GET /api/meetings/:id/chat + spawn_stream`; `6661c05` — `chore(06-01): update Cargo.lock for tempfile optional dep`.

## Deviations from Plan

### Rule 3 (auto-fix blocking issues)

**1. Plan instructed creating new `AppState` with `db`/`llm`/`ws_channels`; AppState already existed (Phase 5) with a richer surface.** Extended in place with `llm` only — `ws_channels` HashMap was not added because the existing per-meeting `events_tx` broadcast already covers the same fan-out need. Updated three pre-existing integration tests' AppState literals.

**2. Plan instructed `WsEvent` enum should already exist with multiple Phase 3/4 variants; it did not.** Phase 3/4 used ad-hoc `serde_json::Value` frames over `events_tx`. Created a typed `WsEvent` in `ws.rs` with only the `ChatChunk` variant; existing flow stays untouched. Both serialize to the same `{type, …}` discriminator.

**3. Plan referenced `LlmClient::stream_chat(Vec<Message>)`; the actual Phase 5 trait method is `LlmClient::stream(ChatRequest { messages, stream })`.** Used the real surface. `MessageRole::User/Assistant` are not separate types — `yogurt_llm::ChatMessage` carries a free-form `role: String` with `::user()` / `::assistant()` / `::system()` constructors.

**4. Plan referenced `yogurt_prompts::CHAT_SYSTEM` static or `ChatSystemPrompt::load()`.** The actual surface is the `Prompts` struct accessor `chat_system() -> Result<String>` already on `AppState.prompts`. Used `state.prompts.chat_system()`.

**5. Plan instructed `Db::get_meeting_transcript`.** Did not exist in yogurt-db. Wrote a local `read_transcript` helper in `api/chat.rs` that uses the Phase 0 `state.storage.read()` pool to query `meetings.transcript_json`. Falls back to empty on any error. Phase 7 will introduce a proper Db accessor — gap intentional and documented in the chat.rs module doc.

**6. axum 0.8 path syntax** — plan used `:id`, actual is `{id}`. Used `{id}`.

**7. `chat_messages` table already exists** in `yogurt-server::storage::migrations` (Phase 0). Resolved by `CREATE TABLE IF NOT EXISTS` in V002 — both runners coexist; whichever fires first wins, schemas agree.

No Rule 4 architectural decisions were required; all deviations were rule-3 mechanical adjustments to match the real codebase shape.

## Authentication Gates

None. The chat endpoint sits behind the existing `require_session_token` middleware; no new credential surface was added in this plan.

## Test Coverage

| Suite | Count | Status |
|-------|-------|--------|
| `yogurt-db --test chat` | 2 | ✅ |
| `yogurt-db --lib + tests` (incl. providers, settings, keychain, migrations) | 15 | ✅ |
| `yogurt-server --test chat_streaming` | 2 | ✅ |
| `yogurt-server` (all) | 77 | ✅ |
| `cargo test --workspace` | 148 (1 ignored — keychain-live) | ✅ |

## MockLlmClient feature-gating confirmation

`MockChunksLlm` lives in `crates/yogurt-server/src/test_support.rs`, gated by `#[cfg(any(test, feature = "test-support"))]`. The `test-support` Cargo feature pulls in optional `tempfile`. Release builds (`cargo build --release` without `--features test-support`) never compile this module. Confirmed: `MockLlm` (the existing Phase 4 mock) remains `pub(crate)`; the new test re-export `__test_only_llm_mock::MockLlm` exposes it ONLY to integration tests in the same crate, not to downstream consumers.

## Files Touched (final inventory)

**Created (7):**
- `.planning/phases/06-in-meeting-chat/06-01-SUMMARY.md`
- `crates/yogurt-db/migrations/V002__chat_messages.sql`
- `crates/yogurt-db/src/chat.rs`
- `crates/yogurt-db/tests/chat.rs`
- `crates/yogurt-server/src/api/chat.rs`
- `crates/yogurt-server/src/test_support.rs`
- `crates/yogurt-server/tests/chat_streaming.rs`

**Modified (12):**
- `Cargo.lock` (tempfile optional dep)
- `crates/yogurt-db/src/lib.rs` (pub mod chat)
- `crates/yogurt-db/src/migrations.rs` (V002 chained)
- `crates/yogurt-server/Cargo.toml` (test-support feature + optional tempfile)
- `crates/yogurt-server/src/api/mod.rs` (pub mod chat)
- `crates/yogurt-server/src/lib.rs` (test re-exports + test_support cfg)
- `crates/yogurt-server/src/routes.rs` (POST/GET route wiring)
- `crates/yogurt-server/src/state.rs` (AppState.llm)
- `crates/yogurt-server/src/ws.rs` (WsEvent enum + test)
- `crates/yogurt-server/tests/e2e_synthetic_audio.rs` (llm field)
- `crates/yogurt-server/tests/meeting_ws.rs` (llm field)
- `crates/yogurt-server/tests/meeting_ws_auth.rs` (llm field)

## Commits

| Hash | Subject |
|------|---------|
| `e47a610` | feat(db,06-01 Task 1): V002 chat_messages migration + CRUD on Db |
| `5d2028a` | feat(server,06-01 Task 2): AppState.llm + WsEvent::ChatChunk variant |
| `01fd54c` | feat(server,06-01 Task 3): POST/GET /api/meetings/:id/chat + spawn_stream |
| `6661c05` | chore(06-01): update Cargo.lock for tempfile optional dep |
| `0b9a92d` | style(06-01): cargo fmt sweep |

## Self-Check: PASSED

- All listed source files exist.
- All listed commit hashes resolve via `git log --oneline`.
- `cargo test --workspace`: 148 passed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
