# Phase 7: Library + Onboarding + States - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Make yogurt feel like a real app. Ship the library home view (sidebar + date-grouped meeting cards as the new default at `localhost:7878`), the `/welcome` onboarding flow, and the full set of empty/permission-denied/enhancing/model-download states. Replace the Phase-3 in-memory meeting store with a SQLite-backed `meetings` table (migration V003) and ensure every notes/enriched mutation writes a canonical markdown file under `~/.yogurt/notes/` via the single `MarkdownExporter` (Phase 4).

This phase additionally absorbs four research-flagged table-stakes adds that did not appear in the original v1 scope:

1. **SQLite FTS5 keyword search** across notes + transcripts (LIB-07).
2. **Copy-markdown / Reveal-in-Finder** per-meeting (LIB-12).
3. **Inline-editable meeting title** with default fallback (LIB-11).
4. **Delete-card UI** from the library (LIB-10 second clause).

After Phase 7, a fresh user lands on `/welcome`; a returning user lands on the library and sees past meetings grouped by day with the green "Local-only · on" sidebar pill reflecting cloud-provider state.

**Note on STATE-03 (enhancing state):** Already shipped by Phase 4 (NOTES-07/08). This phase only tracks the requirement; no new implementation work for STATE-03 happens here. STATE-01, STATE-02, and STATE-04 are net-new in this phase.

</domain>

<decisions>
## Implementation Decisions

### Library default route + layout
- **D-01:** Default page at `localhost:7878` is the library. Phase 3 routed `/` to the meeting view; Phase 7 demotes that to `/m/:id` and promotes Library to `/`.
- **D-02:** Left sidebar is 212px wide. Order top-to-bottom: yogurt swirl logo + "yogurt" wordmark; primary "+ New meeting" blueberry button (`bg-[var(--blue)]` with `shadow-[0_2px_8px_rgba(91,79,199,0.3)]`); nav rows "All meetings" (active = `bg-[var(--blsoft)]` + `text-[var(--blue)]`) and "Starred"; FOLDERS section (3 hardcoded sample folders with "Coming in v1.1" tooltip — color dots only, real folder model deferred to v1.1); footer: green matcha "Local-only · on" pill (`bg-[var(--mtsoft)] text-[var(--matcha)] rounded-full`) shown iff no provider with `kind === "cloud"` is active; `⚙ Settings` row links to `/settings`.
- **D-03:** Main pane greeting in Instrument Serif at 40px ("Good {morning|afternoon|evening}, {name|you}"); caption in JetBrains Mono at 13px ("N meeting{s} · all on this Mac"). Greeting cycles by `hour < 12 / < 18 / else`. Name defaults to "you" (no `/api/me` in v1).
- **D-04:** Search pill is top-right of main pane, rounded-full white card with magnifier glyph and "Search notes & transcripts" placeholder. Phase 7 wires it to the FTS5 backend (D-09); the rendered surface is a real input not a `cursor-not-allowed` stub.

### Meeting cards + date grouping
- **D-05:** Meetings render grouped by date with mono-caption headers `TODAY` / `YESTERDAY` / `EARLIER` (JetBrains Mono, 11px, uppercase, `text-[var(--mut)]`). Newest-first within each group. `bucketFor()` uses local-time midnight boundaries.
- **D-06:** Each meeting card is a Link to `/m/:id`: 42px rounded-[10px] avatar with deterministic palette tint (blueberry-soft, matcha-soft, strawberry-soft cycled by hash of ulid) showing two Instrument-Serif initials; title in Hanken Grotesk 700 at 15px (`text-[var(--ink)]`); meta line in JetBrains Mono at 12px `{time} · {N} min · enhanced` (the `· enhanced` suffix appears iff `enriched_md` is non-null); right-aligned `Local` badge pill (border `var(--line)`, mono 11px) for v1.
- **D-07:** Hover state reveals card actions: inline-editable title (double-click to edit, blur to commit, fallback to "Untitled meeting"); kebab/overflow menu with **Copy markdown**, **Reveal in Finder**, **Delete** entries.

