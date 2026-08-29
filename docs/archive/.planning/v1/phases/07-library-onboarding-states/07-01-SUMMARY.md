---
phase: 07-library-onboarding-states
plan: 01
subsystem: library home + meetings repo
tags: [react, vite, vitest, tanstack-query, axum, rusqlite, sqlite-migrations, markdown-exporter, tailwind, ulid, uuid, chrono]
requires:
  - phase: 00
    artifact: yogurt-server::storage::Storage + Phase-0 `meetings` schema (V001/V0004)
  - phase: 04
    artifact: MarkdownExporter (atomic per-meeting markdown writer)
  - phase: 05
    artifact: yogurt-db::Db handle + ProductionConfig.app_db_path test isolation
  - phase: 06
    artifact: yogurt-db::chat_messages table + V002 migration
  - phase: 01
    artifact: design tokens (--color-paper / --color-ink / --color-blue / --color-blsoft / --color-matcha / --color-mtsoft / --color-line / --color-mut) + Logo brand asset
provides:
  - "yogurt-db::meetings::MeetingRepo (CRUD over the SQLite Library directory)"
  - "yogurt-db::Meeting + MeetingPatch + NewMeeting wire types (re-exported at crate root)"
  - "V003__meetings.sql + V003b__chat_messages_cascade.sql (idempotent Phase-7 migrations)"
  - "backfill_phase7_meeting_columns Rust helper for legacy DB column ALTER + idx_meetings_starred index"
  - "AppState.meeting_repo: Arc<MeetingRepo>"
  - "AppState::patch_and_export(id, patch) — single-shot repo + markdown writer"
  - "yogurt-server::api::meetings::router() — GET/POST /api/meetings + GET/PATCH/DELETE /api/meetings/:id (mounted behind session-token middleware)"
  - "web/src/lib/api/meetings.ts — typed Meeting/MeetingPatch + meetingsApi + useMeetings/useMeeting/useCreateMeeting/useDeleteMeeting"
  - "web/src/hooks/useGreeting.ts — greetingFor(now, name?) + useGreeting hook"
  - "web/src/components/library/Sidebar.tsx — 212px sidebar with logo + CTA + nav + folders + Local-only pill + Settings"
  - "web/src/components/library/MeetingCard.tsx — 42px tinted avatar + meta line + Local pill"
  - "web/src/components/library/DateGroup.tsx — bucketFor + groupMeetings pure helpers + TODAY/YESTERDAY/EARLIER renderer"
  - "web/src/components/library/Greeting.tsx — Instrument-Serif greeting + mono meeting-count caption"
  - "web/src/routes/Library.tsx — sidebar + greeting + DateGroup + EmptyStub"
affects:
  - "crates/yogurt-server/src/routes.rs (deleted Phase-3 create_meeting + get_meeting handlers; merged api::meetings::router() into meeting_routes)"
  - "crates/yogurt-server/src/enhance.rs (mirrors post-enhance state into meeting_repo so the new Library GET stays in sync)"
  - "crates/yogurt-server/src/state.rs (AppState.meeting_repo + ExpMeeting alias)"
  - "crates/yogurt-server/src/test_support.rs (drops the inline CREATE TABLE meetings; uses meeting_repo.create instead)"
  - "crates/yogurt-server/src/api/mod.rs (pub mod meetings)"
  - "crates/yogurt-server/tests/{meeting_ws,meeting_ws_auth,e2e_synthetic_audio}.rs (added meeting_repo field)"
  - "crates/yogurt-server/tests/meeting_rest.rs (updated assertion for new Meeting wire shape)"
  - "crates/yogurt-server/tests/enhance_endpoint.rs (renamed started_at_unix_ms → started_at)"
  - "web/src/router.tsx ('/' → Library; deleted App.tsx mount; added /starred, /welcome, * fallbacks)"
  - "web/src/router.test.tsx (added QueryClientProvider + useMeetings/Sidebar stubs)"
  - "Cargo.toml (workspace chrono dep) + crates/yogurt-db/Cargo.toml (chrono with serde)"
