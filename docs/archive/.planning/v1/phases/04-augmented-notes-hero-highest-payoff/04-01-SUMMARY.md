---
phase: 04-augmented-notes-hero-highest-payoff
plan: 01
subsystem: prompts + storage + llm-client
tags: [prompts, sqlite-migration, llm-client, mock-llm, phase-4-foundations]
dependency_graph:
  requires:
    - phase-0 storage::migrations::run + Mutex<Connection> single-writer
    - phase-0 workspace.dependencies (rust-embed, anyhow, serde, tracing, reqwest)
  provides:
    - yogurt_prompts::Prompts (load/render_enhance/chat_system)
    - yogurt_prompts::EnhanceCtx
    - yogurt_prompts::Mode { Dev, Release }
    - meetings.enriched_doc_json TEXT column in SQLite
    - yogurt_server::llm_openai::OpenAiCompatClient (crate-private)
    - yogurt_server::llm_mock::MockLlm (crate-private)
  affects:
    - Plan 04-03 enhance handler (consumes all three pieces)
    - Phase 5 LlmClient trait promotion (replaces OpenAiCompatClient + MockLlm)
    - Phase 6 in-meeting chat (consumes chat-system.md)
tech_stack:
  added:
    - tinytemplate 1.2 (prompt templating with HTML-escape disabled)
  patterns:
    - rust-embed for release-mode bundled templates
    - PRAGMA table_info guard for idempotent additive SQLite migrations
    - hand-rolled tokio TCP listener for HTTP request-shape assertions
      (avoids wiremock dev-dep)
key_files:
  created:
    - crates/yogurt-prompts/Cargo.toml
    - crates/yogurt-prompts/build.rs
    - crates/yogurt-prompts/src/lib.rs
    - crates/yogurt-prompts/src/ctx.rs
    - crates/yogurt-prompts/templates/enhance.md
    - crates/yogurt-prompts/templates/chat-system.md
    - crates/yogurt-prompts/tests/rendering.rs
    - crates/yogurt-server/src/storage/migrations/V0004__add_enriched_doc_json.sql
    - crates/yogurt-server/src/llm_openai.rs
    - crates/yogurt-server/src/llm_mock.rs
  modified:
    - Cargo.toml (workspace.members += yogurt-prompts; workspace.dependencies += tinytemplate)
    - crates/yogurt-server/src/storage/migrations.rs (V0004 wiring + column_exists guard)
    - crates/yogurt-server/src/lib.rs (mod llm_openai + mod llm_mock registered)
    - crates/yogurt-server/tests/storage.rs (assert enriched_doc_json present + idempotency test)
decisions:
  - "Used `{notes}` / `{transcript}` tinytemplate native syntax (not `{{NOTES}}` / `{{TRANSCRIPT}}`) per CONTEXT D-16 resolution"
  - "Migration runner uses `include_str!()` of V0004 SQL file (single-binary distribution preserved) with PRAGMA table_info guard for idempotency"
  - "OpenAiCompatClient + MockLlm are inherent-method structs, NOT trait impls (Phase 5 introduces LlmClient trait per D-19/D-21)"
  - "Test of HTTP request shape uses hand-rolled tokio TCP listener (avoids pulling wiremock as a dev-dep)"
metrics:
  duration_minutes: ~75
  completed_date: 2026-06-26T02:06:51Z
  tasks_completed: 3
  files_created: 10
  files_modified: 4
  tests_added: 9 (3 prompts + 5 llm + 1 idempotency)
  production_loc:
    llm_openai_rs: 73
    llm_mock_rs: 61
    prompts_lib_rs: 100
    V0004_sql: 10
---

# Phase 4 Plan 01: Hero Foundations Summary

**One-liner:** Bootstrapped the three Phase 4 ingredients the augmented-notes hero depends on — `yogurt-prompts` crate with embedded+hot-reload `enhance.md`/`chat-system.md`, the `enriched_doc_json TEXT` SQLite migration so TipTap marks survive restart, and the tactical ~73-LOC `OpenAiCompatClient` plus deterministic `MockLlm` fallback — all unit-tested in isolation and ready for Plan 04-03's enhance handler to consume.

## Objective achieved

Phase 4 cannot ship its hero "30-second augmented-notes" experience without (a) a prompt to send the LLM, (b) a place to store the result, and (c) a client to call the LLM. This plan lands all three with zero UI dependency, no Keychain wiring (Phase 5), and no trait abstraction (Phase 5). The minimal `OpenAiCompatClient` is the deliberate tactical decision documented in CONTEXT D-19/D-21 — it will be deleted and replaced by `LlmClient` in Phase 5.

## What shipped

### Task 1 — `yogurt-prompts` crate (commit `56c1dfa`)

New workspace member `crates/yogurt-prompts` exposes:

