# Yogurt v1 — Phase 5: LLM Client + Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the MockLLM from Phase 4 with a real OpenAI-compatible LLM client, persist provider/general settings to SQLite, store API keys in the macOS Keychain, and ship the `/settings` page that lets the user paste a Minimax (or any OpenAI-compatible) key, mark a provider active, and have the Re-enhance round-trip actually hit that provider.

**Architecture:** Two new crates — `yogurt-llm` (a single `OpenAiCompatClient` over `reqwest` that handles both non-streaming JSON and streamed SSE chat completions) and `yogurt-db` (rusqlite + `rusqlite_migration` plus a thin `keyring`-backed keychain wrapper). The Phase 4 enhance endpoint is rewired to look up the active provider from the DB, fetch the API key from the Keychain, and call `OpenAiCompatClient`. A new `/settings` page in the React app — two-column layout per PRD §5.6 — lets the user CRUD providers, switch the active one, set general options (port, "open browser on start"), and pick an input device (wired to Phase 2's `GET /api/audio/devices`).

**Tech Stack:** `rusqlite 0.32` (bundled) · `rusqlite_migration 1` · `keyring 3` · `reqwest` (workspace, with `stream` feature) · `eventsource-stream 0.2` for SSE parsing · `serde_json` · `ulid 1` · `directories 5` for `~/.yogurt/` resolution · React 19 · React Router 7 · `@tanstack/react-query 5` · `wiremock 0.6` (Rust test) · Vitest + React Testing Library.

**Reference:** `docs/PRD.md` §4 Q6 (OpenAI-compat only), §5.6 (Settings UI layout), §9 (data model — `providers` / `settings` tables added here; `meetings` / `chat_messages` deferred to Phases 6/7), §10 (REST endpoints `/api/settings`, `/api/settings/providers`, `/api/audio/devices`), §16 (design tokens — blueberry borders for active cards, dashed-border preset chips, matcha "Local-only · on" pill).

**Out of scope (deferred to later phase plans):**
- `meetings` and `chat_messages` tables — created in Phase 6 (database is shared but those migrations live alongside the features that use them).
- The actual chat endpoint (`POST /api/meetings/:id/chat`) and WebSocket streaming for chat — Phase 6. `yogurt-llm` ships the streaming primitive; only the enhance path consumes it in this phase.
- Local STT (whisper.cpp) — Phase 8. The Transcription section in Settings shows the Local card greyed-out with a "Coming in v1" badge.
- Onboarding `/welcome` flow — Phase 7. Settings here is reachable directly via `/settings`; first-run routing comes later.
- Folders / Starred / search in the library sidebar — Phase 6/7. The Settings page is hung off the existing router from Phase 3 only.

---

## File structure produced by this phase

```
yogurt/
├── Cargo.toml                                # MODIFY · add yogurt-llm, yogurt-db, workspace deps
├── crates/
│   ├── yogurt-llm/                           # NEW CRATE
│   │   ├── Cargo.toml                        # NEW
│   │   ├── src/
│   │   │   ├── lib.rs                        # NEW · LlmClient trait, ChatRequest/Response, OpenAiCompatClient
│   │   │   ├── types.rs                      # NEW · OpenAI wire types (serde)
│   │   │   └── streaming.rs                  # NEW · SSE parser → Stream<Item = ChatChunk>
│   │   └── tests/
│   │       ├── mock_server.rs                # NEW · wiremock non-streaming round-trip
│   │       └── streaming.rs                  # NEW · wiremock SSE streaming round-trip
│   ├── yogurt-db/                            # NEW CRATE
│   │   ├── Cargo.toml                        # NEW
│   │   ├── migrations/
│   │   │   └── V001__initial.sql             # NEW · settings + providers tables only
│   │   └── src/
│   │       ├── lib.rs                        # NEW · Db struct, open(), in-memory ctor
│   │       ├── migrations.rs                 # NEW · embeds + runs migrations
│   │       ├── paths.rs                      # NEW · resolve ~/.yogurt/db.sqlite via `directories`
│   │       ├── providers.rs                  # NEW · CRUD + PRESETS const
│   │       ├── settings.rs                   # NEW · KV-style get/set for general settings
│   │       └── keychain.rs                   # NEW · keyring wrapper, service="yogurt"
│   └── yogurt-server/
│       ├── Cargo.toml                        # MODIFY · depend on yogurt-llm + yogurt-db
│       └── src/
│           ├── lib.rs                        # MODIFY · construct Db + share via state
│           ├── state.rs                      # NEW · AppState { db, audio, ... }
│           ├── routes.rs                     # MODIFY · mount /api/settings handlers
│           ├── enhance.rs                    # MODIFY (Phase 4) · MockLLM → OpenAiCompatClient
│           └── api/
│               ├── mod.rs                    # NEW (or MODIFY if Phase 4 already created it)
│               └── settings.rs               # NEW · GET/PATCH /api/settings + provider routes
└── web/
    ├── package.json                          # MODIFY · add @tanstack/react-query
    └── src/
        ├── main.tsx                          # MODIFY · wrap App in QueryClientProvider
        ├── lib/
        │   ├── queryClient.ts                # NEW · TanStack Query singleton + defaults
        │   └── api/
        │       └── settings.ts               # NEW · typed fetch wrappers
        ├── routes/
        │   └── Settings.tsx                  # NEW · /settings page composition
        └── components/settings/
            ├── SidebarNav.tsx                # NEW · 212px left rail w/ section anchors + matcha pill
            ├── ProviderCard.tsx              # NEW · active provider card (blueberry border)
            ├── ProviderRow.tsx               # NEW · inactive provider row
            ├── PresetChip.tsx                # NEW · dashed-border preset chip
            ├── STTPicker.tsx                 # NEW · Cloud + Local card pair (Local disabled)
            ├── AudioSection.tsx              # NEW · input-device dropdown
            └── GeneralSection.tsx            # NEW · port + open-browser-on-start toggle
```

**Why this split:** `yogurt-llm` and `yogurt-db` are both reusable libraries with no axum/web dependencies — keeping them outside `yogurt-server` makes them trivial to test in isolation (wiremock + `:memory:` SQLite) and means Phase 6 (chat) can consume `yogurt-llm` without going through the HTTP layer. `state.rs` is a small `AppState` struct so handlers receive `State<AppState>` rather than chaining `Extension<Db>` + `Extension<AudioCtx>` + ... — it scales as Phase 6/7 add more shared state.

---

## Test conventions (this phase)

- **Rust unit tests:** `#[cfg(test)] mod tests` inside each source file. `yogurt-db` unit tests use `Db::open_in_memory()`; keychain unit tests are gated by a `--features keychain-live` flag (Keychain access prompts the user — see Task 5.4 Step 5 for the strategy).
- **Rust integration tests:** under `crates/<crate>/tests/`. `yogurt-llm` integration tests stand up `wiremock::MockServer` to act as a stand-in OpenAI server.
- **Frontend:** Vitest + React Testing Library. Tests for the Settings page mock `@tanstack/react-query` at the fetch layer via `msw` (already a dev dep candidate — added here if not present from Phase 1).
- **Acceptance test:** the end-to-end "user pastes a Minimax key → marks active → hits Re-enhance" flow is verified by `crates/yogurt-server/tests/enhance_uses_active_provider.rs` which boots a wiremock server, configures it as the active provider via the real `/api/settings` API, then triggers `/api/meetings/:id/enhance` and asserts wiremock saw the call.

---

## Phase 5 task list

11 tasks. Each task ends with a commit. Approximate sequence: ~2 days of focused work (~14–18 hours).

---

### Task 5.1 · `yogurt-db` crate skeleton + migrations

**Files:**
- Modify: `Cargo.toml` (add `yogurt-db` to workspace members + workspace deps)
- Create: `crates/yogurt-db/Cargo.toml`
- Create: `crates/yogurt-db/migrations/V001__initial.sql`
- Create: `crates/yogurt-db/src/lib.rs`
- Create: `crates/yogurt-db/src/migrations.rs`
- Create: `crates/yogurt-db/src/paths.rs`

- [ ] **Step 1: Add workspace deps + member to root `Cargo.toml`.**

Append to `[workspace] members`:

```toml
"crates/yogurt-db",
"crates/yogurt-llm",
```

(Add both now — Task 5.5 creates the `yogurt-llm` crate; declaring the member up front means `cargo metadata` doesn't have to be re-resolved later.)

Append to `[workspace.dependencies]`:

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
rusqlite_migration = "1"
keyring = "3"
directories = "5"
ulid = { version = "1", features = ["serde"] }
eventsource-stream = "0.2"
futures-util = "0.3"
wiremock = "0.6"
```

(`rusqlite`'s `bundled` feature statically links SQLite so end-users don't need a system SQLite. `directories = 5` resolves `~/.yogurt/` cross-platform but we only test the macOS path — see Task 5.2 Step 2.)

- [ ] **Step 2: Write `crates/yogurt-db/Cargo.toml`.**

```toml
[package]
name = "yogurt-db"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
rusqlite = { workspace = true }
rusqlite_migration = { workspace = true }
keyring = { workspace = true }
directories = { workspace = true }
ulid = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
thiserror = "2"
tracing = { workspace = true }

[features]
# Gated because real Keychain reads prompt the user on macOS.
keychain-live = []
```

- [ ] **Step 3: Write `crates/yogurt-db/migrations/V001__initial.sql`.**

```sql
-- yogurt schema v1 (Phase 5 subset: settings + providers only).
-- Phase 6 adds `meetings`; Phase 7 adds `chat_messages`.

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS providers (
    id            TEXT PRIMARY KEY,           -- ulid
    name          TEXT NOT NULL,              -- e.g. "Minimax", "OpenAI"
    base_url      TEXT NOT NULL,              -- e.g. "https://api.minimax.io/v1"
    model         TEXT NOT NULL DEFAULT '',   -- e.g. "MiniMax-Text-01"
    kind          TEXT NOT NULL DEFAULT 'llm',-- 'llm' | (future: 'stt')
    is_active     INTEGER NOT NULL DEFAULT 0, -- bool; exactly 0 or 1 row should be active per kind
    created_at    INTEGER NOT NULL            -- unix millis
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_providers_one_active_per_kind
    ON providers(kind) WHERE is_active = 1;

CREATE INDEX IF NOT EXISTS idx_providers_kind ON providers(kind);

-- Seed default general settings. `INSERT OR IGNORE` keeps re-runs idempotent.
INSERT OR IGNORE INTO settings(key, value) VALUES
    ('general.port', '7878'),
    ('general.open_browser_on_start', 'true'),
    ('audio.input_device', '');
```

The partial unique index enforces "at most one active provider per kind" at the DB layer — much cheaper than a stored procedure.

- [ ] **Step 4: Write the failing migrations test first.**

Create `crates/yogurt-db/tests/migrations.rs`:

```rust
use yogurt_db::Db;

#[test]
fn it_runs_migrations_on_fresh_in_memory_db() {
    let db = Db::open_in_memory().expect("open in-memory db");
    // The settings table should exist and have the seeded defaults.
    let port: String = db
        .conn()
        .query_row("SELECT value FROM settings WHERE key = 'general.port'", [], |r| r.get(0))
        .expect("seeded port row");
    assert_eq!(port, "7878");
}

#[test]
fn it_is_idempotent_to_open_twice() {
    let db1 = Db::open_in_memory().expect("first open");
    drop(db1);
    // A fresh in-memory db won't share state, but we can simulate re-open
    // semantics by running migrations on the same conn twice.
    let db2 = Db::open_in_memory().expect("second open");
    db2.run_migrations().expect("re-running migrations is safe");
}
```

Run: `cargo test -p yogurt-db --test migrations`
Expected: compile failure — `Db` doesn't exist yet.

- [ ] **Step 5: Write `crates/yogurt-db/src/paths.rs`.**

```rust
use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Returns `~/.yogurt/` and creates the dir if missing.
///
/// `directories::BaseDirs::home_dir()` is preferred over `dirs::home_dir()` —
/// `directories` is what the PRD specifies and is actively maintained.
pub fn yogurt_dir() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("could not resolve user home directory"))?;
    let path = base.home_dir().join(".yogurt");
    std::fs::create_dir_all(&path)
        .map_err(|e| anyhow!("creating {}: {e}", path.display()))?;
    Ok(path)
}

pub fn db_path() -> Result<PathBuf> {
    Ok(yogurt_dir()?.join("db.sqlite"))
}
```

- [ ] **Step 6: Write `crates/yogurt-db/src/migrations.rs`.**

```rust
use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// Migrations are embedded at compile time so the binary stays self-contained.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../migrations/V001__initial.sql")),
    ])
}

