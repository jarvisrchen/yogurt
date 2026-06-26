---
phase: 04-augmented-notes-hero-highest-payoff
plan: 02
subsystem: notes
tags: [yogurt-notes, markdown-exporter, ast-diff, structural-diff, tdd, fixtures, hero-feature]
requires:
  - phase-00 (yogurt-server crate + Mode enum)
  - phase-03 (transcript JSON shape — TranscriptSegment ts_ms/channel/text)
provides:
  - yogurt-notes::merge_notes (structural block-level AST diff)
  - yogurt-notes::render::to_markdown (wire-format markdown emit)
  - yogurt-notes::ts::guess_ts_sec (word-overlap timestamp heuristic)
  - yogurt-notes::ast::{Block, parse, block_key}
  - yogurt-server::__test_only_markdown_exporter::{MarkdownExporter, Meeting}
affects:
  - root Cargo.toml workspace members + 3 new workspace deps (pulldown-cmark, regex-lite, insta) + time
  - crates/yogurt-server/Cargo.toml (time dep)
  - crates/yogurt-server/src/lib.rs (markdown_exporter module + test re-export)
tech-stack:
  added:
    - pulldown-cmark 0.12 (no default features)
    - regex-lite 0.1
    - insta 1.41 (dev-dep)
    - time 0.3 (formatting + macros features)
  patterns:
    - "TDD against fixture pairs (notes.md + transcript.json + enriched.md + expected.json) for structural diff coverage"
    - "Compile-time format_description! macro vs deprecated runtime parse"
    - "Atomic file write: tmp + rename pattern"
    - "Test-only #[doc(hidden)] re-export of crate-private modules for integration tests"
key-files:
  created:
    - crates/yogurt-notes/Cargo.toml
    - crates/yogurt-notes/src/lib.rs
    - crates/yogurt-notes/src/ast.rs
    - crates/yogurt-notes/src/diff.rs
    - crates/yogurt-notes/src/render.rs
    - crates/yogurt-notes/src/ts.rs
    - crates/yogurt-notes/tests/merge_fixtures.rs
    - crates/yogurt-notes/tests/fixtures/01_pure_new_ai/{notes.md,transcript.json,enriched.md,expected.json}
    - crates/yogurt-notes/tests/fixtures/02_ai_under_user_heading/{notes.md,transcript.json,enriched.md,expected.json}
    - crates/yogurt-notes/tests/fixtures/03_ai_bullet_next_to_user/{notes.md,transcript.json,enriched.md,expected.json}
    - crates/yogurt-notes/tests/fixtures/04_promote_grey_on_edit/{notes.md,transcript.json,enriched.md,expected.json}
    - crates/yogurt-notes/tests/fixtures/05_reenhance_preserves_promoted/{notes.md,transcript.json,enriched.md,expected.json}
    - crates/yogurt-server/src/markdown_exporter.rs
    - crates/yogurt-server/tests/markdown_exporter.rs
  modified:
    - Cargo.toml (workspace member + 4 deps)
    - Cargo.lock (transitive lockfile updates)
    - crates/yogurt-server/Cargo.toml (time workspace dep)
    - crates/yogurt-server/src/lib.rs (markdown_exporter module + __test_only_markdown_exporter re-export)
decisions:
  - "block_key() strips wire-format marker spans (data-ai-grey / data-transcript-link) before identity comparison — lets a re-enhance preserve user blocks even if the LLM wrapped them in markers"
  - "render::to_markdown skips marker spans on headings (editor color-tints via parent block.source) but wraps list items / paragraphs / blockquotes / code blocks with data-ai-grey + data-transcript-link"
  - "Defensive: any user block missing from enriched_md is appended at end as Source::User — LLM never gets to silently drop user text"
  - "time::macros::format_description! over runtime parse to avoid deprecation warning under -D warnings"
  - "Test-only re-export via __test_only_markdown_exporter so the integration test can exercise crate-private MarkdownExporter without leaking it to the public API; Plan 04-03 will wire it from inside the crate"
metrics:
  duration: "~7 minutes wall-clock"
  completed: "2026-06-25T18:59:00-07:00"
  tasks_completed: 3
  files_created: 24
  files_modified: 4
---

# Phase 4 Plan 02: yogurt-notes AST Diff + MarkdownExporter Summary

Server-side **structural** markdown AST diff (`yogurt-notes`) and atomic per-meeting markdown writer (`MarkdownExporter` inside `yogurt-server`) — locks the load-bearing piece of Phase 4's hero contract with 6 passing tests against 5 hand-built fixtures + 3 atomic-write tests.

## What Was Built

### `yogurt-notes` crate (new)

