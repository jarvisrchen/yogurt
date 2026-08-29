# Phase 4: Augmented Notes Hero (HIGHEST PAYOFF) - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

The hero augmented-notes UX works end-to-end. A user types sparse markdown bullets during a meeting, hits "End meeting", and within 30 seconds sees a unified document where their bullets remain ink-black (`#211D18`) and AI-added bullets render grey (`#A89F90`) with `↳ HH:MM` lilac dotted-underline deep-links into the transcript. Editing a grey range promotes it to ink-black; the `aiGrey` TipTap mark is stripped from edited text. Black ranges are never overwritten on Re-enhance. Closing Yogurt and reopening preserves the black/grey distinction via a new `enriched_doc_json TEXT` column (the Phase 4 portion of the STORE-01 split-mapping). Each meeting is written to both SQLite and a per-meeting markdown file at `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` (with YAML front-matter) via a single `MarkdownExporter`. The bundled `enhance.md` + `chat-system.md` ship in the new `yogurt-prompts` crate with `{{NOTES}}` / `{{TRANSCRIPT}}` placeholders and hot-reload on binary restart. A minimal hardcoded `OpenAiCompatClient` (~50 LOC) ships in this phase to unblock the hero — it will be promoted to a trait-bounded client in Phase 5. No settings UI; no real Keychain wiring; no in-meeting chat — those are Phase 5+.

</domain>

<decisions>
## Implementation Decisions

### Editor & TipTap Marks
- **D-01:** TipTap-based markdown editor centered in the meeting view at `max-width: ~660px`. Uses `@tiptap/starter-kit` baseline configured with headings levels 1-3. Editor renders in both the in-meeting view (`Meeting.tsx`) and the post-meeting view (`MeetingPost.tsx`).
- **D-02:** Custom `aiGrey` TipTap mark (NOT a node) carries a single attribute `transcriptTs: number | undefined` and applies CSS class `ai-grey` rendered as `<span data-ai-grey data-ts="N" class="ai-grey">…</span>`. The mark is only applied to LLM-added inline runs.
- **D-03:** A custom `TranscriptLink` inline atom node (`group: "inline"`, `inline: true`, `atom: true`, `selectable: false`) carries a `ts: number` attribute and renders `↳ HH:MM` as `<span data-transcript-link data-ts="N" class="transcript-link">↳ HH:MM</span>`. NOT a mark — it is a non-editable inline token so the user cannot accidentally split it mid-word.
- **D-04:** Promote-on-edit is enforced by an `appendTransaction` ProseMirror plugin registered on the `aiGrey` mark: every transaction that inserts text into a range covered by `aiGrey` strips the mark from the inserted character span only — surrounding grey is untouched. Black ranges are NEVER overwritten on Re-enhance because the merge step keys user blocks and re-uses the user's exact block text when found.
- **D-05:** Markdown ↔ ProseMirror bridge uses `prosemirror-markdown` (official ProseMirror package) NOT the community `@tiptap/extension-markdown`. Rationale: custom serializer/parser tokens are mandatory either way for `aiGrey` mark + `transcriptLink` node; `prosemirror-markdown` exposes first-class hooks via `MarkdownSerializer` and is actively maintained by the ProseMirror team.
- **D-06:** Wire format for AI runs in markdown is HTML-passthrough spans, NOT a custom shortcode: `<span data-ai-grey data-ts="N">…</span><span data-transcript-link data-ts="N">↳ HH:MM</span>`. Rationale: (a) survives pasteboard round-trips, (b) `prosemirror-markdown`'s default HTML-passthrough handles it, (c) still legible if a user opens the `.md` file in a plain text editor.