pub fn run(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    Ok(())
}
```

- [ ] **Step 7: Write `crates/yogurt-db/src/lib.rs`.**

```rust
mod migrations;
pub mod paths;

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Cloneable, thread-safe handle to the SQLite database.
///
/// `rusqlite::Connection` is not `Sync`, so we wrap it in `Arc<Mutex<_>>`.
/// For Phase 5 contention is negligible (one user, ~ms-latency queries);
/// if it becomes a bottleneck Phase 9 can swap in `r2d2-sqlite`.
#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::run(&mut conn)?;
        Ok(Self { inner: Arc::new(Mutex::new(conn)) })
    }

    pub fn open_default() -> Result<Self> {
        Self::open(paths::db_path()?)
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrations::run(&mut conn)?;
        Ok(Self { inner: Arc::new(Mutex::new(conn)) })
    }

    /// Re-run migrations (safe; idempotent). Exposed for tests.
    pub fn run_migrations(&self) -> Result<()> {
        let mut conn = self.inner.lock().expect("db mutex poisoned");
        migrations::run(&mut conn)
    }

    /// Acquire the underlying connection for a closure-based query.
    /// Prefer the module-level helpers (`providers::list`, `settings::get`, ...)
    /// over reaching into `conn()` directly.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let guard = self.inner.lock().expect("db mutex poisoned");
        f(&guard)
    }

    /// Test-only convenience accessor.
    #[doc(hidden)]
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.inner.lock().expect("db mutex poisoned")
    }
}
```

- [ ] **Step 8: Run the migrations test — expect PASS.**

Run: `cargo test -p yogurt-db --test migrations`
Expected: `it_runs_migrations_on_fresh_in_memory_db ... ok` and `it_is_idempotent_to_open_twice ... ok`.

- [ ] **Step 9: Commit.**

```bash
git add Cargo.toml crates/yogurt-db/
git commit -m "feat(db): add yogurt-db crate with rusqlite + initial migrations"
```

---

### Task 5.2 · `providers` CRUD with preset definitions

**Files:**
- Create: `crates/yogurt-db/src/providers.rs`
- Modify: `crates/yogurt-db/src/lib.rs` (add `pub mod providers;`)

- [ ] **Step 1: Write the failing CRUD test.**

Create `crates/yogurt-db/tests/providers.rs`:

```rust
use yogurt_db::{providers, Db};

#[test]
fn it_inserts_and_lists_a_provider() {
    let db = Db::open_in_memory().unwrap();
    let id = providers::insert(&db, providers::NewProvider {
        name: "Minimax".into(),
        base_url: "https://api.minimax.io/v1".into(),
        model: "MiniMax-Text-01".into(),
    }).unwrap();
    let rows = providers::list(&db).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    assert_eq!(rows[0].name, "Minimax");
    assert!(!rows[0].is_active);
}

#[test]
fn it_sets_only_one_active_provider() {
    let db = Db::open_in_memory().unwrap();
    let a = providers::insert(&db, providers::NewProvider {
        name: "A".into(), base_url: "https://a/v1".into(), model: "m".into(),
    }).unwrap();
    let b = providers::insert(&db, providers::NewProvider {
        name: "B".into(), base_url: "https://b/v1".into(), model: "m".into(),
    }).unwrap();

    providers::set_active(&db, &a).unwrap();
    assert_eq!(providers::active(&db).unwrap().unwrap().id, a);

    // Switching active to B should atomically deactivate A.
    providers::set_active(&db, &b).unwrap();
    let active = providers::active(&db).unwrap().unwrap();
    assert_eq!(active.id, b);
    let all = providers::list(&db).unwrap();
    let active_count = all.iter().filter(|p| p.is_active).count();
    assert_eq!(active_count, 1, "exactly one provider should be active");
}

#[test]
fn it_exposes_presets_as_a_const_slice() {
    let names: Vec<&str> = providers::PRESETS.iter().map(|p| p.name).collect();
    assert!(names.contains(&"Minimax"));
    assert!(names.contains(&"OpenAI"));
    assert!(names.contains(&"Ollama (local)"));
    assert!(names.contains(&"LM Studio (local)"));
    assert!(names.contains(&"OpenRouter"));
}
```

Run: `cargo test -p yogurt-db --test providers`
Expected: compile failure — `providers` module doesn't exist.

- [ ] **Step 2: Write `crates/yogurt-db/src/providers.rs`.**

```rust
use crate::Db;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Built-in provider presets surfaced as dashed-border chips in the Model
/// section's "CLONE A PRESET" row (PRD §5.6).
pub struct Preset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset { name: "Minimax",         base_url: "https://api.minimax.io/v1",  default_model: "MiniMax-Text-01" },
    Preset { name: "OpenAI",          base_url: "https://api.openai.com/v1",  default_model: "gpt-4o-mini" },
    Preset { name: "Ollama (local)",  base_url: "http://localhost:11434/v1",  default_model: "llama3.2" },
    Preset { name: "LM Studio (local)", base_url: "http://localhost:1234/v1", default_model: "" },
    Preset { name: "OpenRouter",      base_url: "https://openrouter.ai/api/v1", default_model: "anthropic/claude-3.5-sonnet" },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub is_active: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewProvider {
    pub name: String,
    pub base_url: String,
    pub model: String,
}

pub fn insert(db: &Db, p: NewProvider) -> Result<String> {
    let id = Ulid::new().to_string();
    let now = chrono_millis();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO providers (id, name, base_url, model, kind, is_active, created_at) \
             VALUES (?1, ?2, ?3, ?4, 'llm', 0, ?5)",
            params![id, p.name, p.base_url, p.model, now],
        )
    })?;
    Ok(id)
}

pub fn list(db: &Db) -> Result<Vec<Provider>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, model, is_active, created_at \
             FROM providers WHERE kind='llm' ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Provider {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    base_url: r.get(2)?,
                    model: r.get(3)?,
                    is_active: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok::<_, rusqlite::Error>(rows)
    })
    .map_err(Into::into)
}

pub fn active(db: &Db) -> Result<Option<Provider>> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT id, name, base_url, model, is_active, created_at \
             FROM providers WHERE kind='llm' AND is_active=1",
            [],
            |r| Ok(Provider {
                id: r.get(0)?,
                name: r.get(1)?,
                base_url: r.get(2)?,
                model: r.get(3)?,
                is_active: true,
                created_at: r.get(5)?,
            }),
        )
        .optional()
    })
    .map_err(Into::into)
}

/// Atomically deactivate all providers of the same kind and activate `id`.
pub fn set_active(db: &Db, id: &str) -> Result<()> {
    db.with_conn(|conn| -> rusqlite::Result<()> {
        // We can't borrow `conn` mutably here (we hold an immutable ref through
        // the closure), so use a transaction via the helper method.
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let res: rusqlite::Result<()> = (|| {
            conn.execute("UPDATE providers SET is_active=0 WHERE kind='llm'", [])?;
            let updated = conn.execute(
                "UPDATE providers SET is_active=1 WHERE id=?1 AND kind='llm'",
                params![id],
            )?;
            if updated == 0 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            Ok(())
        })();
        match res {
            Ok(()) => { conn.execute_batch("COMMIT")?; Ok(()) }
            Err(e) => { let _ = conn.execute_batch("ROLLBACK"); Err(e) }
        }
    })?;
    Ok(())
}

pub fn update(db: &Db, id: &str, name: &str, base_url: &str, model: &str) -> Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE providers SET name=?2, base_url=?3, model=?4 WHERE id=?1",
            params![id, name, base_url, model],
        )
    })?;
    Ok(())
}

pub fn delete(db: &Db, id: &str) -> Result<()> {
    db.with_conn(|conn| {
        conn.execute("DELETE FROM providers WHERE id=?1", params![id])
    })?;
    Ok(())
}

fn chrono_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
```

- [ ] **Step 3: Wire the module in `lib.rs`.**

Add `pub mod providers;` after the `pub mod paths;` line.

- [ ] **Step 4: Run the tests — expect PASS.**

Run: `cargo test -p yogurt-db`
Expected: all 5 tests across the two test files pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-db/
git commit -m "feat(db): add providers CRUD with built-in presets + single-active invariant"
```

---

### Task 5.3 · `settings` KV store

**Files:**
- Create: `crates/yogurt-db/src/settings.rs`
- Modify: `crates/yogurt-db/src/lib.rs` (`pub mod settings;`)

- [ ] **Step 1: Write the failing tests.**

Create `crates/yogurt-db/tests/settings.rs`:

```rust
use yogurt_db::{settings, Db};

#[test]
fn it_returns_seeded_defaults() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(settings::get(&db, "general.port").unwrap().as_deref(), Some("7878"));
    assert_eq!(settings::get(&db, "general.open_browser_on_start").unwrap().as_deref(), Some("true"));
}

#[test]
fn it_upserts_a_value() {
    let db = Db::open_in_memory().unwrap();
    settings::set(&db, "general.port", "9000").unwrap();
    assert_eq!(settings::get(&db, "general.port").unwrap().as_deref(), Some("9000"));
    settings::set(&db, "general.port", "9001").unwrap();
    assert_eq!(settings::get(&db, "general.port").unwrap().as_deref(), Some("9001"));
}

#[test]
fn it_loads_typed_general_struct() {
    let db = Db::open_in_memory().unwrap();
    let g = settings::load_general(&db).unwrap();
    assert_eq!(g.port, 7878);
    assert!(g.open_browser_on_start);
}
```

