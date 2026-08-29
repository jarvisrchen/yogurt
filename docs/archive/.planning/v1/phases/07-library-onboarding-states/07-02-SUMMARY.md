---
phase: 07-library-onboarding-states
plan: 02
subsystem: library search (FTS5)
tags: [sqlite, fts5, bm25, axum, rusqlite, tanstack-query, react, vitest, debounce, tailwind]
requires:
  - phase: 07
    plan: 01
    artifact: yogurt-db::MeetingRepo + V003 meetings schema + /api/meetings REST surface + web/src/lib/api/meetings.ts client + Library route shell
provides:
  - "yogurt-db::migrations V004__meetings_fts.sql — self-contained FTS5 virtual table over (title, notes_md, transcript_text)"
  - "yogurt-db AFTER INSERT/UPDATE/DELETE triggers keeping meetings_fts in sync with the base table"
  - "yogurt-db::meetings::MeetingRepo::search(query, limit) — bm25-ranked Vec<Meeting>, empty query falls through to list()"
  - "yogurt-db::meetings::extract_transcript_text helper — flattens transcript_json (array of {text|transcript}) to a single space-joined string for FTS indexing"
  - "yogurt-server::api::meetings GET /api/meetings/search?q=&limit= endpoint (limit defaults to 50, hard-capped at 200)"
  - "yogurt-server integration test it_fts_searches_meetings covering hit / no-hit / empty-q-returns-all"
  - "web/src/lib/api/meetings.ts useMeetingsSearch hook + meetingsSearchKey cache key"
  - "web/src/components/library/SearchPill.tsx — controlled input with 200ms debounce, 280px design-token pill"
  - "Library route wiring that switches between useMeetings (chronological) and useMeetingsSearch (FTS5) based on trimmed query"
affects:
  - "crates/yogurt-db/src/meetings.rs (MeetingRepo::patch now also UPDATEs meetings_fts to maintain transcript_text after the trigger writes '')"
  - "crates/yogurt-db/src/migrations.rs (V004 registered after V003b)"
  - "crates/yogurt-server/src/api/meetings.rs (search route registered before /{id})"
  - "web/src/routes/Library.tsx (search state, header flex row, NoMatches inline)"
tech-stack:
  added:
    - "SQLite FTS5 virtual table (already bundled with rusqlite 0.40 + libsqlite3-sys 0.38; no new crate deps)"
  patterns:
    - "Self-contained FTS5 (not content='meetings' external-content) because transcript_text has no physical base column — flatten in Rust at write time, store the flattened copy inside the FTS index"
    - "Triggers handle title + notes_md mirroring; MeetingRepo::patch issues an explicit UPDATE on meetings_fts immediately afterwards to fill in transcript_text from the flattened transcript_json (the trigger writes '' because it has no JSON parser)"
    - "Plain DELETE on the FTS virtual table for cleanup — the contentless-form 'delete' command-form is reserved for content= tables and errors on self-contained tables"
    - "Query sanitization: trim, escape \" → \"\" (FTS5 string-quoting convention), wrap as \"<escaped>\"* for single-phrase prefix match. Prevents column-filter / OR / NEAR operators from leaking into user input"
    - "bm25(meetings_fts) ORDER BY with started_at DESC tiebreaker — most-relevant first, newest of equally-scoring rows wins"
    - "Empty / whitespace-only q falls through to repo.list() truncated to limit — preserves the chronological feed when search is cleared, with parity between the matched and unmatched code paths"
    - "Debounced controlled input: SearchPill keeps a local mirror for snappy typing; only the 200ms trailing edge commits the value upward, where useMeetingsSearch is gated on trimmed.length > 0"
key-files:
  created:
    - .planning/phases/07-library-onboarding-states/07-02-SUMMARY.md
    - crates/yogurt-db/migrations/V004__meetings_fts.sql
    - web/src/components/library/SearchPill.tsx
  modified:
    - crates/yogurt-db/src/migrations.rs
    - crates/yogurt-db/src/meetings.rs
    - crates/yogurt-server/src/api/meetings.rs
    - crates/yogurt-server/tests/meetings_api.rs
    - web/src/lib/api/meetings.ts
    - web/src/routes/Library.tsx
decisions:
  - "Self-contained FTS5 vs external-content: chose self-contained because transcript_text is a synthesized column with no base-table counterpart. External-content would have failed every MATCH at query time. ~1KB-per-meeting storage tax is negligible at the scale a local-first app sees."
  - "Always re-UPDATE meetings_fts in patch() — even when only title/notes_md changed — because the AFTER UPDATE trigger zeros transcript_text. One extra UPDATE per patch is amortized into the edit hot path."
  - "Prefix-match wrap (\"<q>\"* ) instead of letting users compose FTS5 boolean syntax. Typing `notes:foo` searches for the literal string, not a column-restricted query. Power-user query syntax can be added later behind a toggle if anyone asks."
  - "200ms debounce in SearchPill (not 150 or 300). Snappy enough to feel live, slow enough to coalesce a burst of keystrokes into one round-trip."
  - "Empty query branch returns repo.list() truncated to limit (not an empty Vec). Lets the REST endpoint serve as a list-or-search single surface; the frontend still routes via the isSearching boolean to keep the cache keys clean."