### Diff Strategy (Server-Side, Structural)
- **D-07:** The diff is **server-side and STRUCTURAL** — computed over the markdown AST at block granularity (heading / paragraph / list item / code block / blockquote / hr). It is **NOT** a character diff. Implementation crate: `yogurt-notes`. Library: `pulldown-cmark = "0.12"`.
- **D-08:** `block_key(b)` strips `<span data-ai-grey…>` / `<span data-transcript-link…>` / `</span>` markers and lowercases / trims before comparison. The merge algorithm: build a HashMap of user-block keys, walk the LLM's enriched blocks in order, emit each as `Source::User` (re-using the user's exact block text) if its key is in the user set, otherwise as `Source::AiGrey { transcript_ts_sec: ts::guess_ts_sec(...) }`. Append any user blocks the LLM dropped at the end as `Source::User` (defensive — never lose user text).
- **D-09:** Transcript timestamp inference (`ts::guess_ts_sec`) is a word-overlap heuristic: tokenize the block on non-alphanumeric, keep tokens with length > 3, count overlaps against each transcript segment's text, pick the segment with the highest overlap count (tie-break to earliest `ts_ms`). Returns the segment's `ts_ms / 1000`. If no overlap and transcript non-empty, fall back to first segment's ts.
- **D-10:** `yogurt-notes` exposes a single public function `merge_notes(user_md: &str, enriched_md: &str, transcript_json: &str) -> Result<MergedDoc>` returning blocks each tagged `Source::User` or `Source::AiGrey { transcript_ts_sec: u64 }`. `render::to_markdown(&MergedDoc) -> String` emits wire-format markdown with marker spans.

### Storage & Persistence
- **D-11:** Schema migration adds `enriched_doc_json TEXT` column to the `meetings` table (Phase 4 portion of STORE-01 split-mapping). The column stores the ProseMirror JSON document so TipTap marks (the black/grey distinction) survive restart. The existing `enriched_md TEXT` column from Phase 0 continues to store the wire-format markdown.
- **D-12:** On every successful `POST /api/meetings/:id/enhance`, the server writes BOTH: (a) `notes_md` (pure user markdown unchanged), `enriched_md` (wire-format markdown from `render::to_markdown`), and `enriched_doc_json` (ProseMirror JSON serialized from the editor doc) to SQLite via the single-writer `Mutex<Connection>` from Phase 0; (b) a per-meeting markdown file via `MarkdownExporter`.
- **D-13:** Per-meeting markdown export path is `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` where the date is derived from `meetings.started_at` and `<slug>` is a lowercased dasherized form of `meetings.title` (fallback `untitled`). The file contains YAML front-matter with `id`, `title`, `started_at`, `ended_at`, and the body is the wire-format `enriched_md` (falling back to `notes_md` if enriched is empty).
- **D-14:** A single `MarkdownExporter` struct in `yogurt-server` is the sole writer to `~/.yogurt/notes/`. Every `notes_md` or `enriched_md` mutation funnels through it. Atomicity: write to `<path>.tmp` then `rename` to final path so a partial write cannot corrupt an existing file.

### Bundled Prompts
- **D-15:** New crate `crates/yogurt-prompts` ships exactly two template files: `templates/enhance.md` and `templates/chat-system.md`. Templates are embedded via `rust-embed` for release builds and read fresh from disk on every call in dev mode (`Mode::Dev`).
- **D-16:** `enhance.md` template uses `{{NOTES}}` and `{{TRANSCRIPT}}` placeholders (per REQUIREMENTS PROMPT-02). Template engine: `tinytemplate = "1.2"` configured with `set_default_formatter(&tinytemplate::format_unescaped)` so HTML-ish content in notes/transcript is NOT escaped before the LLM sees it. The superpowers plan shows the placeholders as `{notes}` / `{transcript}` (tinytemplate syntax); requirement IDs PROMPT-02 specify the user-facing template placeholders as `{{NOTES}}` / `{{TRANSCRIPT}}`. Resolution: the template content uses `{notes}` and `{transcript}` (tinytemplate native syntax); a CONTEXT note documents this for power users editing the file. If a future requirement insists on `{{NOTES}}` syntax exactly, swap the template engine to `handlebars` — the `Prompts::render_enhance` API does not change.
- **D-17:** Hot-reload semantics: in `Mode::Dev`, `Prompts::render_enhance` re-reads the file from `CARGO_MANIFEST_DIR/templates/` on every call. In `Mode::Release`, templates are read once at `Prompts::load()` from the embedded copy. PROMPT-04 ("Reloading binary picks up edits to either file") is satisfied in release by the binary re-loading on restart; in dev by per-call disk reads.
- **D-18:** `chat-system.md` is a static system prompt with no placeholders — used by Phase 6's chat pill. It ships in this phase because PROMPT-03 maps here and to keep the prompts crate cohesive.