### Per-meeting actions (research-flagged adds)
- **D-08:** Inline-editable title edits via `PATCH /api/meetings/:id { title }` and optimistic TanStack-Query update. Empty/whitespace title resets to "Untitled meeting".
- **D-09:** **Copy markdown** action reads the on-disk file at `~/.yogurt/notes/<slug>.md` (written by Phase 4 `MarkdownExporter`) via a new `GET /api/meetings/:id/markdown` endpoint and copies the body+front-matter to the browser clipboard via `navigator.clipboard.writeText`. **Reveal in Finder** issues `POST /api/meetings/:id/reveal` which calls `open -R <path>` (or `Command::new("open").arg("-R").arg(path)`) on the macOS server side. Do NOT reimplement markdown emission — reuse the Phase-4 `MarkdownExporter`.
- **D-10:** Delete affordance hits `DELETE /api/meetings/:id`. Per Phase-4 design, deletion removes the SQLite row + cascades chat_messages, but deliberately **leaves the markdown file in place** as the user's grep-able source of truth (PRD §5.7). Confirmation dialog: "Delete this meeting from the library? The markdown file in ~/.yogurt/notes/ stays put."

### FTS5 search (research-flagged add)
- **D-11:** New migration **V004** adds an FTS5 virtual table `meetings_fts(notes_md, transcript_text, content='meetings', content_rowid='rowid')` plus AFTER INSERT/UPDATE/DELETE triggers on `meetings` keeping the index in sync. `transcript_text` is derived from `transcript_json` server-side at write time (concatenated text segments).
- **D-12:** New endpoint `GET /api/meetings/search?q=<query>` returns ranked `Meeting[]` results using `bm25(meetings_fts)`. Returns top 50. Query is passed through FTS5's `MATCH` operator with sanitization (escape double-quotes, wrap in `"…"` for prefix matching with `*`).
- **D-13:** Search pill becomes a controlled `<input>`. On debounced typing (200ms), `useMeetingsSearch(query)` toggles between `useMeetings()` (empty query) and the search endpoint. Results render in the same `DateGroup` component; no separate UI shell.

### Onboarding flow
- **D-14:** Route `/welcome` is a two-column grid (`1.05fr 0.95fr`) on cream paper. Left: swirl logo + wordmark; Instrument-Serif "Welcome to yogurt." headline (52px); one-liner subhead; terminal mockup card showing `$ yogurt start / ✓ server live on :7878 / ✓ opening your browser… / → waiting for screen-recording grant` with traffic-light dots.
- **D-15:** Right column: 11px mono uppercase "ONE-TIME SETUP" label; three vertical `StepCard` components. State machine per card: `pending` (line border, paper badge), `current` (blueberry 2px border, blueberry-soft badge), `done` (matcha border, matcha-soft badge with ✓). Numbers replace with ✓ when done.
- **D-16:** Step 1 "Screen Recording" — current until `useScreenRecordingStatus().granted === true`. Step 2 "Connect your model" — current once Step 1 done, displays provider chips (Minimax / Ollama / OpenAI / OpenRouter) reflecting `settings.providers[].active` from Phase 5. Step 3 "Pick transcription" — body explains Cloud Deepgram vs Local whisper.cpp; remains `pending` (Phase 7 does not gate on this).
- **D-17:** Primary CTA "Take me to my meetings →" enabled iff `granted && hasActiveProvider`; otherwise blueberry @ 40% opacity + `cursor-not-allowed`. Click flips `settings.first_run_completed = true` via PATCH then navigates to `/`. Footer caption (12px mono, centered): "Restart once after granting — a macOS quirk, not us."