tech-stack:
  added:
    - "chrono 0.4 (workspace dep) — DateTime<Utc> serde adapter for ISO 8601 created_at/updated_at wire shape"
  patterns:
    - "Library directory ≠ streaming registry: AppState.meeting_repo (SQLite, persistent) and AppState.meetings (in-memory, audio/transcript broadcasts) coexist. POST /api/meetings creates both with the SAME UUID-v7 id so /api/meetings/:id/start can find the streaming Meeting."
    - "NewMeeting.id: Option<String> escape hatch — defaults to fresh ULID, but Library handler passes UUID-v7 strings for cross-table id alignment"
    - "MeetingPatch.enriched_md: Option<Option<String>> tri-state — None=leave alone, Some(None)=clear, Some(Some(s))=set"
    - "Idempotent migration retrofit — V003 uses CREATE TABLE IF NOT EXISTS for fresh DBs; Rust-side backfill_phase7_meeting_columns inspects PRAGMA table_info('meetings') and conditionally ALTERs for legacy Phase-0 DBs"
    - "Dual-write pattern: every PATCH funnels through AppState::patch_and_export so SQLite row + ~/.yogurt/notes/<…>.md file stay in lockstep (STORE-04 invariant)"
    - "spawn_blocking-wrapped repo calls — MeetingRepo methods are synchronous (Mutex<Connection>), so the REST handlers move every call onto the blocking pool"
    - "Cache-key convention: ['meetings'] for list, ['meetings', id] for one; create + delete invalidate the list key + removeQueries on the per-row key"
key-files:
  created:
    - .planning/phases/07-library-onboarding-states/07-01-SUMMARY.md
    - crates/yogurt-db/migrations/V003__meetings.sql
    - crates/yogurt-db/migrations/V003b__chat_messages_cascade.sql
    - crates/yogurt-db/src/meetings.rs
    - crates/yogurt-server/src/api/meetings.rs
    - crates/yogurt-server/tests/meetings_api.rs
    - web/src/lib/api/meetings.ts
    - web/src/hooks/useGreeting.ts
    - web/src/hooks/useGreeting.test.ts
    - web/src/components/library/Sidebar.tsx
    - web/src/components/library/MeetingCard.tsx
    - web/src/components/library/DateGroup.tsx
    - web/src/components/library/DateGroup.test.ts
    - web/src/components/library/Greeting.tsx
    - web/src/routes/Library.tsx
  modified:
    - Cargo.toml (workspace chrono dep)
    - Cargo.lock (chrono + deps)
    - crates/yogurt-db/Cargo.toml (chrono = workspace)
    - crates/yogurt-db/src/lib.rs (mod meetings + re-exports)
    - crates/yogurt-db/src/migrations.rs (V003 + V003b + backfill_phase7_meeting_columns)
    - crates/yogurt-server/src/state.rs (meeting_repo field + patch_and_export helper)
    - crates/yogurt-server/src/routes.rs (api::meetings::router merge; deleted create_meeting + get_meeting)
    - crates/yogurt-server/src/api/mod.rs (pub mod meetings)
    - crates/yogurt-server/src/enhance.rs (post-enhance mirror into meeting_repo)
    - crates/yogurt-server/src/test_support.rs (use meeting_repo for seeding)
    - crates/yogurt-server/tests/meeting_ws.rs (meeting_repo field)
    - crates/yogurt-server/tests/meeting_ws_auth.rs (meeting_repo field)
    - crates/yogurt-server/tests/e2e_synthetic_audio.rs (meeting_repo field)
    - crates/yogurt-server/tests/meeting_rest.rs (created_at ISO string)
    - crates/yogurt-server/tests/enhance_endpoint.rs (started_at field rename)
    - web/src/router.tsx (Library at /; Navigate fallbacks)
    - web/src/router.test.tsx (QueryClientProvider + stubs)