### Minimal LLM Client (TACTICAL — Promoted in Phase 5)
- **D-19:** A minimal hardcoded `OpenAiCompatClient` ships in this phase (~50 LOC, lives at `crates/yogurt-server/src/llm_openai.rs` or equivalent). It accepts `(base_url, api_key, model)` from env vars (`YOGURT_LLM_BASE_URL`, `YOGURT_LLM_API_KEY`, `YOGURT_LLM_MODEL`) loaded from `.env.local` in `--dev` mode, makes a single non-streaming POST to `<base_url>/chat/completions`, and returns the full string. NO `LlmClient` trait yet — that lives in Phase 5. NO Keychain wiring — that lives in Phase 5. NO settings UI — that lives in Phase 5.
- **D-20:** Phase 4 ALSO ships a `MockLlm` fallback at `crates/yogurt-server/src/llm_mock.rs` for tests + when env vars are absent. The mock returns a deterministic enriched markdown by echoing user notes verbatim and appending one AI bullet per transcript segment wrapped in the wire-format spans. This unblocks fixture tests + offline dev. Phase 5 deletes `llm_mock.rs` and the env-var path, replacing both with the `LlmClient` trait + Keychain-backed real client.
- **D-21:** The minimal hardcoded client is intentionally NOT promoted to a trait in Phase 4. The Phase 4 acceptance gate ("30-second hero experience") is achievable with a hardcoded client + env vars; the trait abstraction and provider config UX is Phase 5's job. This is a deliberate tactical decision documented in REQUIREMENTS.md split-mapping and the superpowers plan.

### Enhance Endpoint + WebSocket Events
- **D-22:** New route `POST /api/meetings/:id/enhance` accepts the meeting id, reads `notes_md` + `transcript_json` from `AppState.meetings`, renders the prompt via `yogurt-prompts`, calls the LLM (real client if env vars present, else `MockLlm`), runs `yogurt_notes::merge_notes` over the result, persists `enriched_md` + `enriched_doc_json` to SQLite + markdown file, and returns `{ enriched_md: String }`.
- **D-23:** The endpoint emits three `enhance_progress` events to the meeting's WebSocket subscribers via the Phase 3 broadcaster: `{"type":"enhance_progress","phase":"sending"}`, `{"type":"enhance_progress","phase":"streaming","chars":N}`, `{"type":"enhance_progress","phase":"done"}`. The `chars` field carries the running character count (the mock returns the full string at once; the real client's character count comes back at completion in Phase 4 — true token-by-token streaming is a Phase 5 upgrade).
- **D-24:** WebSocket message-type union in `web/src/lib/ws.ts` is extended with `{ type: "enhance_progress"; phase: "sending" | "streaming" | "done"; chars?: number }`.

### Post-Meeting UI
- **D-25:** New route `/meeting/:id/post` rendered by `MeetingPost.tsx`. Layout: sticky-top `EnhancingBanner` (visible during enhance), sticky `Re-enhance` button top-right, `Legend` swatch contract top-right ("□ your notes" / "▢ AI" with `#211D18` + `#A89F90` swatches per PRD §5.3), `YogurtEditor` in the main column at `max-width: 660px`, `margin: 0 auto`.
- **D-26:** Re-enhance button (`ReEnhanceButton.tsx`) is a single button — no dropdown caret, no template picker (template picker is explicitly v2). On click: POST `/api/meetings/:id/enhance`, set `enhancing=true` until the WS `done` event or the fetch resolves, then `editor.commands.setContent(renderToHtml(enriched_md))`.
- **D-27:** End-meeting handler in `Meeting.tsx` (from Phase 3) is updated to: POST `/api/meetings/:id/enhance`, then `navigate('/meeting/:id/post', { state: { enrichedMd: enriched_md } })`. `MeetingPost.tsx` reads `location.state.enrichedMd` if present (avoids re-fetch), else falls back to `GET /api/meetings/:id` and uses `enriched_md` (falling back to `notes_md`).

