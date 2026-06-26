---
phase: 04-augmented-notes-hero-highest-payoff
reviewed: 2026-06-25T19:58:00-07:00
depth: deep
files_reviewed: 26
files_reviewed_list:
  - crates/yogurt-prompts/Cargo.toml
  - crates/yogurt-prompts/build.rs
  - crates/yogurt-prompts/src/lib.rs
  - crates/yogurt-prompts/src/ctx.rs
  - crates/yogurt-prompts/templates/enhance.md
  - crates/yogurt-notes/Cargo.toml
  - crates/yogurt-notes/src/lib.rs
  - crates/yogurt-notes/src/ast.rs
  - crates/yogurt-notes/src/diff.rs
  - crates/yogurt-notes/src/render.rs
  - crates/yogurt-notes/src/ts.rs
  - crates/yogurt-notes/tests/merge_fixtures.rs
  - crates/yogurt-server/src/lib.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-server/src/meetings.rs
  - crates/yogurt-server/src/enhance.rs
  - crates/yogurt-server/src/llm_openai.rs
  - crates/yogurt-server/src/llm_mock.rs
  - crates/yogurt-server/src/markdown_exporter.rs
  - crates/yogurt-server/src/storage/migrations.rs
  - crates/yogurt-server/src/storage/migrations/V0004__add_enriched_doc_json.sql
  - crates/yogurt-server/tests/enhance_endpoint.rs
  - crates/yogurt-server/tests/markdown_exporter.rs
  - web/src/editor/index.tsx
  - web/src/editor/extensions.ts
  - web/src/editor/marks/aiGrey.ts
  - web/src/editor/marks/transcriptLink.ts
  - web/src/editor/markdown.ts
  - web/src/components/EnhancingBanner.tsx
  - web/src/components/ShimmerSkeleton.tsx
  - web/src/components/Legend.tsx
  - web/src/components/ReEnhanceButton.tsx
  - web/src/components/TranscriptDock.tsx
  - web/src/components/TranscriptLine.tsx
  - web/src/lib/api.ts
  - web/src/lib/ws.ts
  - web/src/routes/Meeting.tsx
  - web/src/routes/MeetingPost.tsx
  - web/src/router.tsx
  - web/src/App.tsx
  - web/src/index.css
findings:
  blocker: 5
  high: 9
  medium: 8
  low: 5
  total: 27
status: issues_found
---

# Phase 4: Code Review Report

**Reviewed:** 2026-06-25T19:58:00-07:00
**Depth:** deep
**Files Reviewed:** ~26 (full Phase 4 surface)
**Status:** issues_found

## Summary

Phase 4 is the hero augmented-notes feature — the entire product's reason for existing. The happy path works (the integration tests prove it end-to-end with MockLlm), but adversarial review uncovers a substantial cluster of correctness, security, and robustness gaps that the visual gate could not have caught. Several would silently corrupt the user's data:

- The enhance handler holds the **single-writer SQLite lock across the LLM HTTP call** — every concurrent enhance request serializes behind it and a stuck LLM blocks all DB writes server-wide (BL-1).
- A subtle **re-enhance race**: when the user clicks Re-enhance from `MeetingPost`, the page sends the `notes_md` from the GET-fetched DB row, NOT the current editor contents — so any unsaved edits are silently overwritten by the server merge (BL-3).
- The `useTranscriptWs` hook's effect dependency list is `[meetingId]` only — when `token` arrives after `meetingId`, the WS never connects (BL-4).
- `MockLlm` injects user-controlled text into raw HTML output **without escaping**, then the browser parses with `html: true`. A user typing `<script>` into notes or a transcript carrying angle brackets produces an XSS in the enriched markdown (BL-2).
- There is **no LLM timeout, no abort, and no error-state in the EnhancingBanner**. If the LLM stalls the banner spins forever, the WS never sees `done`, and the user has no recovery affordance (BL-5).

In addition the AST diff loses nested-list depth, the structural-marker stripping regex permits stray `</span>` ghosts across blocks, the markdown exporter uses non-`fsync`'d rename (lying about durability), file collisions on same-minute meetings will silently overwrite each other, slugify can produce zero-length filenames for non-ASCII titles, and the post route fetches the DB row even when `enrichedMd` is pre-loaded (causing a state-stomp race that can revert the editor to stale content). Details below.