decisions:
  - "Plan called for V003 to CREATE the meetings table, but Phase 0 storage already owns it in the same ~/.yogurt/db.sqlite file. Resolution: V003 uses CREATE TABLE IF NOT EXISTS (harmless on a Phase-0 DB) + a Rust-side PRAGMA-guarded ALTER backfill for the three new columns (starred, created_at, updated_at). idx_meetings_starred is created in Rust after the column lands (CREATE INDEX inside V003 SQL would fail on legacy DBs because the column isn't there yet)."
  - "Plan called for the Phase-3 in-memory meetings::Registry to be deleted entirely. Kept it: audio_tx / transcript_tx / events_tx / capture_thread can't move to SQLite. The new meeting_repo owns the *directory*; the registry owns the *streams*. Both fields live on AppState and POST /api/meetings creates a row in both with the same UUID-v7 id so /start /stop /enhance keep working."
  - "Plan called for ULID 26-char ids, but the streaming code uses uuid::Uuid::now_v7 + axum's Path<Uuid> extractor. Resolution: NewMeeting.id: Option<String> — the Library REST handler passes UUID strings (preserves /start interop), MeetingRepo unit tests default to ULID (preserves the ULID-creation acceptance criterion). The repo doesn't care which format the id takes."
  - "Plan called for /m/:id frontend route shape; existing Meeting + MeetingPost mount under /meeting/:id. Kept /meeting/:id to minimize blast radius — same conceptual link, smaller diff. (Documented as Rule 3 auto-fix in the Sidebar.tsx comment.)"
  - "DELETE /api/meetings/:id leaves the on-disk markdown file in place — D-10 / PRD §5.7 calls this out as the user's grep-able source of truth."
  - "Wire shape moved from {id, created_at_ms} (Phase 3) to the full yogurt_db::Meeting projection (ISO 8601 created_at, started_at as i64 unix ms, etc.). Phase 7 frontend matches the new shape; Phase 3 callers (none in the codebase anymore) would break."
  - "Live UI walkthrough — autonomous mode skips boot of dev server + browser sessions. The visual checkpoint (Task 5) is deferred to the release-time smoke pass."
metrics:
  duration_minutes: 25
  completed_date: 2026-06-25
  tasks_completed: 5
  files_created: 15
  files_modified: 17
  commits: 5
---

# Phase 7 Plan 07-01: Library route + SQLite MeetingRepo + date-grouped cards Summary

**One-liner:** Promotes the Library to `/` (212px sidebar + Instrument-Serif greeting + 42px tinted-avatar meeting cards grouped under TODAY/YESTERDAY/EARLIER), replaces the Phase-3 in-memory meeting directory with a SQLite-backed `MeetingRepo` (V003 migration + FK-cascade rebuild on `chat_messages`), and ships full REST CRUD wired to TanStack-Query.

## What Shipped

### Task 1 — V003 migration + MeetingRepo CRUD (commit `ab27a55`)

