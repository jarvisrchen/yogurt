# Yogurt v1 — Phase 6: In-Meeting Chat ("Ask this meeting…") Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the floating "Ask this meeting…" pill + expandable chat window that appears in both the live meeting view and the post-meeting view. User taps the pill or hits `⌘K`, the pill morphs into a 480px-wide chat window via a 260ms ease-out `popUp` animation, and questions stream back from the active LLM provider over WebSocket with the transcript-so-far as context. Notes stay live and editable behind the chat — no dim, no z-index trap.

**Architecture:** Backend adds a `POST /api/meetings/:id/chat` REST endpoint that persists the user message, kicks off an async LLM stream, returns `{message_id}` immediately, and pushes each token as a `chat_chunk` event over the existing `/ws/meetings/:id` socket from Phase 3. A new `chat_messages` table (migration V002) stores both sides of the conversation. Frontend adds three React components (`AskPill`, `ChatWindow`, `ChatMessage`), a `useChat` hook that owns send + stream state, and a `useKeyboardShortcut` hook for `⌘K`. The pop animation is a hand-rolled CSS keyframe — no Framer Motion.

**Tech Stack:** Rust 1.83+ · axum 0.8 (existing) · `yogurt-llm` (Phase 5, `OpenAiCompatClient` with streaming) · `yogurt-db` (Phase 5, rusqlite + refinery migrations) · `yogurt-prompts` (Phase 4, `chat-system.md`) · `ulid` 1 · React 19 · Tailwind 4 · TypeScript 5.6 · CSS keyframes (no Framer Motion).

**Reference:** `docs/PRD.md` §5.4 (Ask pill UX), §5.5 (chat-system.md prompt), §9 (`chat_messages` schema), §10 (`POST /api/meetings/:id/chat` + WS `chat_chunk` event shape), §16.5 (260ms `popUp` ease-out motion token), §16.2 (blueberry / cream / grey colors), §16.8 (480px pill width, 24px bottom anchor).

**Dependencies on prior phases:**
- **Phase 0** — workspace, axum scaffold.
- **Phase 1** — design tokens (`--blue`, `--card`, `--line`, `--grey`, `--ink`).
- **Phase 3** — `/ws/meetings/:id` WebSocket handler with broadcast fan-out; `Meeting.tsx` route.
- **Phase 4** — `yogurt-prompts` crate with `chat-system.md`; `MeetingPost.tsx` route.
- **Phase 5** — `yogurt-llm` (`OpenAiCompatClient::stream_chat`); `yogurt-db` with `Db` handle + V001 `meetings` migration.

**Out of scope (deferred to later phases):**
- Cross-meeting chat / semantic search (v2 per PRD §6).
- Chat message editing or deletion (no UX affordance designed).
- Markdown rendering in chat bubbles — plain text only in v1; PRD §5.4 doesn't require it.
- Per-message regenerate / retry. If a stream fails, the user resends manually.
- Chat history pagination — meetings rarely accumulate more than ~50 messages; load all on open.

---

## File structure produced by this phase

```
yogurt/
├── crates/
│   ├── yogurt-db/
│   │   ├── src/
│   │   │   ├── lib.rs                                 # MODIFY · add chat_messages CRUD
│   │   │   ├── chat.rs                                # NEW · chat_messages types + queries
│   │   │   └── migrations/
│   │   │       └── V002__chat_messages.sql            # NEW · CREATE TABLE chat_messages
│   │   └── tests/
│   │       └── chat.rs                                # NEW · CRUD round-trip + ordering
│   └── yogurt-server/
│       ├── src/
│       │   ├── api/
│       │   │   ├── mod.rs                             # MODIFY · register chat module
│       │   │   └── chat.rs                            # NEW · POST /api/meetings/:id/chat
│       │   ├── ws.rs                                  # MODIFY · add ChatChunk event variant
│       │   └── state.rs                               # MODIFY · expose llm + db + ws_tx
│       └── tests/
│           └── chat_streaming.rs                      # NEW · end-to-end POST → WS chunks
└── web/
    ├── src/
    │   ├── components/
    │   │   ├── AskPill.tsx                            # NEW · collapsed floating pill
    │   │   ├── ChatWindow.tsx                         # NEW · expanded chat panel
    │   │   └── ChatMessage.tsx                        # NEW · user + assistant bubbles
    │   ├── hooks/
    │   │   ├── useChat.ts                             # NEW · send + stream + persist
    │   │   └── useKeyboardShortcut.ts                 # NEW · ⌘K handler
    │   ├── routes/
    │   │   ├── Meeting.tsx                            # MODIFY · render <AskPill /> + window
    │   │   └── MeetingPost.tsx                        # MODIFY · same as above
    │   ├── lib/
    │   │   └── api.ts                                 # MODIFY · add postChatMessage()
    │   └── index.css                                  # MODIFY · @keyframes popUp
    └── src/components/__tests__/
        ├── AskPill.test.tsx                           # NEW · ⌘K expands the pill
        └── ChatWindow.test.tsx                        # NEW · click-outside doesn't close
```

**Why a separate `chat.rs` in `yogurt-db`:** keeps `lib.rs` focused on the `Db` handle and migration loader. Each table gets its own module — `meetings.rs` (Phase 5) and now `chat.rs`. Matches the pattern Phase 5 established.

**Why a `state.rs` extraction in `yogurt-server`:** the chat handler needs the LLM client, the DB, and the WS broadcaster all in one place. By Phase 6 there are 3+ consumers of the same shared state — collecting them in `AppState` is overdue.

---

## Test conventions established (extending Phase 0)

- **Rust unit tests:** as before — `#[cfg(test)] mod tests` inside each source file.
- **Rust integration tests:** `crates/<crate>/tests/<name>.rs`. Phase 6 adds:
  - `crates/yogurt-db/tests/chat.rs` — CRUD round-trip with a `:memory:` SQLite.
  - `crates/yogurt-server/tests/chat_streaming.rs` — POST + WS reader, asserting at least one `chat_chunk` arrives in order within 2s. Uses a **mock LLM** (`MockLlmClient` returning a canned stream of "Hello world" chunks) — Phase 6 does not hit real Minimax / OpenAI in CI.
- **Frontend unit tests:** Vitest + Testing Library. Phase 6 adds:
  - `AskPill.test.tsx` — pressing ⌘K calls the expand callback; clicking the pill calls it too; placeholder text reads "Ask this meeting…".
  - `ChatWindow.test.tsx` — clicking outside the window does NOT close it (sticky); typing + Enter calls `onSend`; assistant chunks appended as they arrive (driven by a fake WS event).
- **No E2E in this phase.** Playwright still deferred per Phase 0 convention.

---

## Phase 6 task list

11 tasks. Each task ends with a commit. Approximate sequence: ~7 hours of focused work — smallest plan of the v1 set.

---

### Task 6.1 · V002 migration: `chat_messages` table

**Files:**
- Create: `crates/yogurt-db/src/migrations/V002__chat_messages.sql`

