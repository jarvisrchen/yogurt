---
phase: 00-skeleton-foundations
plan: 03
subsystem: foundations
tags: [sqlite, wal, websocket, auth, session-token, port-conflict, licensing, readme]
requires:
  - cargo-workspace
  - yogurt-cli
  - yogurt-server
  - api-health-route
  - web-scaffold
  - embedded-spa-release
  - vite-dev-proxy
provides:
  - sqlite-wal-dual-pool
  - v1-schema-migration
  - session-token
  - ws-origin-allowlist
  - ws-token-auth
  - port-conflict-ux
  - license
  - readme
affects:
  - Cargo.toml
  - Cargo.lock
  - crates/yogurt-server/Cargo.toml
  - crates/yogurt-server/src/lib.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-cli/src/commands/start.rs
  - crates/yogurt-cli/tests/cli.rs
tech-stack:
  added:
    - rusqlite 0.32 (bundled feature — vendored SQLite 3.46+)
    - directories 5 (~/.yogurt path resolution)
    - rand 0.8 (32-byte token entropy)
    - base64 0.22 (URL-safe no-pad token encoding)
    - subtle 2 (ConstantTimeEq for token validation)
    - tempfile 3 (dev-dep for per-test ~/.yogurt isolation)
    - tokio-tungstenite 0.24 (dev-dep WS client for ws_auth tests)
    - futures-util 0.3 (dev-dep SinkExt/StreamExt for ws_auth tests)
    - axum ws feature (enabled on yogurt-server)
  patterns:
    - "Storage::init_at(path) → Storage with Arc<Mutex<Connection>> writer + Vec<Arc<Mutex<Connection>>> reads (round-robin via AtomicUsize)"
    - "PRAGMA journal_mode=WAL + synchronous=NORMAL + foreign_keys=ON for the writer; query_only=ON for each read connection"
    - "Idempotent schema migration via `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` inside a transaction"
    - "Session token: 32 random bytes → URL_SAFE_NO_PAD base64 → file opened with O_CREAT|O_TRUNC|O_WRONLY|mode(0o600) before writing"
    - "Constant-time token compare via subtle::ConstantTimeEq with explicit length-gate"
    - "WS Origin allowlist derived from the actual bound port (`http://localhost:{port}` and `http://127.0.0.1:{port}`) so tests on ephemeral ports work"
    - "WS token accepted via `?token=` query param OR `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header"
    - "RunConfig struct with optional db_path + session_token_path overrides — tests pass tempdir paths so they never clobber the dev's real ~/.yogurt/"
    - "CLI port-conflict UX: walk `anyhow::Error::chain()` looking for an io::Error with ErrorKind::AddrInUse; print canonical message to STDERR; exit(1)"
key-files:
  created:
    - crates/yogurt-server/src/storage.rs
    - crates/yogurt-server/src/storage/migrations.rs
    - crates/yogurt-server/src/session.rs
    - crates/yogurt-server/src/ws.rs
    - crates/yogurt-server/tests/storage.rs
    - crates/yogurt-server/tests/ws_auth.rs
    - LICENSE
    - README.md
  modified:
    - Cargo.toml (workspace deps + dev-deps)
    - Cargo.lock
    - crates/yogurt-server/Cargo.toml (storage, session, ws, futures-util deps; axum ws feature)
    - crates/yogurt-server/src/lib.rs (AppState, RunConfig, run_with_config)
    - crates/yogurt-server/src/routes.rs (AppState wiring + /ws route registration)
    - crates/yogurt-cli/src/commands/start.rs (port-conflict UX)
    - crates/yogurt-cli/tests/cli.rs (port-conflict test)