- `crates/yogurt-db/migrations/V003__meetings.sql` — `CREATE TABLE IF NOT EXISTS meetings` with the full Phase-7 column set (`id, title, started_at, ended_at, notes_md, enriched_md, transcript_json, enriched_doc_json, starred, created_at, updated_at`); seeds `settings('first_run_completed', 'false')`.
- `crates/yogurt-db/migrations/V003b__chat_messages_cascade.sql` — rebuilds `chat_messages` so its FK is `ON DELETE CASCADE` (Phase 0 storage created it without; V002's `IF NOT EXISTS` made the V2 cascade declaration a no-op).
- `crates/yogurt-db/src/migrations.rs::backfill_phase7_meeting_columns` — Rust-side `PRAGMA table_info('meetings')` guard that adds `starred / created_at / updated_at` columns to legacy DBs and creates `idx_meetings_starred` after the column lands.
- `crates/yogurt-db/src/meetings.rs` — `MeetingRepo::{create, get, list, patch, delete}` over the shared `Db` handle. `MeetingPatch.enriched_md: Option<Option<String>>` and `MeetingPatch.ended_at: Option<Option<i64>>` are tri-state. `NewMeeting.id` is an optional caller-supplied override (defaults to fresh ULID).
- `Cargo.toml`: added `chrono = { workspace = true, features = ["serde"] }` for the ISO 8601 `created_at` / `updated_at` wire shape.
- 8 new repo tests: ULID id format, empty-title rejection, newest-first list, patch updates `updated_at`, tri-state `enriched_md`, missing-id semantics (None/false/Err), `delete` returns true on hit, `delete` cascades `chat_messages`.

**Verify:** `cargo test -p yogurt-db meetings` — 8 passed (yogurt-db crate total 23 passed, was 15).

### Task 2 — REST endpoints + AppState wiring (commit `b642152`)

- `crates/yogurt-server/src/api/meetings.rs` — new `router()` exposing `GET/POST /api/meetings` + `GET/PATCH/DELETE /api/meetings/:id`. Includes the `ApiError` enum that maps anyhow "not found" / "empty" strings to 404 / 400. POST returns 201; DELETE returns 204.
- `crates/yogurt-server/src/state.rs` — `AppState.meeting_repo: Arc<MeetingRepo>` field + `AppState::patch_and_export(id, patch)` helper that funnels every write through MarkdownExporter (STORE-04 invariant survives the move).
- `crates/yogurt-server/src/routes.rs` — merged `api::meetings::router()` into the auth-gated `meeting_routes`; deleted the Phase-3 `create_meeting` + `get_meeting` handlers.
- `crates/yogurt-server/src/enhance.rs` — after every successful enhance, mirrors the title / notes_md / transcript_json / enriched_md / started_at / ended_at into `meeting_repo` so the new Library GET stays consistent with the Phase 4 storage row.
- `crates/yogurt-server/src/test_support.rs` — `seed_meeting` now calls `meeting_repo.create` (V003 creates the table inside in-memory DBs too, so the inline CREATE TABLE hack is gone).
- POST /api/meetings creates the in-memory streaming Meeting FIRST so the SQLite row inherits the UUID-v7 id; subsequent `/start/:id` resolves through `Path<Uuid>` cleanly.
- New integration suite `crates/yogurt-server/tests/meetings_api.rs` — 4 tests: create+list returns the row, patch writes the markdown file under `~/.yogurt/notes/`, delete returns 204 + GET returns 404, missing id returns 404.
- Updated 3 existing AppState test literals to include the new field; updated `meeting_rest::it_creates_a_meeting_and_returns_an_id` for the new `created_at` ISO 8601 string; updated `enhance_endpoint::it_gets_a_meeting_after_enhance` for the new `started_at` / `ended_at` field names.

**Verify:** `cargo test -p yogurt-server --test meetings_api` — 4 passed. `cargo test --workspace` — 160 passed (was 148).

### Task 3 — Frontend Library route + Sidebar + components (commit `2d13816`)

- `web/src/lib/api/meetings.ts` — typed `Meeting` / `MeetingPatch` interfaces matching the Rust wire shape; `meetingsApi.{list, get, create, patch, delete}`; TanStack hooks `useMeetings` (5s staleTime), `useMeeting`, `useCreateMeeting` (invalidates list), `useDeleteMeeting` (invalidates list + removes per-row cache).
- `web/src/hooks/useGreeting.ts` — pure `greetingFor(now, nameOverride?)` returning `{ timeOfDay, name, greeting }` (morning < 12 / afternoon < 18 / evening); React hook variant snapshots once at render.
- `web/src/components/library/Sidebar.tsx` — 212px paper aside; swirl logo + "yogurt" wordmark; blueberry `+ New meeting` button (mutate + navigate); `All meetings` + `Starred` NavLinks with `bg-blsoft text-blue` active state; 3-sample FOLDERS with `title="Coming in v1.1"` tooltip and opacity-60; matcha `Local-only · on` pill shown iff no active cloud provider (heuristic: base_url contains `localhost / 127.0.0.1 / 0.0.0.0 / :11434 / :1234`); `⚙ Settings` row links to `/settings`.
- `web/src/components/library/MeetingCard.tsx` — `<Link to={`/meeting/${m.id}`}>` with 42px `rounded-[10px]` deterministic-tint avatar (`avatarTint(id)` cycles 3 PALETTE colors by FNV-1a-ish hash), `initials(title)` returns up to 2 uppercase letters or "·", `formatMeta(m)` returns `HH:MM · N min · enhanced` (suffix iff `enriched_md != null`), right-aligned `Local` border-pill.
- `web/src/components/library/DateGroup.tsx` — pure `bucketFor(d, now)` + `groupMeetings(meetings, now)`; renders only non-empty buckets with mono uppercase headers.
- `web/src/components/library/Greeting.tsx` — Instrument-Serif 40px greeting + mono 13px "{N} meeting{s} · all on this Mac" caption.
- `web/src/routes/Library.tsx` — `<div className="flex"><Sidebar /><main>…</main></div>`; renders `<Greeting />` + reserved `data-search-slot` (plan 07-02) + `<DateGroup />` (or `<EmptyStub />` placeholder until plan 07-04).
- `web/src/router.tsx` — `/` → Library; `/starred`, `/welcome`, `*` → `<Navigate to="/" replace />`; existing `/meeting/:id`, `/meeting/:id/post`, `/style-guide`, `/settings` unchanged.
- `web/src/router.test.tsx` — added `QueryClientProvider` wrap; stubs `useMeetings` / `useCreateMeeting` / `settingsApi.get`; asserts `Good {time}, you` heading.
- 4 new Vitest cases in `useGreeting.test.ts` (morning / afternoon / evening / override). 5 new in `DateGroup.test.ts` (TODAY / YESTERDAY / EARLIER / midnight boundary / order preservation).

**Verify:** `pnpm --dir web test` — 109 passed (was 100). `pnpm --dir web build` — success.

### Task 4 — Server bootstrap (deferred)

Per the autonomous-mode directive, the live dev-server boot + curl seeding was skipped. The release-time smoke pass will exercise the checkpoint walkthrough below.

### Task 5 — Visual checkpoint (auto-approved, deferred to release smoke)

The plan's `checkpoint:human-verify` (visit `http://localhost:7878/`, click "+ New meeting", check `~/.yogurt/notes/`) was auto-approved per the AUTONOMOUS mode contract. The full visual walk-through is logged under "Deferred for release smoke" below.

## Deferred for Release Smoke

The autonomous run cannot exercise a browser-mediated visual checkpoint. The following items are scoped for the manual smoke pass before any user-facing release:

- Sidebar 212px width + swirl logo + wordmark visual layout
- `+ New meeting` button hover + focus-ring (blueberry shadow `0_2px_8px_rgba(91,79,199,0.3)`)
- "All meetings" active-state rendering in lilac/blueberry
- 3 hardcoded folder samples with `Coming in v1.1` tooltip + opacity-60
- Matcha "Local-only · on" pill appearance (test only exercises the conditional logic via the settings stub)
- `Good {morning|afternoon|evening}, you` in Instrument Serif at ~40px (test asserts text but not pixel rendering)
- Date-bucket headers (TODAY/YESTERDAY/EARLIER) in mono-uppercase-muted
- 42px tinted-avatar cards: 3-palette cycle verified deterministic
- Link navigation `/meeting/:id` flow
- `~/.yogurt/notes/<…>.md` file emitted on create + on PATCH (server-test exercises this via `tempfile`-isolated notes_dir, but a real ~ path verifies user-home permissions)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Schema conflict] Phase 0 storage already owns the `meetings` table**
- **Found during:** Task 1
- **Issue:** The plan's V003 migration would conflict with the existing Phase 0 `yogurt-server::storage::migrations` declaration of `meetings` in the same physical `~/.yogurt/db.sqlite` file.
- **Fix:** V003 uses `CREATE TABLE IF NOT EXISTS` (harmless when Phase 0 created the table first); a new Rust helper `backfill_phase7_meeting_columns` inspects `PRAGMA table_info('meetings')` and conditionally `ALTER TABLE ADD COLUMN` for the three Phase-7 additions. The `idx_meetings_starred` partial index also moved to Rust so it runs AFTER the column exists.
- **Files modified:** `crates/yogurt-db/migrations/V003__meetings.sql`, `crates/yogurt-db/src/migrations.rs`
- **Commit:** `ab27a55`

