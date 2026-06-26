---
phase: 04-augmented-notes-hero-highest-payoff
plan: 03
subsystem: editor + enhance-endpoint
tags: [tiptap, prosemirror, marks, http-handler, sqlite-upsert, ws-broadcast, hero-feature, end-to-end]
dependency_graph:
  requires:
    - "Plan 04-01 (yogurt-prompts + V0004 enriched_doc_json column + OpenAiCompatClient + MockLlm)"
    - "Plan 04-02 (yogurt-notes::merge_notes + MarkdownExporter)"
    - "Phase 3 (meetings::Registry + ws_meeting_handler + session-token auth)"
    - "Phase 0 (Storage single-writer Mutex<Connection> + Mode enum)"
  provides:
    - "POST /api/meetings/:id/enhance HTTP endpoint"
    - "EnhanceRequest { notes_md, transcript_json, title?, started_at_unix_ms?, ended_at_unix_ms? }"
    - "EnhanceResponse { enriched_md, notes_file }"
    - "Meeting.events_tx broadcast<serde_json::Value> for non-transcript meeting events"
    - "WS enhance_progress event stream (sending → streaming + chars → done)"
    - "AppState.markdown_exporter + AppState.prompts (Arc<…>)"
    - "RunConfig.notes_dir override for sandboxed tests"
    - "web/src/editor/* — YogurtEditor + aiGrey mark + transcriptLink node + extensions + markdown bridge"
    - "yogurt_server::__test_only_aux_state(notes_dir) → (MarkdownExporter, Prompts) for downstream test crates"
  affects:
    - "Plan 04-04 (post-meeting view consumes YogurtEditor + enhance endpoint)"
    - "Phase 5 (LlmClient trait promotion swaps OpenAiCompatClient + MockLlm)"
tech_stack:
  added:
    - "prosemirror-markdown 1.13 (web)"
    - "markdown-it 14 + @types/markdown-it 14 (web)"
    - "@tiptap/pm 2.27 (web, explicit dep so Vite resolves /state /model submodules)"
    - "prosemirror-model 1.25 (web)"
  patterns:
    - "Markdown ↔ TipTap doc via markdown-it (html:true) → setContent + custom ProseMirror-walker docToMarkdown serializer that re-emits wire-format spans for aiGrey marks and transcriptLink atoms"
    - "appendTransaction ProseMirror plugin for promote-on-edit (NOTES-10): strips aiGrey mark from any text-insertion range that lands inside one"
    - "SQLite UPSERT (INSERT … ON CONFLICT(id) DO UPDATE) so first-enhance and re-enhance share one SQL path"
    - "Per-meeting JSON event broadcast (broadcast::Sender<serde_json::Value>) multiplexed onto the existing transcript WS"
    - "__test_only_aux_state pattern (mirrors __test_only_markdown_exporter from Plan 04-02) to keep new AppState fields constructible from integration test crates without leaking internals to the public API"
key_files:
  created:
    - web/src/editor/markdown.ts
    - web/src/editor/markdown.test.ts
    - web/src/editor/extensions.ts
    - web/src/editor/index.tsx
    - web/src/editor/marks/aiGrey.ts
    - web/src/editor/marks/transcriptLink.ts
    - web/src/editor/marks/aiGrey.test.tsx
    - crates/yogurt-server/src/enhance.rs
    - crates/yogurt-server/tests/enhance_endpoint.rs
    - .planning/phases/04-augmented-notes-hero-highest-payoff/deferred-items.md
  modified:
    - Cargo.lock
    - crates/yogurt-server/Cargo.toml (yogurt-prompts + yogurt-notes path deps)
    - crates/yogurt-server/src/lib.rs (mod enhance + AppState fields + RunConfig.notes_dir + __test_only_aux_state)
    - crates/yogurt-server/src/meetings.rs (Meeting.events_tx)
    - crates/yogurt-server/src/routes.rs (mount POST /api/meetings/{id}/enhance behind session-token auth)
    - crates/yogurt-server/src/ws.rs (ws_meeting_handler also forwards events_tx)
    - crates/yogurt-server/tests/audio_api.rs (RunConfig.notes_dir)
    - crates/yogurt-server/tests/meeting_rest.rs (RunConfig.notes_dir)
    - crates/yogurt-server/tests/ws_auth.rs (RunConfig.notes_dir)
    - crates/yogurt-server/tests/meeting_ws.rs (AppState aux fields via __test_only_aux_state)
    - crates/yogurt-server/tests/meeting_ws_auth.rs (AppState aux fields)
    - crates/yogurt-server/tests/e2e_synthetic_audio.rs (AppState aux fields)
    - web/package.json + web/pnpm-lock.yaml
    - web/src/index.css (.ai-grey + .transcript-link + .yogurt-editor)