- `Prompts::load(Mode::{Dev, Release})` — Dev re-reads `templates/` on every call (hot-reload for power users); Release caches the `rust-embed`-baked copy once at construction.
- `Prompts::render_enhance(&EnhanceCtx { notes, transcript })` — substitutes the user's notes and the JSON-serialized transcript into `enhance.md` via tinytemplate. `set_default_formatter(&tinytemplate::format_unescaped)` is the critical bit — HTML-ish content in user notes (`<emphasis>`, `&`) must reach the LLM verbatim, not as `&lt;emphasis&gt;`.
- `Prompts::chat_system()` — returns `chat-system.md` literally; consumed by Phase 6.

Templates: `enhance.md` is the hero "merge user notes with transcript" prompt with the wire-format span contract spelled out (`<span data-ai-grey data-ts="N">…</span>`, `<span data-transcript-link data-ts="N">↳ HH:MM</span>`). `chat-system.md` is a single paragraph telling the model to answer from transcript-so-far only.

Three rendering tests (`crates/yogurt-prompts/tests/rendering.rs`):
1. `it_renders_enhance_with_notes_and_transcript` — pins notes + transcript substitute and the prompt scaffolding ("USER NOTES") is present.
2. `it_serves_chat_system_unmodified` — pins the chat-system literal.
3. `it_does_not_html_escape_special_chars_in_notes` — pins the HTML-escape-disabled invariant.

### Task 2 — `enriched_doc_json` migration V0004 (commit `1d6792f`)

The Phase 4 portion of the STORE-01 split-mapping. Phase 0's `storage::migrations::run` was extended with an additive `ALTER TABLE meetings ADD COLUMN enriched_doc_json TEXT`, guarded by a `PRAGMA table_info(meetings)` check inside the same transaction (SQLite lacks `ALTER TABLE … IF NOT EXISTS`).

The SQL itself lives in `crates/yogurt-server/src/storage/migrations/V0004__add_enriched_doc_json.sql` for grep/audit and is `include_str!()`ed into the runner so the binary is still single-file. The existing `tests/storage.rs` `it_initializes_db_with_wal_and_tables` test had its Phase-0-only "must NOT contain enriched_doc_json" assertion inverted to "must contain", and a new `it_adds_enriched_doc_json_only_once_across_multiple_inits` test pins the idempotency guard by running `Storage::init_at` three times and asserting `pragma_table_info('meetings') WHERE name = 'enriched_doc_json'` returns exactly 1 row.

### Task 3 — `OpenAiCompatClient` + `MockLlm` (commit `7207664`)

`crates/yogurt-server/src/llm_openai.rs` (73 lines of production code, under the ≤80 acceptance ceiling):

- `OpenAiCompatClient::from_env()` reads `YOGURT_LLM_BASE_URL` + `YOGURT_LLM_API_KEY` + `YOGURT_LLM_MODEL`; returns `None` if any are absent (caller falls back to MockLlm).
- `OpenAiCompatClient::new(base_url, api_key, model)` — explicit construction for tests and Phase 5 migration.
- `complete(system, user) -> Result<String>` — POSTs OpenAI-compat JSON to `<base_url>/chat/completions`, parses `choices[0].message.content`. Non-streaming; Phase 5 adds SSE.

`crates/yogurt-server/src/llm_mock.rs`:

- `MockLlm::complete` parses the `## USER NOTES` and `## TRANSCRIPT` markers out of the prompt (we own `enhance.md`, so we know the format), echoes the notes verbatim, and appends one `<span data-ai-grey data-ts="N">first-8-words <span data-transcript-link data-ts="N">↳ HH:MM</span></span>` bullet per transcript segment. Deterministic, no network, suitable for fixture tests + offline dev.

Both registered in `lib.rs` as `pub(crate)` modules. Plan 04-03 will write:
```rust
let text = match OpenAiCompatClient::from_env() {
    Some(c) => c.complete(system, user).await?,
    None => MockLlm.complete(system, user).await?,
};
```

Five tests:
- `llm_openai::tests::it_posts_chat_completions_and_returns_content` — hand-rolled single-shot tokio TCP listener captures the request, asserts POST `/chat/completions`, `Authorization: Bearer <key>`, model + system + user in body, response content parsed.
- `llm_openai::tests::from_env_returns_none_when_any_var_missing` — clears env vars, asserts `from_env()` returns `None`, restores.
- `llm_mock::tests::it_echoes_notes_and_adds_one_bullet_per_segment` — hero shape: notes preserved, AI bullet tagged with `data-ai-grey data-ts="120"` and `↳ 02:00`.
- `llm_mock::tests::it_returns_empty_friendly_doc_when_prompt_lacks_markers` — defensive: malformed prompt yields empty body, not a panic.
- `llm_mock::tests::it_produces_no_ai_bullets_when_transcript_is_empty` — empty transcript yields zero AI bullets.