### Empty + error states
- **D-18:** **EmptyLibrary** (STATE-01) — centered column 24 below top: 64px swirl logo wrapped in `.float-3500` class (CSS keyframe `float 3.5s ease-in-out infinite`, translateY 0 → -8px → 0); Instrument-Serif "No meetings yet" headline (34px); supporting copy "Start one and Yogurt listens to both sides of the call — no bot joins. Your notes and audio stay on this Mac."; primary "Start your first meeting" button with `⌘N` kbd badge; mono caption "notes saved to `~/.yogurt/notes/*.md`". Float class name encodes the duration (`float-3500`) so any timing refactor trips a visible diff.
- **D-19:** **PermissionDenied** (STATE-02) — max-w-2xl centered card: 12x12 strawberry-soft (`#FBE6E0`) badge with `⚠` icon; Instrument-Serif "Yogurt can't hear the call yet" headline; numbered 3-step recovery list (1. Open System Settings → Privacy & Security → Screen Recording, 2. Toggle Yogurt on, 3. Restart Yogurt once); mono caption "a macOS requirement, not us"; CTA pair — primary "Open System Settings" linking to **exactly** `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`, secondary "Restart Yogurt" linking to `/api/restart` (placeholder, real endpoint ships Phase 9).
- **D-20:** **ModelDownloadStub** (STATE-04) — white card with 10x10 matcha-soft down-arrow badge, "Downloading {model}" title, mono `whisper.cpp · {sizeMb} MB` caption, h-1.5 matcha progress bar (0% in stub), body "Most users stay on cloud STT and never see this.", Cancel + "Run in background" button pair. Not routed in Phase 7 — Phase 8 mounts it on `/settings`.
- **D-21:** **Enhancing state** (STATE-03) — already shipped by Phase 4 via NOTES-07/NOTES-08 (lilac progress banner with active dot pulse + shimmer skeleton bullets). Phase 7 only verifies the banner still renders after the new routing layer; no new code.

### Server-side meeting persistence
- **D-22:** New `MeetingRepo` in `yogurt-db::meetings` with `create / get / list / patch / delete` over SQLite via the existing `Db` handle. ULID 26-char ids. Trimmed empty titles are rejected. `MeetingPatch.enriched_md: Option<Option<String>>` distinguishes "leave alone" from "explicitly clear". V003 migration retrofits the FK on `chat_messages.meeting_id` (table rebuild required since SQLite cannot ALTER TABLE ADD CONSTRAINT) and seeds `settings.first_run_completed = false`.
- **D-23:** Phase-3 in-memory `HashMap<String, Meeting>` is removed. Both the REST router and the WS handlers go through `Arc<MeetingRepo>` on `AppState`. A helper `AppState::patch_and_export(id, patch)` calls `repo.patch + exporter.write` in one shot so both layers stay in sync.
- **D-24:** `DELETE /api/meetings/:id` removes the row + cascades chat_messages but **does not delete the markdown file**. Documented as intentional deviation from PRD §10; markdown file is the user's source of truth.

### Routing rewire
- **D-25:** `web/src/App.tsx` routes: `/` → Library, `/welcome` → Welcome, `/m/:id` → Meeting (existing Phase-3 file, just rewired to consume `id` param), `/settings` → Settings (Phase-5 file), `/starred` → `<Navigate to="/" replace/>` placeholder, `*` → `<Navigate to="/" replace/>`. `useFirstRunRedirect()` mounts at top of `<Shell>` and redirects `/` → `/welcome` when `!first_run_completed || !granted || !hasProvider`.

### Claude's Discretion
- Exact pixel choices for sidebar item spacing beyond what tokens dictate.
- Whether to extract `CardActionsMenu` into its own component vs inline in `MeetingCard`.
- Confirmation dialog style for delete (alert vs inline confirm).
- FTS5 tokenizer choice (defaults to `unicode61` which is fine).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 7 plan (primary source of truth)
- `docs/superpowers/plans/2026-06-25-yogurt-phase-7-library-and-onboarding.md` — 11 tasks covering V003 migration, MeetingRepo, MarkdownExporter wiring, REST endpoints, in-memory→repo refactor, frontend Library + Welcome + states + routing + E2E. **Note:** the superpowers plan defers search to v2 with a stub; Phase 7 (per ROADMAP and REQUIREMENTS LIB-07) explicitly absorbs FTS5 search — the plan-02 below extends beyond the superpowers plan on this point.