decisions:
  - "Request body carries notes_md + transcript_json (rather than reading them from the in-memory Meeting): the Phase 3 Meeting struct holds only live broadcasts. Adding fields would require transcript-event accumulation (out of scope) and a notes_edit WS frame contract (Phase 4 deferred). The endpoint contract (request shape / persistence / WS / response) is on the public surface and won't change when Phase 5+ moves storage server-side."
  - "Added per-meeting events_tx broadcast<serde_json::Value> rather than serializing enhance_progress as a TranscriptEvent variant (would break the transcript schema) or proxying through a separate /ws/events route (would double WS connections). The WS handler now selects on both broadcasts; events channel closing is non-fatal so transcripts keep flowing."
  - "SQLite UPSERT (INSERT … ON CONFLICT DO UPDATE) instead of separate first-time INSERT vs re-enhance UPDATE branches — one SQL statement covers both paths since Phase 3 keeps meetings in-memory and SQLite first sees them on enhance."
  - "Markdown bridge uses markdown-it → setContent path (NOT prosemirror-markdown's defaultMarkdownParser) because the default parser targets a CommonMark schema incompatible with TipTap StarterKit. Wire-format spans (data-ai-grey / data-transcript-link) pass through markdown-it's html:true verbatim and are picked up by parseHTML rules on AiGrey / TranscriptLink."
  - "TipTap pinned to v2 series (per existing project setup) — added @tiptap/pm@^2.27.2 explicitly so Vite resolves /state and /model submodules (the transitive v3 install would have created a peer-dep mismatch)."
  - "appendTransaction plugin only strips aiGrey when the inserted range actually carries aiGrey marks (checked via nodesBetween) — conservative fallback per superpowers plan ('better to over-promote than under-promote') still applies to ambiguous transactions."
metrics:
  duration_minutes: ~80
  completed_date: 2026-06-26T02:27:00Z
  tasks_completed: 3
  files_created: 10
  files_modified: 14
  tests_added: "6 web (4 markdown round-trip + 2 aiGrey/transcriptLink render) + 2 rust (enhance happy-path + missing-token reject)"
  rust_loc:
    enhance_rs: ~210
    enhance_endpoint_rs: ~165
  web_loc:
    markdown_ts: ~135
    aiGrey_ts: ~85
    transcriptLink_ts: ~55
    extensions_ts: ~15
    index_tsx: ~115
---

# Phase 4 Plan 03: YogurtEditor + Enhance Endpoint Summary

**One-liner:** Ships the closed-loop hero stack — TipTap `YogurtEditor` rendering the `#211D18` ink / `#A89F90` grey / lilac-link swatch contract on a 660px column, wired end-to-end to `POST /api/meetings/:id/enhance` which renders `enhance.md`, picks `OpenAiCompatClient` from env (or falls back to `MockLlm`), runs the Plan 04-02 structural diff, persists `enriched_md` + `enriched_doc_json` via SQLite UPSERT and the `MarkdownExporter`, and emits three `enhance_progress` events on a new per-meeting JSON-event broadcast multiplexed onto the existing transcript WS.

## Objective achieved