metrics:
  duration: "~30 minutes"
  completed: "2026-06-25T23:55Z"
  tasks_completed: 3
  files_created: 3
  files_modified: 6
  cargo_tests: "167 passed (+7 new: 6 db, 1 server)"
  vitest: "109 passed (unchanged — SearchPill has no dedicated test file yet)"
---

# Phase 7 Plan 07-02: FTS5 search + SearchPill Summary

SQLite FTS5 keyword search across title + notes_md + flattened transcripts, exposed as `GET /api/meetings/search` and wired into a debounced `<SearchPill />` at the top-right of the Library main pane.

## Tasks

| # | Name | Commit | Files |
|---|------|--------|-------|
| 1 | V004 FTS5 migration + MeetingRepo::search + 6 unit tests | `eda5098` | `migrations/V004__meetings_fts.sql`, `src/migrations.rs`, `src/meetings.rs` |
| 2 | `/api/meetings/search` REST endpoint + integration test | `5690226` | `crates/yogurt-server/src/api/meetings.rs`, `tests/meetings_api.rs` |
| 3 | SearchPill + useMeetingsSearch + Library wiring | `a6f6b7a` | `web/src/lib/api/meetings.ts`, `components/library/SearchPill.tsx`, `routes/Library.tsx` |
| – | cargo fmt sweep | `c3aaf55` | meetings.rs, meetings_api.rs |

## What got built

- **`meetings_fts` virtual table** (V004) — self-contained FTS5 over title + notes_md + transcript_text, unicode61 + diacritic-folding tokenizer. Three triggers (AFTER INSERT / UPDATE / DELETE on `meetings`) keep the index in sync.
- **`extract_transcript_text` helper** — flattens the `transcript_json` array of `{text|transcript}` segments into a single space-joined string. Empty / invalid JSON returns `""`.
- **`MeetingRepo::patch` FTS sync** — after the per-column UPDATE, re-reads the row and issues an explicit `UPDATE meetings_fts SET title, notes_md, transcript_text WHERE rowid = …`. Necessary because the AFTER UPDATE trigger writes `''` for transcript_text (no JSON parser in SQL).
- **`MeetingRepo::search(query, limit)`** — bm25-ranked. Empty / whitespace query falls through to `list()` truncated to `limit`. Quoted as `"<escaped>"*` so user input can't inject FTS5 query operators.
- **`/api/meetings/search`** route registered *before* `/{id}` so axum 0.8's literal-vs-param matcher dispatches correctly. `limit` defaults to 50, hard-capped at 200.
- **`useMeetingsSearch(q)`** — TanStack Query hook with key `["meetings", "search", trimmed]`, 5s staleTime, `enabled: trimmed.length > 0`. Uses the existing `json<T>` helper so the bootstrap session token is attached.
- **`<SearchPill />`** — 280px rounded controlled input with a 200ms trailing-edge debounce. Local mirror for snappy typing; only the debounced value lifts up.
- **Library route** — flex header (`Greeting` left, `SearchPill` right); body chooses between `useMeetings` and `useMeetingsSearch` based on `trimmedQuery.length > 0`. Inline "No matches" row when the search is active but returns nothing.

## Verification

- `cargo test -p yogurt-db meetings` → 14 passed (8 prior + 6 new FTS tests)
- `cargo test -p yogurt-server --test meetings_api` → 5 passed (4 prior + 1 FTS integration test)
- `cargo test --workspace` → 167 passed (1 ignored, 0 failed) — was 160, +7 from this plan
- `cargo clippy --all-targets -- -D warnings` → clean
- `cargo fmt --check` → clean (post-sweep)
- `pnpm --dir web test` → 109 passed (unchanged)
- `pnpm --dir web build` → ok (837 kB JS / 274 kB gzip — same shape as 07-01)

## Acceptance criteria