- [ ] **Step 2: Write `crates/yogurt-db/src/settings.rs`.**

```rust
use crate::Db;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

pub fn get(db: &Db, key: &str) -> Result<Option<String>> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
    })
    .map_err(Into::into)
}

pub fn set(db: &Db, key: &str, value: &str) -> Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )
    })?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    pub port: u16,
    pub open_browser_on_start: bool,
    pub audio_input_device: String,
}

pub fn load_general(db: &Db) -> Result<General> {
    Ok(General {
        port: get(db, "general.port")?
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7878),
        open_browser_on_start: get(db, "general.open_browser_on_start")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true),
        audio_input_device: get(db, "audio.input_device")?.unwrap_or_default(),
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneralPatch {
    pub port: Option<u16>,
    pub open_browser_on_start: Option<bool>,
    pub audio_input_device: Option<String>,
}

pub fn save_general_patch(db: &Db, patch: GeneralPatch) -> Result<General> {
    if let Some(p) = patch.port {
        set(db, "general.port", &p.to_string())?;
    }
    if let Some(o) = patch.open_browser_on_start {
        set(db, "general.open_browser_on_start", if o { "true" } else { "false" })?;
    }
    if let Some(d) = patch.audio_input_device {
        set(db, "audio.input_device", &d)?;
    }
    load_general(db)
}
```

- [ ] **Step 3: Wire the module in `lib.rs`.**

Add `pub mod settings;`.

- [ ] **Step 4: Run.**

Run: `cargo test -p yogurt-db`
Expected: all tests pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-db/
git commit -m "feat(db): add general settings KV store with typed load/patch helpers"
```

---

### Task 5.4 · `keychain` wrapper

**Files:**
- Create: `crates/yogurt-db/src/keychain.rs`
- Modify: `crates/yogurt-db/src/lib.rs` (`pub mod keychain;`)

> **⚠ Note:** The `keyring` crate touches the real macOS Keychain on every call, which prompts the user the first time. Unit-testing it directly is impractical in CI. We define an `ApiKeyStore` trait so the server can be wired with a memory-backed fake in tests and the real keychain in production. The `keychain-live` feature flag (declared in Task 5.1) gates a manual integration test that only runs locally.

- [ ] **Step 1: Write the test for the trait + memory impl.**

Create `crates/yogurt-db/tests/keychain.rs`:

```rust
use yogurt_db::keychain::{ApiKeyStore, MemoryKeyStore};

#[test]
fn memory_store_roundtrips() {
    let store = MemoryKeyStore::default();
    assert_eq!(store.get("prov_abc").unwrap(), None);
    store.set("prov_abc", "sk-test-1234").unwrap();
    assert_eq!(store.get("prov_abc").unwrap().as_deref(), Some("sk-test-1234"));
    store.delete("prov_abc").unwrap();
    assert_eq!(store.get("prov_abc").unwrap(), None);
}