**2. [Rule 3 - Architectural integration] Keep Phase-3 streaming Registry**
- **Found during:** Task 2
- **Issue:** Plan says to delete the Phase-3 in-memory `meetings::Registry`, but it owns audio/transcript/events broadcasts and capture-thread join handles — none of which can move to SQLite. Existing `/api/meetings/:id/start /stop /enhance` + chat WS depend on it.
- **Fix:** Added `AppState.meeting_repo: Arc<MeetingRepo>` as a sibling field. Library REST surface uses the new repo; streaming surface keeps the Registry. POST `/api/meetings` creates a row in BOTH with the SAME UUID-v7 id so `/start/:id` can find the streaming meeting via the SQLite-minted id.
- **Files modified:** `crates/yogurt-server/src/state.rs`, `crates/yogurt-server/src/api/meetings.rs`, `crates/yogurt-db/src/meetings.rs` (added `NewMeeting.id` escape hatch)
- **Commit:** `b642152`

**3. [Rule 3 - ID type mismatch] ULID vs UUID v7**
- **Found during:** Task 2
- **Issue:** Plan calls for ULID 26-char ids; entire existing streaming layer uses `uuid::Uuid::now_v7` and axum's `Path<Uuid>` extractor. Switching everything to ULID would break every chat/enhance/ws test.
- **Fix:** `NewMeeting.id: Option<String>` — default mints fresh ULID (preserves the `id should be a 26-char ULID` acceptance criterion in repo unit tests); REST handler passes `live.id.to_string()` (UUID v7 hex) when creating Library rows. The repo doesn't care about the id format.
- **Files modified:** `crates/yogurt-db/src/meetings.rs`, `crates/yogurt-server/src/api/meetings.rs`
- **Commit:** `b642152`