## Verification

All acceptance gates green:

```
$ cargo build --release -p yogurt-prompts        ✅ 0 warnings
$ cargo build --release -p yogurt-server         ✅ compiles (Plan 04-02's
                                                   markdown_exporter.rs has
                                                   a pre-existing deprecated-
                                                   API warning — out of scope)
$ cargo test -p yogurt-prompts                   ✅ 3 passed
$ cargo test -p yogurt-server --lib llm          ✅ 5 passed
$ cargo test -p yogurt-server --test storage     ✅ 6 passed (was 4 in Phase 0;
                                                   +1 inverted +1 new idempotency)
$ cargo clippy -p yogurt-prompts -- -D warnings  ✅ clean
$ cargo clippy -p yogurt-server --lib --tests    ✅ clean
$ cargo fmt -- --check                           ✅ clean
$ grep "enriched_doc_json TEXT" .../V0004*.sql   ✅ matches
$ wc -l crates/yogurt-server/src/llm_openai.rs   174 total
$ awk-strip-tests llm_openai.rs                  73 LOC ≤ 80 ✅
$ grep -r "trait LlmClient" crates/yogurt-server ✅ empty (Phase 5)
$ grep "USER NOTES" templates/enhance.md         ✅ matches
$ grep "watching a meeting" templates/chat-*.md  ✅ matches
$ grep "set_default_formatter" prompts/src/lib   ✅ matches
$ grep "tinytemplate" Cargo.toml                 ✅ matches
```

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] MockLlm `trim_matches` stripped leading bullet dashes**
- **Found during:** Task 3 test run (`it_echoes_notes_and_adds_one_bullet_per_segment`)
- **Issue:** The superpowers-plan-verbatim `trim_matches(|c: char| c == '-' || c.is_whitespace())` ate the leading `- ` from each user note bullet — output was `pricing\n- timeline\n` instead of `- pricing\n- timeline\n`. Violated the "user notes preserve verbatim" invariant (PRD §5.3 hero contract).
- **Fix:** Replaced with `notes.trim()` — outer whitespace only, leaves bullets intact.
- **Files modified:** `crates/yogurt-server/src/llm_mock.rs`
- **Commit:** `7207664`

**2. [Rule 1 — Test bug] OpenAiCompatClient test asserted on case-sensitive `Authorization` header**
- **Found during:** Task 3 test run (`it_posts_chat_completions_and_returns_content`)
- **Issue:** `reqwest` writes HTTP headers in lowercase wire form (`authorization: …`). Test asserted on `Authorization: Bearer test-key` and failed.
- **Fix:** Lowercased the request string before substring-asserting on the header.
- **Files modified:** `crates/yogurt-server/src/llm_openai.rs` (test only)
- **Commit:** `7207664`

**3. [Rule 2 — Missing critical] Dead-code warnings broke clippy gate**
- **Found during:** Task 3 clippy pass
- **Issue:** `OpenAiCompatClient` + `MockLlm` are reachable only from their own tests until Plan 04-03 wires them up. With clippy `-D warnings`, the `dead_code` lints failed the build. Plan 04-01 must produce a clean clippy pass.
- **Fix:** Added `#[allow(dead_code)]` to both structs and their impl blocks, with a doc comment explaining the transitional state. Plan 04-03 can remove the allows when wiring the modules.
- **Files modified:** `crates/yogurt-server/src/llm_openai.rs`, `crates/yogurt-server/src/llm_mock.rs`
- **Commit:** `7207664`

**4. [Rule 2 — Missing critical] V0004 migration not idempotent**
- **Found during:** Task 2 implementation review
- **Issue:** The plan's example SQL `ALTER TABLE meetings ADD COLUMN enriched_doc_json TEXT;` raises `SQLITE_ERROR: duplicate column name: enriched_doc_json` on a second boot. The existing migration runner is idempotent by design (Phase 0 uses `CREATE TABLE IF NOT EXISTS`), so a non-idempotent V0004 would break every restart after first boot.
- **Fix:** Wrapped the ALTER in a `column_exists()` helper that consults `PRAGMA table_info(meetings)` inside the same transaction and skips the ALTER when the column is already present. Added `it_adds_enriched_doc_json_only_once_across_multiple_inits` regression test (3 inits → exactly 1 column row).
- **Files modified:** `crates/yogurt-server/src/storage/migrations.rs`, `crates/yogurt-server/tests/storage.rs`
- **Commit:** `1d6792f`

### Deferred Issues