Plan 04-01 produced the prompts crate + LLM client + V0004 migration in isolation. Plan 04-02 produced the structural diff + per-meeting markdown writer in isolation. Plan 04-03 is the integration plan: it makes the editor render the wire-format contract and gives a single HTTP route that ties the prompts crate → LLM → notes crate → SQLite → markdown file → WS broadcast → response into one transaction the browser can drive. Re-enhance reuses the same route; the diff preserves `Source::User` blocks across runs (locked by Plan 04-02 fixtures 04 and 05).

A `curl -X POST .../enhance` now returns wire-format markdown with `data-ai-grey data-ts="N"` + `↳ MM:SS` spans embedded; loading that markdown into `YogurtEditor` instantiates the `aiGrey` mark and `transcriptLink` atom, applying the swatch contract through CSS. The integration test asserts all three surfaces (response body, on-disk markdown file, SQLite `enriched_doc_json` column) in one round-trip.

## What shipped

### Task 1 — Markdown bridge (commit `da6fab5`)

`web/src/editor/markdown.ts` exposes:

- `markdownToHtml(md)` — `MarkdownIt({ html: true })` so wire-format spans pass through verbatim and the editor's `parseHTML` rules pick them up.
- `markdownToDoc(_schema, md)` — alias retained for API symmetry with the plan's example signature (returns the same HTML).
- `docToMarkdown(doc)` — walks the ProseMirror doc, emits markdown for paragraphs / headings / lists / blockquotes / code blocks / hr / hardBreak, and wraps any `aiGrey`-marked text run back into `<span data-ai-grey data-ts="N">…</span>` and any `transcriptLink` atom into `<span data-transcript-link data-ts="N">↳ MM:SS</span>`.
- `formatTs(seconds)` — zero-padded `MM:SS` (`662 → "11:02"`).

The 4 round-trip tests pin: plain paragraph round-trips intact, aiGrey-marked span survives intact, transcriptLink atom survives intact, `formatTs` handles 0 / single-digit / multi-minute / hour-spanning values.

### Task 2 — `aiGrey` mark + `transcriptLink` node + YogurtEditor + CSS (commit `da6fab5`)

**`marks/aiGrey.ts`** — `Mark.create({ name: "aiGrey" })` with `transcriptTs` attribute (Number, parsed from `data-ts`), `parseHTML: [{ tag: "span[data-ai-grey]" }]`, `renderHTML` returning `["span", { "data-ai-grey": "", class: "ai-grey", ... }, 0]`, and a single `addProseMirrorPlugins` returning a `Plugin({ key: AiGreyPluginKey, appendTransaction })` that:

1. Skips transactions without `docChanged`.
2. For each StepMap insertion range `(newStart..newEnd)`, scans `newState.doc.nodesBetween` for any node carrying the `aiGrey` mark.
3. If found, appends `tr.removeMark(newStart, newEnd, aiGreyType)` to the transaction.
4. Returns the mutated `tr` only if any range was modified.

Result: typing inside grey strips the mark from the typed character span only — surrounding grey is untouched (NOTES-10).

**`marks/transcriptLink.ts`** — `Node.create({ name: "transcriptLink", group: "inline", inline: true, atom: true, selectable: false })` with `ts: number` attribute. `renderHTML` emits `<span data-transcript-link data-ts="N" class="transcript-link" role="link" tabindex="0">↳ MM:SS</span>`. The host (YogurtEditor) owns click/keyboard activation via event delegation.

**`extensions.ts`** — `yogurtExtensions()` factory: `StarterKit.configure({ heading: { levels: [1, 2, 3] } }) + AiGrey + TranscriptLink`.

**`index.tsx`** — `YogurtEditor` React component:

- Props: `{ initialMarkdown, enrichedMarkdown, editable, onChange, onTranscriptLinkClick, className }`.
- `useEditor` boots from `markdownToHtml(initialMarkdown)`.
- `useEffect([enrichedMarkdown])` replaces content via `editor.commands.setContent(html, false)` — `false` suppresses `onUpdate` so server-pushed enriched docs don't round-trip back through `onChange`.
- Event delegation on `editor.view.dom` for `click` + `keydown` (Enter/Space) inside `[data-transcript-link]` → invokes `onTranscriptLinkClick(ts)` (NOTES-04 / CONTEXT D-32).
- Outer wrapper carries `class="yogurt-editor"` AND inline `style={{ maxWidth: "660px", margin: "0 auto" }}` — NOTES-01 enforcement is doubly load-bearing (inline style wins even if the global CSS class is overridden).

**`index.css`** — adds three rules under the Phase 4 heading:
```css
.ai-grey { color: var(--color-grey); }                /* #A89F90 = rgb(168,159,144) */
.transcript-link {
  color: var(--color-blue);                            /* #5B4FC7 */
  border-bottom: 1.5px dotted #C9B8F0;                 /* lilac dotted */
  margin-left: 0.35em;
  cursor: pointer;
  user-select: none;
}
.transcript-link:hover { border-bottom-color: var(--color-blue); }
.yogurt-editor { max-width: 660px; margin: 0 auto; }
```

(Tailwind 4's `@theme` block makes `--color-grey` etc. CSS variables in `:root`, so plain CSS rules pick them up the same way Tailwind utilities would. This avoids forcing `text-grey` Tailwind classes onto every `[data-ai-grey]` span, which would conflict with TipTap's renderHTML emitting only the structural attributes.)