**4. [Rule 3 - Frontend route shape] `/m/:id` vs `/meeting/:id`**
- **Found during:** Task 3
- **Issue:** Plan suggested `/m/:id`; existing Phase 3 Meeting and Phase 4 MeetingPost already mount under `/meeting/:id` and `/meeting/:id/post`.
- **Fix:** Kept `/meeting/:id`. Same conceptual link, smaller diff. Documented in `Sidebar.tsx` header comment.
- **Commit:** `2d13816`

**5. [Rule 3 - Wire shape change] `created_at_ms` → `created_at` ISO 8601**
- **Found during:** Task 2
- **Issue:** Phase 3's `POST /api/meetings` returned `{id, created_at_ms: u64}`. The new yogurt_db::Meeting serializes via chrono as ISO 8601 strings, plus exposes notes_md / transcript_json / starred / etc.
- **Fix:** Updated `meeting_rest::it_creates_a_meeting_and_returns_an_id` to assert on the new `created_at` string + `title` field. Updated `enhance_endpoint::it_gets_a_meeting_after_enhance` to use `started_at` / `ended_at` (was `*_unix_ms`).
- **Files modified:** `crates/yogurt-server/tests/meeting_rest.rs`, `crates/yogurt-server/tests/enhance_endpoint.rs`
- **Commit:** `b642152`

**6. [Rule 3 - Frontend hooks] No useFirstRunRedirect / useScreenRecordingStatus yet**
- **Found during:** Task 3
- **Issue:** Plan referenced `useFirstRunRedirect` (plan 07-04) and `useScreenRecordingStatus` (Phase 2 — turns out not yet exposed as a hook).
- **Fix:** Library.tsx ships without these gates. Plan 07-04 will wire them when those hooks land.
- **Commit:** `2d13816`

## Verification

- ✅ `cargo test -p yogurt-db meetings` — 8 passed
- ✅ `cargo test -p yogurt-server --test meetings_api` — 4 passed
- ✅ `cargo test -p yogurt-server` — full suite passes (server-only delta well under 1s)
- ✅ `cargo test --workspace` — 160 passed (baseline 148, +12 new tests)
- ✅ `cargo clippy --all-targets -- -D warnings` — clean
- ✅ `cargo fmt --all` — applied (style sweep committed separately)
- ✅ `pnpm --dir web test` — 109 passed (baseline 100, +9 new tests)
- ✅ `pnpm --dir web build` — succeeds
- ✅ `grep -n "HashMap" crates/yogurt-server/src/meetings.rs` — only Phase-3 Registry HashMap remains (deliberate — streaming surface)
- ✅ DELETE handler does NOT call `std::fs::remove_file` on the notes path (verified by code inspection at `api/meetings.rs::delete_one`)
- ⚠️ Live UI walkthrough — DEFERRED to release smoke (autonomous mode)

## Self-Check: PASSED

- ✅ Created files present: V003__meetings.sql, V003b__chat_messages_cascade.sql, yogurt-db/src/meetings.rs, yogurt-server/src/api/meetings.rs, yogurt-server/tests/meetings_api.rs, web/src/lib/api/meetings.ts, web/src/hooks/useGreeting.ts + .test.ts, web/src/components/library/{Sidebar,MeetingCard,DateGroup,Greeting}.tsx + DateGroup.test.ts, web/src/routes/Library.tsx
- ✅ Commits present: ab27a55 (Task 1), b642152 (Task 2), 2d13816 (Task 3), 9331b0f (fmt sweep)
- ✅ All verification gates green except the live visual walkthrough (autonomous mode skips browser sessions; documented under Deferred)