### Enhancing-State Visual Contract (Load-bearing per PRD §16.5)
- **D-28:** `EnhancingBanner` is a sticky lilac (`var(--blsoft, #ECE9FB)`) banner across the top of the meeting view. Components: pulsing dot (1.4s `recpulse` ease-in-out infinite), "Weaving your notes into the transcript…" copy at 13px/600 weight, animated progress bar (1.8s `enhancing-bar` ease-in-out infinite, blue `var(--blue, #5B4FC7)` fill on `#D9D4F4` track), JetBrains-Mono character-streaming count (e.g. "1,234 chars") when present.
- **D-29:** `ShimmerSkeleton` placeholders animate at exactly **1.25s linear infinite** (`shimmer` keyframes, gradient `#EFE6D6 → #F8F1E0 → #EFE6D6`, `background-size: 200% 100%`). PRD §16.5 motion token.
- **D-30:** Shimmer skeletons resolve in stagger at **exactly 140ms / 340ms / 560ms / 760ms** after the post-page mounts. These are hardcoded `staggerMs` props passed to `<ShimmerSkeleton>` instances; values are load-bearing per PRD §16.5 — do not round, do not interpolate.

### Color Tokens (Hero Contract)
- **D-31:** User-authored content renders ink-black: `#211D18` (`var(--ink)`). AI-added content renders grey: `#A89F90` (`var(--grey)`). The transcript link `↳ HH:MM` renders blueberry `var(--blue, #5B4FC7)` with `1.5px dotted #C9B8F0` underline. These are the EXACT computed colors verified in the acceptance gate — DevTools must report `rgb(33, 29, 24)` and `rgb(168, 159, 144)` respectively.

### Click-to-Jump Transcript Integration
- **D-32:** Clicking `↳ HH:MM` in the editor fires an event delegation handler on `editor.view.dom` that reads `data-ts` from the closest `[data-transcript-link]` ancestor and invokes `onTranscriptLinkClick?.(ts)`. `MeetingPost.tsx` wires this to: (a) open the Phase 3 transcript dock if collapsed, (b) dispatch `window.dispatchEvent(new CustomEvent("yogurt:transcript:scrollTo", { detail: { ts } }))` which the Phase 3 `TranscriptPanel` listens for and scrolls to. Hover tooltip showing the transcript excerpt is rendered via CSS `title` attribute populated from the closest transcript segment text on render.

### Claude's Discretion
- Exact CSS for the dotted-underline lilac `↳ HH:MM` styling (Phase 1 design tokens provide the colors; any standard CSS approach is fine).
- Internal layout of the `MarkdownExporter` struct (any standard atomic-rename pattern is acceptable; the `tempfile = "3"` crate is OK but a hand-rolled `<path>.tmp` + `std::fs::rename` is preferred to minimize dep surface).
- Whether the per-meeting markdown export happens synchronously on the writer thread or via a `tokio::spawn` task — both satisfy STORE-04 ("rewritten on every mutation"). Prefer synchronous to keep failure handling simple.
- Internal naming + module structure of the new `yogurt-prompts` and `yogurt-notes` crates beyond the public surfaces specified.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Authoritative implementation plan
- `docs/superpowers/plans/2026-06-25-yogurt-phase-4-augmented-notes-hero.md` — Authoritative source-of-truth implementation plan for this phase. Tasks 4.0–4.10 inside that file are the source of truth for exact Rust, TypeScript, fixture content, prompt template text, and acceptance smokes. GSD plans below chunk those into waves. The superpowers plan defers persistence (`enriched_doc_json` SQLite column + per-meeting markdown export); this phase's GSD plans add it back per the REQUIREMENTS.md split-mapping for STORE-01.