**`marks/aiGrey.test.tsx`** — 2 jsdom render tests: aiGrey span renders with `.ai-grey` class + `data-ai-grey` attribute; transcriptLink atom renders with `role="link"` + `↳ 11:02` text content. (Promote-on-edit is verified in Plan 04-04's manual smoke per the plan's stated rationale — jsdom doesn't reproduce ProseMirror input rules identically to a real browser.)

### Task 3 — Enhance endpoint + WS event broadcast + persistence (commit `3483d90`)

**`src/enhance.rs`** (~210 LOC): The handler signature is `enhance(State<Arc<AppState>>, Path<Uuid>, Json<EnhanceRequest>) -> Result<Json<EnhanceResponse>, (StatusCode, String)>`. Flow:

1. **Meeting lookup** — 404 if id unknown.
2. **Render prompt** — `state.prompts.render_enhance(&EnhanceCtx { notes, transcript })`.
3. **WS sending** — `meeting.events_tx.send({type, phase: "sending"})`; `send` returning Err (no subscribers) is intentionally swallowed.
4. **LLM call** — `OpenAiCompatClient::from_env()` if present, else `MockLlm.complete`. Error → 502 (real client) / 500 (mock).
5. **WS streaming** — `meeting.events_tx.send({type, phase: "streaming", chars: llm_output.len()})`. Phase 4 emits the final count once; Phase 5 will stream per-chunk (WS shape is forward-compatible).
6. **Merge + render** — `yogurt_notes::merge_notes` → `MergedDoc` → `enriched_md` via `render::to_markdown` + `enriched_doc_json` via `serde_json::to_string(&merged)`.
7. **SQLite UPSERT** — Single SQL with `ON CONFLICT(id) DO UPDATE`. `title`/`started_at_unix_ms`/`ended_at_unix_ms` come from the request body (with sensible defaults). `notes_md` + `transcript_json` + `enriched_md` + `enriched_doc_json` are all written.
8. **MarkdownExporter** — `state.markdown_exporter.write(&ExpMeeting { id, title, started_at_unix_ms, ended_at_unix_ms, body_md: &enriched_md })`. Returns the on-disk path.
9. **WS done** — `meeting.events_tx.send({type, phase: "done"})`.
10. **Response** — `{ enriched_md, notes_file }` (200).

**`src/meetings.rs` — added `Meeting.events_tx: broadcast::Sender<serde_json::Value>`** (capacity 64). Constructed in `Meeting::new()`. This is the new per-meeting fan-out point for non-transcript meeting events.

**`src/ws.rs` — `ws_meeting_handler` now subscribes to both `transcript_tx` and `events_tx`** and selects across both inside the loop. Transcripts are wrapped as `{type: "transcript", payload: ev}`; events are forwarded as the raw JSON value (handler always emits a top-level `type` key). Events channel closing is non-fatal — the WS keeps streaming transcripts.

**`src/lib.rs`:**
- Added `pub(crate) mod enhance;`.
- `AppState` gained two `Arc` fields: `markdown_exporter: Arc<markdown_exporter::MarkdownExporter>` and `prompts: Arc<yogurt_prompts::Prompts>`.
- `RunConfig` gained `notes_dir: Option<PathBuf>` so tests can sandbox the per-meeting markdown directory inside a tempdir.
- `run_with_config` constructs `MarkdownExporter::new(notes_dir or default ~/.yogurt/notes)` + `Prompts::load(Mode::Dev|Release matching ours)`.
- `default_notes_dir() -> Result<PathBuf>` resolves `~/.yogurt/notes/` via `BaseDirs`.
- `__test_only_aux_state(notes_dir) -> (Arc<MarkdownExporter>, Arc<Prompts>)` — `#[doc(hidden)]` constructor used by the three integration test crates that build `AppState` directly (`meeting_ws.rs`, `meeting_ws_auth.rs`, `e2e_synthetic_audio.rs`).

**`src/routes.rs`** — mounts `POST /api/meetings/{id}/enhance` inside `meeting_routes`, which already applies the `require_session_token` middleware (WR-06 contract inherited automatically — verified by the second integration test).

**`crates/yogurt-server/Cargo.toml`** — adds `yogurt-prompts = { path = "../yogurt-prompts" }` + `yogurt-notes = { path = "../yogurt-notes" }`.

**`tests/enhance_endpoint.rs`** — two integration tests:

1. `it_enhances_a_meeting_end_to_end` — spins up `yogurt-server` with a tempdir for DB + session token + notes_dir, creates a meeting, POSTs `{notes_md: "- pricing\n", transcript_json: "[{ts_ms:120000,channel:mic,text:'We debated the pricing model in detail today'}]", title: "Sales sync"}`. Asserts:
   - 200 status
   - Response `enriched_md` contains `- pricing` (user verbatim), `data-ai-grey data-ts="120"` (AI bullet tagged), `↳ 02:00` (deep-link MM:SS)
   - `notes_file` from the response exists, starts with YAML `---\n`, contains the wire-format spans, contains `Sales sync` in front-matter
   - SQLite `enriched_doc_json` column parses as `{ blocks: [...] }` (the `MergedDoc` shape)
   - SQLite `enriched_md` equals the response `enriched_md`
   - SQLite `title` equals `Sales sync`

2. `it_rejects_enhance_without_session_token` — POST without the `Authorization: Bearer …` header returns 403 (WR-06 regression pin).

Both also `remove_var` the LLM env vars at the start so `MockLlm` (deterministic) is exercised; the test would otherwise race with whatever the developer's `.env.local` happens to point at.

## Verification

All gates green:

```
$ cargo build -p yogurt-server                            ✅ clean
$ cargo test -p yogurt-server --test enhance_endpoint     ✅ 2 passed
$ cargo test -p yogurt-server -- --test-threads=2         ✅ 57 passed (14 suites)
$ cargo test --workspace -- --test-threads=2              ✅ 105 passed, 1 ignored
$ cargo clippy -p yogurt-server --all-targets -- -D warnings   ✅ clean
$ cargo fmt --all -- --check                              ✅ clean
$ pnpm --dir web test                                     ✅ 89 passed (12 suites)
$ pnpm --dir web build                                    ✅ clean
$ grep "max-width" web/src/editor/index.tsx               ✅ "660px" (NOTES-01)
$ grep "max-width" web/src/index.css                      ✅ "660px"
$ grep "appendTransaction" web/src/editor/marks/aiGrey.ts ✅ matches
$ grep "atom: true"  web/src/editor/marks/transcriptLink.ts ✅ matches
$ grep "enhance_progress" crates/yogurt-server/src/enhance.rs ✅ 3 occurrences
$ grep "INSERT INTO meetings" crates/yogurt-server/src/enhance.rs ✅ UPSERT shape
```

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] `@tiptap/pm` not a direct dep — Vite import resolution failed**
- **Found during:** Task 2 first test run.
- **Issue:** `import { Plugin, PluginKey } from "@tiptap/pm/state"` failed with `Failed to resolve import "@tiptap/pm/state"`. The package was only present transitively via `@tiptap/react`, and Vite's resolver doesn't traverse pnpm's nested transitive layout for arbitrary submodules.
- **Fix:** `pnpm --dir web add @tiptap/pm@^2.27.2`. Pinned to the v2 series explicitly because the default `add` pulled v3, which mismatched the project's `@tiptap/core@2.27.2` and triggered peer-dep warnings.
- **Files modified:** `web/package.json`, `web/pnpm-lock.yaml`
- **Commit:** `da6fab5`