### Product requirements
- `docs/PRD.md` §5.9 — Library home view layout (sidebar + main pane + meeting cards)
- `docs/PRD.md` §5.10 — Onboarding `/welcome` two-column flow
- `docs/PRD.md` §5.11 — Empty / permission-denied / enhancing / model-download states
- `docs/PRD.md` §9 — `meetings` + `chat_messages` schema
- `docs/PRD.md` §10 — REST endpoints + FK constraints
- `docs/PRD.md` §16.2 — Color palette (paper/ink/blueberry/strawberry/matcha + soft variants)
- `docs/PRD.md` §16.3 — Typography (Instrument Serif / Hanken Grotesk / JetBrains Mono)
- `docs/PRD.md` §16.5 — Motion tokens, especially 3.5s float
- `docs/PRD.md` §16.6 — Component primitives
- `docs/PRD.md` §16.8 — Layout invariants
- `docs/PRD.md` §16.9 — Lucide icon set + deferred drag-and-drop

### Requirements traceability
- `.planning/REQUIREMENTS.md` "Library (Home View)" — LIB-01 through LIB-12
- `.planning/REQUIREMENTS.md` "Onboarding" — ONB-01 through ONB-08
- `.planning/REQUIREMENTS.md` "Empty & Error States" — STATE-01 through STATE-04

### Roadmap
- `.planning/ROADMAP.md` §"Phase 7: Library + Onboarding + States" — success criteria 1-4

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Phase 1 design tokens** (`web/src/styles/tokens.css` or Tailwind config): `var(--paper) / --ink / --mut / --line / --blue / --blsoft / --matcha / --mtsoft / --straw`; font families `font-serif` (Instrument Serif), `font-sans` (Hanken Grotesk), `font-mono` (JetBrains Mono). Reuse without redefining.
- **Phase 1 SwirlLogo** at `web/src/components/brand/SwirlLogo.tsx` — render in Sidebar header, Welcome left column, EmptyLibrary center.
- **Phase 1 button primitives** — primary blueberry, secondary outlined — reuse for "+ New meeting", "Start your first meeting", "Take me to my meetings →", "Open System Settings", "Restart Yogurt".
- **Phase 4 MarkdownExporter** at `crates/yogurt-server/src/markdown_export.rs` — the canonical writer of `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md`. **Reveal in Finder** and **Copy markdown** MUST read these files; do not reimplement formatting.
- **Phase 4 enhancing-state banner** — already present at top of meeting view; Phase 7 confirms it survives the routing rewire (STATE-03).
- **Phase 5 useSettings + provider list** — `useSettings()` returns `{ providers: [{ name, kind: "cloud"|"local", active }], first_run_completed }`. Drives sidebar Local-only pill, Welcome step state, and first-run redirect.
- **Phase 5 Settings route** — already mounted; Phase 7 router rewire keeps `/settings`.
- **Phase 2 useScreenRecordingStatus** hook — returns `{ granted, loading }`. Drives Welcome Step 1 and Library PermissionDenied gate.
- **Phase 0 SQLite migration runner** — `crates/yogurt-db/src/lib.rs` `MIGRATIONS` slice. Add V003 + V004 entries following the same `include_str!` pattern.
- **Phase 6 chat_messages table** — V003 retrofits the FK; Phase 6 tests must still pass post-migration.
- **TanStack Query 5** (Phase 5) — `useQuery / useMutation / useQueryClient`. Cache keys: `["meetings"]`, `["meetings", id]`, `["meetings", "search", q]`.