### Product requirements
- `docs/PRD.md` §5.3 — Hero feature spec (black-user / grey-AI augmented-notes merge, full contract)
- `docs/PRD.md` §5.5 — Bundled prompts (`enhance.md`, `chat-system.md`); template picker explicitly cut from v1
- `docs/PRD.md` §5.11 — Enhancing state shimmer + stagger contract
- `docs/PRD.md` §10 — `POST /api/meetings/:id/enhance` endpoint contract + `enhance_progress` WS event shape
- `docs/PRD.md` §13 — Risk #2: TipTap structural diff is the single highest-risk item in v1
- `docs/PRD.md` §16.4 — Motion tokens (260ms popUp, 340ms slideInRight, 600ms staggered reveal, 1.4s recpulse, 1.25s shimmer)
- `docs/PRD.md` §16.5 — Staggered reveal beats: 140 / 340 / 560 / 760 ms (load-bearing)
- `docs/PRD.md` §16.7 — Variant A grey text picked — swatch contract (`#211D18` user / `#A89F90` AI)

### Project planning
- `.planning/REQUIREMENTS.md` — Section "Augmented Notes (Hero Feature)" (NOTES-01 through NOTES-13), "Local Storage" (STORE-01 — Phase 4 `enriched_doc_json` migration portion only; STORE-03, STORE-04), and "Bundled Prompts" (PROMPT-01 through PROMPT-04)
- `.planning/ROADMAP.md` — "### Phase 4: Augmented Notes Hero (HIGHEST PAYOFF)" success criteria (5 must-be-true gates)
- `.planning/PROJECT.md` — Core value statement (the augmented-notes UX IS the product), `.env.local` dev convention with `MINIMAX_API_KEY` / `YOGURT_LLM_BASE_URL` for the minimal hardcoded client

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Phase 0 SQLite single-writer `Mutex<Connection>`** (D-22 of Phase 0 context): the schema migration adding `enriched_doc_json TEXT` reuses the existing migration runner; new writes funnel through the existing single-writer guard. No new pool needed.
- **Phase 0 `Mode { Dev, Release }` enum** in `yogurt-server`: re-used by `yogurt_prompts::Prompts::load(Mode)` to select between embedded (Release) and disk-read (Dev) prompt sourcing.
- **Phase 1 design tokens**: `--ink #211D18`, `--grey #A89F90`, `--blsoft #ECE9FB`, `--blue #5B4FC7`, motion tokens (1.25s `shimmer`, 1.4s `recpulse`, 340ms `slideInRight`). All consumed verbatim by `EnhancingBanner` / `ShimmerSkeleton` / `aiGrey` CSS — do not hardcode hex codes when the variable exists.
- **Phase 2 audio capture**: not directly invoked from Phase 4 — transcript JSON is the only upstream input we touch.
- **Phase 3 `AppState`**: in-memory `meetings: Arc<RwLock<HashMap<MeetingId, Meeting>>>` already holds `notes_md` + `transcript_json` per meeting; Phase 4 adds reads + writes to the new `enriched_md` / `enriched_doc_json` fields (and persists them via the Phase 0 SQLite writer).
- **Phase 3 WebSocket broadcaster**: re-used to emit `enhance_progress` events. The Phase 3 message-type union is extended (not replaced) with the new event variant.
- **Phase 3 right-edge transcript dock**: re-used by `↳ HH:MM` click handler — clicking a deep-link opens the dock (if collapsed) and dispatches a `yogurt:transcript:scrollTo` event the panel already listens for.