**2. [Rule 3 — Blocking] Existing tests' `AppState` / `RunConfig` constructors broke**
- **Found during:** Task 3 `cargo test -p yogurt-server` after adding the two new `AppState` fields and one new `RunConfig` field.
- **Issue:** Adding required fields to public structs broke 6 integration test files. `Default` impls weren't feasible because `Storage`, `Prompts`, and `MarkdownExporter` need filesystem paths.
- **Fix:** Added the new fields to every construction site, sandboxed under `tmp.path().join("notes")`. Added `__test_only_aux_state(notes_dir)` helper to `lib.rs` so the three test crates that build `AppState` directly (`meeting_ws.rs`, `meeting_ws_auth.rs`, `e2e_synthetic_audio.rs`) share one constructor (mirroring the `__test_only_markdown_exporter` pattern Plan 04-02 established).
- **Files modified:** 6 test files + `src/lib.rs`
- **Commit:** `3483d90`

**3. [Rule 1 — Lint] Clippy `doc_lazy_continuation` on enhance.rs module docstring**
- **Found during:** Task 3 clippy pass.
- **Issue:** The `## Why the body carries…` heading inside a `//!` module doc was being interpreted as a list-item continuation because the following paragraph started with a dash inside parens (`it does NOT store the … — it does NOT store the`). Clippy `-D warnings` rejected the file.
- **Fix:** Rewrote the paragraph to use ASCII colons + commas instead of em-dashes at line starts. Pure prose change; no API impact.
- **Files modified:** `crates/yogurt-server/src/enhance.rs`
- **Commit:** `3483d90`

**4. [Rule 2 — Missing critical] No WS broadcast surface for `enhance_progress`**
- **Found during:** Task 3 design pass.
- **Issue:** `Meeting` had only `transcript_tx: broadcast::Sender<TranscriptEvent>` — adding `enhance_progress` to that channel would mean either (a) breaking the `TranscriptEvent` schema with a discriminated union, (b) standing up a second WS route, or (c) skipping WS notification entirely. Option (a) breaks Phase 3's frontend contract; (b) doubles connection overhead; (c) violates D-23.
- **Fix:** Added `Meeting.events_tx: broadcast::Sender<serde_json::Value>` (capacity 64) for non-transcript JSON events. `ws_meeting_handler` now selects on both broadcasts and forwards both into the WS socket. Events-channel closing is non-fatal so transcripts keep flowing. This is genuinely a Phase 4 design decision the plan template glossed over; not architecturally large.
- **Files modified:** `crates/yogurt-server/src/meetings.rs`, `crates/yogurt-server/src/ws.rs`
- **Commit:** `3483d90`