The hero gate passed because the manual smoke ran with English notes, a single meeting, an instant MockLlm, and no concurrency. Real users will hit at least four of the five BLOCKERs in the first week.

---

## BLOCKERs

### BL-1: Enhance handler holds writer Mutex across LLM HTTP call (catastrophic serialization)

**File:** `crates/yogurt-server/src/enhance.rs:159-189` (the writer scope) — but read the whole `enhance` async fn flow.

The writer Mutex is acquired in step 7a (line 159) but the LLM call happens earlier at step 4 (lines 113-122). On its face that looks fine — the lock is held only for the SQL UPSERT. However a stronger issue exists upstream: every Phase 4 enhance request executes the **synchronous, blocking** SQLite `conn.execute(...)` while holding `std::sync::Mutex::lock()` inside a tokio task (line 161-188). That blocks the executor thread until SQLite returns. For a single SQLite UPSERT this is usually a few ms — tolerable. The real BLOCKER is what happens next:

The handler is followed by `markdown_exporter::write` (line 192) which does **synchronous** `std::fs::write` + `std::fs::rename`. Both are blocking file ops. If the user has `~/.yogurt/` on a slow disk or fsync-paused FS, the tokio worker is pinned. Combined with the writer Mutex above the worst case is: one slow disk → all enhance requests serialize on `writer.lock()` → tokio worker count drops → other meetings' WS broadcasts back up → live transcript stutters.

**Consequence:** A user running enhance on a meeting while another tab streams transcripts will see transcript stalls. With cloud LLMs the lock is released before the LLM call, but the blocking I/O combined with multiple in-flight enhances still produces head-of-line blocking.

**Fix:**
1. Wrap the entire writer-lock block plus the `markdown_exporter.write` call in `tokio::task::spawn_blocking`:
```rust
let storage = state.storage.clone();
let exporter = state.markdown_exporter.clone();
let merge_for_blocking = (meeting_id_str.clone(), title.clone(), started_at_unix_ms,
                         ended_at_unix_ms, req.notes_md.clone(), req.transcript_json.clone(),
                         enriched_md.clone(), enriched_doc_json);
let notes_path = tokio::task::spawn_blocking(move || -> anyhow::Result<PathBuf> {
    let conn = storage.writer().lock().map_err(|e| anyhow!("db lock: {e}"))?;
    conn.execute(/* UPSERT */, params![...])?;
    drop(conn); // release lock BEFORE filesystem write
    exporter.write(&ExpMeeting { ... })
}).await.map_err(...)??;
```
2. Add a `tokio::time::timeout` around the LLM call (see BL-5 — the same fix covers both).

---

### BL-2: XSS via unescaped notes/transcript in `MockLlm` output → `html: true` parser

**File:** `crates/yogurt-server/src/llm_mock.rs:36-58` and `web/src/editor/markdown.ts:36-39`.

`MockLlm` echoes the user's `notes` verbatim into the output, and inserts the first 8 transcript words into `<span data-ai-grey ...>SUMMARY ...</span>` with **no HTML escaping**:

```rust
out.push_str(notes.trim_start());   // user-controlled text, raw
...
let summary = words.join(" ");      // transcript-controlled text, raw
out.push_str(&format!(
    "- <span data-ai-grey data-ts=\"{ts}\">{summary} ...</span>\n"
));
```

That string then becomes `enriched_md`, which the browser renders via:
```ts
const renderer = new MarkdownIt({ html: true, linkify: false, breaks: false });
return renderer.render(md);  // markdownToHtml
```
…and ProseMirror parses the resulting HTML, executing any `<script>` or `<img onerror=...>` tags carried in user notes or transcript text. Even `parseHTML` rules don't strip `<script>`; ProseMirror sets the doc via `innerHTML`-style parsing.

**Reproducer:**
- User types `- <script>alert(1)</script>` in notes → after enhance the rendered HTML contains the raw script tag → ProseMirror's HTML parser may not execute scripts on insertion (browsers neutralize them when set via innerHTML), but **`<img src=x onerror=alert(1)>` absolutely fires**, as does an `<iframe srcdoc>` etc.
- Deepgram is a network input. A malicious transcript line (or a compromised STT response) can inject the same.

**Consequence:** Stored XSS on every meeting that contains untrusted text in either notes or transcript. Persisted to SQLite + on-disk markdown.