- [ ] **Step 1: Confirm V001 (`meetings`) exists from Phase 5.**

Run: `ls crates/yogurt-db/src/migrations/`
Expected: `V001__meetings.sql` present. If missing, stop — Phase 5 isn't merged yet and this phase has no foundation.

- [ ] **Step 2: Write `crates/yogurt-db/src/migrations/V002__chat_messages.sql`.**

> **⚠ FK note:** PRD §9 declares `meeting_id TEXT NOT NULL REFERENCES meetings(id)`. The `meetings` table lands in Phase 7 (per the phase split). Phase 5 ships an early stub of the `meetings` table sufficient to satisfy the FK at the SQL level — confirm `V001__meetings.sql` includes at minimum `id TEXT PRIMARY KEY`. If Phase 5's stub does NOT yet create `meetings(id)`, this migration must drop the FK clause and add it back in Phase 7 via a `V00X__chat_messages_fk.sql` migration. Document whichever choice is made at the top of the SQL file.

```sql
-- V002: chat_messages — per-meeting chat history for the "Ask this meeting…" pill.
-- See PRD §5.4 and §9.
--
-- Note: meeting_id FK assumes V001 already created meetings(id). If V001 only
-- stubs a placeholder meetings table, the FK still resolves; Phase 7 will
-- augment meetings with the full schema without disturbing this FK.

CREATE TABLE chat_messages (
    id          TEXT PRIMARY KEY,         -- ulid
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    role        TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content     TEXT NOT NULL,
    created_at  INTEGER NOT NULL          -- unix millis
);

CREATE INDEX idx_chat_meeting ON chat_messages(meeting_id, created_at);
```

- [ ] **Step 3: Verify the migration loader picks it up.**

Run: `cargo test -p yogurt-db --lib`
Expected: any Phase-5 tests that call `Db::open(":memory:")` still pass, proving refinery sees and applies V002 without error. (No new tests yet — Task 6.2 adds them.)

- [ ] **Step 4: Commit.**

```bash
git add crates/yogurt-db/src/migrations/V002__chat_messages.sql
git commit -m "feat(db): add V002 chat_messages migration"
```

---

### Task 6.2 · `chat_messages` CRUD in `yogurt-db` (TDD)

**Files:**
- Create: `crates/yogurt-db/src/chat.rs`
- Modify: `crates/yogurt-db/src/lib.rs`
- Create: `crates/yogurt-db/tests/chat.rs`

- [ ] **Step 1: Write the failing integration test first.**

Create `crates/yogurt-db/tests/chat.rs`:

```rust
use yogurt_db::{chat::{ChatMessage, Role}, Db};

fn seed_meeting(db: &Db, id: &str) {
    // Phase 5 exposes a minimal `insert_meeting_stub(id, title)` helper for tests.
    // If that helper doesn't exist, use a raw rusqlite execute via Db::conn():
    db.conn()
        .execute(
            "INSERT INTO meetings (id, title, started_at) VALUES (?, ?, ?)",
            rusqlite::params![id, "test", 0_i64],
        )
        .unwrap();
}

#[test]
fn it_inserts_and_lists_messages_in_chronological_order() {
    let db = Db::open(":memory:").expect("open in-memory db");
    seed_meeting(&db, "01HXMEETING000000000000000");

    let m1 = ChatMessage::new("01HXMEETING000000000000000", Role::User, "what did we decide?");
    let m2 = ChatMessage::new("01HXMEETING000000000000000", Role::Assistant, "you decided to ship Phase 6.");

    db.insert_chat_message(&m1).unwrap();
    // Make sure created_at differs to test ordering.
    std::thread::sleep(std::time::Duration::from_millis(2));
    db.insert_chat_message(&m2).unwrap();

    let listed = db.list_chat_messages("01HXMEETING000000000000000").unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, m1.id);
    assert_eq!(listed[1].id, m2.id);
    assert_eq!(listed[1].role, Role::Assistant);
    assert_eq!(listed[1].content, "you decided to ship Phase 6.");
}

#[test]
fn it_scopes_messages_by_meeting() {
    let db = Db::open(":memory:").unwrap();
    seed_meeting(&db, "01HXA0000000000000000000000");
    seed_meeting(&db, "01HXB0000000000000000000000");

    db.insert_chat_message(&ChatMessage::new("01HXA0000000000000000000000", Role::User, "a")).unwrap();
    db.insert_chat_message(&ChatMessage::new("01HXB0000000000000000000000", Role::User, "b")).unwrap();

    let a = db.list_chat_messages("01HXA0000000000000000000000").unwrap();
    let b = db.list_chat_messages("01HXB0000000000000000000000").unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_ne!(a[0].id, b[0].id);
}
```

- [ ] **Step 2: Run — expect compile failure (`chat` module not exposed).**

Run: `cargo test -p yogurt-db --test chat`
Expected: `error[E0432]: unresolved import 'yogurt_db::chat'`

- [ ] **Step 3: Write `crates/yogurt-db/src/chat.rs`.**

```rust
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "user" => Ok(Role::User),
            "assistant" => Ok(Role::Assistant),
            other => anyhow::bail!("unknown chat role: {other}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub meeting_id: String,
    pub role: Role,
    pub content: String,
    pub created_at: i64,
}

impl ChatMessage {
    pub fn new(meeting_id: impl Into<String>, role: Role, content: impl Into<String>) -> Self {
        Self {
            id: Ulid::new().to_string(),
            meeting_id: meeting_id.into(),
            role,
            content: content.into(),
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

impl crate::Db {
    pub fn insert_chat_message(&self, msg: &ChatMessage) -> Result<()> {
        self.conn().execute(
            "INSERT INTO chat_messages (id, meeting_id, role, content, created_at)
             VALUES (?, ?, ?, ?, ?)",
            params![
                msg.id,
                msg.meeting_id,
                msg.role.as_str(),
                msg.content,
                msg.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_chat_messages(&self, meeting_id: &str) -> Result<Vec<ChatMessage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, role, content, created_at
             FROM chat_messages
             WHERE meeting_id = ?
             ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt
            .query_map(params![meeting_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter()
            .map(|(id, meeting_id, role, content, created_at)| {
                Ok(ChatMessage {
                    id,
                    meeting_id,
                    role: Role::from_str(&role)?,
                    content,
                    created_at,
                })
            })
            .collect()
    }

    /// Get a single message — used by the streaming handler to update content
    /// once the LLM stream completes (we store the final concatenated text).
    pub fn get_chat_message(&self, id: &str) -> Result<Option<ChatMessage>> {
        self.conn()
            .query_row(
                "SELECT id, meeting_id, role, content, created_at
                 FROM chat_messages WHERE id = ?",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?
            .map(|(id, meeting_id, role, content, created_at)| {
                Ok(ChatMessage {
                    id,
                    meeting_id,
                    role: Role::from_str(&role)?,
                    content,
                    created_at,
                })
            })
            .transpose()
    }

    pub fn update_chat_message_content(&self, id: &str, content: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE chat_messages SET content = ? WHERE id = ?",
            params![content, id],
        )?;
        Ok(())
    }
}
```