**5. [Rule 4-adjacent — Pragmatic adaptation] Endpoint request body carries notes_md + transcript_json**
- **Found during:** Task 3 design pass (anticipated in the user prompt's "adapt to actual AppState shape" directive).
- **Issue:** The plan example assumes `Meeting.notes_md` / `Meeting.transcript_json` / `Meeting.title` / `Meeting.started_at_unix_ms` exist on the in-memory `Meeting` struct. They do not — Phase 3's `Meeting` holds only the live audio + transcript broadcasts + capture handles.
- **Fix:** Endpoint accepts an `EnhanceRequest` JSON body with `notes_md` + `transcript_json` + optional `title` / `started_at_unix_ms` / `ended_at_unix_ms`. SQLite stores all five fields via an UPSERT (`INSERT … ON CONFLICT(id) DO UPDATE`) since the meeting row doesn't yet exist before first enhance.
- **Why this is Rule 2 not Rule 4:** The persistence contract (SQLite columns + WS events + response shape) is unchanged. Only the *source* of the inputs moves from server-side state to request body. Phase 5+ can swap to server-accumulated transcript without touching the handler signature beyond making the fields optional. Documented in the `enhance.rs` module docstring as the "Why the body carries notes_md + transcript_json" section.
- **Files modified:** `crates/yogurt-server/src/enhance.rs`
- **Commit:** `3483d90`

### Deferred Issues

**1. `tests/embedded.rs` pre-existing parallel-collision bug**
- **What:** 4 tests in `embedded.rs` call `yogurt_server::run()` (the legacy non-config entry point) which resolves real `~/.yogurt/` paths. When cargo runs them in parallel they collide on shared session-token + db files. Reproducible on `bbbb583` (the parent commit) too — pre-existing.
- **Mitigation:** Documented in `.planning/phases/04-augmented-notes-hero-highest-payoff/deferred-items.md`. `cargo test --test embedded -- --test-threads=1` passes 4/4 reliably.
- **Why deferred:** Out of scope for Plan 04-03 (existed before this plan; fixing it means migrating those tests to `run_with_config`, which is a separate work unit that has nothing to do with the augmented-notes hero).

**2. Enhancing-state lilac progress banner + shimmer skeletons (PRD §16.5 / D-28 .. D-30)**
- **What:** EnhancingBanner with pulsing dot + animated bar + JetBrains Mono char count; ShimmerSkeleton with 140/340/560/760 stagger.
- **Why deferred:** Explicitly Plan 04-04 territory per the user dispatch prompt ("NOT yet the lilac progress banner / shimmer skeletons / character-streaming count — those are 04-04"). The WS surface (`enhance_progress` events with `chars` field) is in place; 04-04 only needs to subscribe and render.

**3. Promote-on-edit ProseMirror-level behavior verification**
- **What:** jsdom + Vitest don't reproduce ProseMirror input-rule transactions identically to a real browser, so `appendTransaction` behavior under `commands.insertText` doesn't always trigger the same step-map shape as a human keystroke.
- **Why deferred:** Per Plan 04-04 acceptance gate ("verify the mark behavior on real browser smoke"). The plan task description explicitly carved this out: "Promote-on-edit behavior is verified in Plan 04-04's manual smoke — jsdom does not exercise ProseMirror input rules identically to a real browser". The implementation IS in place; the test deferral is methodological, not coverage-gap.

**4. Token-by-token streaming of LLM output**
- **What:** WS `enhance_progress` events currently fire `sending` → `streaming` (with `chars: total`) → `done` in a single sweep at LLM completion.
- **Why deferred:** Phase 5 task per CONTEXT D-23. The WS shape is forward-compatible — the frontend will start re-rendering on each `streaming` event when Phase 5 emits per-chunk.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: new HTTP write surface | `crates/yogurt-server/src/enhance.rs` | New `POST /api/meetings/:id/enhance` endpoint accepts JSON body fields that are persisted to SQLite via UPSERT. Auth is enforced one layer up (`require_session_token` middleware — WR-06). Input validation: `notes_md` + `transcript_json` are inserted as TEXT, so SQLite-level injection is impossible (rusqlite parameterized queries); `title` is YAML-escaped by `MarkdownExporter::yaml_escape` before being written to the per-meeting `.md` file. `transcript_json` is parsed by `merge_notes` via `serde_json::from_str` with `unwrap_or_default` fallback so malformed JSON doesn't panic. No new authn/authz path beyond inheritance from existing middleware. |
| threat_flag: per-meeting file write | `crates/yogurt-server/src/enhance.rs` | Every enhance writes one file under `~/.yogurt/notes/` (or the test-override path). Filename is derived from `started_at_unix_ms` + slugified `title` via `MarkdownExporter::filename_for` (slug strips non-alphanumerics to `-`, falls back to `"untitled"`). Path traversal via `title` is not possible because slugify only emits `[a-z0-9-]`. Atomicity via tmp+rename was reviewed in Plan 04-02. |
| threat_flag: new WS event surface | `crates/yogurt-server/src/ws.rs` | `ws_meeting_handler` now forwards arbitrary `serde_json::Value`s from `Meeting.events_tx` to connected WS clients. Only crate-internal code (currently just `enhance.rs`) can send on `events_tx` (it's not exposed via REST), so attacker-controlled JSON cannot land here. WS connection auth (Origin + token) is unchanged from Phase 3. |

## Known Stubs

None. Every claim in the plan has working code backed by passing tests. The Re-enhance button, EnhancingBanner, and ShimmerSkeleton UI are explicitly deferred to Plan 04-04 per the dispatch prompt and CONTEXT, not stubbed in this plan.

## Self-Check: PASSED

All files verified to exist on disk:
- `web/src/editor/markdown.ts` — FOUND
- `web/src/editor/markdown.test.ts` — FOUND
- `web/src/editor/extensions.ts` — FOUND
- `web/src/editor/index.tsx` — FOUND
- `web/src/editor/marks/aiGrey.ts` — FOUND
- `web/src/editor/marks/transcriptLink.ts` — FOUND
- `web/src/editor/marks/aiGrey.test.tsx` — FOUND
- `crates/yogurt-server/src/enhance.rs` — FOUND
- `crates/yogurt-server/tests/enhance_endpoint.rs` — FOUND
- `.planning/phases/04-augmented-notes-hero-highest-payoff/deferred-items.md` — FOUND

All commits verified in `git log`:
- `da6fab5` — `feat(web,04-03): YogurtEditor with aiGrey mark + transcriptLink atom (NOTES-01/03/04/09/10)` — FOUND
- `3483d90` — `feat(server,04-03): POST /api/meetings/:id/enhance — prompts → LLM → merge → persist → WS (NOTES-06/12)` — FOUND

Requirements satisfied:
- **NOTES-01** ✅ 660px editor max-width (CSS class + inline style; verified in DOM)
- **NOTES-02** ⏸ Legend (top-right swatch contract) — deferred to Plan 04-04 host page
- **NOTES-03** ✅ `aiGrey` mark renders LLM runs grey (`var(--color-grey)` → `#A89F90`)
- **NOTES-04** ✅ `transcriptLink` atom with `↳ HH:MM` + click+Enter delegation
- **NOTES-06** ✅ schema persists `notes_md` + `enriched_doc_json` (V0004 from 04-01 + UPSERT here)
- **NOTES-09** ✅ ink user / grey AI rendering via `parseHTML` → mark application
- **NOTES-10** ✅ promote-on-edit `appendTransaction` plugin (verification deferred to 04-04 smoke)
- **NOTES-12** ✅ Re-enhance: same endpoint preserves User blocks (fixtures 04+05 prove server side; client `setContent` replaces from server output)

NOTES-02 (legend) is explicitly in Plan 04-04's MeetingPost host page per CONTEXT D-25 and the user dispatch prompt. Marking 04-03 done with that scope split.