- **`merge_notes(user_md, enriched_md, transcript_json) -> MergedDoc`** — the public surface. Pure function: three strings in, one structured `MergedDoc` out. The 5 fixtures lock the contract so Phase 5+ refactors can't silently break behavior.
- **`ast::parse` + `Block` enum** (`Heading | Paragraph | ListItem | CodeBlock | BlockQuote | Hr`) — pulldown-cmark event stream collapsed into a flat block list with depth-tracked list items.
- **`ast::block_key(b)`** — canonical identity function that strips `<span data-ai-grey ...>` / `<span data-transcript-link ...>` / `</span>` markers (via `regex-lite`) before lowercasing + trimming, so the LLM's re-wrapping of a user block does not break identity.
- **`diff::merge`** — HashMap-keyed walk: for each enriched block, emit `Source::User` (using the user's exact text) if `block_key` hits the user set, else `Source::AiGrey { transcript_ts_sec }`. Appends any user blocks the LLM dropped at the end (defensive — never lose user text).
- **`ts::guess_ts_sec`** — word-overlap heuristic: tokenize block on non-alphanumeric, keep tokens with `len > 3`, count overlaps against each transcript segment, pick highest count (tie-break earliest ts). Falls back to first segment if no overlap; returns `None` if block has no >3-char tokens and transcript is empty.
- **`render::to_markdown`** — `MergedDoc` -> wire-format markdown. List items / paragraphs / blockquotes / code blocks tagged `Source::AiGrey` get wrapped in `<span data-ai-grey data-ts="N">…</span>` with trailing `<span data-transcript-link data-ts="N">↳ HH:MM</span>`. Headings deliberately do NOT get wire-format markers — the editor color-tints them via the parent block's `source`.

### Test fixtures (TDD heart of Phase 4)

5 hand-built fixture pairs covering every contractual claim in PRD §5.3:

| Fixture | Scenario | Asserts |
|---------|----------|---------|
| 01_pure_new_ai | empty notes, 2 transcript segments, LLM produces heading + 2 bullets | All 3 blocks tagged `AiGrey` with timestamps 120/120/240 (word-overlap matches "pricing"→120, "roadmap"→240) |
| 02_ai_under_user_heading | user wrote `## Pricing`, LLM added 2 bullets under it | Heading is `User` (preserved), bullets are `AiGrey` (ts=0, empty transcript) |
| 03_ai_bullet_next_to_user | user wrote 2 bullets, LLM inserted 1 between | Positions 0+2 `User`, position 1 `AiGrey` |
| 04_promote_grey_on_edit | re-enhance: user edited a previously-AI bullet, LLM tried to re-add original | Both User blocks survive; LLM's shorter version becomes `AiGrey` (defensive append moves user's edited block to end) |
| 05_reenhance_preserves_promoted | same as 04 + LLM also adds new "Annual plan" bullet | "pricing" + "Annual plan" inline, then user's edited bullet appended (User) |

Plus 1 render round-trip test asserting wire-format spans (`data-ai-grey data-ts="120"`, `data-ai-grey data-ts="240"`, `data-transcript-link data-ts="240">↳ 04:00</span>`).

### `MarkdownExporter` (`yogurt-server`)

- `pub struct MarkdownExporter { notes_dir: PathBuf }`
- `MarkdownExporter::new(notes_dir)` — creates `notes_dir` if absent (calls `create_dir_all`).
- `MarkdownExporter::write(&Meeting) -> Result<PathBuf>` — atomic write via `<final>.tmp` + `std::fs::rename`. Filename `<YYYY-MM-DD-HHmm>-<slug>.md` derived from `started_at_unix_ms` (UTC, via `time::macros::format_description!`) + dasherized title (falls back to `untitled` for blank/whitespace input).
- YAML front-matter carries `id`, `title` (quoted + escaped), `started_at`, `ended_at` (or `null`).
- Crate-private module; re-exported via `__test_only_markdown_exporter` (`#[doc(hidden)]`) for the integration test until Plan 04-03 wires it into the enhance handler.

## Verification Results

| Gate | Result |
|------|--------|
| `cargo build -p yogurt-notes -p yogurt-server` | clean |
| `cargo test -p yogurt-notes` | **6 passed** (5 merge_fixtures + 1 render round-trip) |
| `cargo test -p yogurt-server --test markdown_exporter` | **3 passed** |
| `cargo clippy -p yogurt-notes -p yogurt-server -- -D warnings` | clean |
| `cargo fmt -p yogurt-notes` / `-p yogurt-server` | clean |
| 5 fixture dirs each with 4 files | confirmed (20 fixture files total) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] `time::format_description::parse` deprecated under `-D warnings`**
- **Found during:** Task 3 clippy gate
- **Issue:** Plan-template-suggested `time::format_description::parse("[year]...")` is deprecated in `time` 0.3.x — emits a `-D deprecated` warning that clippy promotes to an error.
- **Fix:** Switched to compile-time `time::macros::format_description!("[year]-[month]-[day]-[hour][minute]")`. Same format, no allocation, no deprecation. Requires `time` `macros` feature (already enabled).
- **Files modified:** `crates/yogurt-server/src/markdown_exporter.rs`
- **Commit:** included in Task 3 commit `67239bd`

**2. [Rule 3 — Blocking] Workspace missing `time` dep**
- **Found during:** Task 3 build
- **Issue:** PRD recommended `time = "0.3"` but Phase 0 hadn't actually wired it into `[workspace.dependencies]`. `MarkdownExporter::write` needs date formatting for the dated-slug filename.
- **Fix:** Added `time = { version = "0.3", features = ["formatting", "macros"] }` to workspace deps and wired into `yogurt-server`. Aligns with PRD §11 stack pick.
- **Files modified:** `Cargo.toml`, `crates/yogurt-server/Cargo.toml`
- **Commit:** included in Task 3 commit `67239bd`