- [ ] **Step 4: Expose `chat` in `crates/yogurt-db/src/lib.rs`.**

Add near the existing module declarations:

```rust
pub mod chat;
```

Make sure `Db::conn(&self) -> &rusqlite::Connection` is publicly accessible (it should be from Phase 5; if it's private, expose it now — the chat module uses it via `self.conn()` which is a method on `Db`).

- [ ] **Step 5: Add `ulid` and `chrono` to `crates/yogurt-db/Cargo.toml` (if not already).**

Phase 5 likely added `ulid` for meeting IDs. If not, add to `[dependencies]`:

```toml
ulid = "1"
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

- [ ] **Step 6: Run — expect PASS.**

Run: `cargo test -p yogurt-db --test chat`
Expected: `test it_inserts_and_lists_messages_in_chronological_order ... ok` and `test it_scopes_messages_by_meeting ... ok`.

- [ ] **Step 7: Commit.**

```bash
git add crates/yogurt-db/
git commit -m "feat(db): chat_messages CRUD with role enum and ordered list"
```

---

### Task 6.3 · `AppState` extraction in `yogurt-server`

**Files:**
- Create / Modify: `crates/yogurt-server/src/state.rs`
- Modify: `crates/yogurt-server/src/lib.rs` (thread `AppState` through `Router`)

> **Why now:** the chat handler in 6.4 needs three shared things — `Db`, `LlmClient`, and a `tokio::sync::broadcast::Sender<WsEvent>` (or per-meeting registry). Phase 3 likely threaded the broadcaster directly; Phase 5 added `Db` as a separate `Extension`. Collecting them into one `AppState` now avoids a pile of `.with_state` chains.

- [ ] **Step 1: Inspect the current state passing in `lib.rs` / `routes.rs`.**

Run: `grep -rn "with_state\|Extension::" crates/yogurt-server/src/`
Expected: a couple of `Extension(Arc<Db>)`-style hookups from Phase 5 and a `broadcast::Sender` shoved into the WS handler from Phase 3.

- [ ] **Step 2: Write `crates/yogurt-server/src/state.rs`.**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use yogurt_db::Db;
use yogurt_llm::LlmClient;

use crate::ws::WsEvent;

/// Per-process shared state, cloned cheaply into every handler.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub llm: Arc<dyn LlmClient>,
    /// Per-meeting WS broadcasters. The WS handler (Phase 3) subscribes to the
    /// matching sender on connect; the chat handler (Phase 6) publishes
    /// `WsEvent::ChatChunk` to it as tokens arrive.
    pub ws_channels: Arc<RwLock<HashMap<String, broadcast::Sender<WsEvent>>>>,
}

impl AppState {
    pub fn new(db: Arc<Db>, llm: Arc<dyn LlmClient>) -> Self {
        Self {
            db,
            llm,
            ws_channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Lazily create a broadcast channel for a meeting, returning the sender.
    /// Both the WS handler and any event producer (chat, transcript, enhance)
    /// go through this so subscribers and publishers always agree on the channel.
    pub async fn channel_for(&self, meeting_id: &str) -> broadcast::Sender<WsEvent> {
        let read = self.ws_channels.read().await;
        if let Some(tx) = read.get(meeting_id) {
            return tx.clone();
        }
        drop(read);
        let mut write = self.ws_channels.write().await;
        write
            .entry(meeting_id.to_string())
            .or_insert_with(|| broadcast::channel::<WsEvent>(256).0)
            .clone()
    }
}
```

- [ ] **Step 3: Migrate the existing router to take `AppState`.**

In `crates/yogurt-server/src/lib.rs`, replace the per-handler `Extension(...)`s with:

```rust
let state = AppState::new(db.clone(), llm.clone());
let app = routes::router(state.clone(), mode);
```

In `routes::router`, switch to `Router::with_state(state)` and update Phase-3 WS handler + Phase-5 meetings handlers to accept `State(state): State<AppState>` instead of `Extension(...)`. This is mechanical — keep the diff narrow.

- [ ] **Step 4: Verify existing tests still pass.**

Run: `cargo test -p yogurt-server`
Expected: all prior tests green. If a Phase-3 WS test broke because of the channel-registry change, update it to fetch the sender via `state.channel_for(meeting_id).await` instead of holding a top-level `broadcast::Sender`.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "refactor(server): collect Db + LLM + WS channels into AppState"
```

---

### Task 6.4 · `WsEvent::ChatChunk` variant + WS handler relay

**Files:**
- Modify: `crates/yogurt-server/src/ws.rs`

- [ ] **Step 1: Inspect the existing `WsEvent` enum.**

Run: `grep -n "enum WsEvent\|#\[serde" crates/yogurt-server/src/ws.rs`
Expected: a `#[serde(tag = "type", rename_all = "snake_case")]` enum with at least `Transcript { ... }` from Phase 3.

- [ ] **Step 2: Add the `ChatChunk` variant.**

In `crates/yogurt-server/src/ws.rs`, append to the `WsEvent` enum (preserving any existing variants):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    // ... existing variants from Phase 3 (Transcript, NotesSynced, EnhanceProgress) ...

    /// Streaming chat tokens. Multiple events for one `message_id`, in order,
    /// terminated by `done = true` so the client knows to seal the bubble.
    ChatChunk {
        message_id: String,
        delta: String,
        #[serde(default)]
        done: bool,
    },
}
```

- [ ] **Step 3: Confirm the WS handler already forwards arbitrary `WsEvent`s.**

The Phase 3 handler should be looping on `rx.recv().await` and sending each event as JSON. If it pattern-matches per variant, refactor it to a generic forward — every event type goes through the same `axum::extract::ws::Message::Text(serde_json::to_string(&event)?)` path. Don't special-case `ChatChunk`.

- [ ] **Step 4: Add a unit test for serialization shape.**

In `crates/yogurt-server/src/ws.rs` add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_serializes_chat_chunk_with_expected_keys() {
        let ev = WsEvent::ChatChunk {
            message_id: "01HXMSG".into(),
            delta: "hello".into(),
            done: false,
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "chat_chunk");
        assert_eq!(json["message_id"], "01HXMSG");
        assert_eq!(json["delta"], "hello");
        assert_eq!(json["done"], false);
    }
}
```

- [ ] **Step 5: Run — expect PASS.**