#[test]
fn memory_store_returns_masked_last_four() {
    let store = MemoryKeyStore::default();
    store.set("prov_abc", "sk-supersecret-9876").unwrap();
    let mask = store.masked("prov_abc").unwrap();
    assert_eq!(mask.as_deref(), Some("••••9876"));
}
```

Run: `cargo test -p yogurt-db --test keychain`
Expected: compile failure.

- [ ] **Step 2: Write `crates/yogurt-db/src/keychain.rs`.**

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Mutex;

/// All Keychain entries are namespaced under this service so uninstall +
/// reinstall doesn't leak keys (per PRD §5.6 constraint).
pub const SERVICE: &str = "yogurt";

/// Storage abstraction so handlers and tests can share the same code path.
pub trait ApiKeyStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;

    /// Convenience: returns "••••XXXX" with the last 4 chars of the secret, or None.
    fn masked(&self, account: &str) -> Result<Option<String>> {
        Ok(self.get(account)?.map(|s| {
            let tail = s.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>();
            format!("••••{tail}")
        }))
    }
}

/// In-memory fake for tests and for the server when the real Keychain is
/// unavailable (e.g. CI). Used by `crates/yogurt-server/tests/*`.
#[derive(Default)]
pub struct MemoryKeyStore {
    inner: Mutex<HashMap<String, String>>,
}

impl ApiKeyStore for MemoryKeyStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.inner.lock().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        self.inner.lock().unwrap().insert(account.to_string(), secret.to_string());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        self.inner.lock().unwrap().remove(account);
        Ok(())
    }
}

/// Real macOS Keychain implementation via the `keyring` crate.
pub struct KeychainStore;

impl ApiKeyStore for KeychainStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        entry.set_password(secret)?;
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
```

- [ ] **Step 3: Wire the module + add the `keychain-live` integration test.**

Add `pub mod keychain;` to `lib.rs`. Then create `crates/yogurt-db/tests/keychain_live.rs`:

```rust
#![cfg(feature = "keychain-live")]
//! Manual: run with `cargo test -p yogurt-db --features keychain-live -- --ignored`.
//! Requires user to approve the Keychain prompts on first run.

use yogurt_db::keychain::{ApiKeyStore, KeychainStore};

#[test]
#[ignore]
fn it_roundtrips_against_real_keychain() {
    let store = KeychainStore;
    let account = "yogurt-test-acct";
    store.set(account, "real-secret-XYZA").unwrap();
    assert_eq!(store.get(account).unwrap().as_deref(), Some("real-secret-XYZA"));
    store.delete(account).unwrap();
    assert_eq!(store.get(account).unwrap(), None);
}
```

- [ ] **Step 4: Run the unit tests.**

Run: `cargo test -p yogurt-db --test keychain`
Expected: 2 passed.

- [ ] **Step 5: Manual verification on the workstation (do NOT run in CI).**

Run: `cargo test -p yogurt-db --features keychain-live -- --ignored`
Expected: macOS prompts twice for Keychain access, both prompts approved → 1 passed.

(If running from inside a sandboxed terminal where Keychain prompts can't surface, skip this step and verify via the end-to-end acceptance test in Task 5.11 instead.)

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-db/
git commit -m "feat(db): add Keychain wrapper with ApiKeyStore trait + memory fake for tests"
```

---

### Task 5.5 · `yogurt-llm` crate — non-streaming OpenAI-compatible client

**Files:**
- Create: `crates/yogurt-llm/Cargo.toml`
- Create: `crates/yogurt-llm/src/lib.rs`
- Create: `crates/yogurt-llm/src/types.rs`
- Create: `crates/yogurt-llm/tests/mock_server.rs`

- [ ] **Step 1: Write `crates/yogurt-llm/Cargo.toml`.**

```toml
[package]
name = "yogurt-llm"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
futures-util = { workspace = true }
eventsource-stream = { workspace = true }
async-trait = "0.1"
anyhow = { workspace = true }
thiserror = "2"
tracing = { workspace = true }

[dev-dependencies]
wiremock = { workspace = true }
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Write the failing wiremock test first.**

Create `crates/yogurt-llm/tests/mock_server.rs`:

```rust
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yogurt_llm::{ChatMessage, ChatRequest, LlmClient, OpenAiCompatClient};

#[tokio::test]
async fn it_sends_messages_and_returns_assistant_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hello yogurt." },
                "finish_reason": "stop"
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "gpt-4o-mini".into());
    let resp = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect("client call ok");
    assert_eq!(resp.content, "Hello yogurt.");
}

#[tokio::test]
async fn it_surfaces_4xx_as_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "message": "Invalid API key", "type": "auth_error" }
        })))
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-bad".into(), "gpt-4o-mini".into());
    let err = client
        .complete(ChatRequest { messages: vec![ChatMessage::user("hi")], stream: false })
        .await
        .expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("401") || msg.contains("Invalid API key"), "got: {msg}");
}
```

Run: `cargo test -p yogurt-llm --test mock_server`
Expected: compile failure — types don't exist yet.

- [ ] **Step 3: Write `crates/yogurt-llm/src/types.rs`.**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
}

impl ChatMessage {
    pub fn system(s: impl Into<String>) -> Self { Self { role: "system".into(), content: s.into() } }
    pub fn user(s: impl Into<String>) -> Self { Self { role: "user".into(), content: s.into() } }
    pub fn assistant(s: impl Into<String>) -> Self { Self { role: "assistant".into(), content: s.into() } }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct ChatChunk {
    pub delta: String,
    pub done: bool,
}

// ─── Wire types (private; serde mirrors of the OpenAI shape) ─────────────────

#[derive(Serialize)]
pub(crate) struct OpenAiRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [ChatMessage],
    pub stream: bool,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiResponse {
    pub model: String,
    pub choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiChoice {
    pub message: ChatMessage,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiStreamChunk {
    pub choices: Vec<OpenAiStreamChoice>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiStreamChoice {
    pub delta: OpenAiDelta,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiDelta {
    #[serde(default)]
    pub content: Option<String>,
}
```

- [ ] **Step 4: Write `crates/yogurt-llm/src/lib.rs` (non-streaming portion).**

```rust
mod streaming;
mod types;

pub use types::{ChatChunk, ChatMessage, ChatRequest, ChatResponse};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::stream::BoxStream;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse>;

    /// Streaming variant. Implementations should set `stream=true` regardless of
    /// the caller-supplied value; the parameter exists so the same `ChatRequest`
    /// type works for both paths.
    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>>;
}

#[derive(Clone)]
pub struct OpenAiCompatClient {
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl OpenAiCompatClient {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("reqwest client builds with defaults");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            http,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatClient {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse> {
        let body = types::OpenAiRequest {
            model: &self.model,
            messages: &req.messages,
            stream: false,
        };
        let resp = self.http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("LLM call failed: {status} — {body}"));
        }
        let parsed: types::OpenAiResponse = resp.json().await?;
        let content = parsed.choices.into_iter().next()
            .ok_or_else(|| anyhow!("no choices in response"))?
            .message.content;
        Ok(ChatResponse { content, model: parsed.model })
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        streaming::stream(self, req).await
    }
}
```

- [ ] **Step 5: Write a placeholder `streaming.rs` (real impl in Task 5.6).**

```rust
use crate::{ChatChunk, ChatRequest, OpenAiCompatClient};
use anyhow::Result;
use futures_util::stream::BoxStream;

pub(crate) async fn stream(
    _client: &OpenAiCompatClient,
    _req: ChatRequest,
) -> Result<BoxStream<'static, Result<ChatChunk>>> {
    anyhow::bail!("streaming not yet implemented (Task 5.6)")
}
```

- [ ] **Step 6: Run.**

Run: `cargo test -p yogurt-llm --test mock_server`
Expected: 2 passed.

- [ ] **Step 7: Commit.**

```bash
git add Cargo.toml crates/yogurt-llm/
git commit -m "feat(llm): add OpenAI-compatible non-streaming client with wiremock tests"
```

---

### Task 5.6 · SSE streaming in `yogurt-llm`

**Files:**
- Modify: `crates/yogurt-llm/src/streaming.rs`
- Create: `crates/yogurt-llm/tests/streaming.rs`

- [ ] **Step 1: Write the failing streaming test.**

Create `crates/yogurt-llm/tests/streaming.rs`:

```rust
use futures_util::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yogurt_llm::{ChatMessage, ChatRequest, LlmClient, OpenAiCompatClient};

#[tokio::test]
async fn it_streams_sse_chunks_into_chat_chunks() {
    // OpenAI's SSE format: each event is `data: {json}\n\n`, terminated by `data: [DONE]\n\n`.
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo \"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"yogurt.\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "gpt-4o-mini".into());
    let mut stream = client
        .stream(ChatRequest { messages: vec![ChatMessage::user("hi")], stream: true })
        .await
        .expect("stream opens");

    let mut deltas = Vec::new();
    let mut saw_done = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("chunk ok");
        if !chunk.delta.is_empty() { deltas.push(chunk.delta); }
        if chunk.done { saw_done = true; }
    }
    assert_eq!(deltas.join(""), "Hello yogurt.");
    assert!(saw_done, "stream should emit a terminal `done=true` chunk");
}
```

- [ ] **Step 2: Replace `crates/yogurt-llm/src/streaming.rs` with the real impl.**

```rust
use crate::{types, ChatChunk, ChatRequest, OpenAiCompatClient};
use anyhow::{anyhow, Result};
use eventsource_stream::Eventsource;
use futures_util::stream::{BoxStream, StreamExt};

pub(crate) async fn stream(
    client: &OpenAiCompatClient,
    req: ChatRequest,
) -> Result<BoxStream<'static, Result<ChatChunk>>> {
    let body = serde_json::to_value(types::OpenAiRequest {
        model: client_model(client),
        messages: &req.messages,
        stream: true,
    })?;

    let resp = http(client)
        .post(format!("{}/chat/completions", base_url(client)))
        .bearer_auth(api_key(client))
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("LLM stream open failed: {status} — {body}"));
    }

    let byte_stream = resp.bytes_stream();
    let events = byte_stream.eventsource();

    let mapped = events.map(|ev| -> Result<ChatChunk> {
        let ev = ev.map_err(|e| anyhow!("SSE parse error: {e}"))?;
        if ev.data.trim() == "[DONE]" {
            return Ok(ChatChunk { delta: String::new(), done: true });
        }
        let chunk: types::OpenAiStreamChunk = serde_json::from_str(&ev.data)
            .map_err(|e| anyhow!("invalid chunk JSON: {e} — payload: {}", ev.data))?;
        let (delta, done) = match chunk.choices.into_iter().next() {
            Some(choice) => (
                choice.delta.content.unwrap_or_default(),
                choice.finish_reason.is_some(),
            ),
            None => (String::new(), false),
        };
        Ok(ChatChunk { delta, done })
    });

    Ok(mapped.boxed())
}

// Tiny accessor helpers to avoid making client fields `pub(crate)`.
fn http(c: &OpenAiCompatClient) -> &reqwest::Client { c.http_for_streaming() }
fn base_url(c: &OpenAiCompatClient) -> &str { c.base_url_for_streaming() }
fn api_key(c: &OpenAiCompatClient) -> &str { c.api_key_for_streaming() }
fn client_model(c: &OpenAiCompatClient) -> &str { c.model_for_streaming() }
```

- [ ] **Step 3: Expose the four accessors on `OpenAiCompatClient` (in `lib.rs`).**

Append to the `impl OpenAiCompatClient` block:

```rust
    #[doc(hidden)] pub fn http_for_streaming(&self) -> &reqwest::Client { &self.http }
    #[doc(hidden)] pub fn base_url_for_streaming(&self) -> &str { &self.base_url }
    #[doc(hidden)] pub fn api_key_for_streaming(&self) -> &str { &self.api_key }
    #[doc(hidden)] pub fn model_for_streaming(&self) -> &str { &self.model }
```

(These are deliberately not part of the public surface — `#[doc(hidden)]` keeps them out of the rendered docs while still being accessible to the sibling `streaming.rs` module.)

- [ ] **Step 4: Run.**

Run: `cargo test -p yogurt-llm`
Expected: all 3 tests pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-llm/
git commit -m "feat(llm): add SSE streaming via eventsource-stream"
```

---

### Task 5.7b · `.env.local` bootstrap — seed providers from env vars on first run

> **⚠ Dependency order:** This task depends on `AppState`, which is created in **Task 5.7** below. Execute Task 5.7 first, then return to this task. (Numbering is `5.7b` to reflect this ordering; the task is listed here in the file because it pairs conceptually with the LLM client section above.)

**Files:**
- Modify: `Cargo.toml` (add `dotenvy = "0.15"` to workspace deps)
- Modify: `crates/yogurt-cli/Cargo.toml` (add `dotenvy = { workspace = true }`)
- Modify: `crates/yogurt-cli/src/main.rs` (load `.env.local` before anything else)
- Create: `crates/yogurt-server/src/bootstrap.rs` — `seed_from_env(state) -> Result<SeedReport>`
- Modify: `crates/yogurt-server/src/lib.rs` (call `bootstrap::seed_from_env()` after `AppState` is built, before serving)
- Create: `crates/yogurt-server/tests/bootstrap.rs`

**Why this task exists:** The user has a Minimax key in `.env.local` and contributors will too. Without this step they'd have to re-paste keys into the Settings UI after every `~/.yogurt/db.sqlite` reset, which is painful. Aligns with PRD §5.6 "Dev convenience — env-var bootstrap" subsection.

- [ ] **Step 1: Add `dotenvy` to workspace + yogurt-cli deps.**

Workspace `Cargo.toml`:
```toml
dotenvy = "0.15"
```

`crates/yogurt-cli/Cargo.toml`:
```toml
dotenvy = { workspace = true }
```

- [ ] **Step 2: Load `.env.local` at the top of `main()`.**

Modify `crates/yogurt-cli/src/main.rs` — add as the very first line of `main()`, before `tracing_subscriber::fmt()`:

```rust
// Load .env.local if present. Errors are silently ignored — production users
// install via brew and never have a .env.local. The file is gitignored.
let _ = dotenvy::from_filename(".env.local");
```

- [ ] **Step 3: Define the env-var → preset mapping table.**

Create `crates/yogurt-server/src/bootstrap.rs`:

```rust
//! On first run, seed the providers table from `YOGURT_*_API_KEY` env vars.
//! Idempotent — never overwrites existing rows or keys.

use crate::state::AppState;
use anyhow::Result;
use yogurt_db::providers::{NewProvider, ProviderKind};

const ENV_PRESETS: &[(&str, &str, &str, &str, ProviderKind)] = &[
    // (env_var, name, base_url, default_model, kind)
    ("YOGURT_MINIMAX_API_KEY",    "Minimax",    "https://api.minimax.io/v1",  "MiniMax-Text-01",                 ProviderKind::Llm),
    ("YOGURT_OPENAI_API_KEY",     "OpenAI",     "https://api.openai.com/v1",  "gpt-4o-mini",                     ProviderKind::Llm),
    ("YOGURT_OPENROUTER_API_KEY", "OpenRouter", "https://openrouter.ai/api/v1", "anthropic/claude-3.5-sonnet",  ProviderKind::Llm),
    ("YOGURT_DEEPGRAM_API_KEY",   "Deepgram",   "https://api.deepgram.com/v1", "nova-2",                         ProviderKind::Stt),
    ("YOGURT_ASSEMBLYAI_API_KEY", "AssemblyAI", "https://api.assemblyai.com/v2", "best",                         ProviderKind::Stt),
    ("YOGURT_GROQ_API_KEY",       "Groq",       "https://api.groq.com/openai/v1", "whisper-large-v3-turbo",     ProviderKind::Stt),
];

#[derive(Debug, Default)]
pub struct SeedReport {
    pub seeded: Vec<String>,   // provider names newly seeded
    pub skipped: Vec<String>,  // env vars present but provider already exists
}

pub async fn seed_from_env(state: &AppState) -> Result<SeedReport> {
    let mut report = SeedReport::default();
    let existing_names = state.db.providers().list_names()?;

    for &(env_var, name, base_url, model, kind) in ENV_PRESETS {
        let Ok(key) = std::env::var(env_var) else { continue };
        if key.trim().is_empty() { continue; }
        if existing_names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            report.skipped.push(name.to_string());
            continue;
        }

        // Insert the provider row (no key in DB).
        let id = state.db.providers().create(NewProvider {
            name: name.to_string(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            kind,
        })?;

        // Store the key in Keychain. Failures are logged but non-fatal —
        // dev users can re-enter via Settings UI.
        if let Err(e) = state.keychain.set(&id, &key) {
            tracing::warn!(provider = name, error = %e, "failed to store key in keychain");
        }

        // If this is the first LLM provider seeded, mark it active.
        if matches!(kind, ProviderKind::Llm)
            && state.db.providers().active_llm()?.is_none()
        {
            state.db.providers().set_active(&id)?;
        }

        report.seeded.push(name.to_string());
    }

    Ok(report)
}
```

- [ ] **Step 4: Call `seed_from_env` after AppState construction.**

In `crates/yogurt-server/src/lib.rs`, inside `run()` after the DB connects and the `AppState` is built, before `axum::serve(...)`:

```rust
match bootstrap::seed_from_env(&state).await {
    Ok(report) => {
        if !report.seeded.is_empty() {
            tracing::info!(seeded = ?report.seeded, "seeded providers from .env.local");
        }
        if !report.skipped.is_empty() {
            tracing::debug!(skipped = ?report.skipped, "skipped already-configured providers");
        }
    }
    Err(e) => tracing::error!(error = %e, "bootstrap failed; continuing without env-var seeding"),
}
```

- [ ] **Step 5: TDD — bootstrap test.**

Create `crates/yogurt-server/tests/bootstrap.rs`:

```rust
use yogurt_db::Db;
use yogurt_server::{bootstrap, state::AppState};

#[tokio::test]
async fn it_seeds_minimax_from_env() {
    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-test-minimax-12345");
    let state = AppState::test_in_memory(); // helper that uses :memory: db + memory keystore
    let report = bootstrap::seed_from_env(&state).await.expect("ok");
    assert_eq!(report.seeded, vec!["Minimax"]);
    let active = state.db.providers().active_llm().unwrap().expect("active provider");
    assert_eq!(active.name, "Minimax");
    let stored_key = state.keychain.get(&active.id).expect("key in store");
    assert_eq!(stored_key, "sk-test-minimax-12345");
    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
}

#[tokio::test]
async fn it_is_idempotent() {
    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-test-minimax-12345");
    let state = AppState::test_in_memory();
    bootstrap::seed_from_env(&state).await.unwrap();
    let report = bootstrap::seed_from_env(&state).await.unwrap();
    assert!(report.seeded.is_empty());
    assert_eq!(report.skipped, vec!["Minimax"]);
    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
}

#[tokio::test]
async fn it_does_not_override_existing_active() {
    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-test-minimax-12345");
    std::env::set_var("YOGURT_OPENAI_API_KEY", "sk-test-openai-67890");
    let state = AppState::test_in_memory();
    bootstrap::seed_from_env(&state).await.unwrap();
    // Minimax seeded first → should be active. OpenAI should be configured but inactive.
    let active = state.db.providers().active_llm().unwrap().unwrap();
    assert_eq!(active.name, "Minimax");
    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
    std::env::remove_var("YOGURT_OPENAI_API_KEY");
}
```

- [ ] **Step 6: Run.**

Run: `cargo test -p yogurt-server --test bootstrap`
Expected: all 3 pass.

- [ ] **Step 7: Smoke against the user's real `.env.local`.**

With the user's actual `.env.local` containing `YOGURT_MINIMAX_API_KEY=...`, run: `cargo run -p yogurt -- start --no-open` then `curl -s localhost:7878/api/settings | jq .providers`.

Expected: response includes a `Minimax` provider with `active: true`. The `api_key` field MUST be absent or `null` — verify keys are not leaked through this endpoint.

- [ ] **Step 8: Commit.**

```bash
git add Cargo.toml crates/yogurt-cli/ crates/yogurt-server/src/bootstrap.rs crates/yogurt-server/src/lib.rs crates/yogurt-server/tests/bootstrap.rs
git commit -m "feat(server): bootstrap providers from .env.local on first run"
```

---

### Task 5.7 · `AppState`, wire `Db` into `yogurt-server`, add `/api/settings` routes

**Files:**
- Modify: `crates/yogurt-server/Cargo.toml`
- Create: `crates/yogurt-server/src/state.rs`
- Create: `crates/yogurt-server/src/api/mod.rs` (if missing)
- Create: `crates/yogurt-server/src/api/settings.rs`
- Modify: `crates/yogurt-server/src/lib.rs`
- Modify: `crates/yogurt-server/src/routes.rs`

> **⚠ Note:** Phase 4 may have already created `api/mod.rs` for the `enhance` endpoint. If so, only add the `pub mod settings;` line. If not, create both files.

- [ ] **Step 1: Add deps to `crates/yogurt-server/Cargo.toml`.**

Append to `[dependencies]`:

```toml
yogurt-db = { path = "../yogurt-db" }
yogurt-llm = { path = "../yogurt-llm" }
async-trait = "0.1"
```

- [ ] **Step 2: Write `crates/yogurt-server/src/state.rs`.**

```rust
use std::sync::Arc;
use yogurt_db::keychain::{ApiKeyStore, KeychainStore, MemoryKeyStore};
use yogurt_db::Db;

/// Shared application state passed to every axum handler via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub keys: Arc<dyn ApiKeyStore>,
}

impl AppState {
    /// Production wiring: real Db at `~/.yogurt/db.sqlite` and the macOS Keychain.
    pub fn production() -> anyhow::Result<Self> {
        Ok(Self {
            db: Db::open_default()?,
            keys: Arc::new(KeychainStore),
        })
    }

    /// Test wiring: in-memory DB and an in-memory key store.
    pub fn in_memory() -> anyhow::Result<Self> {
        Ok(Self {
            db: Db::open_in_memory()?,
            keys: Arc::new(MemoryKeyStore::default()),
        })
    }
}
```

- [ ] **Step 3: Write `crates/yogurt-server/src/api/settings.rs`.**

```rust
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use yogurt_db::{providers, settings};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/settings", get(get_settings).patch(patch_settings))
        .route("/api/settings/providers", get(list_providers).post(create_provider))
        .route(
            "/api/settings/providers/:id",
            patch(update_provider).delete(delete_provider),
        )
        .route("/api/settings/providers/:id/activate", post(activate_provider))
        .route("/api/settings/providers/:id/key", post(set_provider_key))
        .route("/api/settings/presets", get(list_presets))
}

// ─── Provider serialization (NO API KEY EVER) ────────────────────────────────

#[derive(Serialize)]
struct ProviderView {
    id: String,
    name: String,
    base_url: String,
    model: String,
    is_active: bool,
    created_at: i64,
    /// `Some("••••XXXX")` when a key is stored, `None` otherwise. Never the raw key.
    api_key_masked: Option<String>,
}

fn to_view(state: &AppState, p: providers::Provider) -> ProviderView {
    let masked = state.keys.masked(&p.id).ok().flatten();
    ProviderView {
        id: p.id,
        name: p.name,
        base_url: p.base_url,
        model: p.model,
        is_active: p.is_active,
        created_at: p.created_at,
        api_key_masked: masked,
    }
}

#[derive(Serialize)]
struct PresetView {
    name: &'static str,
    base_url: &'static str,
    default_model: &'static str,
}

#[derive(Serialize)]
struct SettingsView {
    general: settings::General,
    providers: Vec<ProviderView>,
    presets: Vec<PresetView>,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn get_settings(State(s): State<AppState>) -> Result<Json<SettingsView>, Error> {
    let general = settings::load_general(&s.db)?;
    let providers = providers::list(&s.db)?
        .into_iter()
        .map(|p| to_view(&s, p))
        .collect();
    let presets = providers::PRESETS.iter().map(|p| PresetView {
        name: p.name, base_url: p.base_url, default_model: p.default_model,
    }).collect();
    Ok(Json(SettingsView { general, providers, presets }))
}

async fn patch_settings(
    State(s): State<AppState>,
    Json(patch): Json<settings::GeneralPatch>,
) -> Result<Json<settings::General>, Error> {
    Ok(Json(settings::save_general_patch(&s.db, patch)?))
}

async fn list_providers(State(s): State<AppState>) -> Result<Json<Vec<ProviderView>>, Error> {
    Ok(Json(providers::list(&s.db)?.into_iter().map(|p| to_view(&s, p)).collect()))
}

async fn list_presets() -> Json<Vec<PresetView>> {
    Json(providers::PRESETS.iter().map(|p| PresetView {
        name: p.name, base_url: p.base_url, default_model: p.default_model,
    }).collect())
}

async fn create_provider(
    State(s): State<AppState>,
    Json(body): Json<providers::NewProvider>,
) -> Result<Json<ProviderView>, Error> {
    let id = providers::insert(&s.db, body)?;
    let p = providers::list(&s.db)?.into_iter().find(|p| p.id == id)
        .ok_or_else(|| Error::Internal("inserted provider missing".into()))?;
    Ok(Json(to_view(&s, p)))
}

#[derive(Deserialize)]
struct UpdateProviderBody { name: String, base_url: String, model: String }

async fn update_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateProviderBody>,
) -> Result<Json<ProviderView>, Error> {
    providers::update(&s.db, &id, &body.name, &body.base_url, &body.model)?;
    let p = providers::list(&s.db)?.into_iter().find(|p| p.id == id)
        .ok_or_else(|| Error::NotFound)?;
    Ok(Json(to_view(&s, p)))
}

async fn delete_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    // Clean up the matching Keychain entry if any so uninstall/reinstall stays clean.
    let _ = s.keys.delete(&id);
    providers::delete(&s.db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn activate_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProviderView>, Error> {
    providers::set_active(&s.db, &id)?;
    let p = providers::active(&s.db)?
        .ok_or_else(|| Error::Internal("active provider missing after set_active".into()))?;
    Ok(Json(to_view(&s, p)))
}

#[derive(Deserialize)]
struct SetKeyBody { api_key: String }

async fn set_provider_key(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetKeyBody>,
) -> Result<StatusCode, Error> {
    // Verify provider exists before touching the keychain.
    if providers::list(&s.db)?.iter().all(|p| p.id != id) {
        return Err(Error::NotFound);
    }
    s.keys.set(&id, &body.api_key)
        .map_err(|e| Error::Internal(format!("keychain set: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Error mapping ───────────────────────────────────────────────────────────

#[derive(Debug)]
enum Error {
    NotFound,
    Internal(String),
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self { Self::Internal(format!("{e:#}")) }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            Error::Internal(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
        }
    }
}
```

- [ ] **Step 4: Write `crates/yogurt-server/src/api/mod.rs` (or modify if Phase 4 already created it).**

```rust
pub mod settings;
// Phase 4 enhance module is re-exported here too, if it was placed under api/.
// (If Phase 4 put enhance at crates/yogurt-server/src/enhance.rs, leave it alone.)
```

- [ ] **Step 5: Wire `AppState` and the new router into `crates/yogurt-server/src/lib.rs`.**

Add the state module and propagate it. The exact diff depends on what Phase 4 did to `lib.rs`; the conceptual change is:

```rust
mod api;
mod assets;
mod dev_proxy;
mod routes;
pub mod state; // pub so the CLI / tests can build an AppState

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub use state::AppState;

#[derive(Debug, Clone, Copy)]
pub enum Mode { Dev, Release }

pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    let app_state = AppState::production()?;
    run_with_state(addr, mode, app_state).await
}

pub async fn run_with_state(addr: SocketAddr, mode: Mode, state: AppState) -> Result<()> {
    let app = routes::router(mode).with_state(state);
    tracing::info!(?addr, ?mode, "yogurt-server starting");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

Then modify `crates/yogurt-server/src/routes.rs` to merge the settings router and accept `AppState`:

```rust
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::assets::serve_embedded;
use crate::state::AppState;
use crate::Mode;

pub fn router(mode: Mode) -> Router<AppState> {
    let mut router = Router::new()
        .route("/api/health", get(health))
        .merge(crate::api::settings::router());
    // (Phase 4's enhance router is merged here too — leave that line untouched.)

    router = match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    };
    router
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
```

- [ ] **Step 6: Write integration tests for the settings API.**

Create `crates/yogurt-server/tests/settings_api.rs`:

```rust
use serde_json::{json, Value};
use yogurt_server::{run_with_state, AppState, Mode};

async fn boot(port: u16) -> AppState {
    let state = AppState::in_memory().expect("state");
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    let s = state.clone();
    tokio::spawn(async move { run_with_state(addr, Mode::Release, s).await });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    state
}

#[tokio::test]
async fn it_lists_seeded_settings_with_no_providers() {
    let _state = boot(18001).await;
    let v: Value = reqwest::get("http://127.0.0.1:18001/api/settings")
        .await.unwrap().json().await.unwrap();
    assert_eq!(v["general"]["port"], 7878);
    assert_eq!(v["providers"].as_array().unwrap().len(), 0);
    assert!(v["presets"].as_array().unwrap().len() >= 5);
}

#[tokio::test]
async fn it_creates_a_provider_and_round_trips_via_get() {
    let _state = boot(18002).await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post("http://127.0.0.1:18002/api/settings/providers")
        .json(&json!({ "name": "Minimax", "base_url": "https://x/v1", "model": "M" }))
        .send().await.unwrap()
        .json().await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let listed: Value = reqwest::get("http://127.0.0.1:18002/api/settings/providers")
        .await.unwrap().json().await.unwrap();
    let arr = listed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
    assert_eq!(arr[0]["api_key_masked"], Value::Null);
}

#[tokio::test]
async fn api_responses_never_include_the_raw_api_key() {
    let _state = boot(18003).await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post("http://127.0.0.1:18003/api/settings/providers")
        .json(&json!({ "name": "P", "base_url": "https://x/v1", "model": "m" }))
        .send().await.unwrap()
        .json().await.unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:18003/api/settings/providers/{id}/key"))
        .json(&json!({ "api_key": "sk-supersecret-XYZA" }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 204);

    let listed: Value = reqwest::get("http://127.0.0.1:18003/api/settings/providers")
        .await.unwrap().json().await.unwrap();
    let s = serde_json::to_string(&listed).unwrap();
    assert!(!s.contains("sk-supersecret-XYZA"), "raw key leaked in: {s}");
    assert!(s.contains("••••XYZA"), "masked key should be present: {s}");
}
```

- [ ] **Step 7: Run.**

Run: `cargo test -p yogurt-server --test settings_api`
Expected: 3 passed. The "no raw key" test is load-bearing — never weaken it.

- [ ] **Step 8: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): add /api/settings routes + AppState wiring + no-key-leak test"
```

---

### Task 5.8 · Replace MockLLM in the enhance endpoint with `OpenAiCompatClient`

**Files:**
- Modify: `crates/yogurt-server/src/enhance.rs` (created in Phase 4)
- Create: `crates/yogurt-server/tests/enhance_uses_active_provider.rs`

> **⚠ Note:** This task assumes Phase 4 landed `enhance.rs` with a `MockLLM` placeholder behind a small trait or direct call. Adjust the exact symbols below to match what Phase 4 actually shipped — the conceptual change is: read the active provider + key from `AppState`, build an `OpenAiCompatClient`, call `complete()`, return the rendered enriched markdown. Any test that previously verified MockLLM's hardcoded output must be deleted or rewritten.

- [ ] **Step 1: Write the failing acceptance test first.**

Create `crates/yogurt-server/tests/enhance_uses_active_provider.rs`:

```rust
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yogurt_server::{run_with_state, AppState, Mode};

#[tokio::test]
async fn enhance_routes_through_active_provider() {
    // 1. Stand up a wiremock pretending to be Minimax.
    let llm = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "MiniMax-Text-01",
            "choices": [{ "message": { "role": "assistant",
                "content": "# Enriched\n- bullet from minimax" } }]
        })))
        .expect(1)
        .mount(&llm)
        .await;

    // 2. Boot the server with an in-memory state + memory key store.
    let state = AppState::in_memory().unwrap();
    let s2 = state.clone();
    tokio::spawn(async move {
        run_with_state("127.0.0.1:18010".parse().unwrap(), Mode::Release, s2).await
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // 3. Configure Minimax-via-wiremock as the active provider through the real API.
    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post("http://127.0.0.1:18010/api/settings/providers")
        .json(&json!({ "name": "Minimax", "base_url": llm.uri(), "model": "MiniMax-Text-01" }))
        .send().await.unwrap()
        .json().await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    client.post(format!("http://127.0.0.1:18010/api/settings/providers/{id}/key"))
        .json(&json!({ "api_key": "sk-mini" }))
        .send().await.unwrap();
    client.post(format!("http://127.0.0.1:18010/api/settings/providers/{id}/activate"))
        .send().await.unwrap();

    // 4. Trigger enhance.
    //    Phase 4 should expose either /api/enhance (test endpoint) or
    //    /api/meetings/:id/enhance. We hit whichever exists.
    let resp = client
        .post("http://127.0.0.1:18010/api/enhance")
        .json(&json!({ "notes": "- foo", "transcript": "T: hello" }))
        .send().await.unwrap();
    assert!(resp.status().is_success(), "enhance status: {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["enriched_md"].as_str().unwrap().contains("bullet from minimax"),
        "expected wiremock content in response, got {body}");
}
```

(If Phase 4 only created `/api/meetings/:id/enhance`, adjust the URL + add a `POST /api/meetings` call to seed a meeting first. The structural guarantee being tested — "enhance hits the active provider" — is the same.)

- [ ] **Step 2: Modify `enhance.rs` to use the real client.**

Conceptual rewrite of the enhance handler body:

```rust
// crates/yogurt-server/src/enhance.rs (modified)

use crate::state::AppState;
use anyhow::anyhow;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use yogurt_db::providers;
use yogurt_llm::{ChatMessage, ChatRequest, LlmClient, OpenAiCompatClient};

#[derive(Deserialize)]
pub struct EnhanceReq { pub notes: String, pub transcript: String }

#[derive(Serialize)]
pub struct EnhanceResp { pub enriched_md: String }

pub async fn enhance(
    State(s): State<AppState>,
    Json(body): Json<EnhanceReq>,
) -> Result<Json<EnhanceResp>, (StatusCode, String)> {
    let provider = providers::active(&s.db)
        .map_err(internal)?
        .ok_or((StatusCode::PRECONDITION_FAILED, "no active LLM provider configured".into()))?;
    let api_key = s.keys.get(&provider.id)
        .map_err(internal)?
        .ok_or((StatusCode::PRECONDITION_FAILED, "no API key stored for active provider".into()))?;

    let client = OpenAiCompatClient::new(provider.base_url, api_key, provider.model);

    // Render the bundled enhance.md prompt (from yogurt-prompts, added in Phase 4).
    let system = crate::prompts::enhance_system();
    let user = format!("NOTES:\n{}\n\nTRANSCRIPT:\n{}", body.notes, body.transcript);
    let resp = client.complete(ChatRequest {
        messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
        stream: false,
    }).await.map_err(internal)?;

    Ok(Json(EnhanceResp { enriched_md: resp.content }))
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
}
```

(The `crate::prompts::enhance_system()` reference assumes Phase 4 set up a tiny module that returns the `enhance.md` contents. If it's named differently, substitute the actual symbol.)

- [ ] **Step 3: Delete any Phase-4 test that asserted MockLLM-specific output.**

Search: `rg "MockLLM\|mock_llm\|mock-llm" crates/yogurt-server`
For each remaining test that relied on hardcoded MockLLM output, either delete it or rewrite it to seed an in-memory provider + wiremock, mirroring the acceptance test above.

- [ ] **Step 4: Run.**

Run: `cargo test -p yogurt-server --test enhance_uses_active_provider`
Expected: 1 passed. Then run the full suite — `cargo test -p yogurt-server` — and expect green.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(enhance): wire real OpenAI-compat client (replaces MockLLM)"
```

---

### Task 5.9 · Frontend — TanStack Query + Settings API client

**Files:**
- Modify: `web/package.json`
- Create: `web/src/lib/queryClient.ts`
- Create: `web/src/lib/api/settings.ts`
- Modify: `web/src/main.tsx`

- [ ] **Step 1: Add dependencies.**

Run: `pnpm --dir web add @tanstack/react-query @tanstack/react-query-devtools`
Run: `pnpm --dir web add -D msw`

(`msw` lets us mock fetch in Vitest without touching the network.)

- [ ] **Step 2: Write `web/src/lib/queryClient.ts`.**

```ts
import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Settings is the only consumer in Phase 5; a 30s staleTime keeps
      // typing-fast UX while still picking up changes from other tabs eventually.
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
    mutations: { retry: 0 },
  },
});
```

- [ ] **Step 3: Write `web/src/lib/api/settings.ts`.**

```ts
export interface General {
  port: number;
  open_browser_on_start: boolean;
  audio_input_device: string;
}

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  model: string;
  is_active: boolean;
  created_at: number;
  /** "••••XXXX" if a key is stored, null otherwise. */
  api_key_masked: string | null;
}