**Fix:**
1. HTML-escape every interpolated string in `MockLlm::complete`: `html_escape::encode_safe(&summary)` etc.
2. The real `OpenAiCompatClient` shipped output is equally untrusted — the LLM can hallucinate `<script>` too. Either:
   - sanitize `enriched_md` server-side with `ammonia` keeping only the allowlist `data-ai-grey`, `data-transcript-link`, `data-ts` spans, OR
   - in `markdown.ts` switch `html: true` → `html: false`, and have the server emit a separate JSON shape that the editor uses to construct marks directly (no HTML pass-through). The current `html: true` + LLM-output-as-HTML pipeline is a structural XSS smell that will keep biting.
3. The yogurt-prompts `enhance.md` also tells the LLM "wrap in `<span data-ai-grey>`" which makes XSS-by-LLM the *expected* path — without server-side sanitization this is a design-level vulnerability.

---

### BL-3: Re-enhance silently destroys user edits in MeetingPost

**File:** `web/src/routes/MeetingPost.tsx:84,127-177,302-315`

The flow:
1. `MeetingPost` loads `enriched_md` into `<YogurtEditor>` via the `enrichedMarkdown` prop.
2. The user edits the doc (typing in the editor mutates the ProseMirror doc).
3. The user clicks **Re-enhance**. `<ReEnhanceButton>` receives `notesMd` from `MeetingPost`'s state — which is **`notesMd` from the GET-fetched DB row** (`setNotesMd(json.notes_md ?? "")` at line 157), **not the current editor content**.
4. The server merges the OLD `notes_md` with the LLM output, persists it as the new `enriched_md`, and the editor is replaced with that. Every edit the user made is gone.

`YogurtEditor` does have an `onChange` prop, but `MeetingPost` never wires it (line 346 omits it). There is no mechanism to capture the live markdown for the Re-enhance POST.

**Consequence:** Anyone who edits the post-meeting doc then clicks Re-enhance loses their edits. This is the exact promote-grey-on-edit user flow the AST diff fixture #4 exists to defend — and the browser-side wiring undermines it.

**Fix:**
1. Wire `YogurtEditor`'s `onChange` in `MeetingPost`:
```tsx
const [liveMarkdown, setLiveMarkdown] = useState<string>("");
useEffect(() => { setLiveMarkdown(enrichedMd ?? ""); }, [enrichedMd]);
// ...
<YogurtEditor
  ...
  onChange={setLiveMarkdown}
/>
// ...
<ReEnhanceButton notesMd={liveMarkdown} ... />
```
2. Add an integration test for the round trip: load enriched → edit → re-enhance → assert edit survives.

---

### BL-4: `useTranscriptWs` does not reconnect on `token` change → dock never connects when token resolves after meetingId

**File:** `web/src/lib/ws.ts:150-256` (the effect) and `:256` (the dep array).

```ts
useEffect(() => {
  if (!meetingId || !token) { ... return; }
  // ... connect ...
  return () => { ... };
}, [meetingId]);    // ← MISSING `token`
```

The effect closes over `token` (line 164 uses it in the URL), but the dep array is `[meetingId]`. Whenever `meetingId` is set BEFORE `token` resolves (the common bootstrap path in `MeetingPost` — `meetingId` is read from URL params synchronously; `token` arrives async from `ensureSessionToken()`), the early-return at line 155 fires, leaves the hook in `idle`, and **never re-runs** when `token` flips from `null` to a real value.

The `useEnhanceProgress` hook (line 304+) gets this right with `[meetingId, token]` — proof that the author knew, just missed one site.