Run: `cargo test -p yogurt-server --lib ws::tests::it_serializes_chat_chunk_with_expected_keys`
Expected: green.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/src/ws.rs
git commit -m "feat(server): add WsEvent::ChatChunk variant + shape test"
```

---

### Task 6.5 · `POST /api/meetings/:id/chat` handler (no streaming yet)

**Files:**
- Create: `crates/yogurt-server/src/api/chat.rs`
- Modify: `crates/yogurt-server/src/api/mod.rs`
- Modify: `crates/yogurt-server/src/routes.rs` (mount the route)

- [ ] **Step 1: Write the failing integration test for the POST contract.**

Create `crates/yogurt-server/tests/chat_streaming.rs`:

```rust
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn it_returns_message_id_on_post_chat() {
    // Boot a server with a mock LLM that returns ["hello ", "world"].
    let addr = "127.0.0.1:17886".parse().unwrap();
    let _h = tokio::spawn(async move {
        yogurt_server::test_support::run_with_mock_llm(addr, &["hello ", "world"]).await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Seed a meeting (test_support helper writes directly to the in-memory Db).
    let meeting_id = "01HXMEETING00000000000TEST";
    yogurt_server::test_support::seed_meeting(meeting_id).await;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:17886/api/meetings/{meeting_id}/chat"))
        .json(&serde_json::json!({ "content": "what did we decide?" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let msg_id = body["message_id"].as_str().expect("message_id is a string");
    assert!(msg_id.len() >= 26, "message_id looks like a ulid");
}
```

> **Note:** `yogurt_server::test_support` is a small dev-only module that mints a `Db::open(":memory:")` + mock LLM. Sketch it now and wire it under `#[cfg(any(test, feature = "test-support"))]`. The Phase 5 plan likely already established the pattern; if not, create it here:
>
> ```rust
> // crates/yogurt-server/src/test_support.rs (added in this task)
> #[cfg(any(test, feature = "test-support"))]
> pub async fn run_with_mock_llm(addr: SocketAddr, chunks: &'static [&'static str]) -> anyhow::Result<()> { /* boot server with MockLlmClient */ }
>
> #[cfg(any(test, feature = "test-support"))]
> pub async fn seed_meeting(id: &str) { /* INSERT INTO meetings */ }
> ```
> Expose via `[features] test-support = []` in `Cargo.toml` and `pub mod test_support;` in `lib.rs` behind the feature gate.

- [ ] **Step 2: Run — expect compile / route 404.**

Run: `cargo test -p yogurt-server --test chat_streaming`
Expected: either compile error (handler doesn't exist) or 404 from axum (route unmounted).

- [ ] **Step 3: Write `crates/yogurt-server/src/api/chat.rs` (POST handler only — streaming wired in 6.6).**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use yogurt_db::chat::{ChatMessage, Role};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub message_id: String,
}

pub async fn post_chat(
    State(state): State<AppState>,
    Path(meeting_id): Path<String>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    if req.content.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "content must not be empty".into()));
    }

    // 1. Persist the user message immediately so the client + history agree.
    let user_msg = ChatMessage::new(&meeting_id, Role::User, req.content.clone());
    state
        .db
        .insert_chat_message(&user_msg)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 2. Create the assistant message row up front (empty content), so the
    //    streaming task in 6.6 can fill it in as chunks arrive.
    let assistant_msg = ChatMessage::new(&meeting_id, Role::Assistant, "");
    state
        .db
        .insert_chat_message(&assistant_msg)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 3. Kick off the streaming task — wired in 6.6. For now, just return.
    crate::api::chat::spawn_stream(state.clone(), meeting_id.clone(), assistant_msg.id.clone())
        .await;

    Ok(Json(ChatResponse {
        message_id: assistant_msg.id,
    }))
}

/// Placeholder — Task 6.6 replaces this with the real LLM stream loop.
pub async fn spawn_stream(_state: AppState, _meeting_id: String, _message_id: String) {
    // no-op in 6.5; 6.6 wires it up
}

#[derive(Debug, Serialize)]
pub struct ChatHistoryResponse {
    pub messages: Vec<ChatMessage>,
}

pub async fn get_chat_history(
    State(state): State<AppState>,
    Path(meeting_id): Path<String>,
) -> Result<Json<ChatHistoryResponse>, (StatusCode, String)> {
    let messages = state
        .db
        .list_chat_messages(&meeting_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(ChatHistoryResponse { messages }))
}
```

- [ ] **Step 4: Wire into `crates/yogurt-server/src/api/mod.rs` and `routes.rs`.**

In `api/mod.rs`:

```rust
pub mod chat;
// ... other modules
```

In `routes.rs` (inside `router(state, mode)`):

```rust
.route(
    "/api/meetings/:id/chat",
    axum::routing::post(api::chat::post_chat).get(api::chat::get_chat_history),
)
```

- [ ] **Step 5: Run — expect the test to pass.**

Run: `cargo test -p yogurt-server --test chat_streaming`
Expected: `it_returns_message_id_on_post_chat ... ok`. The mock-LLM stream isn't asserted yet — that's 6.6. We're just confirming the route + persistence round-trip.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): POST /api/meetings/:id/chat persists user + assistant rows"
```

---

### Task 6.6 · Stream LLM tokens → `WsEvent::ChatChunk` → WS subscribers

**Files:**
- Modify: `crates/yogurt-server/src/api/chat.rs`
- Modify: `crates/yogurt-server/tests/chat_streaming.rs` (append streaming-order test)

- [ ] **Step 1: Write the failing streaming test.**

Append to `crates/yogurt-server/tests/chat_streaming.rs`:

```rust
use futures_util::{SinkExt, StreamExt};

#[tokio::test(flavor = "multi_thread")]
async fn it_streams_chat_chunks_in_order_over_ws() {
    let addr = "127.0.0.1:17887".parse().unwrap();
    let _h = tokio::spawn(async move {
        yogurt_server::test_support::run_with_mock_llm(addr, &["hello ", "world", "."]).await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let meeting_id = "01HXMEETING00000000000WSTS";
    yogurt_server::test_support::seed_meeting(meeting_id).await;

    // Connect WS first so we don't race the POST.
    let (mut ws, _) = tokio_tungstenite::connect_async(
        format!("ws://127.0.0.1:17887/ws/meetings/{meeting_id}")
    ).await.unwrap();

    // Fire the chat POST.
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("http://127.0.0.1:17887/api/meetings/{meeting_id}/chat"))
        .json(&serde_json::json!({ "content": "say something" }))
        .send().await.unwrap()
        .json().await.unwrap();
    let expected_msg_id = resp["message_id"].as_str().unwrap().to_string();

    // Collect chunks until done=true or 2s timeout.
    let mut collected = String::new();
    let mut got_done = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_millis(500), ws.next()).await;
        let Ok(Some(Ok(msg))) = next else { continue };
        let Some(text) = msg.into_text().ok() else { continue };
        let val: serde_json::Value = serde_json::from_str(&text).unwrap();
        if val["type"] != "chat_chunk" { continue; }
        assert_eq!(val["message_id"], expected_msg_id);
        if let Some(delta) = val["delta"].as_str() {
            collected.push_str(delta);
        }
        if val["done"].as_bool() == Some(true) {
            got_done = true;
            break;
        }
    }
    assert!(got_done, "expected a chat_chunk with done=true");
    assert_eq!(collected, "hello world.");
}
```

Add `futures-util` and `tokio-tungstenite` to `[dev-dependencies]` of `yogurt-server`:

```toml
futures-util = "0.3"
tokio-tungstenite = "0.24"
```

- [ ] **Step 2: Run — expect timeout / no chunks (spawn_stream is a no-op).**

Run: `cargo test -p yogurt-server --test chat_streaming it_streams_chat_chunks_in_order_over_ws`
Expected: assertion fail on `got_done` because Task 6.5's `spawn_stream` does nothing.

- [ ] **Step 3: Implement `spawn_stream` for real.**

Replace the placeholder in `crates/yogurt-server/src/api/chat.rs`:

```rust
use futures_util::StreamExt;
use yogurt_db::chat::Role;
use yogurt_prompts::ChatSystemPrompt;

use crate::ws::WsEvent;

pub async fn spawn_stream(state: AppState, meeting_id: String, message_id: String) {
    tokio::spawn(async move {
        // 1. Assemble the prompt: system + transcript-so-far + chat history + new user msg.
        let system = ChatSystemPrompt::load();  // from yogurt-prompts crate (Phase 4)
        let transcript = match state.db.get_meeting_transcript(&meeting_id) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(?e, "chat: failed to load transcript — falling back to empty");
                String::new()
            }
        };
        let history = state.db.list_chat_messages(&meeting_id).unwrap_or_default();

        // The most recent assistant row (the one we just created with empty content)
        // and the most recent user row are already in `history`. Drop the placeholder
        // assistant row before sending to the LLM so we don't feed it an empty turn.
        let history_for_llm: Vec<yogurt_db::chat::ChatMessage> = history
            .into_iter()
            .filter(|m| !(m.id == message_id && m.role == Role::Assistant))
            .collect();

        let messages = build_openai_messages(&system, &transcript, &history_for_llm);

        // 2. Get (or create) the broadcaster for this meeting.
        let tx = state.channel_for(&meeting_id).await;

        // 3. Stream from LLM, fan each delta out as a WsEvent::ChatChunk.
        let mut stream = match state.llm.stream_chat(messages).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(?e, "chat: stream_chat failed");
                let _ = tx.send(WsEvent::ChatChunk {
                    message_id: message_id.clone(),
                    delta: format!("\n\n[stream error: {e}]"),
                    done: true,
                });
                return;
            }
        };

        let mut accumulated = String::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(delta) => {
                    accumulated.push_str(&delta);
                    let _ = tx.send(WsEvent::ChatChunk {
                        message_id: message_id.clone(),
                        delta,
                        done: false,
                    });
                }
                Err(e) => {
                    tracing::warn!(?e, "chat: chunk error mid-stream");
                    break;
                }
            }
        }

        // 4. Persist the final concatenated content and emit the terminal chunk.
        if let Err(e) = state.db.update_chat_message_content(&message_id, &accumulated) {
            tracing::error!(?e, "chat: failed to persist assistant content");
        }
        let _ = tx.send(WsEvent::ChatChunk {
            message_id,
            delta: String::new(),
            done: true,
        });
    });
}

fn build_openai_messages(
    system: &str,
    transcript: &str,
    history: &[yogurt_db::chat::ChatMessage],
) -> Vec<yogurt_llm::Message> {
    let mut out = Vec::with_capacity(history.len() + 2);
    out.push(yogurt_llm::Message::system(system));
    out.push(yogurt_llm::Message::system(format!(
        "TRANSCRIPT SO FAR (most recent at bottom):\n\n{transcript}"
    )));
    for m in history {
        let role = match m.role {
            Role::User => yogurt_llm::MessageRole::User,
            Role::Assistant => yogurt_llm::MessageRole::Assistant,
        };
        out.push(yogurt_llm::Message::new(role, m.content.clone()));
    }
    out
}
```

> **Note on `ChatSystemPrompt::load()`:** Phase 4 created `crates/yogurt-prompts/templates/chat-system.md` and an `include_str!`-style accessor. Confirm the symbol name during 6.5 prep — if it's exposed as a constant (e.g. `yogurt_prompts::CHAT_SYSTEM`), use that instead.

> **Note on `get_meeting_transcript`:** Phase 3 / Phase 5 should expose either `db.get_meeting_transcript(id) -> Result<String>` or equivalent. If it doesn't yet (because Phase 7 owns the meetings table), have `spawn_stream` accept a callback or read from the in-memory store. Document the gap in the commit message and resolve it in Phase 7.

- [ ] **Step 4: Run — expect the streaming test to pass.**

Run: `cargo test -p yogurt-server --test chat_streaming`
Expected: both tests green; `collected == "hello world."`.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): stream LLM tokens as WsEvent::ChatChunk per meeting channel"
```

---

### Task 6.7 · `popUp` CSS keyframe + design-token additions

**Files:**
- Modify: `web/src/index.css`

- [ ] **Step 1: Add the keyframe per PRD §16.5.**

Append to `web/src/index.css`:

```css
/* --- Phase 6: chat window pop animation (PRD §16.5) --- */
@keyframes popUp {
  0% {
    transform: translate(-50%, 8px) scale(0.96);
    opacity: 0;
  }
  100% {
    transform: translate(-50%, 0) scale(1);
    opacity: 1;
  }
}

.anim-popUp {
  animation: popUp 260ms ease-out both;
}

/* Pop-shadow used by ChatWindow (PRD §16.4 elevation: Pop) */
.shadow-pop {
  box-shadow: 0 12px 30px -10px rgba(40, 30, 15, 0.22);
}
```

> **Note:** the `translate(-50%, ...)` keeps the bottom-center anchor stable as the element scales. The pill itself already sits at `left: 50%; transform: translateX(-50%)` (see Task 6.8).

- [ ] **Step 2: Manual visual check (no test — animation timing is not unit-testable).**

Run: `pnpm --dir web dev` and load a meeting route. Trigger the chat manually (next tasks) or temporarily slap `<div class="anim-popUp">test</div>` into `App.tsx` to confirm the keyframe is registered. Remove the scratch div before committing.

- [ ] **Step 3: Commit.**

```bash
git add web/src/index.css
git commit -m "style(web): add 260ms popUp keyframe + pop shadow per PRD §16.5"
```

---

### Task 6.8 · `<AskPill />` component (collapsed state)

**Files:**
- Create: `web/src/components/AskPill.tsx`
- Create: `web/src/hooks/useKeyboardShortcut.ts`
- Create: `web/src/components/__tests__/AskPill.test.tsx`

- [ ] **Step 1: Write `useKeyboardShortcut.ts`.**

```ts
import { useEffect } from "react";

interface ShortcutOptions {
  /** Lower-case key, e.g. "k". */
  key: string;
  /** Require Cmd on macOS / Ctrl elsewhere. */
  metaOrCtrl?: boolean;
  /** Disable when chat is already open or any modal is up. */
  enabled?: boolean;
}

export function useKeyboardShortcut(
  opts: ShortcutOptions,
  handler: (e: KeyboardEvent) => void,
) {
  const { key, metaOrCtrl = false, enabled = true } = opts;

  useEffect(() => {
    if (!enabled) return;
    const listener = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() !== key) return;
      if (metaOrCtrl && !(e.metaKey || e.ctrlKey)) return;
      e.preventDefault();
      handler(e);
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [key, metaOrCtrl, enabled, handler]);
}
```

- [ ] **Step 2: Write the failing test for `AskPill`.**

Create `web/src/components/__tests__/AskPill.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AskPill } from "../AskPill";