export interface Preset {
  name: string;
  base_url: string;
  default_model: string;
}

export interface SettingsView {
  general: General;
  providers: ProviderView[];
  presets: Preset[];
}

export interface NewProvider { name: string; base_url: string; model: string; }
export interface UpdateProvider { name: string; base_url: string; model: string; }

async function http<T>(input: string, init?: RequestInit): Promise<T> {
  const res = await fetch(input, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  if (res.status === 204) return undefined as unknown as T;
  return res.json() as Promise<T>;
}

export const settingsApi = {
  get: () => http<SettingsView>("/api/settings"),
  patch: (patch: Partial<General>) =>
    http<General>("/api/settings", { method: "PATCH", body: JSON.stringify(patch) }),
  createProvider: (p: NewProvider) =>
    http<ProviderView>("/api/settings/providers", { method: "POST", body: JSON.stringify(p) }),
  updateProvider: (id: string, p: UpdateProvider) =>
    http<ProviderView>(`/api/settings/providers/${id}`, { method: "PATCH", body: JSON.stringify(p) }),
  deleteProvider: (id: string) =>
    http<void>(`/api/settings/providers/${id}`, { method: "DELETE" }),
  activateProvider: (id: string) =>
    http<ProviderView>(`/api/settings/providers/${id}/activate`, { method: "POST" }),
  setProviderKey: (id: string, api_key: string) =>
    http<void>(`/api/settings/providers/${id}/key`, { method: "POST", body: JSON.stringify({ api_key }) }),
};

// Phase 2 audio devices endpoint — re-exported here for the Audio section.
export interface AudioDevice { id: string; name: string; is_default: boolean; }
export const audioApi = {
  devices: () => http<AudioDevice[]>("/api/audio/devices"),
};
```

- [ ] **Step 4: Wrap the app in `QueryClientProvider` (in `web/src/main.tsx`).**

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { queryClient } from "./lib/queryClient";
import { App } from "./App";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
      {import.meta.env.DEV ? <ReactQueryDevtools initialIsOpen={false} /> : null}
    </QueryClientProvider>
  </StrictMode>
);
```

- [ ] **Step 5: Smoke build.**

Run: `pnpm --dir web build`
Expected: tsc + vite both succeed; bundle includes `@tanstack/react-query` chunk.

- [ ] **Step 6: Commit.**

```bash
git add web/package.json web/pnpm-lock.yaml web/src/lib/ web/src/main.tsx
git commit -m "feat(web): add TanStack Query + typed settings API client"
```

---

### Task 5.10 · Settings page UI

**Files:**
- Create: `web/src/routes/Settings.tsx`
- Create: `web/src/components/settings/SidebarNav.tsx`
- Create: `web/src/components/settings/ProviderCard.tsx`
- Create: `web/src/components/settings/ProviderRow.tsx`
- Create: `web/src/components/settings/PresetChip.tsx`
- Create: `web/src/components/settings/STTPicker.tsx`
- Create: `web/src/components/settings/AudioSection.tsx`
- Create: `web/src/components/settings/GeneralSection.tsx`
- Modify: Phase-3 router (e.g. `web/src/App.tsx` or a `web/src/router.tsx`) to add `/settings`

> **⚠ Note:** Phase 3 introduced React Router 7 with at least a `/` route. The exact router file may be `App.tsx` or `router.tsx` — adapt Step 7 to wherever the `createBrowserRouter` / `<Routes>` block lives.

- [ ] **Step 1: Write `SidebarNav.tsx`.**

```tsx
import { ProviderView } from "../../lib/api/settings";

type Section = "model" | "transcription" | "audio" | "general";

interface Props {
  active: Section;
  onChange: (s: Section) => void;
  providers: ProviderView[];
}

const SECTIONS: { id: Section; label: string }[] = [
  { id: "model",         label: "Model" },
  { id: "transcription", label: "Transcription" },
  { id: "audio",         label: "Audio" },
  { id: "general",       label: "General" },
];

export function SidebarNav({ active, onChange, providers }: Props) {
  // "Local-only" is true iff no active provider points to a non-localhost host.
  const localOnly = !providers.some((p) =>
    p.is_active && !/localhost|127\.0\.0\.1/.test(p.base_url)
  );

  return (
    <nav className="w-[212px] shrink-0 bg-[var(--paper)] border-r border-neutral-200 flex flex-col">
      <ul className="flex-1 py-6 px-3 space-y-1">
        {SECTIONS.map((s) => (
          <li key={s.id}>
            <button
              type="button"
              onClick={() => onChange(s.id)}
              className={`w-full text-left px-3 py-2 rounded-md font-medium ${
                active === s.id
                  ? "bg-[var(--blsoft)] text-[var(--blue)]"
                  : "text-[var(--ink)] hover:bg-neutral-100"
              }`}
            >
              {s.label}
            </button>
          </li>
        ))}
      </ul>
      <footer className="p-4 border-t border-neutral-200 space-y-2">
        {localOnly ? (
          <span className="inline-flex items-center gap-1.5 text-xs font-medium text-white bg-[var(--matcha)] px-2.5 py-1 rounded-full">
            <span className="w-1.5 h-1.5 rounded-full bg-white" /> Local-only · on
          </span>
        ) : null}
        <div className="font-mono text-[10px] text-neutral-500 leading-relaxed">
          keys → macOS Keychain<br />data → ~/.yogurt/
        </div>
      </footer>
    </nav>
  );
}
```

- [ ] **Step 2: Write `ProviderCard.tsx` (the active provider, blueberry border).**

```tsx
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ProviderView, settingsApi } from "../../lib/api/settings";

interface Props { provider: ProviderView }

export function ProviderCard({ provider }: Props) {
  const qc = useQueryClient();
  const [keyDraft, setKeyDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState({
    name: provider.name, base_url: provider.base_url, model: provider.model,
  });

  const update = useMutation({
    mutationFn: () => settingsApi.updateProvider(provider.id, draft),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["settings"] }); setEditing(false); },
  });
  const setKey = useMutation({
    mutationFn: (k: string) => settingsApi.setProviderKey(provider.id, k),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["settings"] }); setKeyDraft(""); },
  });

  return (
    <article className="rounded-xl border-[1.5px] border-[var(--blue)] bg-white p-5 shadow-sm space-y-4">
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h3 className="font-serif text-xl">{provider.name}</h3>
          <span className="text-[10px] font-mono uppercase tracking-wider bg-[var(--blsoft)] text-[var(--blue)] px-2 py-0.5 rounded">
            Active
          </span>
        </div>
        <button className="text-sm text-[var(--blue)] hover:underline" onClick={() => setEditing((e) => !e)}>
          {editing ? "Cancel" : "Edit"}
        </button>
      </header>
      <div className="grid grid-cols-2 gap-x-6 gap-y-3">
        <Field label="BASE URL">
          {editing ? (
            <input className="w-full font-mono text-sm border-b border-neutral-300 focus:border-[var(--blue)] outline-none"
              value={draft.base_url} onChange={(e) => setDraft({ ...draft, base_url: e.target.value })} />
          ) : <code className="text-sm">{provider.base_url}</code>}
        </Field>
        <Field label="MODEL">
          {editing ? (
            <input className="w-full font-mono text-sm border-b border-neutral-300 focus:border-[var(--blue)] outline-none"
              value={draft.model} onChange={(e) => setDraft({ ...draft, model: e.target.value })} />
          ) : <code className="text-sm">{provider.model || "—"}</code>}
        </Field>
      </div>
      {editing ? (
        <button className="text-sm bg-[var(--blue)] text-white px-3 py-1.5 rounded-md"
                disabled={update.isPending} onClick={() => update.mutate()}>
          {update.isPending ? "Saving…" : "Save"}
        </button>
      ) : null}
      <div className="border-t border-neutral-200 pt-3 space-y-2">
        <div className="text-[10px] font-mono uppercase tracking-wider text-neutral-500">
          API KEY · in Keychain
        </div>
        {provider.api_key_masked ? (
          <div className="flex items-center gap-2 text-sm font-mono">
            <span>{provider.api_key_masked}</span>
            <span className="text-[var(--matcha)]">✓ stored</span>
          </div>
        ) : (
          <div className="text-sm text-neutral-500">No key stored yet.</div>
        )}
        <div className="flex items-center gap-2">
          <input
            type="password" placeholder="Paste new key…"
            className="flex-1 font-mono text-sm border border-neutral-300 rounded px-2 py-1 focus:border-[var(--blue)] outline-none"
            value={keyDraft} onChange={(e) => setKeyDraft(e.target.value)} />
          <button
            disabled={!keyDraft || setKey.isPending}
            className="text-sm bg-[var(--blue)] text-white px-3 py-1.5 rounded-md disabled:opacity-50"
            onClick={() => setKey.mutate(keyDraft)}>
            {setKey.isPending ? "Saving…" : "Save key"}
          </button>
        </div>
      </div>
    </article>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <div className="text-[10px] font-mono uppercase tracking-wider text-neutral-500">{label}</div>
      <div>{children}</div>
    </div>
  );
}
```

- [ ] **Step 3: Write `ProviderRow.tsx` (inactive providers).**

```tsx
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ProviderView, settingsApi } from "../../lib/api/settings";

export function ProviderRow({ provider }: { provider: ProviderView }) {
  const qc = useQueryClient();
  const activate = useMutation({
    mutationFn: () => settingsApi.activateProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  const remove = useMutation({
    mutationFn: () => settingsApi.deleteProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  return (
    <div className="flex items-center justify-between border-b border-neutral-200 py-3">
      <div className="flex items-baseline gap-3">
        <span className="font-medium">{provider.name}</span>
        <code className="text-xs text-neutral-500">{provider.base_url}</code>
        {provider.api_key_masked ? (
          <span className="text-xs text-[var(--matcha)] font-mono">✓ key</span>
        ) : (
          <span className="text-xs text-neutral-400 font-mono">no key</span>
        )}
      </div>
      <div className="flex items-center gap-4 text-sm">
        <button className="text-[var(--blue)] hover:underline"
                onClick={() => activate.mutate()} disabled={activate.isPending}>
          Set active
        </button>
        <button className="text-neutral-400 hover:text-[var(--strawberry)]"
                onClick={() => remove.mutate()} disabled={remove.isPending}>
          Remove
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Write `PresetChip.tsx`.**

```tsx
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Preset, settingsApi } from "../../lib/api/settings";