**1. Plan 04-02's `markdown_exporter.rs` has a deprecated-API warning**
- **What:** `time::format_description::parse` is deprecated in current `time` crate — use `parse_borrowed` instead.
- **Why deferred:** Out of scope — `markdown_exporter.rs` is Plan 04-02's file, not mine. The user-provided dispatch instructions explicitly told me "Don't touch anything outside the scope listed below". Logged here for Plan 04-02 / verification to address.
- **Impact:** None on my acceptance gates (`cargo clippy -p yogurt-server --lib --tests` excludes the markdown_exporter target).

## Coordination with parallel Plan 04-02

Plan 04-02 was dispatched in parallel on the same branch. Coordination notes:

1. **Workspace `Cargo.toml`:** Both plans added members + workspace deps to the root `Cargo.toml`. My additions (`yogurt-prompts` member, `tinytemplate` dep) and 04-02's additions (`yogurt-notes` member; `pulldown-cmark`, `regex-lite`, `insta` deps) were composed cleanly without conflict.
2. **`crates/yogurt-server/src/lib.rs`:** Both plans needed to register new modules. 04-02 added `mod markdown_exporter` + a `__test_only_markdown_exporter` re-export first; I added `mod llm_openai` and `mod llm_mock` alongside without touching theirs.
3. **First commit (`56c1dfa`) collision:** During Task 1 commit, a race between my `git add` and 04-02's parallel staging caused `crates/yogurt-notes/*` files (Plan 04-02's WIP) to be swept into my commit. The combined commit is functionally additive (no overlap with my prompts files; 04-02 still owns the yogurt-notes content via subsequent commits). Tasks 2 and 3 used tighter immediate-add-then-commit cycles and produced clean 3-file commits.

## Files Created / Modified

**Created (10):**
- `crates/yogurt-prompts/Cargo.toml`
- `crates/yogurt-prompts/build.rs`
- `crates/yogurt-prompts/src/lib.rs`
- `crates/yogurt-prompts/src/ctx.rs`
- `crates/yogurt-prompts/templates/enhance.md`
- `crates/yogurt-prompts/templates/chat-system.md`
- `crates/yogurt-prompts/tests/rendering.rs`
- `crates/yogurt-server/src/storage/migrations/V0004__add_enriched_doc_json.sql`
- `crates/yogurt-server/src/llm_openai.rs`
- `crates/yogurt-server/src/llm_mock.rs`

**Modified (4):**
- `Cargo.toml` — workspace member `yogurt-prompts`, workspace dep `tinytemplate`
- `crates/yogurt-server/src/storage/migrations.rs` — V0004 wiring + `column_exists` helper
- `crates/yogurt-server/src/lib.rs` — `mod llm_openai` + `mod llm_mock` registered
- `crates/yogurt-server/tests/storage.rs` — Phase 4 column-present assertion + idempotency test

## Commits

- `56c1dfa` — `feat(prompts): bootstrap yogurt-prompts crate with enhance.md + chat-system.md`
- `1d6792f` — `feat(server,STORE-01): V0004 — add enriched_doc_json TEXT to meetings`
- `7207664` — `feat(server): minimal OpenAiCompatClient + MockLlm (Phase 4 tactical)`

## What this plan does NOT do

- **No `LlmClient` trait** — Phase 5 introduces it (CONTEXT D-19/D-21). Phase 4 is intentionally tactical so the hero ships without blocking on the provider-config UX.
- **No Keychain wiring** — Phase 5. Phase 4 reads env vars directly.
- **No settings UI** — Phase 5.
- **No enhance endpoint** — Plan 04-03 (after the merge logic in Plan 04-02 lands).
- **No TipTap marks / editor wiring / deep-link click handlers** — Plans 04-03 and 04-04.
- **No `.env.local` loader** — if Phase 5 doesn't already add one, document a one-line `export YOGURT_LLM_BASE_URL=…` in README at Phase 5 time.
- **No streaming** — `complete()` is single-shot. Phase 5 adds SSE.

## Threat Flags

No new security-relevant surface introduced beyond what the plan's threat model contemplates. `OpenAiCompatClient` reads three env vars (`YOGURT_LLM_*`) — these are convention-only and explicitly deferred to Phase 5's Keychain integration; no plaintext key persists to disk via this plan.

## Self-Check: PASSED

All files verified to exist on disk:
- `crates/yogurt-prompts/{Cargo.toml,build.rs,src/lib.rs,src/ctx.rs,templates/enhance.md,templates/chat-system.md,tests/rendering.rs}` — FOUND
- `crates/yogurt-server/src/storage/migrations/V0004__add_enriched_doc_json.sql` — FOUND
- `crates/yogurt-server/src/{llm_openai.rs,llm_mock.rs}` — FOUND

All commits verified to exist via `git log --oneline`:
- `56c1dfa` — FOUND
- `1d6792f` — FOUND
- `7207664` — FOUND