- [x] V004__meetings_fts.sql contains `CREATE VIRTUAL TABLE meetings_fts USING fts5`
- [x] Three triggers `meetings_ai`, `meetings_ad`, `meetings_au`
- [x] `lib.rs` (migrations.rs) MIGRATIONS slice includes V004 via `include_str!`
- [x] `MeetingRepo::search(&self, query: &str, limit: usize) -> Result<Vec<Meeting>>` with `bm25(meetings_fts)` in ORDER BY
- [x] `extract_transcript_text` helper
- [x] All 6 new search unit tests pass
- [x] `/api/meetings/search` route registered before `/{id}`
- [x] `SearchQuery { q, limit? }` struct
- [x] Integration test `it_fts_searches_meetings` passes; all prior tests still pass
- [x] `useMeetingsSearch` exported
- [x] SearchPill is a real `<input type="text">`, not a stub
- [x] Library route uses both `useMeetings` + `useMeetingsSearch` toggled by query
- [x] `pnpm --dir web build` succeeds

## Deviations from Plan

### [Rule 3 - Blocking] FTS5 external-content table failed at query time

**Found during:** Task 1, first test run.
**Issue:** The plan specified `CREATE VIRTUAL TABLE meetings_fts USING fts5(... content='meetings', content_rowid='rowid' ...)`. With external-content tables, FTS5 reads column values from the base table at query time — but `transcript_text` is a synthesized projection of `transcript_json` and has no physical column on `meetings`. Every UPDATE and MATCH failed with `no such column: T.transcript_text`.
**Fix:** Dropped `content=` and `content_rowid=` from the CREATE VIRTUAL TABLE statement. The FTS index is now self-contained — stores its own copy of the indexed columns, populated by the triggers + the explicit `UPDATE meetings_fts` in `MeetingRepo::patch`. Storage cost ~1KB/meeting, immaterial at local-first scale. Also swapped the AFTER DELETE trigger from the contentless `('delete', ...)` command form to a plain `DELETE FROM meetings_fts WHERE rowid = old.rowid` — the contentless form is only valid for `content=''` (truly contentless) FTS5 tables.
**Files modified:** `crates/yogurt-db/migrations/V004__meetings_fts.sql`
**Captured in:** the Task 1 commit `eda5098` (no separate fix commit — the bug was caught and corrected before the first green test run).

### [Rule 1 - Bug] Plan task 2 referenced wrong AppState field

**Found during:** Task 2 implementation.
**Issue:** The plan's handler example used `s.meetings.search(...)`. The actual streaming registry is `s.meetings: MeetingsRegistry` (in-memory audio/broadcast state) — the persistent SQLite directory is `s.meeting_repo: Arc<MeetingRepo>`.
**Fix:** Used `s.meeting_repo.clone()` and ran the synchronous `repo.search` inside `tokio::task::spawn_blocking` for parity with the other handlers in this file.
**Files modified:** `crates/yogurt-server/src/api/meetings.rs`
**Captured in:** the Task 2 commit `5690226`.

### [Rule 3 - Blocking] Integration test signature mismatch

**Found during:** Task 2.
**Issue:** The plan's test example called `spawn_server()` and destructured into `(base, _tmp)`, but the actual fixture at `tests/meetings_api.rs` returns `(SocketAddr, String /* token */, JoinHandle, PathBuf, TempDir)` and the `/api/meetings*` routes require a session token.
**Fix:** Adapted the new `it_fts_searches_meetings` test to the real signature: 5-tuple destructuring, `bearer_auth(&token)` on every request, `http://{addr}/...` URL form, `handle.abort()` cleanup.
**Files modified:** `crates/yogurt-server/tests/meetings_api.rs`
**Captured in:** the Task 2 commit `5690226`.

No Rule 4 (architectural) decisions, no Rule 2 (missing-critical) additions. No checkpoints required — autonomous flag was honored.

## Known stubs

None introduced by this plan. The earlier Library `EmptyStub` placeholder (from Plan 07-01) is preserved verbatim; the new `NoMatches` row is itself stub-shaped but explicit by design (Plan 07-04 may polish it).

## Threat flags

None — this plan adds a read-only search endpoint behind the existing session-token middleware. No new auth paths, no new trust boundaries, no new file-system surface. User input is escaped before reaching SQLite (FTS5 quoting) and `limit` is server-capped at 200.

## Self-Check: PASSED

- [x] `crates/yogurt-db/migrations/V004__meetings_fts.sql` exists
- [x] `crates/yogurt-db/src/migrations.rs` registers V004 with `include_str!`
- [x] `crates/yogurt-db/src/meetings.rs` contains `pub fn search` and `fn extract_transcript_text`
- [x] `crates/yogurt-server/src/api/meetings.rs` contains `async fn search` and `SearchQuery`
- [x] `crates/yogurt-server/tests/meetings_api.rs` contains `it_fts_searches_meetings`
- [x] `web/src/lib/api/meetings.ts` exports `useMeetingsSearch`
- [x] `web/src/components/library/SearchPill.tsx` renders `<input type="text">`
- [x] `web/src/routes/Library.tsx` calls both `useMeetings` and `useMeetingsSearch`
- [x] Commits in `git log`: `eda5098`, `5690226`, `a6f6b7a`, `c3aaf55`