decisions:
  - "Read pool sized at 4 connections (D-22 leaves this to discretion; 4 covers the chat + library + transcript fan-out the late-phase UI will exercise without blowing fds)."
  - "Token transport: support both `?token=` query param and `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header (CONTEXT D-21 listed both — implemented both rather than picking one)."
  - "Constant-time compare via `subtle` crate (added as workspace dep) rather than hand-rolled XOR loop — auditable, well-known, and the explicit length-gate before `ct_eq` keeps the constant-time region clean."
  - "RunConfig with explicit override fields rather than environment-variable backdoors — tests inject tempdir paths via a struct literal, no global state, no test pollution risk."
  - "Port-conflict suggestion uses `port.wrapping_add(1)` rather than scanning for the next free port — the message is a hint, not a binding decision, and scanning adds latency + nondeterminism with no UX gain."
metrics:
  duration_min: ~18
  completed: 2026-06-25
  tasks_completed: 3
  files_created: 8
  files_modified: 7
  tests_added: 6 (2 storage, 3 ws_auth, 1 cli port-conflict)
---

# Phase 00 Plan 03: Pitfall Mitigations (SQLite WAL + WS Auth + Port-Conflict UX) Summary

Baked the three ROADMAP success-criteria pitfall mitigations into the skeleton: SQLite at `~/.yogurt/db.sqlite` with WAL mode + read pool + single-writer mutex + v1 schema migration; WebSocket endpoint at `/ws` locked down via Origin allowlist + session-token auth (token persisted with mode `0600`); CLI port-conflict UX printing the canonical `Port {port} is already in use. Try --port {next_port} or run lsof -i :{port}` to STDERR. Also shipped MIT LICENSE + a real README with install/dev/release quickstarts. Phase 0 ROADMAP success criteria 1–5 now demonstrably true on a fresh clone.

## What Was Built

### Storage layer (`crates/yogurt-server/src/storage.rs` + `storage/migrations.rs`)

- **`Storage` struct** holds `Arc<Mutex<Connection>>` writer + `Vec<Arc<Mutex<Connection>>>` read pool (size 4) + `AtomicUsize` round-robin cursor. `Storage::init()` resolves the default path (`~/.yogurt/db.sqlite`) and delegates to `Storage::init_at(path)` for test injection.
- **Pragmas:** writer gets `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`. Each read connection gets `query_only=ON` + `foreign_keys=ON` so an accidental write through the read handle is rejected at the SQLite layer.
- **v1 schema migration** runs inside a transaction. Creates `meetings(id, title, started_at, ended_at, notes_md, enriched_md, transcript_json)` and `chat_messages(id, meeting_id FK, role, content, created_at)`, plus indexes `idx_meetings_started_at ON meetings(started_at DESC)` and `idx_chat_messages_meeting_id ON chat_messages(meeting_id, created_at)`. All `IF NOT EXISTS` so a second `Storage::init_at` on the same path is a no-op. **`enriched_doc_json` column is intentionally absent — deferred to Phase 4 per STORE-01 split mapping.**

### Session token (`crates/yogurt-server/src/session.rs`)

- **`SessionToken(pub String)`** wraps the raw URL-safe base64 of 32 random bytes.
- **`load_or_create(path)`** reads the existing token if the file exists (trims whitespace; regenerates on empty); else generates 32 bytes via `rand::thread_rng().fill_bytes` and persists via `OpenOptions` with `mode(0o600)` set before write on Unix (Windows: best-effort plain create — Yogurt is macOS-only, so this branch is theoretical).
- **`validate(candidate)`** uses `subtle::ConstantTimeEq` with an explicit length gate.
- **`default_token_path()`** → `<home>/.yogurt/session-token`.

### WebSocket endpoint (`crates/yogurt-server/src/ws.rs`)