### Established Patterns
- `crates/yogurt-server/src/api/<resource>.rs` modules each expose `pub fn router() -> Router<AppState>`; main `lib.rs` merges them.
- `crates/yogurt-db/src/<resource>.rs` modules export a `Repo` struct holding `Db` and provide `pub` async-free methods returning `anyhow::Result`.
- All new TipTap/UI tokens already in `web/src/index.css`; only `@keyframes float` is new (Phase 1 may already have it — verify).
- `Db::with_conn(|c| {…})` is the canonical SQLite access pattern; do not bypass.

### Integration Points
- **AppState extension** — add `meetings: Arc<MeetingRepo>` + `exporter: Arc<MarkdownExporter>` (Phase 4 already added exporter; reuse). All existing handlers get the new fields automatically via `.with_state(state)`.
- **WS handlers** (`ws.rs` from Phase 3/4) — every transcript-chunk / notes-edit / enhance-complete site moves from `HashMap` writes to `state.patch_and_export(...)`.
- **`MarkdownExporter::default_location()`** — already honors `YOGURT_HOME` env var (Phase 4); reuse for E2E tests.
- **FTS5 trigger wiring** — V004 migration creates triggers; `MeetingRepo` doesn't need to manually maintain the index. Verify SQLite is compiled with FTS5 (`rusqlite` bundled feature includes it by default — confirm in tests).

</code_context>

<specifics>
## Specific Ideas

- "Make yogurt feel like a real app" is the explicit phase goal — the Library + Welcome surfaces are the user's first impression and must hit the design board's polish bar (Granola-level).
- The four research-flagged adds (FTS5 search, copy/reveal, inline title, delete UI) are non-negotiable table stakes — Granola, Hyprnote, and Meetily all ship them. Cutting any of them would make Yogurt feel like a tech demo.
- The "Local-only · on" matcha pill in the sidebar is load-bearing for the privacy-first narrative — must be visible iff no cloud provider is active.
- The 3.5s float animation on the EmptyLibrary logo is explicitly locked in PRD §16.5; the `.float-3500` class name is intentional bait for catching drift in code review.
- Permission-denied "Open System Settings" link must be **exactly** `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture` (asserted via Playwright).
- The phase ships TWO new migrations: V003 (meetings table + chat_messages FK retrofit) and V004 (FTS5 virtual table + triggers). Both run before Phase-6 tests on cold start.

</specifics>

<deferred>
## Deferred Ideas

- **Folders data model + folder CRUD** — Sidebar shows 3 hardcoded sample folders ("Work / Hiring / 1:1s") with color dots and a `title="Coming in v1.1"` tooltip. Real `folders` table defers to v1.1 (LIB-V2-01).
- **Per-meeting "keep audio" retention toggle** — v1.1 (LIB-V2-02).
- **Auto-save "Saved · 2s ago" indicator** — v1.1 (LIB-V2-03).
- **Strawberry + Matcha-dark themes** — v2 (LIB-V2-04 / LIB-V2-05).
- **Real whisper.cpp model download** — Phase 8. Phase 7 only ships `ModelDownloadStub.tsx` (visual only).
- **Per-meeting Starred toggle UI** — `meetings.starred` column added in V003 for v1.1; no UI in Phase 7. `/starred` route redirects to `/`.
- **`/api/restart` endpoint** — PermissionDenied "Restart Yogurt" button links to it as a placeholder; real endpoint ships Phase 9.
- **`/api/me` for greeting personalization** — Greeting defaults to "you" until Phase 9+ adds a username source.
- **Drag-and-drop / folder reorder** — PRD §16.9 defers this.
- **Calendar integration** — v2 (CAL-01/02).
- **Semantic search across meetings (embeddings + sqlite-vss)** — v2 (CROSS-01). Phase 7 ships keyword FTS5 only.

</deferred>

---

*Phase: 07-library-onboarding-states*
*Context gathered: 2026-06-25*