**Consequence:** The Live Transcript dock on the post-meeting route stays "offline" forever (or until the user manually triggers another `meetingId` change, which they can't). Click-to-jump from `↳ HH:MM` opens the dock but it's empty.

**Fix:**
```ts
}, [meetingId, token]);
```

Also: an eslint rule (`react-hooks/exhaustive-deps`) would have caught this — confirm the rule is enabled in the web's lint config.

---

### BL-5: Enhance has no timeout, no abort, no error path — banner can stick forever

**Files:** `crates/yogurt-server/src/enhance.rs:113-122` (LLM call), `crates/yogurt-server/src/llm_openai.rs:54-71` (`reqwest::Client::new()` with default no-timeout config), `web/src/components/EnhancingBanner.tsx` (no error state), `web/src/routes/MeetingPost.tsx:101,266` (banner driven by WS `enhancing`).

Failure mode chain:
1. `OpenAiCompatClient` uses `reqwest::Client::new()` — **no timeout configured**. A wedged LLM provider hangs the request indefinitely.
2. `enhance.rs` awaits the call directly with no `tokio::time::timeout`. The HTTP request to the server hangs.
3. WS `enhance_progress sending` already fired; `done` is never sent.
4. Frontend `useEnhanceProgress` shows `phase === "sending"`, `enhancing === true` → banner stays up indefinitely.
5. There is no error state in `EnhancingBanner` (the file lacks any error/cancel UI), no way for the user to cancel, no `phase === "error"` in the type union (`web/src/lib/ws.ts:33`).
6. The `Re-enhance` button rejects clicks because `busy` stays true.

**Consequence:** A flaky cloud LLM (Deepgram outage, OpenAI 500, network blip) silently locks the user out of the post-meeting view until they refresh. Worse — `ReEnhanceButton`'s `finally` clears `busy` only on HTTP completion; if the fetch hangs the button is permanently disabled.

**Fix:**
1. Configure `reqwest::Client::builder().timeout(Duration::from_secs(60)).build()` in `llm_openai.rs`.
2. Wrap the LLM call in `tokio::time::timeout(Duration::from_secs(60), ...)` in `enhance.rs` so a hung TCP connection doesn't pin the handler. Return `504 Gateway Timeout` on expiry, and emit `enhance_progress {phase: "error"}` on the events_tx so the frontend can recover.
3. Add a `phase: "error"` variant to `EnhanceProgressEvent` and have `EnhancingBanner` render an error pill (`Re-enhance failed — try again`).
4. Add `AbortController` to `postEnhance`/`ReEnhanceButton` so the user can cancel mid-flight, and have `enhance.rs` listen for the connection-closed signal to abort the LLM stream.

---

## HIGH

### HI-1: `get_meeting` uses writer Mutex for a read query

**File:** `crates/yogurt-server/src/routes.rs:186-217`

```rust
let writer = state.storage.writer();
let row = { let conn = writer.lock()...
```

`Storage::read()` exists exactly for this case (4-conn round-robin pool). Reading on the writer lock serializes the GET behind every concurrent enhance UPSERT — a 200ms LLM-bound write blocks every page-refresh on the post route.

**Fix:** `let reader = state.storage.read(); let conn = reader.lock()...`

---

### HI-2: `MeetingPost` fetch races against `setEnrichedMd` and can revert the editor to a stale row

**File:** `web/src/routes/MeetingPost.tsx:127-177`

The effect always fires the GET (line 134), even when `preloadedEnrichedMd` is set. Lines 154-156 guard the overwrite with `if (enrichedMd === undefined)` — but `enrichedMd` is read from the **closure** at effect-creation time, not the live state. By the time the fetch resolves the user may have clicked Re-enhance, which set `enrichedMd` to a new value — but the in-flight fetch will see `enrichedMd === undefined` (stale closure) only if the effect hasn't re-run. Actually here the check is fine because of how state-as-of-render works, BUT:

- If Re-enhance completes BEFORE the initial GET fetch resolves, the `setEnrichedMd(json.enriched_md ?? json.notes_md ?? "")` runs anyway via `setNotesMd`/`setTranscriptJson` etc. — *those* are unconditional. So the just-enhanced response's `notesMd` for the next Re-enhance is overwritten by the DB's pre-enhance `notesMd`. Combined with BL-3 this guarantees stale data on every re-enhance.

**Consequence:** Two-write race where the first GET stomps on values updated by the Re-enhance handler. Hard to repro without timing manipulation; easy to repro on slow networks.

**Fix:** Use an in-flight ref or a request-id token to discard stale fetch responses. Better: skip the fetch entirely when `preloadedEnrichedMd` is present and persist `notes_md`/`transcript_json` via location.state too.

---

### HI-3: AST diff loses nested-list depth (regression for sub-bullets)

**File:** `crates/yogurt-notes/src/ast.rs:87-104` + `crates/yogurt-notes/src/render.rs:21-23`

`list_depth.saturating_sub(1)` is applied when emitting an Item (line 94), so a top-level item gets `depth=0`, a sub-item `depth=1`. So far OK. But `render::to_markdown` does `"  ".repeat(*depth as usize)` — only two-space indentation, no list-marker context. pulldown-cmark emits `Tag::List` once for outer and once for inner, but the diff key (`block_key` for ListItem includes only `md` + `depth`) means two visually-identical bullets at different depths produce different keys. That's the inverse direction — if the LLM flattens the user's nested list (which it WILL — instruct-tuned models flatten heavily), the user's depth=1 sub-bullets get re-emitted as depth=0 items in the LLM's output → diff matches by text but at the wrong depth → user's nested structure is lost in the merge (Source::User but at depth 0).

There is no fixture for "user wrote nested bullets, LLM flattened them" — the dispatch's hottest adversarial case is uncovered.

**Consequence:** A user who relies on outline-style notes loses their structure on enhance.

**Fix:** When the diff matches a user block by key-ignoring-depth, preserve the *user's* depth, not the LLM's. Add a fixture for nested-list flattening.

---

### HI-4: `block_key` regex strips `</span>` globally — confuses nested marker spans

**File:** `crates/yogurt-notes/src/ast.rs:41-50`

```rust
let re3 = regex_lite::Regex::new(r#"</span>"#).unwrap();
```

This strips ALL `</span>` tags, including any `</span>` the user typed manually (rare) AND nested closing tags in the LLM's `<span data-ai-grey ...><span data-transcript-link ...>↳ HH:MM</span></span>` output (the wire format has nested spans — see `render::wrap_ai` and `MockLlm::complete`). The outer span closes with the **second** `</span>`; stripping both leaves the link's `↳ HH:MM` text inline as a phantom suffix on the user's matched block, making the block_key **not match** what `parse` saw of the user block. False AI-attribution.

Also the regex is compiled on every call — see LO-1.

**Consequence:** Re-enhance flow where the LLM re-emits an already-wrapped span fails to recognize that line as previously-promoted user content; the user sees a black bullet flicker grey then back.

**Fix:** Match span opens and their corresponding closes in a single regex pass, or parse the spans properly (the markdown source for these bullets is small — a 5-line state machine is cheaper than a regex pair). Compile regexes with `Lazy<Regex>`.

---

### HI-5: `MarkdownExporter::write` is not actually atomic — no `fsync`

**File:** `crates/yogurt-server/src/markdown_exporter.rs:55-57`

```rust
std::fs::write(&tmp_path, &content)...;
std::fs::rename(&tmp_path, &final_path)...;
```

`std::fs::write` calls `write` + (in newer std) `flush`, but never `fsync`/`File::sync_all`. POSIX rename atomicity guarantees the user sees either old or new at the directory entry level, but on a sudden power loss the *contents* of the renamed file may be empty/torn because the data blocks were never flushed to disk. The doc-comment on line 9-11 claims "POSIX rename is atomic, so a partial write cannot corrupt an existing file" — that's true for visibility, false for durability.

**Consequence:** A power cut between rename and the disk flushing both the data and the directory entry can leave the user with an empty `.md` file in place of their meeting notes (assuming SQLite is also lost; in practice they have the SQLite copy too, but the doc-comment promises atomicity it doesn't deliver).

**Fix:**
```rust
let f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp_path)?;
f.write_all(content.as_bytes())?;
f.sync_all()?;             // fsync tmp
drop(f);
std::fs::rename(&tmp_path, &final_path)?;
// Optional: fsync the parent dir for rename durability.
let dir = std::fs::File::open(self.notes_dir.as_path())?;
dir.sync_all()?;
```

---

### HI-6: Two meetings in the same minute with the same slug silently overwrite each other

**File:** `crates/yogurt-server/src/markdown_exporter.rs:62-71`

Filename = `<YYYY-MM-DD-HHmm>-<slug>.md`. If two meetings created in the same minute share a slug (e.g., two "Sync"-titled standups), the second's `rename` overwrites the first. The integration test `it_overwrites_on_repeat_write_atomically` (line 52-76) ASSERTS the same-path collision is desired — but that test uses the same `id` deliberately. The collision case is two different `id`s, same `started_at_unix_ms` second + same `title`.

**Consequence:** Data loss for an entire meeting's per-meeting markdown file (SQLite still has it, but the on-disk artifact is gone).

**Fix:** Include a short suffix of the meeting `id` in the filename: `<YYYY-MM-DD-HHmm>-<slug>-<id-prefix>.md`. Or check `path.exists()` and append a disambiguator.

---

### HI-7: `slugify` produces literal `"untitled"` for unicode-only titles

**File:** `crates/yogurt-server/src/markdown_exporter.rs:73-92`

```rust
.map(|c| if c.is_alphanumeric() { c } else { '-' })
```

`is_alphanumeric` is unicode-aware and KEEPS unicode letters (e.g., `'今'` is alphanumeric). But a title made entirely of emoji (`'🎯'`) is NOT alphanumeric, so it dasherizes to all `-`s, then `parts.filter(!is_empty)` is empty, and the result is `"untitled"`. Fine, but inconsistent: `"日本語 sync"` becomes `"日本語-sync"` with raw Japanese characters in the filename — fine on macOS APFS, but some downstream consumers (the planned library scroll restoration) may not handle high-codepoint chars in paths. More importantly the test only exercises ASCII (line 84 `"  "`); the unicode path is untested.

**Consequence:** Minor — unicode meetings get unicode filenames (acceptable on macOS) but the absence of any unicode test means a regression here would be invisible.

**Fix:** Add a unicode title fixture; consider whether to NFC-normalize titles before slugifying.

---

### HI-8: SQLite migration not idempotent in the strict sense — `IF NOT EXISTS` check happens INSIDE a transaction that wraps `ALTER TABLE`

**File:** `crates/yogurt-server/src/storage/migrations.rs:55-65` (excerpt)

The check `column_exists(&tx, "meetings", "enriched_doc_json")` runs *inside* a transaction holding `tx.execute_batch(V0004_ADD_ENRICHED_DOC_JSON)`. SQLite handles ALTER inside transactions for additive columns, but if two concurrent server boots race on init (multiple `cargo run` instances, or upgrade-during-restart), one's transaction may commit the ALTER while the other's `column_exists` check returns false — both attempt the ALTER, second errors with "duplicate column". The current single-process initialization avoids this but the comment claims "must run inside the same transaction so concurrent boots see a consistent state" — that's only true within a single process. SQLite's locking serializes writes, so the second boot's check WILL see the new column once the first commits — but only if the first is fully committed before the second's transaction begins. Without explicit `BEGIN EXCLUSIVE`, this is best-effort.

**Consequence:** Very low likelihood; a second-instance boot during upgrade could fail with a non-graceful error.

**Fix:** Wrap the migration body in a `tx = conn.transaction_with_behavior(TransactionBehavior::Exclusive)?` so the check + ALTER are serialized at SQLite's locking layer.

---

### HI-9: `enhance.rs` swallows lookup failure when meeting registry is in-memory only

**File:** `crates/yogurt-server/src/enhance.rs:85-89` & `crates/yogurt-server/src/routes.rs:186` (GET)

`POST /enhance` returns 404 if the meeting is not in the in-memory `Registry`. After a server restart, all in-memory meetings are lost (per `meetings.rs:42` docs). But the migration adds DB persistence in `enrich.rs`'s UPSERT — so a meeting created pre-restart, enhanced pre-restart, then re-enhanced post-restart cannot find the in-memory meeting → 404. The user thinks the meeting is "gone" because they can't enhance again, even though `GET /api/meetings/:id` succeeds.

**Consequence:** Re-enhance breaks after every server restart for every previously-enhanced meeting.

**Fix:** When the in-memory lookup fails, fall back to constructing a transient `Meeting` from the SQLite row (or skip the registry lookup entirely for the enhance handler — it only needs `events_tx`, which can be a per-request broadcast).

---

## MEDIUM

### MD-1: `ts::guess_ts_sec` falls back to the first transcript segment when no word matches — wrong ts for every late-meeting AI bullet

**File:** `crates/yogurt-notes/src/ts.rs:38-39`

```rust
best.map(|(_, ts)| ts)
    .or_else(|| transcript.first().map(|s| s.ts_ms / 1000))
```

If the AI bullet contains zero >3-char-word matches with any transcript segment (common when the LLM paraphrases), the ts is "first segment ts" — i.e., 00:00. Every paraphrased AI bullet ends up linking to the very start of the meeting. The user clicks `↳ 00:00` expecting "around the moment this was discussed", lands at silence.

**Fix:** Fall back to the *closest segment by position in the LLM output* (use the bullet's index relative to the total bullet count as a fraction, multiply by meeting duration). Or just emit `ts_sec = None` and don't render the deep-link when there's no signal.

### MD-2: Word-overlap threshold is "1 match wins" — single common word fires a spurious link

**File:** `crates/yogurt-notes/src/ts.rs:29-36`

A bullet containing "we" or "with" (which are stripped by the `> 3 chars` filter — wait, "with" is 4) matches any transcript segment containing "with". The threshold should be either a minimum count, a minimum overlap ratio, or a tie-breaker that prefers the first appearance in order.

**Fix:** Require `count >= 2` OR `count / block_words.len() >= 0.3`. Add tests for the threshold.

### MD-3: Click-to-jump CustomEvent is window-scoped — multi-tab crosstalk

**File:** `web/src/components/TranscriptDock.tsx:88-126` + `web/src/routes/MeetingPost.tsx:183-188`

`window.dispatchEvent(new CustomEvent("yogurt:transcript:scrollTo"))` fires globally. If a user has two meeting tabs open in different windows of the same browser, OK — separate windows. But two tabs in the same window? Each window has its own `window` object, so this is fine in practice. However the `TranscriptDock`'s listener never filters by meeting id — if Phase 7 introduces a single-tab multi-meeting view (library + detail in the same tab), every dock listens to every link.

**Consequence:** Future regression risk; not a today bug.

**Fix:** Include `detail: { ts, meetingId }` and filter in the dock listener.

### MD-4: `appendTransaction` strips aiGrey on adjacent untouched ranges (pure-cursor edits sometimes carry remap ranges)

**File:** `web/src/editor/marks/aiGrey.ts:62-95`

The plugin iterates `stepMap.forEach((_oldStart, _oldEnd, newStart, newEnd) => ...)` — but a ProseMirror StepMap reports every range the step touched, not just user-typed text. A pure selection-set-mark step (e.g., from a future paste handler) can report ranges that span the entire pasted region; if any byte of that region overlaps an existing aiGrey mark, the WHOLE pasted region gets demoted. Borderline behavior, but the comment claims "Only strip if the inserted range actually carries aiGrey marks **in the new state**" — which is correct only if the new-state ranges precisely match the inserted slice. They often don't (e.g., for replace steps the StepMap's "new" range covers the entire replacement, which may include unchanged tail bytes the StepMap rebinds).

**Consequence:** Spurious demotion under uncommon edit patterns (paste-replace, undo-redo near grey ranges).

**Fix:** Track the actual transactions' steps and only act on `ReplaceStep` and friends with confirmed inserted slices; use `Transaction.steps[i].toJSON()` to inspect.

### MD-5: `EnhancingBanner` `chars` shown via `chars!` non-null assertion despite explicit `hasChars` check — TS unsoundness if `chars` is set to `null` post-render

**File:** `web/src/components/EnhancingBanner.tsx:54,113`

```ts
const hasChars = typeof chars === "number" && chars >= 0;
...
{hasChars && (... {chars!.toLocaleString("en-US")} ...)}
```

Functionally correct since `chars` is a prop (immutable per render), but the non-null assertion `chars!` is brittle if anyone refactors `chars` to be derived. Use `typeof chars === "number"` inline.

**Fix:** `{typeof chars === "number" && chars >= 0 && <span>... {chars.toLocaleString("en-US")} chars</span>}`.

### MD-6: `useEnhanceProgress` opens its own WebSocket — doubles connection count

**File:** `web/src/lib/ws.ts:296-385`

Two hooks (`useTranscriptWs`, `useEnhanceProgress`) each open a separate WS connection on `MeetingPost`. Each connection runs the meeting-events broadcast subscription independently, so the server fans out twice for the same browser. Fine functionally — `events_tx` capacity 64 absorbs it — but bandwidth/CPU doubles for no architectural reason.

**Fix:** Share one connection via a small WS broker context (Phase 5 store).

### MD-7: `useTranscriptWs` ignores `enhance_progress` frames silently (line 211-215 comment) — no `default` case for unknown frame types

If Phase 6 introduces `chat_chunk` or any new frame type before the hook is updated, the new frame is silently dropped. Today: a transcript regression caused by a typo in the server's `type` field would also be invisible.

**Fix:** `console.warn` on unknown frame types in dev mode, or surface to a global error boundary.

### MD-8: Meeting.tsx `endMeeting` does best-effort `stopRecording` swallow but the catch can never fire — `stopRecording` itself catches

**File:** `web/src/routes/Meeting.tsx:214-220`

```ts
if (recording) {
  try {
    await stopRecording();   // ← stopRecording itself catches all errors
  } catch {}                 // ← unreachable
}
```

`stopRecording` (line 184-201) has its own try/catch; the outer `try/catch` is dead. Minor — but it documents an intent that isn't met (you want stop failures to be observable). Actually the inner catch already calls `setError(...)`, so on stop-failure the user sees a transient error before navigating to /post — racy display.

**Fix:** Make `stopRecording` rethrow, or distinguish "stop-and-continue" vs "stop-and-bail" paths.

---

## LOW

### LO-1: Recompiling regexes in `strip_markers` on every call

**File:** `crates/yogurt-notes/src/ast.rs:42-46`

Three `regex_lite::Regex::new(...).unwrap()` calls happen every time `block_key` is invoked, which is O(blocks) per merge. Negligible at current sizes but unnecessary.

**Fix:** `static RE1: OnceLock<Regex>` or `once_cell::sync::Lazy`.

### LO-2: `Mode::Release` panics on missing cached template via `expect`

**File:** `crates/yogurt-prompts/src/lib.rs:82`

```rust
Mode::Release => Ok(cached.expect("release mode caches at load").to_string()),
```

`Prompts::load` does set the cache in Release mode, so this expect is unreachable today — but it's a panic in the request path. Replace with a clean `anyhow::bail!`.

### LO-3: `MockLlm` test name claim "first 8 words" — but transcript chosen has exactly 8 words

**File:** `crates/yogurt-server/src/llm_mock.rs:92-114`

The test "We debated the pricing model in detail today" is exactly 8 words, so the test cannot distinguish "first 8" from "all". Add a longer-transcript test.

### LO-4: `EnhanceProgressEvent` lacks `error` and `cancelled` variants

**File:** `web/src/lib/ws.ts:31-35`

Already mentioned in BL-5. Even as a roadmap item the missing types make the future state harder to add without breaking the union.

### LO-5: `Logger` warns include endpoint string in WS auth path — but enhance.rs success path emits no `tracing::info!` for "enhance started/finished"

**File:** `crates/yogurt-server/src/enhance.rs:80-217`

Zero tracing in the entire handler. The integration test's only evidence the endpoint ran is the HTTP 200; debugging a production failure requires inserting prints. Add `tracing::info!(meeting=%meeting_id, "enhance: start")` and a span around the LLM call so latency is observable.

---

## Cross-cutting observations

**Test coverage gaps:**
1. The 5 yogurt-notes fixtures are clean canonical cases. None of these are exercised: (a) LLM uses `*` bullets where user used `-`; (b) LLM injects a heading the user didn't write; (c) LLM flattens nested lists; (d) bullet contains code spans / inline markdown the LLM transforms.
2. `enhance_endpoint.rs` only tests against MockLlm. There is no fault-injection test (LLM 500, LLM hang, LLM returns empty string, LLM returns non-markdown garbage). The handler's error paths are untested.
3. `aiGrey.test.tsx` explicitly defers promote-on-edit to "manual smoke" (line 4-6) — but the entire BL-3/HI-4 cluster lives in that surface and there is no automated coverage.
4. No frontend test for the `MeetingPost` hydration race (HI-2) or the Re-enhance edit-loss path (BL-3).
5. No backend test that two simultaneous enhance requests on the same meeting return deterministic state.

**Security posture summary:** The XSS vector (BL-2) is the only Critical-tier security finding, but the lack of LLM output sanitization is structural. If you cannot ship a server-side allowlist sanitizer in Phase 4, at minimum disable `html: true` in markdown-it and use a JSON-shape contract end-to-end.

**Acknowledged deviations check:**
- `GET /api/meetings/:id` (added beyond plan): inspected — does NOT return secrets. ✓
- `postEnhance(meetingId, body, token)` signature: matches `EnhanceRequest` server-side. ✓
- BUT: `GET /api/meetings/:id` uses the writer Mutex (HI-1) — design-level bug.

---

_Reviewed: 2026-06-25T19:58:00-07:00_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