- **`ws_handler`** is an axum handler accepting `State<AppState>`, `Query<WsParams>`, `HeaderMap`, and `WebSocketUpgrade`. Performs auth BEFORE upgrade so denials are clean HTTP 403s.
- **Origin check:** allowlist `{http://localhost:{bind_port}, http://127.0.0.1:{bind_port}}` — derived from the actual bound port so ephemeral test ports work.
- **Token check:** prefers `?token=` query param; falls back to parsing the `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header (D-21 listed both — implemented both).
- **Phase 0 stub:** on successful upgrade, echoes text frames and ponds pings. Binary frames are silently ignored. Real WS payloads land in Phase 3.

### Server wiring (`crates/yogurt-server/src/lib.rs` + `routes.rs`)

- **`AppState { mode, storage: Arc<Storage>, session: Arc<SessionToken>, bind_port: u16 }`** is the single state struct threaded through `Router::with_state`.
- **`RunConfig { addr, mode, db_path: Option<PathBuf>, session_token_path: Option<PathBuf> }`** is the test-injectable entry point. `run(addr, mode)` is a thin wrapper that uses the real `~/.yogurt/` paths; `run_with_config(cfg)` accepts overrides.
- **`/ws` route** registered in `routes::router` alongside `/api/health`, before the mode-dependent fallback.

### CLI port-conflict UX (`crates/yogurt-cli/src/commands/start.rs`)

- On `yogurt_server::run` error, walks `anyhow::Error::chain()` looking for an `io::Error` whose `kind() == ErrorKind::AddrInUse`. If found, prints to STDERR via `eprintln!`:
  ```
  Port {port} is already in use. Try --port {next_port} or run lsof -i :{port}
  ```
  where `next_port = port.wrapping_add(1)`. Then `std::process::exit(1)`. Any other error propagates as a normal `anyhow` chain.

### LICENSE + README (repo root)

- **`LICENSE`** — standard MIT text, `Copyright (c) 2026 Jarvis Chen`.
- **`README.md`** — title, status banner pointing at `docs/PRD.md`, "Install (eventually)" `brew install` snippet flagged as Phase 9, "Run from source today" dual-terminal workflow + release-build steps, "Architecture (short)" linking PRD §7/§8, "CLI" section showing the three flags and the canonical port-conflict message, "Privacy posture" bullets, "License" link.

## Tests

| #  | Test                                                       | Crate / File                          | Status   |
| -- | ---------------------------------------------------------- | ------------------------------------- | -------- |
| 1  | `it_prints_help`                                           | yogurt (tests/cli.rs)                 | passed   |
| 2  | `it_starts_server_and_serves_health`                       | yogurt (tests/cli.rs)                 | passed   |
| 3  | `it_reports_port_conflict_with_friendly_error`             | yogurt (tests/cli.rs)                 | passed (NEW) |
| 4  | `it_responds_to_health`                                    | yogurt-server (tests/health.rs)       | passed   |
| 5  | `it_serves_embedded_index_in_release_mode`                 | yogurt-server (tests/embedded.rs)     | passed   |
| 6  | `it_returns_bad_gateway_in_dev_mode_when_vite_is_down`     | yogurt-server (tests/embedded.rs)     | passed   |
| 7  | `it_initializes_db_with_wal_and_tables`                    | yogurt-server (tests/storage.rs)      | passed (NEW) |
| 8  | `it_exposes_both_read_and_writer_handles`                  | yogurt-server (tests/storage.rs)      | passed (NEW) |
| 9  | `it_rejects_ws_with_bad_origin`                            | yogurt-server (tests/ws_auth.rs)      | passed (NEW) |
| 10 | `it_rejects_ws_without_token`                              | yogurt-server (tests/ws_auth.rs)      | passed (NEW) |
| 11 | `it_accepts_ws_with_correct_origin_and_token`              | yogurt-server (tests/ws_auth.rs)      | passed (NEW) |

- `cargo test --workspace` final: **11 passed (8 suites, 0.88s)**.

## Verification

- ✅ `cargo build --workspace` — clean.
- ✅ `cargo test --workspace` — 11 passed.
- ✅ `cargo clippy --all-targets -- -D warnings` — no issues.
- ✅ `cargo fmt --all -- --check` — clean (after `cargo fmt --all` applied in Task 3).
- ✅ `pnpm --dir web build` — 135 modules, `web/dist/{index.html,assets/index-*.css,assets/index-*.js}` emitted.
- ✅ `cargo build --release` — 22s, optimized profile, zero warnings.
- ✅ Release smoke (`./target/release/yogurt start --no-open --port 17884`):
  - `GET /api/health` → `{"service":"yogurt-server","status":"ok"}` ✅
  - `GET /` → HTML containing `<div id="root">` ✅
- ✅ Port-conflict smoke (run two `yogurt start --port 17884` instances):
  - Second instance STDERR: `Port 17884 is already in use. Try --port 17885 or run lsof -i :17884`
  - Exit code: 1
- ✅ `~/.yogurt/db.sqlite` exists after first run; `sqlite3 ~/.yogurt/db.sqlite "PRAGMA journal_mode;"` returns `wal`.
- ✅ `~/.yogurt/session-token` exists after first run with mode `-rw-------` (0600) and a 43-byte URL-safe base64 token (32 random bytes encoded).

## Commits

| # | Hash      | Message                                                                  |
| - | --------- | ------------------------------------------------------------------------ |
| 1 | `354c0d9` | `feat(server): sqlite wal + dual pool + v1 schema migration`             |
| 2 | `12d2577` | `feat(server): ws endpoint with origin allowlist + session-token auth`   |
| 3 | `1397960` | `feat(cli): friendly port-conflict error with --port override hint`      |
| 4 | `c923b36` | `docs(00-03): add LICENSE + README + apply cargo fmt gate`               |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking issue] cargo PATH not propagated to subshells**

- **Found during:** Task 1, first `cargo build` invocation.
- **Issue:** The Bash tool's spawn environment did not include `/Users/rchen/.rustup/toolchains/stable-aarch64-apple-darwin/bin` on `$PATH`, so `cargo` was unresolved (`No such file or directory`).
- **Fix:** Prefixed `cargo` invocations with `export PATH="/Users/rchen/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"` for the rest of the plan. No source-code or Cargo.toml change required.
- **Files modified:** none.
- **Commit:** n/a (environment workaround only).

**2. [Rule 3 — Blocking issue] Missing `futures-util` for ws_auth integration test**

- **Found during:** Task 2, writing `tests/ws_auth.rs`.
- **Issue:** The plan called for `tokio-tungstenite = "0.24"` as a dev-dep but the WS round-trip in `it_accepts_ws_with_correct_origin_and_token` needs `SinkExt::send` + `StreamExt::next`, which live in `futures-util`. tokio-tungstenite does not re-export them.
- **Fix:** Added `futures-util = "0.3"` to workspace.dependencies and to `crates/yogurt-server/Cargo.toml [dev-dependencies]`.
- **Files modified:** `Cargo.toml`, `crates/yogurt-server/Cargo.toml`.
- **Commit:** rolled into commit 2 (`12d2577`).

**3. [Rule 1 — Bug] Clippy `err_expect` lints on `.err().expect(...)`**

- **Found during:** Task 2, `cargo clippy --all-targets -- -D warnings` after first ws_auth test write.
- **Issue:** clippy 1.96 flags `.err().expect("...")` on a `Result` and recommends `.expect_err("...")` (no `Option` allocation).
- **Fix:** Replaced both call sites in `crates/yogurt-server/tests/ws_auth.rs` with `expect_err`. Functionally identical, idiomatic.
- **Files modified:** `crates/yogurt-server/tests/ws_auth.rs`.
- **Commit:** rolled into commit 2 (`12d2577`).

### Auth gates

None. No external credentials touched — the session token is self-generated.

### Pre-existing issues (out of scope)

- `vite build` warns the main bundle is >500kB (React + ProseMirror + TipTap baseline). Documented as out-of-scope in Plan 02 already; no action this plan.
- `cargo build` reports a couple of indirect dependencies on the new-rand-core split (rand 0.8 vs rand 0.9 coexisting via transitive resolution). No clippy warnings, no functional impact — left alone.

## Known Stubs

- **`ws::handle_socket` echoes text frames.** This is the Phase 0 stub by design — real WS payload schemas land in Phase 3 (live transcript dock). The auth gate IS production-grade; only the payload handler is a placeholder. The plan explicitly scopes this. Not blocking.

## Threat Flags

None. This plan tightens an existing trust boundary (the WS endpoint) rather than introducing one. The new SQLite file is local-only with mode honoring umask (default 644 on macOS) — the file contains nothing sensitive yet (no API keys, no transcripts). The session-token file is mode 0600 by construction.

## Notes for Plan 01-XX and Beyond

- **Storage handle is `Arc<Storage>` in `AppState`.** Phase 4's `enriched_doc_json TEXT` column migration should bump the schema version. The migration module currently has no version-tracking table — Plan 4-N should add `PRAGMA user_version` bookkeeping when it lands the second migration. The single-statement `migrations::run` is intentionally bare for Phase 0 simplicity.
- **`AppState` is the place to extend** for future phases — add `audio: Arc<AudioBus>`, `stt: Arc<dyn SttEngine>`, etc. in their respective phases. Don't re-derive `Clone` per field; the `Arc`s are cheap.
- **Test isolation via `RunConfig`.** All future server integration tests should use `RunConfig` with tempdir paths rather than spawning the CLI binary. The 3 ws_auth tests demonstrate the pattern.
- **Tag push (`v0.0.1-phase-0`) is intentionally deferred** pending the user's explicit confirmation, per superpowers Task 0.10 Step 5. The user can run `git tag v0.0.1-phase-0 && git push origin v0.0.1-phase-0` at their discretion.
- **Phase 0 is done.** All five ROADMAP §Phase 0 success-criteria gates are now demonstrably true:
  1. `yogurt start` serves embedded SPA at `localhost:7878` ✅ (Plan 02)
  2. SQLite WAL + dual pool at `~/.yogurt/db.sqlite` ✅ (this plan)
  3. WS endpoint locked down by Origin + session token ✅ (this plan)
  4. Port-conflict UX with canonical message ✅ (this plan)
  5. LICENSE + README ship ✅ (this plan)

## Self-Check: PASSED

- **Files** — all declared key-files present on disk:
  - `crates/yogurt-server/src/storage.rs` ✅
  - `crates/yogurt-server/src/storage/migrations.rs` ✅
  - `crates/yogurt-server/src/session.rs` ✅
  - `crates/yogurt-server/src/ws.rs` ✅
  - `crates/yogurt-server/tests/storage.rs` ✅
  - `crates/yogurt-server/tests/ws_auth.rs` ✅
  - `LICENSE` ✅ (first line `MIT License`, contains `Copyright (c) 2026 Jarvis Chen`)
  - `README.md` ✅ (contains `# yogurt`, `Local-first`, `pnpm --dir web dev`, `cargo run -p yogurt -- start --dev`, `MIT`)
- **Commits** — `git log --oneline -4` shows `c923b36`, `1397960`, `12d2577`, `354c0d9` on `gsd/autonomous`.
- **Plan acceptance criteria** — every `must_haves.truth` bullet confirmed:
  - `~/.yogurt/db.sqlite` exists, `PRAGMA journal_mode=wal` ✅
  - Server exposes read pool + writer mutex via `AppState.storage` ✅
  - `/ws` rejects bad origin with 403 ✅ (test `it_rejects_ws_with_bad_origin`)
  - `/ws` rejects missing token with 403 ✅ (test `it_rejects_ws_without_token`)
  - `yogurt start --port 17884` against an occupied port prints the canonical message + exits 1 ✅ (test `it_reports_port_conflict_with_friendly_error` + manual smoke)
  - `./target/release/yogurt start --no-open` boots and serves embedded SPA in <1s ✅
  - LICENSE + README ship at repo root ✅
- **Phase requirements covered:** FOUND-04, FOUND-05, FOUND-06, STORE-01 (scaffold; `enriched_doc_json` deferred to Phase 4), STORE-02, STORE-05 — all demonstrably met by the test suite + manual smokes documented above.