**3. [Rule 3 — Blocking] Plan-suggested fixture `notes.md` content `(empty)` would break the merge**
- **Found during:** Task 2 RED→GREEN
- **Issue:** The superpowers plan shows fixture 02's `notes.md` as `## Pricing\n\n(empty)\n` — if `(empty)` were literal content it would parse as an unmatched user paragraph block, get appended defensively at end as `Source::User`, and break the expected output (which has exactly Heading + 2 AiGrey bullets, no trailing User paragraph).
- **Fix:** Interpreted `(empty)` annotations as placeholder text meaning "the file has no further content past the prior line". Fixture 01 is a truly empty file; fixture 02 ends after `## Pricing\n`; fixtures 03/04/05 only contain the bullets shown.
- **Files modified:** all 5 `notes.md` fixture files
- **Commit:** absorbed into commit `56c1dfa` (see Concurrent-Agent Note below)

### Concurrent-Agent Commit Interleave

Plan 04-01 (parallel Wave 1) and this plan ran simultaneously on `gsd/autonomous`. My Task 2 work was staged but the parallel agent's `git commit` (commit `56c1dfa feat(prompts): bootstrap yogurt-prompts crate ...`) consumed everything in the index at that moment, so:

- **Task 1 (yogurt-notes scaffold)** landed in `48ac611 feat(04-02): scaffold yogurt-notes crate with pulldown-cmark block AST` — clean, just my files.
- **Task 2 (5 fixtures + diff + ts + render + merge_fixtures.rs)** was absorbed into `56c1dfa feat(prompts): bootstrap yogurt-prompts crate ...`. The work IS in git history (verifiable: `git show 56c1dfa --stat` lists all 24 of my Task 2 files alongside the prompts files) — it's only the commit message that doesn't match. No code-level conflict; the file scopes were fully disjoint.
- **Task 3 (MarkdownExporter)** landed in `67239bd feat(04-02): MarkdownExporter — atomic per-meeting markdown writer (STORE-03, STORE-04)` — clean, just my files.

This is acceptable for a parallel-wave plan, but worth flagging: future parallel-wave plans should consider per-agent worktrees instead of a shared branch to prevent commit-message bleed. Not blocking — the actual code is correct, tested, and reviewable.

## Requirements Satisfied

| Requirement | Status | Notes |
|-------------|--------|-------|
| NOTES-05 (server-side AST diff over markdown — structural, NOT character diff) | DONE | `yogurt-notes::merge_notes` — structural block-level diff via pulldown-cmark; 5 fixture tests lock contract |
| STORE-03 (per-meeting markdown file at `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md`) | DONE — module ready | `MarkdownExporter::write` produces the exact filename + YAML format. Wiring (call from enhance handler on every mutation) lands in Plan 04-03. |
| STORE-04 (markdown rewritten on every notes_md or enriched_md mutation via single MarkdownExporter) | PARTIAL — single-writer ready | The single-writer surface is final and atomic. The "every mutation funnels through" guarantee is established by Plan 04-03 wiring the enhance handler to call exporter; verified end-to-end in Plan 04-04 acceptance smoke. |

## Threat Flags

None — no new network surface, no auth path, no schema-touching code. Markdown parsing operates on already-validated UTF-8 strings; filename generation is purely server-internal (Plan 04-03 will own input-validation at the HTTP boundary).

## Known Stubs

None. Every public function has a real implementation backed by tests.

## Deferred Items

- **Exporter wiring into enhance handler** — Plan 04-03 task (the enhance endpoint will call `MarkdownExporter::write` on every successful enhance, satisfying STORE-04's "every mutation" guarantee end-to-end).
- **`enriched_doc_json TEXT` migration** — Plan 04-01 territory (handled by parallel agent).
- **Live SQLite + markdown dual-write** — Plan 04-03 acceptance gate.
- **Token-by-token streaming render** — Phase 5 (`enhance_progress` WS event surface is already designed forward-compatible by Plan 04-01).

## Self-Check: PASSED

Files verified:
- `crates/yogurt-notes/Cargo.toml` — FOUND
- `crates/yogurt-notes/src/{lib,ast,diff,render,ts}.rs` — all FOUND
- `crates/yogurt-notes/tests/merge_fixtures.rs` — FOUND
- 5 × 4 = 20 fixture files under `crates/yogurt-notes/tests/fixtures/` — FOUND
- `crates/yogurt-server/src/markdown_exporter.rs` — FOUND
- `crates/yogurt-server/tests/markdown_exporter.rs` — FOUND

Commits verified:
- `48ac611` — Task 1 scaffold — FOUND in git log
- `56c1dfa` — Task 2 work absorbed by parallel-agent commit — FOUND in git log (file scope verified)
- `67239bd` — Task 3 MarkdownExporter — FOUND in git log