export function PresetChip({ preset }: { preset: Preset }) {
  const qc = useQueryClient();
  const clone = useMutation({
    mutationFn: () => settingsApi.createProvider({
      name: preset.name, base_url: preset.base_url, model: preset.default_model,
    }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  return (
    <button
      onClick={() => clone.mutate()}
      disabled={clone.isPending}
      className="text-xs font-mono uppercase tracking-wider px-3 py-1.5 rounded-full
                 border border-dashed border-neutral-400 text-neutral-600
                 hover:border-[var(--blue)] hover:text-[var(--blue)] disabled:opacity-50"
    >
      {clone.isPending ? "…" : preset.name}
    </button>
  );
}
```

- [ ] **Step 5: Write `STTPicker.tsx` (Cloud + disabled Local).**

```tsx
export function STTPicker() {
  return (
    <div className="grid grid-cols-2 gap-4">
      <article className="rounded-xl border-[1.5px] border-[var(--blue)] bg-white p-5 space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="font-serif text-lg">Cloud</h3>
          <span className="text-[10px] font-mono uppercase text-[var(--blue)]">Selected</span>
        </div>
        <p className="text-sm text-neutral-600">
          Real-time partials, ~2s end-to-end. Audio is sent to the provider.
        </p>
        <div className="flex gap-2 flex-wrap">
          {["Deepgram", "AssemblyAI", "Groq"].map((n) => (
            <span key={n} className={`text-xs px-2.5 py-1 rounded-full font-mono ${
              n === "Deepgram"
                ? "bg-[var(--blsoft)] text-[var(--blue)]"
                : "border border-neutral-300 text-neutral-500"
            }`}>{n}</span>
          ))}
        </div>
      </article>
      <article className="rounded-xl border border-neutral-300 bg-neutral-50 p-5 space-y-3 opacity-60">
        <div className="flex items-center justify-between">
          <h3 className="font-serif text-lg">Local · whisper.cpp</h3>
          <span className="text-[10px] font-mono uppercase bg-[var(--matchasoft)] text-[var(--matcha)] px-2 py-0.5 rounded">
            Coming in v1
          </span>
        </div>
        <p className="text-sm text-neutral-600">
          Fully on-device transcription via Metal-accelerated whisper.cpp.
        </p>
        <div className="flex gap-2 flex-wrap text-xs font-mono text-neutral-400">
          <span>tiny.en</span><span>small.en</span><span>medium.en</span><span>large-v3</span>
        </div>
      </article>
    </div>
  );
}
```

- [ ] **Step 6: Write `AudioSection.tsx` + `GeneralSection.tsx`.**

`AudioSection.tsx`:

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { audioApi, settingsApi, General } from "../../lib/api/settings";

export function AudioSection({ general }: { general: General }) {
  const qc = useQueryClient();
  const devices = useQuery({ queryKey: ["audio-devices"], queryFn: audioApi.devices });
  const patch = useMutation({
    mutationFn: (audio_input_device: string) => settingsApi.patch({ audio_input_device }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  return (
    <section className="space-y-3">
      <h2 className="font-serif text-xl">Audio</h2>
      <label className="block text-sm">
        Input device
        <select
          className="mt-1 w-full max-w-sm border border-neutral-300 rounded px-2 py-1.5"
          value={general.audio_input_device}
          onChange={(e) => patch.mutate(e.target.value)}
          disabled={devices.isLoading || patch.isPending}
        >
          <option value="">System default</option>
          {devices.data?.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}{d.is_default ? " (default)" : ""}
            </option>
          ))}
        </select>
      </label>
      <p className="text-xs font-mono text-neutral-500">
        System audio is captured via ScreenCaptureKit — no extra setup.
      </p>
    </section>
  );
}
```

`GeneralSection.tsx`:

```tsx
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { General, settingsApi } from "../../lib/api/settings";

export function GeneralSection({ general }: { general: General }) {
  const qc = useQueryClient();
  const patch = useMutation({
    mutationFn: (p: Partial<General>) => settingsApi.patch(p),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  return (
    <section className="space-y-3">
      <h2 className="font-serif text-xl">General</h2>
      <label className="block text-sm">
        Port
        <input type="number" min={1024} max={65535}
          className="mt-1 block w-32 border border-neutral-300 rounded px-2 py-1.5 font-mono"
          defaultValue={general.port}
          onBlur={(e) => {
            const port = parseInt(e.target.value, 10);
            if (!Number.isNaN(port) && port !== general.port) patch.mutate({ port });
          }} />
      </label>
      <label className="flex items-center gap-2 text-sm">
        <input type="checkbox" defaultChecked={general.open_browser_on_start}
          onChange={(e) => patch.mutate({ open_browser_on_start: e.target.checked })} />
        Open browser on start
      </label>
      <p className="text-xs font-mono text-neutral-500">
        Port change applies on next `yogurt start`.
      </p>
    </section>
  );
}
```

- [ ] **Step 7: Write `Settings.tsx` and mount it on `/settings`.**

```tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { settingsApi } from "../lib/api/settings";
import { SidebarNav } from "../components/settings/SidebarNav";
import { ProviderCard } from "../components/settings/ProviderCard";
import { ProviderRow } from "../components/settings/ProviderRow";
import { PresetChip } from "../components/settings/PresetChip";
import { STTPicker } from "../components/settings/STTPicker";
import { AudioSection } from "../components/settings/AudioSection";
import { GeneralSection } from "../components/settings/GeneralSection";

type Section = "model" | "transcription" | "audio" | "general";

export function Settings() {
  const [section, setSection] = useState<Section>("model");
  const q = useQuery({ queryKey: ["settings"], queryFn: settingsApi.get });

  if (q.isLoading) return <div className="p-10 text-neutral-500">Loading settings…</div>;
  if (q.isError) return <div className="p-10 text-[var(--strawberry)]">Failed to load: {String(q.error)}</div>;
  const data = q.data!;

  const active = data.providers.find((p) => p.is_active);
  const inactive = data.providers.filter((p) => !p.is_active);

  return (
    <div className="flex min-h-screen bg-[var(--paper)]">
      <SidebarNav active={section} onChange={setSection} providers={data.providers} />
      <main className="flex-1 max-w-3xl px-10 py-8 space-y-10">
        {section === "model" && (
          <section className="space-y-6">
            <header className="space-y-1">
              <div className="flex items-baseline gap-3">
                <h2 className="font-serif text-2xl">Model</h2>
                <code className="text-xs font-mono text-neutral-500">OpenAI-compatible</code>
              </div>
              <p className="text-sm text-neutral-600">
                Paste a base URL and key. Anthropic &amp; Gemini reachable via OpenRouter.
              </p>
            </header>
            {active ? <ProviderCard provider={active} /> : (
              <p className="text-sm text-neutral-500">No active provider — clone a preset below.</p>
            )}
            {inactive.length > 0 && (
              <div>{inactive.map((p) => <ProviderRow key={p.id} provider={p} />)}</div>
            )}
            <div className="pt-4 border-t border-dashed border-neutral-300">
              <div className="text-[10px] font-mono uppercase tracking-wider text-neutral-500 mb-2">
                Clone a preset →
              </div>
              <div className="flex flex-wrap gap-2 items-center">
                {data.presets.map((p) => <PresetChip key={p.name} preset={p} />)}
                <button className="text-xs text-[var(--blue)] hover:underline">+ Add</button>
              </div>
            </div>
          </section>
        )}
        {section === "transcription" && (
          <section className="space-y-4">
            <h2 className="font-serif text-2xl">Transcription</h2>
            <STTPicker />
          </section>
        )}
        {section === "audio" && <AudioSection general={data.general} />}
        {section === "general" && <GeneralSection general={data.general} />}
      </main>
    </div>
  );
}
```

Then in the Phase-3 router file (likely `web/src/App.tsx` or `web/src/router.tsx`), add a `/settings` route pointing at `<Settings />`. Example using React Router 7:

```tsx
// inside the existing <Routes> block:
<Route path="/settings" element={<Settings />} />
```

- [ ] **Step 8: Add a Vitest smoke test for the Settings page.**

Create `web/src/routes/Settings.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Settings } from "./Settings";

vi.mock("../lib/api/settings", () => ({
  settingsApi: {
    get: vi.fn().mockResolvedValue({
      general: { port: 7878, open_browser_on_start: true, audio_input_device: "" },
      providers: [
        { id: "p1", name: "Minimax", base_url: "https://api.minimax.io/v1",
          model: "M", is_active: true, created_at: 0, api_key_masked: "••••WXYZ" },
      ],
      presets: [
        { name: "Ollama (local)", base_url: "http://localhost:11434/v1", default_model: "llama3.2" },
      ],
    }),
  },
  audioApi: { devices: vi.fn().mockResolvedValue([]) },
}));

describe("Settings", () => {
  it("renders the active provider card with masked key", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}><Settings /></QueryClientProvider>
    );
    await waitFor(() => expect(screen.getByText("Minimax")).toBeInTheDocument());
    expect(screen.getByText("••••WXYZ")).toBeInTheDocument();
    expect(screen.getByText(/Local-only · on/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 9: Run + manual smoke.**

Run: `pnpm --dir web test`
Expected: green.

Run: `pnpm --dir web dev` + `cargo run -p yogurt -- start --dev --no-open`, open `http://localhost:7878/settings`.
Expected: layout matches PRD §5.6 — left rail with 4 sections + matcha pill, Model section shows empty state + dashed preset chips, clicking "Minimax" chip creates a new provider that appears in the inactive row list.

- [ ] **Step 10: Commit.**

```bash
git add web/
git commit -m "feat(web): add Settings page with provider cards, preset chips, audio + general"
```

---

### Task 5.11 · End-to-end acceptance + cleanup

**Files:** none — verification only.

- [ ] **Step 1: Run the full suite.**

Run: `cargo test --workspace`
Expected: green.

Run: `pnpm --dir web test`
Expected: green.

- [ ] **Step 2: Manual acceptance — the Minimax round-trip.**

Two terminals:

```bash
# t1
pnpm --dir web dev
# t2
cargo run -p yogurt -- start --dev --no-open
```

1. Open `http://localhost:7878/settings`.
2. Click the `Minimax` preset chip → row appears.
3. Click `Set active` on the new row.
4. Click `Edit` on the active card, paste a real `MINIMAX_API_KEY` into the key field, click `Save key`. Verify the row shows `••••<last4>` + `✓ stored`.
5. Open Keychain Access (the macOS app), search for `yogurt`. You should see one entry with the provider's ULID as the account name and the key as the password (after auth prompt). Confirm the raw key value matches.
6. Navigate to a meeting (one of the Phase 4 fixtures or via the test enhance endpoint). Hit `Re-enhance`. The request should succeed and the enriched markdown should be real Minimax output — not the Phase-4 MockLLM placeholder text.

- [ ] **Step 3: Verify no API key leaks.**

```bash
curl -s localhost:7878/api/settings | jq | grep -i 'minimax\|sk-'
```
Expected: only the masked `••••XXXX` form, never the raw key.

Also check the SQLite file:

```bash
sqlite3 ~/.yogurt/db.sqlite "SELECT * FROM providers; SELECT * FROM settings;"
```
Expected: no `api_key` column, no secret material — only `id, name, base_url, model, kind, is_active, created_at`.

- [ ] **Step 4: Verify ~/.yogurt was created.**

```bash
ls -la ~/.yogurt
```
Expected: directory exists; contains `db.sqlite` (+ `db.sqlite-wal`, `db.sqlite-shm` if WAL is active).

- [ ] **Step 5: Format + lint.**

Run: `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `pnpm --dir web build`
Expected: tsc + vite both succeed.

- [ ] **Step 6: Push.**

```bash
git push origin main
```

- [ ] **Step 7: Tag the phase milestone — only with explicit user confirmation.**

```bash
git tag -a v0.0.6-phase-5 -m "Phase 5 complete: real LLM client + settings page"
git push origin v0.0.6-phase-5
```

---

## Phase 5 acceptance criteria

All six must be true:

1. `cargo test --workspace` passes (including `enhance_uses_active_provider`).
2. `pnpm --dir web test` passes (including Settings smoke).
3. `GET /api/settings` never includes raw API key material — only the `••••XXXX` masked form. Asserted by `api_responses_never_include_the_raw_api_key`.
4. **The Minimax round-trip works manually:** pasting a real key, marking the provider active, and hitting Re-enhance results in a successful enhance call routed through Minimax (verified via Keychain Access + a real enhance response).
5. Keychain entries are scoped to `service="yogurt"` so uninstalling the binary doesn't leak keys; the `keychain-live` integration test passes locally.
6. The DB file lives at `~/.yogurt/db.sqlite`; the parent directory is created if missing; re-opening the DB runs migrations idempotently.

## What this phase does NOT do

Explicitly out of scope (next plans cover these):
- The `meetings` and `chat_messages` tables (Phase 6 migration V002 adds them).
- The actual `POST /api/meetings/:id/chat` endpoint and WebSocket streaming for chat (Phase 6 — `yogurt-llm::OpenAiCompatClient::stream` is ready and tested here).
- Local STT card actually working (Phase 8 — Local card is intentionally disabled with a "Coming in v1" badge).
- Onboarding `/welcome` and library `/` route polish (Phase 7).
- Per-provider connection test ("Test key" button) — Phase 5.1 quality-of-life if needed.
- Cross-process file watching of `~/.yogurt/db.sqlite` (Phase 9 multi-tab story).

## Next plan

After Phase 5 lands, write `docs/superpowers/plans/<date>-yogurt-phase-6-meetings-and-chat.md` covering:
- Migration V002: `meetings` + `chat_messages` tables (matching PRD §9 schema).
- `POST /api/meetings` + library list + `PATCH /api/meetings/:id` for the notes editor.
- WebSocket `/ws/meetings/:id` for `transcript`, `notes_synced`, `enhance_progress`, `chat_chunk` (consuming `yogurt-llm::stream` from this phase).
- Markdown export under `~/.yogurt/notes/`.
- Wiring the floating "Ask this meeting…" pill into the chat endpoint.