### Established Patterns
- **Cargo workspace member declarations** (Phase 0 D-01): new crates `yogurt-prompts` and `yogurt-notes` are added to `[workspace.members]` in the root `Cargo.toml` and depend on workspace-pinned `tokio`, `serde`, `serde_json`, `anyhow`, `tracing`. Phase 4 adds `tinytemplate = "1.2"`, `pulldown-cmark = "0.12"`, `regex-lite = "0.1"`, `async-trait = "0.1"`, `insta = "1.41"` to `[workspace.dependencies]`.
- **`#[cfg(test)] mod tests` for Rust unit tests + `crates/<crate>/tests/<area>.rs` for integration tests** (Phase 0 D-28). Phase 4 adds fixture-driven tests to `crates/yogurt-notes/tests/merge_fixtures.rs` and an integration test at `crates/yogurt-server/tests/enhance_endpoint.rs`.
- **Vitest at `web/src/**/*.test.tsx`** (Phase 0 D-29). Phase 4 adds `web/src/editor/marks/aiGrey.test.tsx` and `web/src/components/EnhancingBanner.test.tsx`.

### Integration Points
- `crates/yogurt-server/src/lib.rs` — registers the new `enhance` route, the new `llm` module, the new `llm_mock` module (test-only), and the new `markdown_exporter` module.
- `crates/yogurt-server/src/routes.rs` — mounts `POST /api/meetings/:id/enhance`.
- `web/src/lib/ws.ts` — extends the `WsMessage` union with `enhance_progress`.
- `web/src/lib/api.ts` — adds `postEnhance(meetingId)`.
- `web/src/routes/Meeting.tsx` — End-meeting handler hits the enhance endpoint then navigates to `/meeting/:id/post`.
- `web/src/App.tsx` (or `routes.tsx`) — registers `<Route path="/meeting/:id/post" element={<MeetingPost />} />`.

</code_context>

<specifics>
## Specific Ideas

- "Indistinguishable from Granola" is the felt-quality bar for the hero. The 30-second timer starts when the user clicks "End meeting" and stops when they can read a coherent, deep-linkable, black/grey document.
- The legend in the top-right of the post-meeting view shows the swatch contract literally: a black square + "your notes" and a grey square + "AI". Per PRD §5.3 this is non-negotiable — users should not have to guess what grey means.
- Shimmer skeletons resolving in stagger (140/340/560/760) is the felt-quality differentiator vs. "spinner + done". The animation is the message: the AI is weaving, not waiting.
- Click `↳ HH:MM` → transcript opens and scrolls to that timestamp; hover shows the actual transcript excerpt as a tooltip. This is the "augmented" part of augmented notes: the link is the proof.
- Promote-on-edit: when the user types inside grey, the typed character turns black instantly. The surrounding grey is left alone. This is the felt-quality test that says "the AI's text is suggestion, not commitment".
- 5-fixture TDD against `merge_notes`: pure new AI, AI under user heading, AI bullet between user bullets, promote-grey-on-edit, re-enhance-preserves-promoted. These five scenarios cover every contractual claim in PRD §5.3 — if all five pass, the merge logic is right.

</specifics>

<deferred>
## Deferred Ideas

- **Real OpenAI-compatible LLM client behind `LlmClient` trait** — Phase 5. Phase 4 ships a minimal hardcoded client + `MockLlm` fallback.
- **Settings UI for LLM provider / API keys / Keychain** — Phase 5.
- **In-meeting chat pill ("Ask this meeting…" ⌘K)** — Phase 6. `chat-system.md` already ships in this phase to keep the prompts crate cohesive.
- **Token-by-token streaming of the LLM response into the editor** — Phase 5. The WS event shape (`chars` running count) is forward-compatible; Phase 4 emits it once at completion.
- **Template picker popover (Standup / Generic / 1:1 / Interview debrief)** — explicitly cut from v1 per PRD §5.5 and PROJECT.md "Out of Scope". Re-enhance always re-runs the single bundled `enhance.md`.
- **Versions rail (v1/v2/v3 enhance outputs)** — explicitly cut from v1. Re-enhance overwrites in place.
- **SQLite FTS5 search across notes + transcripts** — Phase 7 (LIB-07).
- **Library home view + onboarding + empty/error states** — Phase 7.
- **`yogurt doctor` + Homebrew distribution** — Phase 9.

</deferred>

---

*Phase: 04-augmented-notes-hero-highest-payoff*
*Context gathered: 2026-06-25*