describe("AskPill", () => {
  it("renders the placeholder copy and ⌘K hint", () => {
    render(<AskPill onExpand={() => {}} />);
    expect(screen.getByText(/Ask this meeting…/i)).toBeInTheDocument();
    expect(screen.getByText(/⌘K/)).toBeInTheDocument();
  });

  it("calls onExpand when clicked", () => {
    const onExpand = vi.fn();
    render(<AskPill onExpand={onExpand} />);
    fireEvent.click(screen.getByRole("button", { name: /ask this meeting/i }));
    expect(onExpand).toHaveBeenCalledTimes(1);
  });

  it("calls onExpand when ⌘K is pressed", () => {
    const onExpand = vi.fn();
    render(<AskPill onExpand={onExpand} />);
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(onExpand).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 3: Run — expect compile failure (no `AskPill`).**

Run: `pnpm --dir web test AskPill`
Expected: `Cannot find module '../AskPill'`.

- [ ] **Step 4: Write `web/src/components/AskPill.tsx`.**

```tsx
import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";

interface AskPillProps {
  onExpand: () => void;
}

export function AskPill({ onExpand }: AskPillProps) {
  useKeyboardShortcut({ key: "k", metaOrCtrl: true }, onExpand);

  return (
    <button
      type="button"
      onClick={onExpand}
      aria-label="Ask this meeting"
      className="
        fixed bottom-6 left-1/2 -translate-x-1/2
        w-[480px] h-12
        flex items-center justify-between gap-3
        px-4 py-2
        bg-[var(--card)] border border-[var(--line)] rounded-full
        shadow-pop
        text-left text-[14px] text-[var(--mut)]
        hover:border-[var(--blue)] hover:text-[var(--ink)]
        transition-colors
        z-30
      "
    >
      <span className="flex-1 truncate">Ask this meeting…</span>
      <span
        aria-hidden="true"
        className="
          inline-flex items-center justify-center
          h-6 px-2 rounded-md
          text-[11px] font-mono text-[var(--mut)]
          bg-[var(--blsoft)]
        "
      >
        ⌘K
      </span>
      <span
        aria-hidden="true"
        className="
          inline-flex items-center justify-center
          h-7 w-7 rounded-full
          bg-[var(--blue)] text-white
        "
      >
        {/* small up-arrow / send glyph */}
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path d="M6 9.5V2.5M6 2.5L2.5 6M6 2.5L9.5 6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </span>
    </button>
  );
}
```

- [ ] **Step 5: Run the test — expect PASS.**

Run: `pnpm --dir web test AskPill`
Expected: `3 passed`.

- [ ] **Step 6: Commit.**

```bash
git add web/src/components/AskPill.tsx web/src/hooks/useKeyboardShortcut.ts web/src/components/__tests__/AskPill.test.tsx
git commit -m "feat(web): AskPill floating component with ⌘K shortcut"
```

---

### Task 6.9 · `useChat` hook (REST send + WS stream wiring)

**Files:**
- Modify: `web/src/lib/api.ts` — add `postChatMessage` + `fetchChatHistory`
- Create: `web/src/hooks/useChat.ts`

- [ ] **Step 1: Extend `api.ts`.**

Append to `web/src/lib/api.ts`:

```ts
export type ChatRole = "user" | "assistant";

export interface ChatMessage {
  id: string;
  meeting_id: string;
  role: ChatRole;
  content: string;
  created_at: number;
}

export async function postChatMessage(meetingId: string, content: string): Promise<{ message_id: string }> {
  const res = await fetch(`/api/meetings/${meetingId}/chat`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ content }),
  });
  if (!res.ok) throw new Error(`chat send failed: ${res.status}`);
  return res.json();
}

export async function fetchChatHistory(meetingId: string): Promise<ChatMessage[]> {
  const res = await fetch(`/api/meetings/${meetingId}/chat`);
  if (!res.ok) throw new Error(`chat history failed: ${res.status}`);
  const body = await res.json();
  return body.messages;
}
```

- [ ] **Step 2: Write `useChat.ts`.**

This hook assumes a `useMeetingSocket(meetingId)` hook exists from Phase 3 that exposes a `subscribe(eventType, handler)` API or a typed event-emitter. If Phase 3 only exposes a raw `WebSocket`, adapt by passing the socket in via props or context — keep the surface area narrow.

```ts
import { useCallback, useEffect, useRef, useState } from "react";
import { fetchChatHistory, postChatMessage, type ChatMessage } from "../lib/api";
import { useMeetingSocket } from "./useMeetingSocket"; // from Phase 3

interface ChatChunkEvent {
  type: "chat_chunk";
  message_id: string;
  delta: string;
  done: boolean;
}

export function useChat(meetingId: string) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streamingId, setStreamingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { subscribe } = useMeetingSocket(meetingId);
  const streamingIdRef = useRef<string | null>(null);
  streamingIdRef.current = streamingId;

  // Load history once on mount / meeting change.
  useEffect(() => {
    let cancelled = false;
    fetchChatHistory(meetingId)
      .then((history) => { if (!cancelled) setMessages(history); })
      .catch((e) => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, [meetingId]);

  // Wire WS chunks into message state.
  useEffect(() => {
    return subscribe("chat_chunk", (ev: ChatChunkEvent) => {
      setMessages((prev) => {
        const idx = prev.findIndex((m) => m.id === ev.message_id);
        if (idx === -1) {
          // Assistant message arrived before history reload — append a stub.
          return [
            ...prev,
            {
              id: ev.message_id,
              meeting_id: meetingId,
              role: "assistant",
              content: ev.delta,
              created_at: Date.now(),
            },
          ];
        }
        const updated = [...prev];
        updated[idx] = { ...updated[idx], content: updated[idx].content + ev.delta };
        return updated;
      });
      if (ev.done && streamingIdRef.current === ev.message_id) {
        setStreamingId(null);
      }
    });
  }, [meetingId, subscribe]);

  const send = useCallback(async (content: string) => {
    setError(null);
    // Optimistic user bubble.
    const tempUser: ChatMessage = {
      id: `tmp-${Date.now()}`,
      meeting_id: meetingId,
      role: "user",
      content,
      created_at: Date.now(),
    };
    setMessages((prev) => [...prev, tempUser]);

    try {
      const { message_id } = await postChatMessage(meetingId, content);
      setStreamingId(message_id);
      // Pre-create the assistant bubble so chunks have somewhere to land.
      setMessages((prev) => [
        ...prev,
        {
          id: message_id,
          meeting_id: meetingId,
          role: "assistant",
          content: "",
          created_at: Date.now(),
        },
      ]);
    } catch (e) {
      setError(String(e));
      // Roll back optimistic user bubble on failure.
      setMessages((prev) => prev.filter((m) => m.id !== tempUser.id));
    }
  }, [meetingId]);

  return { messages, send, streamingId, error };
}
```

- [ ] **Step 3: No unit test for this hook in Phase 6.**

`useChat` is exercised end-to-end by `ChatWindow.test.tsx` in Task 6.10. A dedicated hook test would require mocking `useMeetingSocket`, which is fragile. Defer to integration coverage.

- [ ] **Step 4: Commit.**

```bash
git add web/src/lib/api.ts web/src/hooks/useChat.ts
git commit -m "feat(web): useChat hook — REST send + WS chat_chunk subscription"
```

---

### Task 6.10 · `<ChatMessage />` + `<ChatWindow />` components

**Files:**
- Create: `web/src/components/ChatMessage.tsx`
- Create: `web/src/components/ChatWindow.tsx`
- Create: `web/src/components/__tests__/ChatWindow.test.tsx`

- [ ] **Step 1: Write `ChatMessage.tsx`.**

```tsx
import type { ChatMessage as Msg } from "../lib/api";

interface Props {
  message: Msg;
  isStreaming?: boolean;
}

export function ChatMessage({ message, isStreaming = false }: Props) {
  const isUser = message.role === "user";
  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={
          isUser
            ? "max-w-[78%] px-3 py-2 rounded-2xl rounded-br-md bg-[var(--blue)] text-white text-[14px] leading-relaxed"
            : "max-w-[78%] px-3 py-2 rounded-2xl rounded-bl-md bg-[var(--card)] border border-[var(--line)] text-[var(--ink)] text-[14px] leading-relaxed"
        }
      >
        {message.content}
        {isStreaming && !isUser && (
          <span className="inline-block w-[6px] h-[14px] ml-1 align-middle bg-[var(--ink)] opacity-60 animate-pulse" />
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Write the failing test for `ChatWindow`.**

Create `web/src/components/__tests__/ChatWindow.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ChatWindow } from "../ChatWindow";

const baseMessages = [
  { id: "1", meeting_id: "m", role: "user" as const, content: "what's the topic?", created_at: 1 },
  { id: "2", meeting_id: "m", role: "assistant" as const, content: "phase 6 in-meeting chat.", created_at: 2 },
];

describe("ChatWindow", () => {
  it("renders all messages with correct alignment classes", () => {
    render(
      <ChatWindow
        messages={baseMessages}
        streamingId={null}
        onSend={() => {}}
        onCollapse={() => {}}
      />,
    );
    expect(screen.getByText(/what's the topic\?/i)).toBeInTheDocument();
    expect(screen.getByText(/phase 6 in-meeting chat\./i)).toBeInTheDocument();
  });

  it("sends on Enter and clears the input", () => {
    const onSend = vi.fn();
    render(
      <ChatWindow messages={[]} streamingId={null} onSend={onSend} onCollapse={() => {}} />,
    );
    const input = screen.getByPlaceholderText(/ask this meeting/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "hi" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSend).toHaveBeenCalledWith("hi");
    expect(input.value).toBe("");
  });

  it("does NOT call onCollapse when clicking outside the window (sticky)", () => {
    const onCollapse = vi.fn();
    render(
      <div>
        <div data-testid="outside">click me</div>
        <ChatWindow messages={[]} streamingId={null} onSend={() => {}} onCollapse={onCollapse} />
      </div>,
    );
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(onCollapse).not.toHaveBeenCalled();
  });

  it("collapses only when the caret button is clicked", () => {
    const onCollapse = vi.fn();
    render(
      <ChatWindow messages={[]} streamingId={null} onSend={() => {}} onCollapse={onCollapse} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /collapse chat/i }));
    expect(onCollapse).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 3: Write `ChatWindow.tsx`.**

```tsx
import { useEffect, useRef, useState } from "react";
import type { ChatMessage as Msg } from "../lib/api";
import { ChatMessage } from "./ChatMessage";

interface Props {
  messages: Msg[];
  streamingId: string | null;
  onSend: (content: string) => void;
  onCollapse: () => void;
}

export function ChatWindow({ messages, streamingId, onSend, onCollapse }: Props) {
  const [draft, setDraft] = useState("");
  const scrollerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom on new messages / new tokens.
  useEffect(() => {
    if (scrollerRef.current) {
      scrollerRef.current.scrollTop = scrollerRef.current.scrollHeight;
    }
  }, [messages]);

  const submit = () => {
    const v = draft.trim();
    if (!v) return;
    onSend(v);
    setDraft("");
  };

  return (
    <div
      // role="dialog" without modal semantics — chat is sticky but does NOT trap focus or
      // dim the page (PRD §5.4: notes stay live behind it). Clicks outside intentionally
      // do nothing; only the caret collapses the window.
      role="region"
      aria-label="Ask the meeting chat"
      className="
        fixed bottom-6 left-1/2 -translate-x-1/2
        w-[480px] max-h-[60vh]
        flex flex-col
        bg-[var(--paper)] border border-[var(--line)] rounded-2xl
        shadow-pop
        anim-popUp
        z-30
      "
    >
      <header className="flex items-center justify-between px-4 py-3 border-b border-[var(--line)]">
        <div className="flex items-center gap-2">
          {/* TODO: swap for actual swirl logo component from Phase 1 */}
          <div className="h-5 w-5 rounded-full bg-[var(--blue)]" aria-hidden="true" />
          <span className="text-[13px] font-medium text-[var(--ink)]">Ask the meeting</span>
        </div>
        <button
          type="button"
          aria-label="Collapse chat"
          onClick={onCollapse}
          className="h-7 w-7 rounded-md flex items-center justify-center text-[var(--mut)] hover:bg-[var(--blsoft)]"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M2.5 4.5L6 8L9.5 4.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      </header>

      <div
        ref={scrollerRef}
        className="flex-1 overflow-y-auto px-4 py-3 space-y-2 min-h-[120px]"
      >
        {messages.length === 0 ? (
          <p className="text-[13px] text-[var(--mut)] italic text-center pt-6">
            Ask anything about what's been said so far.
          </p>
        ) : (
          messages.map((m) => (
            <ChatMessage
              key={m.id}
              message={m}
              isStreaming={streamingId === m.id}
            />
          ))
        )}
      </div>

      <div className="border-t border-[var(--line)] px-3 py-3">
        <input
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          placeholder="Ask this meeting…"
          className="
            w-full h-9 px-3
            bg-[var(--card)] border border-[var(--line)] rounded-full
            text-[14px] text-[var(--ink)] placeholder:text-[var(--mut)]
            focus:outline-none focus:border-[var(--blue)]
          "
        />
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run the test — expect PASS.**

Run: `pnpm --dir web test ChatWindow`
Expected: `4 passed`. If the "click outside doesn't collapse" test fails, you probably added an outside-click listener — remove it (the spec explicitly forbids dismissal on outside-click).

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/ChatMessage.tsx web/src/components/ChatWindow.tsx web/src/components/__tests__/ChatWindow.test.tsx
git commit -m "feat(web): ChatWindow + ChatMessage with sticky (no click-outside dismiss)"
```

---

### Task 6.11 · Mount on `Meeting.tsx` and `MeetingPost.tsx` + smoke

**Files:**
- Modify: `web/src/routes/Meeting.tsx`
- Modify: `web/src/routes/MeetingPost.tsx`

- [ ] **Step 1: Extract a tiny `<AskExperience />` wrapper to avoid duplicating logic across two routes.**

Create `web/src/components/AskExperience.tsx`:

```tsx
import { useState } from "react";
import { AskPill } from "./AskPill";
import { ChatWindow } from "./ChatWindow";
import { useChat } from "../hooks/useChat";

interface Props { meetingId: string }

export function AskExperience({ meetingId }: Props) {
  const [open, setOpen] = useState(false);
  const { messages, send, streamingId } = useChat(meetingId);

  return open ? (
    <ChatWindow
      messages={messages}
      streamingId={streamingId}
      onSend={send}
      onCollapse={() => setOpen(false)}
    />
  ) : (
    <AskPill onExpand={() => setOpen(true)} />
  );
}
```

- [ ] **Step 2: Mount in `web/src/routes/Meeting.tsx`.**

At the bottom of the JSX tree (sibling to the notes column, not nested inside it — `position: fixed` makes nesting harmless but keeping it at the route root makes intent obvious):

```tsx
import { AskExperience } from "../components/AskExperience";

// ...inside the route component:
return (
  <main className="...">
    {/* existing notes column + transcript dock */}
    <NotesEditor ... />
    <TranscriptDock ... />

    <AskExperience meetingId={meetingId} />
  </main>
);
```

- [ ] **Step 3: Mount identically in `web/src/routes/MeetingPost.tsx`.**

Same import + same `<AskExperience meetingId={meetingId} />` at the route root. PRD §5.4: pill persists into the post-meeting view.

- [ ] **Step 4: Smoke test.**

Two-terminal: `pnpm --dir web dev` + `cargo run -p yogurt -- start --dev`.

1. Open `localhost:7878/meetings/<some-id>` (use a meeting created via the existing Phase 5 POST).
2. Confirm pill renders centered at the bottom, 480px wide, 24px from the edge. Notes editor is unobscured.
3. Click the pill — chat window pops up over 260ms (visible as a quick scale-from-0.96 + fade). No layout shift in the notes column.
4. Type "what was said so far?" + Enter. User bubble appears right-aligned in blueberry. Assistant bubble appears left-aligned in cream and begins streaming tokens within ~2s (assuming an LLM key is configured per Phase 5).
5. Click somewhere in the notes editor — verify the notes column is still editable and the chat does NOT collapse.
6. Click the chevron in the chat header — chat collapses back to the pill.
7. Press `⌘K` while the pill is showing — chat re-opens with prior conversation intact.
8. Navigate to `localhost:7878/meetings/<id>/post` — pill is present in the post-meeting view too, conversation history loads.

- [ ] **Step 5: Run the full test suite to catch regressions.**

Run: `cargo test --workspace && pnpm --dir web test`
Expected: all green.

- [ ] **Step 6: Commit.**

```bash
git add web/src/routes/Meeting.tsx web/src/routes/MeetingPost.tsx web/src/components/AskExperience.tsx
git commit -m "feat(web): mount AskExperience on Meeting + MeetingPost routes"
```

---

## Phase 6 acceptance criteria

All five must be true:

1. `cargo test --workspace` passes (including `chat_streaming` and `yogurt-db::chat` tests).
2. `pnpm --dir web test` passes (including `AskPill` + `ChatWindow` suites).
3. **Streaming contract:** POSTing to `/api/meetings/:id/chat` returns a `message_id` < 100ms; the first `chat_chunk` WS event arrives < 2s after POST when pointed at a working LLM endpoint; chunks for one `message_id` arrive in order and conclude with `done: true`.
4. **UX contract per PRD §5.4 and §16.5:** pill is 480px wide, anchored bottom-center 24px from the edge; ⌘K and click both expand it; the expand animation is exactly 260ms ease-out; the chat does NOT collapse on outside-click; notes column behind the chat remains editable (no dim, no z-index trap).
5. **Persistence:** reopening a meeting (route remount) loads all prior `chat_messages` rows for that meeting in chronological order; assistant messages have their final concatenated content after the stream completes.

## What this phase does NOT do

Explicitly out of scope (deferred to v2 or later phases):

- **Cross-meeting chat / semantic search** (v2 per PRD §6 item 3).
- **Markdown rendering inside chat bubbles** — plain text only.
- **Per-message regenerate / retry button** — on stream error, the user resends manually. We DO surface a `[stream error: ...]` chunk so failure is visible.
- **Chat history pagination** — load all messages on open. Acceptable for v1; meetings should rarely accumulate >50 messages.
- **Meetings-table foreign key enforcement at the schema level.** The FK is declared in V002 and depends on Phase 5's `meetings(id)` stub. Phase 7 (library) will land the full `meetings` schema. If a Phase 5 sequencing slip means the stub doesn't exist when this phase merges, drop the FK clause and re-add it via `V003__chat_messages_fk.sql` in Phase 7 — clearly noted at the top of `V002__chat_messages.sql` (Task 6.1, Step 2).
- **Image / file attachments in chat** — text only.
- **Chat export to markdown** — out of scope; users can copy/paste.

## Notes on prior-phase assumptions

- **`yogurt-llm::LlmClient::stream_chat(messages) -> Result<impl Stream<Item = Result<String>>>`** is assumed to exist from Phase 5. If Phase 5 only shipped `complete_chat` (non-streaming), Task 6.6 will need to extend the trait first — flag this during the Task 6.5 prep step and add a `0.5d` if so.
- **`useMeetingSocket(meetingId)`** is assumed to exist from Phase 3 with a `subscribe(eventType, handler)` API. If Phase 3 only exposes a raw `WebSocket` object, wrap it in a thin event-emitter in Task 6.9 (5 lines) rather than refactoring Phase 3.
- **`yogurt_prompts::CHAT_SYSTEM` (or equivalent loader)** is assumed to exist from Phase 4. Confirm the symbol during Task 6.6, Step 3.
- **`db.get_meeting_transcript(meeting_id)`** is assumed to return the transcript joined to a single `String` from Phase 5 (or Phase 3 if transcript persistence landed earlier). If the API differs, adapt the call site in `spawn_stream` without redesigning the streaming flow.

## Next plan

After Phase 6 lands, write `docs/superpowers/plans/<date>-yogurt-phase-7-library-onboarding.md` covering:
- Library home (sidebar + folders + date-grouped meeting cards + search) per PRD §5.9.
- Onboarding `/welcome` flow per PRD §5.10.
- Empty / error states per PRD §5.11.
- **Critical for Phase 6 closure:** Phase 7's full `meetings` schema landing must verify the V002 `chat_messages` FK still resolves; add a migration test that round-trips a chat insert against the production schema.

Subsequent phase plans follow the PRD §12 roadmap (Phase 8: local STT; Phase 9: polish + distribution).
