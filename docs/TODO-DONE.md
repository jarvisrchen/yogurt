# Done

Closed-out tickets moved out of `docs/TODO.md` to keep that file short.
Every item keeps its own `- [x] **ID** title` line and its own indented `<details>` body, as a flat list with no subsections - the old subsections went stale (two separate `### Meetings` groups existed before this split) so this file does not try to maintain them.
ID allocation counts this file too: the next ID for a prefix is the highest number for that prefix across `docs/TODO.md` and this file, plus one.
It stays in `docs/` rather than `docs/archive/` for that reason - it is live input to ID allocation, not a stale record.

New closed items go at the bottom.

- [x] **MTG-11** Auto-start recording when yogurt detects a meeting has begun
  <details>
  <summary>Details</summary>

  Shipped as **detect-and-prompt**, not auto-record.
  `yogurt_audio::detect` polls `SCShareableContent` every 5s for an on-screen window matching a small allow-list of (bundle id, in-call window title) pairs - Zoom, Google Meet in a browser, Teams, Slack huddles.
  Only the Google Meet rule is verified against a live call; the other three are inferred from documented window naming and should be confirmed with `yogurt ctl windows` during a real call before being trusted.
  That reuses the `screencapturekit` dependency and the Screen Recording grant system-audio loopback already requires, so detection costs no new dependency and no new TCC prompt.
  The two alternatives in the original scoping note were both worse: a calendar read needs a new permission and fires on events you skip, and a system-audio heuristic means holding a capture stream open around the clock, which is the thing the privacy constraint exists to prevent.

  The prompt is a floating banner (`MeetingDetectedBanner`) offering "Start recording" / "Not now"; Start runs the same create-then-`autoStart` path as "+ New meeting".
  Detection never starts a recording on its own - a false positive costs a dismissable banner, not a recorded room.
  It does auto-*stop*: while the setting is on, a recording running alongside a detected meeting window is linked to it and stops once that window has been gone for three polls.

  Settings -> General carries an on-by-default toggle.
  Known blind spot: a browser window's title is its active tab's title, so a Meet call in a background tab is invisible; seeing it would need Accessibility permission.
  `yogurt ctl windows` (CLI-4) dumps the live window list with each row's verdict, for retuning the title patterns when a vendor renames a window.
  </details>

- [x] **UI-1** BASE URL and MODEL fields overlap on long URLs in provider cards
  <details>
  <summary>Details</summary>

  On the Settings → LLM providers page, the Google Gemini card (active) has its BASE URL value `https://generativelanguage.googleapis.com/v1beta/openai` running straight through the MODEL value `gemini-2.5-flash`. The two are visually overlaid instead of sitting side-by-side in their two grid columns.

  Root cause is in `web/src/components/settings/ProviderRow.tsx:111` - `<div className="grid grid-cols-2 gap-x-6 gap-y-3">`. Grid items default to `min-width: auto`, so a child with non-wrapping content (here the URL wrapped in `break-all` text but the inner `<code>` is still wider than its column at full URL length) refuses to shrink below its intrinsic content size and overflows into the sibling column.

  Visual evidence:
  ![BASE URL runs through MODEL field](attachments/2026-08-29-base-url-model-overlap.png)
  </details>

  Landed in #9. `min-w-0` added to each `Field` child in the grid, in both `ProviderRow.tsx` (inactive card) and `ProviderCard.tsx` (active/edit card, whose BASE URL `<code>` was also missing `break-all`).

- [x] **UI-2** Add a "Test" button for the Deepgram key in Settings → Transcription
  <details>
  <summary>Details</summary>

  The LLM providers section has a Test button that does one live round-trip and reports a verdict inline (`web/src/components/settings/TestKeyButton.tsx`, backed by `POST /api/settings/providers/{id}/test`). The Transcription section's Cloud card has no equivalent, so a wrong or expired Deepgram key stays silent until a meeting produces no transcript, which is the worst possible time to find out.

  Two halves:

  1. Backend: a test endpoint for the STT key, alongside the existing `POST /api/settings/stt/key` in `crates/yogurt-server/src/api/settings.rs`. There is no STT equivalent of `test_provider` today. Deepgram's cheapest liveness check is probably a short authenticated request rather than opening a streaming socket; pick one that fails fast on a bad key without burning quota.
  2. Frontend: render the button on `CloudSTTCard` in `web/src/components/settings/STTPicker.tsx`. `TestKeyButton` is close to reusable as-is, but it is currently hardcoded to `settingsApi.testProvider(providerId, ...)`, so it needs the mutation passed in or a sibling component. Keep its two good behaviors: test the stored key when the input is empty, and hide the verdict once the draft no longer matches what was tested, so a green tick never sits next to edited text.

  Match the LLM card's verdict styling (matcha for ok, straw for failure) so the two sections read as one system.
  </details>

  Landed in PRs #2 and #3 (v0.2.0). The 2xx and 401 probe paths remain untested: the Deepgram URL is hardcoded, so it cannot be pointed at wiremock the way a provider's `base_url` can.

- [x] **UI-3** Chat input loses its pill shape on focus
  <details>
  <summary>Details</summary>

  The library search field stays pill-shaped when focused, with a lavender pill outline wrapping the input.
  The in-meeting "Ask this meeting..." input is also pill-shaped when collapsed, but once you click into it, the text field drops the pill background and renders as a standard rectangular input with a thick focus ring.
  It should match the search field - same pill shape on focus.

  Visual evidence:
  ![search field stays pill-shaped on focus](attachments/2026-08-28-search-focused-pill-baseline.png)
  ![chat input collapsed, pill correct](attachments/2026-08-28-chat-input-collapsed-pill.png)
  ![chat input focused, pill lost](attachments/2026-08-28-chat-input-focused-no-pill.png)
  </details>

- [x] **UI-4** Thick blue ring around the AI notes section when interacting
  <details>
  <summary>Details</summary>

  In the in-meeting notes panel, typing into "your notes" leaves a heavy blue/purple border wrapped around the AI-generated content on the right.
  Same on post-meeting notes: clicking into the meeting summary section triggers the same thick ring around the whole AI area.
  Nothing is actually focused in those cases - it's a styling bug (likely `:focus-within` or panel-active state styling the wrong target).
  Remove or restyle so it doesn't read as a focus indicator.

  Visual evidence:
  ![notes AI section shows a thick focus ring](attachments/2026-08-28-notes-ai-section-focus-ring.png)
  </details>

- [x] **UI-5** "See all XXX models" link and Save button crowd each other on the active provider card
  <details>
  <summary>Details</summary>

  On the active provider card in Settings → LLM providers, the "See all Minimax models →" link sits right up against the Save button with barely any gap - looks like it's about to overlap, not great UX.

  ![see all models link crowds the Save button](attachments/2026-08-31-provider-card-link-save-overlap.png)
  </details>

  Landed in #41. `ProviderCard.tsx`: the docs link and `Save` now share one `flex … gap-3` row. They were bare siblings of the card's `space-y-4` container, which only sets `margin-top` - and since both are inline-level, they shared a line box and rendered flush.

- [x] **UI-6** Add a dark mode toggle

  Landed in #45. Settings > General has an Appearance control (System / Light / Dark). The preference is browser-local: an inline script in `web/index.html` stamps `data-theme` on `<html>` before first paint, `web/src/lib/theme.ts` keeps it current, and `index.css` re-points the `@theme` color tokens under `:root[data-theme="dark"]`. Components that hardcoded palette hex values were moved onto the tokens so they follow the theme; new UI must use tokens (`bg-card`, `text-ink`, `var(--color-*)`), never `bg-white` or a hex.

- [x] **UI-7** AI text in augmented notes is too low-contrast

  Landed in #47. AI-added runs shared `--color-grey` with 10px captions and transcript speaker labels, which put body prose at 2.45:1 on paper in light mode. They now use their own `--color-ai` token (`#7A7264` light, 4.5:1; `#A69D90` dark, 6.6:1), still well below `--color-ink` so user notes remain the higher-contrast layer. `--color-grey` is unchanged for captions.

- [x] **MTG-1** Live transcript comes back empty after navigating to Library and back, but a refresh restores it
  <details>
  <summary>Details</summary>

  Root-caused. The seed-on-remount machinery already exists and is correct; a token race wipes it immediately after it runs.

  `useTranscriptWs` has two effects, in this declaration order (`web/src/lib/ws.ts:209` and `:224`):

  ```ts
  // 1. seed from persisted history, guarded to fire ONCE per meetingId
  if (seededMeetingIdRef.current === meetingId) return;
  if (!seedHistory || seedHistory.length === 0) return;
  seededMeetingIdRef.current = meetingId;
  setEvents((prev) => [...seedHistory, ...prev]);

  // 2. open the WS
  if (!meetingId || !token) {
    setEvents([]);          // <- ws.ts:232
    ...
  }
  ```

  `token` is fetched asynchronously in `Meeting.tsx` (`ensureSessionToken().then(...)`), so it is `null` on the first render of every mount. Effect 2 therefore always runs its `setEvents([])` branch first time through.

  Whether that matters depends on whether `seedHistory` was ready on that same first render, and that is exactly what differs between the two paths:

  - **Client-side nav (broken).** React Query's cache is warm from the Library, so `meetingRow.transcript_json` is available on the very first render. Effect 1 seeds *and latches* `seededMeetingIdRef`. Effect 2 then runs and clears `events`. When `token` resolves a tick later, only effect 2 re-runs; effect 1's deps (`meetingId`, `seedHistory`) are unchanged, and the latched ref would short-circuit it anyway. The seed is gone for good and the dock only fills with lines that arrive after connect - the lone `Me 00:00:00 Thank you.` in the second screenshot.
  - **Hard refresh (works).** The cache is cold, so `seedHistory` is `undefined` on first render and effect 1 returns early *without* latching the ref. Effect 2 clears an already-empty list. The query resolves later, by which point the token has too, effect 1 finally fires, and the history lands.

  So the bug is not "seeding is missing", it is "seeding is destroyed by a clear that fires for the wrong reason". The clear exists to stop a previous meeting's lines bleeding into the next one, which is a `meetingId` concern, not a `token` concern. Fix: move `setEvents([])` into its own effect keyed on `meetingId` alone, and let the token gate only skip connecting. Guard against reintroducing it by asserting the ordering directly - mount the hook with `seedHistory` populated and `token` null, then supply the token, and assert the seeded events survive.

  Worth checking `MeetingPost` for the same shape while in here; it takes the same token-after-mount path.

  Live meeting before navigating away:
  ![live dock populated](attachments/2026-08-30-live-dock-before-nav.png)

  Back from Library, dock empty except for lines that arrived after reconnect:
  ![live dock empty after client-side nav](attachments/2026-08-30-live-dock-empty-after-nav.png)

  Same meeting, still recording, after a hard refresh:
  ![live dock restored by refresh](attachments/2026-08-30-live-dock-restored-by-refresh.png)

  Fixed: the clear moved out of the connect effect and into the seed effect, keyed on `meetingId` alone (`web/src/lib/ws.ts`), so a token arriving after mount no longer wipes the seed. Covered by a unit test on the effect ordering and by `web/e2e/live-transcript-history.spec.ts`, which drives the real Library round trip.
  </details>

- [x] **MTG-2** First "End meeting" click is a no-op while still recording; it takes two clicks to reach enhance
  <details>
  <summary>Details</summary>

  Root-caused, one-line fix. `MeetingPost` bounces straight back to the live route whenever the cached active-recording poll still names this meeting:

  ```tsx
  // web/src/routes/MeetingPost.tsx:578
  if (activeRecording.data?.id === meetingId) {
    return <Navigate to={`/meeting/${meetingId}`} replace />;
  }
  ```

  That guard exists for deep links, refresh, and the back button landing on the frozen post view for a meeting that is genuinely still recording. It cannot tell those apart from a legitimate `endMeeting` navigation, because the value it reads is stale by up to five seconds: `useActiveRecording` (`web/src/lib/api/meetings.ts:347`) polls `/api/meetings/active` on `refetchInterval: 5_000`, and `stopRecording` (`web/src/routes/Meeting.tsx:295`) invalidates only `meetingKey(meetingId)` - never `activeRecordingKey`.

  So the sequence is: `endMeeting` flushes notes, awaits `stopRecording`, navigates to `/post`, `MeetingPost` mounts against a cache that still says "recording", and `<Navigate replace>` sends the user right back. Nothing errors, nothing logs, the button just flashes.

  Both workarounds in the report follow from the same cause. A second click works because the 5 s poll has returned `null` by then; "Stop recording, then End meeting" works because the gap between the two clicks covers the staleness.

  Fix in `stopRecording`, not in the guard, so every caller is covered: `qc.setQueryData(activeRecordingKey, null)` (or invalidate it) alongside the existing `meetingKey` invalidation. Setting it directly beats invalidating, since invalidation still races the in-flight refetch. Regression test: stop, navigate to `/post` with the active-recording cache still populated, assert the route renders rather than redirecting.
  </details>

  **Landed.** `stopRecording` now writes `null` into `activeRecordingKey` itself rather than waiting for the poll (`web/src/routes/Meeting.tsx:322`). Regression test in `Meeting.test.tsx` seeds the cache the way the poll would, ends the meeting, and asserts the cache is cleared; it fails with `expected { id: 'meeting-3', … } to be null` if the line is removed.


- [x] **MTG-3** Black "your notes" vs grey AI blocks in the enhanced summary is confusing and misapplies when notes are empty
  <details>
  <summary>Details</summary>

  The post-meeting enhanced summary color-tints each block by source: `Source::User` blocks render black ("your notes"), `Source::AiGrey` blocks render grey with a transcript deep-link (`crates/yogurt-notes/src/render.rs` wraps them in `<span data-ai-grey data-ts="N">`).
  When the user's notes are empty, the expectation is that everything in the enhanced AI summary is grey AI output.
  Observed instead: the whole summary reads as black "your notes", so the grey/black distinction feels broken or inverted.

  Two things to do:
  1. Clarify and document how the black-vs-grey split is actually supposed to work (what makes a block `User` vs `AiGrey` - see `crates/yogurt-notes/src/diff.rs` for the user/AI merge rules), and confirm the intended mapping against the frontend styling.
  2. Fix the mismatch: when the user has no notes (`user_notes` empty), the merged doc should come out all `AiGrey` (all grey) - if it's rendering black instead, find where blocks are being tagged `Source::User` when there was no user input to attribute to them.

  Verify E2E: record a meeting with no notes typed, enhance, and confirm every block in the summary is grey; then type notes and confirm those specific blocks render black while AI additions stay grey.

  Resolved 2026-08-30. Two root causes:
  1. Frontend: the `aiGrey` promote-on-edit plugin (`web/src/editor/marks/aiGrey.ts`) treated TipTap's programmatic `setContent(html, false)` replace as one giant user insertion and stripped the grey mark from the entire document on every post-view load, Re-enhance, and tab switch. It now skips transactions carrying TipTap's `preventUpdate` meta.
  2. Backend: `crates/yogurt-notes/src/render.rs` never marked AI headings (and h1/h3 fell through to the paragraph branch), so with empty notes the inferred headings always read as ink "your notes". AI headings are now wrapped in `<span data-ai-grey>` (no deep-link).
  Contract, as documented in `render.rs`: a block is black only when `diff::merge` finds its text in the user's own notes; everything else the LLM produced is grey, headings included.
  </details>

- [x] **MTG-4** Add an "enhanced with" pill next to the existing STT pill on meeting headers
  <details>
  <summary>Details</summary>

  `MeetingMetaPills` (`web/src/components/MeetingMetaPills.tsx`) already shows a `[Local · small.en]` / `[Cloud · nova-2]` pill for the STT engine, stamped at recording start.
  Mirror that for the LLM: when a meeting has been enhanced (or is in the middle of being enhanced), surface an `[Enhanced with · gpt-5-mini]` (or `Local · llama-3` / etc.) pill in the same row so users know which model fused their notes with the transcript.

  Shape to match the existing pill: `parseSttEngine`-style splitter, `Local` (matcha) vs `Cloud` (blue) tones, `Sparkles`/`Cloud`/`HardDrive` lucide glyph, neutral when no LLM ran yet.

  Backend: stamp `llm_enhancement` on the meeting row when enhance kicks off (parallel to `stt_engine` at recording start) - something like `"local · qwen2.5-7b"` or `"cloud · claude-haiku-4-5"`. Persist the same shape used today for STT so the pill code can share parsing.
  Render the pill only after enhance has actually run (or is in flight) - a meeting that was recorded but never enhanced should show only the STT pill, not a guessed LLM pill.
  Library card (`MeetingCard`) already reuses `MeetingMetaPills` - it should pick this up for free once the row carries the new column.

  Verify E2E: record → enhance locally with a known model → confirm the pill appears in the post-meeting header, the library card, and during a re-enhance with a different model the pill updates to the new one.

  Done 2026-08-29.
  Reused the existing `meetings.llm_model` column (V008, bare model name stamped by enhance.rs) instead of a new `llm_enhancement` column, and skipped the local/cloud split: the LLM is always a BYO OpenAI-compatible endpoint, so the app has no local-vs-cloud signal to key a tone on.
  `LlmPill` (`Sparkles` glyph, blueberry tone, `Enhanced · <model>`) sits next to `EnginePill` in the live header, post-meeting header, and library card; `POST /enhance` now returns `llm_model` so the post-meeting pill follows a re-enhance without a refetch.
  Verified in the browser against a real MiniMax enhance; a failed enhance keeps the previous stamp.
  </details>

- [x] **MTG-5** LLM pill missing on a live meeting (STT pill shows, LLM pill doesn't)
  <details>
  <summary>Details</summary>

  Starting a new meeting showed the STT pill but no LLM pill.
  Not a bug in the pill: `stt_engine` is stamped on the row at recording start (`routes.rs::start_meeting`), while `llm_model` is only stamped by `enhance.rs` after a successful enhance, so a live meeting has nothing to render and `LlmPill` correctly refuses to guess.

  Fixed 2026-08-30 in the live header only (`web/src/routes/Meeting.tsx`): when the row has no `llm_model`, fall back to the active provider's model from the already-cached `useSettings()`, mirroring the existing `stt_engine ?? activeRecording.stt` fallback.
  `MeetingMetaPills` grew an `llmPending` flag that flips the pill's tooltip to "Will enhance with <model>" so a pre-enhance pill doesn't claim the meeting was already enhanced.
  Post-meeting header and library card are untouched - a stored meeting that was never enhanced still shows no pill.
  Verified E2E against the running backend: "+ New meeting" now renders `[Local · large-v3-turbo] [MiniMax-M3]` while recording.
  </details>

- [x] **MTG-6** Short / empty post-meeting meetings should say "too short" and return to the library
  <details>
  <summary>Details</summary>

  When a meeting ends but has nothing meaningful to transcribe or enhance (very short duration, mostly silence, audio captured but no usable content), the post-meeting view runs enhance on near-empty input today.
  Detect this case and surface a clear "Meeting too short" state instead of producing augmented notes from nothing.
  Auto-return to the library (home screen) so the user isn't left in a blank post-meeting view they have to navigate out of manually.
  </details>

- [x] **MTG-7** Delete-confirm check should overlay, not shift the post-meeting topbar
  <details>
  <summary>Details</summary>

  In the post-meeting view, clicking the trashcan button shows the confirm checkmark inline, which then pushes the Enhance button and the rest of the top bar around (format/visual layout changes mid-click).
  Make the confirm step overlay-anchored under the trashcan (floating popover) instead of taking up flow space, so the topbar stays put.

  Visual evidence:
  ![post-meeting delete confirm with check](attachments/2026-08-28-delete-confirm-check-appears.png)
  ![post-meeting delete confirm shifts the topbar](attachments/2026-08-28-delete-confirm-shifts-topbar.png)
  </details>

- [x] **MTG-8** Live transcript duplicates when audio plays through the machine
  <details>
  <summary>Details</summary>

  During a live meeting, audio played on the Mac (system-audio playback, video with speech, etc.) shows up in the live transcript as duplicates rather than once.
  Root cause likely lives between `crates/yogurt-audio`'s mic and ScreenCaptureKit channels (acoustic echo into the mic plus digital capture, or an internal double-publish) - investigate and de-dupe.

  Visual evidence:
  ![transcript duplicates on machine audio](attachments/2026-08-28-transcript-duplicates-on-machine-audio.png)
  </details>

- [x] **MTG-9** Make a meeting's summary/transcript discoverable by an agent from just its URL
  <details>
  <summary>Details</summary>

  Richard wants to hand an agent (Claude Code or otherwise) a link like `http://127.0.0.1:7878/meeting/<id>/post` and have it know where to look: pull the summary first since it's cheap, then fetch the full transcript only if it needs more detail.

  Today that requires knowing the internal shape - `GET /api/meetings/{id}` for the row (`enriched_md` is the AI summary, `transcript_json` the raw transcript) or `GET /api/meetings/{id}/markdown` for the on-disk export - and both sit behind `routes::require_session_token`.

  No auth story needed for this, though: everything here is local reads against `localhost:7878` / `~/.yogurt/`, nothing leaves the machine, so this is a read tool, not an API client. Two ways to build it, both skip the session token entirely:

  1. A skill (`.claude/skills/`) that knows the URL pattern, extracts the meeting ID, and reads the on-disk markdown export directly - already positioned as the "grep-able source of truth" per the doc comment in `crates/yogurt-server/src/api/meetings.rs:16-22` - falling back to `transcript_json` in SQLite for the raw transcript when the markdown summary isn't enough.
  2. A `yogurt meeting <id> --summary` / `--transcript` CLI subcommand doing the same lookup, useful for non-Claude agents too.

  Either way: parse the ID out of the URL, summary first, transcript only on request.
  </details>

  Done 2026-08-31. No server code: every mechanism the ticket asks for already existed, so this was a discoverability fix in the two places an agent actually looks.

  The "summary first, cheap" endpoint is `GET /:id/markdown` - it serves the canonical `~/.yogurt/notes/*.md` bytes, whose body is `enriched_md ?? notes_md` and carries no transcript. Measured on a real meeting: 849 bytes against 7,864 for `GET /api/meetings/:id`, which bundles the summary and the raw transcript in one payload and so cannot answer "summary only" at all. The transcript stays on the row and gets filtered out client-side with `jq`, so only the transcript reaches the agent's context.

  Option 2 (a `yogurt meeting <id>` subcommand) was not built. It would be new Rust re-deriving what one `curl` already returns, and `yogurt-cli` has only `start` and `doctor` today. Worth revisiting only for a non-Claude agent that cannot shell out to `curl`.

  Two things the scoping turned up that the entry did not anticipate:
  - The markdown export is wrapped in `<span data-ai-grey …>` tags from the MTG-3 grey-tinting renderer. They mean nothing outside the browser and roughly double the byte count, so the documented recipe pipes through `sed -E 's/<[^>]+>//g'`, which leaves the `↳ mm:ss` transcript deep-links readable.
  - The on-disk fallback must match the front-matter `id:`, not the filename. Meeting ids are UUIDv7 and therefore time-ordered, so the `-<id6>` suffix in `<date>-<slug>-<id6>.md` is not unique - three files in the local notes dir share `01a05a`.

  Landed as: `docs/AI-INTEGRATION.md` "Read what got captured" rewritten around URL -> id parsing and summary-first ordering, and `.claude/skills/yogurt-control/SKILL.md` extended to cover reading (its description now triggers on a pasted meeting URL, not just start/stop). Both take the port from the URL rather than hardcoding 7878, since `just dev` moves the backend for a second worktree (CLI-3).

  `README.md` gained an "Ask an agent about a meeting" section, because the skill lives at `.claude/skills/` and yogurt ships as a brew binary - so the people MTG-9 was written for never clone the repo and would never see it.

  Install is `npx skills add jarvisrchen/yogurt --skill yogurt-control --global` (`skills`, from `vercel-labs/skills`). This needs **no repo changes at all**: `.claude/skills` is already in that CLI's `AGENT_PROJECT_SKILL_DIRS`, so it discovers yogurt's skills straight out of the existing layout with no manifest. Verified against the real repo before writing the docs - `--list` on `main` found both `release` and `yogurt-control`, and a sandboxed-`HOME` install put one copy at `~/.agents/skills/yogurt-control` with `~/.claude/skills/yogurt-control` symlinked at it, so all 77 supported agents share a single source of truth.

  The README pins `--skill yogurt-control` on purpose: the repo also ships the contributor-only `release` skill (tags a version, merges the Homebrew tap PR), and an unpinned install would hand every yogurt user a release-cutting skill they cannot use. A hand `curl` into `~/.claude/skills/` or `~/.codex/skills/` is kept as the no-Node fallback; Codex uses the identical `SKILL.md` frontmatter, so it is one file for both, and `curl -fsSL -o` exits non-zero and writes nothing on a 404 so a bad URL cannot half-install a broken skill.

  Deliberately not built: per-agent plugin manifests. Claude Code reads `.claude-plugin/marketplace.json` and Codex reads `.agents/plugins/marketplace.json` (different schema), and Cursor/Windsurf/Gemini/opencode each want their own wrapper - ponytail ships eight such copies of one skill's text. The `skills` CLI makes all of that unnecessary here.


- [x] **AUD-1** Live partials shrink mid-sentence on local STT, and never appear at all for system audio
  <details>
  <summary>Details</summary>

  Two symptoms, one cause each, both in the local whisper.cpp partial ticker (`crates/yogurt-stt/src/whisper_local.rs:261`). Deepgram is not involved; `ts_ms 00:00:00` on the in-flight line in the first screenshot is the local ticker's hardcoded `ts_ms: 0`, which is how you tell which engine produced a partial.

  **Partials shrink as you keep talking.** The ticker decodes a *rolling 5-second* buffer:

  ```rust
  // crates/yogurt-stt/src/whisper_local.rs:348
  let max = 16_000 * 5;
  if buf.len() > max {
      let excess = buf.len() - max;
      buf.drain(..excess);
  }
  ```

  Every 1 s tick re-decodes only the last 5 s of audio and emits the result as a whole-line replacement, so once an utterance passes five seconds the earliest words scroll out of the window and the displayed partial gets *shorter*. It reads as the transcript eating itself. The final is unaffected because it comes from a different path - the VAD segmenter, good for up to `MAX_SEGMENT_MS = 25_000` (`crates/yogurt-stt/src/vad.rs:63`) - which is why the full sentence reappears the moment you pause.

  Deepgram does not have this bug: `fold_deepgram_event` accumulates window-finals in `UtteranceState.buf` and emits `join_words(&st.buf, &transcript)`, so its partials only ever grow. The local ticker keeps no equivalent buffer.

  Fix direction: make the partial cumulative within the current VAD segment rather than a fixed rolling window - clear `partial_buf` on segment emit and let it grow to `MAX_SEGMENT_MS` - and cap decode cost by re-decoding at a coarser interval as the buffer grows, rather than by dropping audio. Confirm the decode stays inside the 1 s tick budget at 25 s of audio before committing to it; if it does not, the fallback is to keep the rolling window but emit an append-only delta rather than replacing the line.

  ![partial wipes mid-utterance](attachments/2026-08-30-live-transcript-partial-wipe.png)
  ![the same utterance, complete, once the final lands](attachments/2026-08-30-live-transcript-final-catchup.png)

  **No partials at all on headphones.** The ticker is mic-only by design: *"Skipped for system audio in v1 to halve whisper.cpp pressure - PRD §13 only requires the still-listening indicator on the user's own voice."* `Channel::System` therefore has no partial path and only ever renders VAD-segment finals, which is the reported "shows in chunks after each sentence".

  This also explains why the report says the bug only happens on the Mac speaker. On speakers the mic re-captures the speaker output, so the meeting audio lands on `Channel::Mic` *as well* and picks up the ticker - visible in the first screenshot as `Me` and `Them` carrying identical text at `00:01:51`. On headphones there is no bleed, the far end exists only on `Channel::System`, and the streaming indicator disappears. The mystery in the report ("I don't know how it's getting the audio at all") is just system loopback: SCK capture is independent of the output device.

  Decide whether the v1 mic-only tradeoff still holds now that the asymmetry is user-visible. Enabling the ticker for `Channel::System` doubles whisper.cpp pressure, so it needs a perf spike on the smaller models first. The echo-bleed duplicate lines are a separate issue - worth its own entry if it bothers you beyond this bug.

  Done 2026-08-31.
  Both symptoms were one cause: the ticker kept its own rolling 5 s window over raw mic audio, parallel to the utterance buffer `Segmenter` already maintains.
  It now reads `Segmenter::pending()` instead - the in-flight utterance, cleared on segment emit, bounded by `MAX_SEGMENT_MS` - so "partials only ever grow" is structural rather than a rule the ticker has to follow, and the mic-only wiring disappeared with the window.
  System partials are on: the ticker round-robins the two channels and decodes only the one with an utterance in flight, so whisper.cpp pressure is unchanged at one greedy decode per tick rather than doubled. Only the refresh rate gives, and only while both people talk at once (2 s per side instead of 1 s).
  The perf spike the entry asked for is committed as `crates/yogurt-stt/examples/partial_decode_cost.rs`: on large-v3-turbo a full 25 s buffer decodes in ~585 ms, against a 1 s tick, because whisper.cpp pads every input to a 30 s mel window and the encoder cost therefore does not scale with buffer length.
  Partials also carry the utterance's real `start_ms` now, matching what the deepgram adapter already did, so the in-flight line stops rendering as `00:00:00` and the timestamp no longer jumps when the final replaces it.
  Design: `docs/.planning/aud1-live-partials-design.md`. Verified before/after by driving the real `WhisperLocal::start` with 45 s of speech on both channels: on `main` the partial peaked at 95 chars then shrank to 60 as leading words fell out of the window, with no system partials at all; after, both channels grow monotonically to 400+ chars and reset cleanly on each final.
  </details>

- [x] **AUD-4** Ship a local STT model with the release instead of downloading from HuggingFace on first run
  <details>
  <summary>Details</summary>

  Every entry in `REGISTRY` (`crates/yogurt-stt/src/models.rs`) pointed at `huggingface.co/ggerganov/whisper.cpp`, so first-run local STT needed HF reachable - a locked-down machine could install yogurt and still not transcribe.

  Licensing was not the blocker: the HF repo's card reports `license: mit` and the upstream OpenAI weights are MIT, so redistribution is fine (attribution lives in `THIRD_PARTY_LICENSES.md`, hand-written since model weights aren't crates).

  ### Decisions

  - **Mirror `small.en` first, not `tiny.en`.** The shipped default is `small.en` (`V005__stt_provider_model.sql` seeds it, `settings.rs` falls back to it).
  - **One immutable `models-v1` release tag, not a per-release asset.** The bytes are pinned by SHA256 (`download_to` hard-fails on mismatch), so per-tag URLs would buy nothing the hash doesn't already guarantee. A tag that never moves keeps old binaries resolving.
  - **Delivery is a companion Homebrew formula, not a bigger main formula.** `brew install yogurt` stays 11.7 MB for cloud-STT users; opt-in is a second formula per model, `brew install jarvisrchen/yogurt/yogurt-model-<name>`.
  - **HuggingFace stays as a fallback, ordered second.** The mirror is a mirror, not a new source of truth.
  - **`large-v3` is not mirrored.** At 2.88 GiB it's over GitHub's 2 GiB release asset cap; the other four (tiny.en, small.en, medium.en, large-v3-turbo) all fit.

  Done 2026-08-31.
  All three remaining pieces landed:
  - `./scripts/publish-model-mirror.sh` uploaded the four mirrorable models to the `models-v1` GitHub release, each verified against the `models.rs`-pinned SHA256 before publishing.
  - The tap (`jarvisrchen/homebrew-yogurt` PR #4) gained the four `yogurt-model-*` formulae via `./scripts/render-model-formula.sh <name>`, each writing its own `.sha256` sidecar at install time so `list_models` never re-hashes a Homebrew-owned copy.
  - `ModelSpec` gained `mirror_url: Option<&'static str>`, pinned to the GitHub release asset for the four mirrorable models (`None` for `large-v3`). `download()` now tries `mirror_url` before `url`, so the in-app download button also stops depending on HuggingFace reachability. Fallback-order behavior is covered by unit tests against an in-process mock server (`crates/yogurt-stt/src/models.rs`, `mod tests`).

  Not solved by any of this: air-gapped machines, and `large-v3` on an HF-blocked network. Both already have a workaround - `is_downloaded_at` only checks the hash, so a model copied into `~/.yogurt/models/` by any means is accepted.

  Side finding from scoping: `scripts/refresh-model-hashes.sh` used to download 5.2 GB to learn hashes that HuggingFace serves as text - `GET /<repo>/raw/main/<file>` returns the Git LFS pointer, whose `oid sha256` is the blob's hash, for about 5 KB of traffic instead.
  </details>

- [x] **AUD-5** Add the ability to delete a downloaded local STT model
  <details>
  <summary>Details</summary>

  The Settings → Local whisper.cpp panel lists every model with a `✓` + size when it's already downloaded under `~/.yogurt/models/`, but offers no way to remove one.
  Case in point: `large-v3` is sitting at 3.0 GB on disk and the only way to reclaim that space today is `rm -rf` by hand.
  Add a delete affordance per downloaded chip - probably a trash/X icon on hover, with a confirm step (matching the post-meeting trashcan UX) so a stray click doesn't nuke a 1.6 GB model.
  Backend: new `DELETE /api/local-stt/models/:name` that removes the directory + SHA-pinned file, returns the freed bytes, and rejects if the model is the currently active one (force the user to switch first).
  Frontend: invalidate the model list query so the chip flips back to "download" state and the size badge clears.

  Done 2026-08-29.
  Backend: the existing `DELETE /api/stt/models/{name}` now 409s when the model is the active local one, does the fs work on the blocking pool, and returns `200 {freed_bytes}` (idempotent, 0 if already gone).
  Frontend: trash icon next to every downloaded, non-active pill in `ModelPicker`, inline `Delete? / Cancel` confirm that auto-reverts after 3s, and a transient `Deleted <name> - freed <size>` line under the picker. Verified E2E in a sandboxed `$HOME` against a real `small.en` copy.
  </details>

- [x] **AUD-6** Find a way to mute just the mic during a live meeting so it's excluded from the transcript, while system audio keeps recording
  <details>
  <summary>Details</summary>

  Richard wants to step away and talk to someone mid-meeting without that conversation landing in the transcript, without having to stop and restart the recording.
  This is mic-only: he'd already be muted in the meeting app itself, so the other side's audio (`Channel::System`) should keep capturing normally the whole time - only his own mic (`Channel::Mic`) needs to stop feeding the pipeline while he's talking to someone off to the side.
  Needs a pause/mute control reachable during an active recording that stops feeding mic audio into the pipeline (rather than just muting playback), and a way to see at a glance that mic capture is currently paused.

  Design: [docs/.planning/aud6-mic-mute-design.md](.planning/aud6-mic-mute-design.md)

  Done 2026-09-01.
  `FrameChunker` (`crates/yogurt-audio/src/mic.rs`) gained a shared `Arc<AtomicBool>` mute flag - `feed` still drains its buffer and advances the timestamp clock while muted, it just skips the broadcast, so unmuting produces no timestamp jump. `MicCapture::set_muted` / `AudioStream::set_mic_muted` expose it; mute resets to unmuted across a mid-mute device switch (documented simplification, not carried through).
  Server: new `AudioCommand::SetMicMuted`, serviced by the same `run_capture_control_loop` as the existing device hot-swap (now one dispatch closure, since both variants need `&mut stream`). `Meeting.mic_muted` mirrors `stt_engine`; `Registry::set_mic_muted` mirrors `switch_mic_device` exactly (lookup, 5s-timeout send, error mapping).
  Route: `POST /api/meetings/:id/mic-muted`, and `GET /api/meetings/active` now carries `mic_muted` so a reload or second tab reflects the true state off the existing 5s poll - no new WS plumbing.
  Frontend: `MicMuteToggle` is a big, full-width button between the mic picker and the notes card, always mounted (disabled with an explanatory tooltip while not recording, rather than disappearing) since it's a core in-meeting action. `M` hotkey (no modifier, `ignoreWhenTyping`) via the existing `useKeyboardShortcut` hook. `secondary`/new `warn` `<Button>` variant - solid strawberry, white text, matching the app's existing warn-tone pattern - rather than an icon-only toolbar chip.
  Design mockups: [docs/.lavish/aud6-mic-mute.html](.lavish/aud6-mic-mute.html) (also reviewed via Open Design against the app's real tokens before implementation).
  </details>

- [x] **CLI-3** `just dev` should pick a free port pair so two worktrees can run at once
  <details>
  <summary>Details</summary>

  Everything about the dev loop assumed one instance: `vite.config.ts` hard-coded `5173` (server port, HMR client port, and the `/api` + `/ws` proxy targets to `7878`), `run-frontend.sh` refused to start on any other port ("the backend proxy is hard-coded to 5173"), and `allowed_origins` in `ws.rs` hard-coded the `5173` dev origin. Running a branch therefore meant stopping whatever was already running, which is exactly backwards when the point of a worktree is to have two branches side by side.

  Only the *port* was hard-coded, not the plumbing - the binary already read `YOGURT_VITE_BASE` for its proxy target and already took `--port`. So the fix is to thread one pair of numbers through:

  - `just dev` resolves both ports up front (default policy `next` rather than `ask`: the usual reason `:7878` is busy is another worktree) and exports `YOGURT_VITE_PORT`, `YOGURT_BACKEND_PORT`, `YOGURT_VITE_BASE` for both scripts.
  - `vite.config.ts` reads both, for its own port, its HMR client port, and its `/api` + `/ws` proxy targets.
  - `run-frontend.sh` lost the 5173-only bail; `run-backend.sh` derives `YOGURT_VITE_BASE` from the port when it is not set and prints the target it will proxy to.
  - `allowed_origins` follows Vite's actual port, so the WS handshake works on the moved pair and still rejects the *other* worktree's origin.
  - Playwright got its own port (5199) - `reuseExistingServer` is on locally, so sharing 5173 meant a run could silently pass against whatever worktree was serving there.

  Verified with two instances at once: an existing `just dev` on 5173/7878, then `just dev` from a second worktree, which took 5174/7879 with no prompt. Both served their SPA, and the WS handshake against 7879 returned 101 for `localhost:7879` and `localhost:5174`, 403 for `localhost:5173` (the other worktree) and for a junk origin.

  Not addressed: both instances still share `~/.yogurt/` (one SQLite, one keys file, one models dir), so they see the same meetings and only one should record at a time. A `YOGURT_DATA_DIR` override is the fix if that ever bites.
  </details>

- [x] **CLI-2** Audio lag warnings spam the terminal on a normal meeting; move them behind `RUST_LOG`
  <details>
  <summary>Details</summary>

  Left open by CLI-1, which kept them at `warn!` on the theory that dropped audio is signal. In practice every meeting opens with a burst of them and there is nothing to act on:

  ```
  WARN whisper_rs::ggml_logging_hook: ggml_metal_device_init: tensor API disabled for pre-M5 and pre-A19 devices
  WARN yogurt_server::meetings: audio adapter: system lagged n=220
  WARN yogurt_server::meetings: audio adapter: mic lagged n=218
  WARN yogurt_stt::whisper_local: whisper_local audio rx lagged; dropping n=131
  ... 10 more in the same millisecond
  ```

  All of it lands in one burst while the STT engine loads its model and the ring buffers catch up, then nothing for the rest of the meeting.

  Two changes:

  1. The three lag sites (`audio adapter: mic lagged`, `audio adapter: system lagged`, `whisper_local audio rx lagged`) are `debug!` now, so `RUST_LOG=yogurt=debug` still gets them. The transcript relay and persistence lag warnings keep their level - those are dropped *transcript*, not dropped audio frames, and they do not fire routinely.
  2. Default filter is `yogurt=info,whisper_rs=error` (was `whisper_rs=warn`). The only whisper WARN that ever shows is ggml's `tensor API disabled for pre-M5 and pre-A19 devices`, a hardware-capability note on every model load. Decode failures are ERROR and still surface, plus the UI's `stt_error` banner.

  Verified against a real local-STT meeting (`large-v3-turbo`, mic + system capture, 6 transcript segments): start, record, stop is 6 lines and 0 warnings. The burst itself is load-dependent and did not recur in the verification run (warm model), so what the run proves is the level swap and a quiet terminal, not the burst being caught in the act.

  If lag ever turns sustained rather than a startup artifact, the fix is a periodic count, not a line per event - noted at the call site.
  </details>

- [x] **CLI-1** Quiet the terminal during a live meeting; keep lifecycle lines and errors, drop the rest
  <details>
  <summary>Details</summary>

  `yogurt start` should read as a status line, not a firehose. The wanted set is roughly: server up and its URL, meeting started, meeting stopped, enhance started and finished, and anything that actually went wrong. Everything else belongs behind `RUST_LOG`.

  While in here, the default filter is `"yogurt=info,yogurt_server=info"` (`crates/yogurt-cli/src/main.rs:79`). `EnvFilter` matches targets by plain `starts_with` (tracing-subscriber 0.3.23, `filter/env/directive.rs:246`), so `yogurt=info` already covers `yogurt_server`, `yogurt_audio`, `yogurt_stt`, and every other `yogurt_*` crate. The second half is redundant and reads as if it were doing something. Drop it, or replace both with per-crate levels once the noisy targets are known.

  Careful not to overshoot: the point is a quiet terminal that still proves the thing is working, so lifecycle lines stay at `info!` and everything currently at `warn!`/`error!` keeps its level. Demote to `debug!` rather than delete, so `RUST_LOG=yogurt=debug` still gets you the detail when something needs diagnosing.
  
  **Landed.** Measured before fixing: whisper.cpp and ggml write 45 lines to stderr on model load and another 17 on *every* decode, through ggml's own log callback - which `params.set_print_*(false)` does not control, because those only govern whisper's transcript printing. With the partial ticker decoding once a second, a three-minute local-STT meeting emitted roughly 3,000 lines.

  Three changes:

  1. `whisper_rs::install_logging_hooks()` in `WhisperLocal::load`, plus the `tracing_backend` feature on the `whisper-rs` dep, so the C-side stream goes into `tracing` instead of straight to stderr.
  2. Default filter is now `yogurt=info,whisper_rs=warn` - ggml's INFO banners are dropped, its warnings and errors still surface, and `RUST_LOG=whisper_rs=debug` brings the detail back.
  3. Added the lifecycle lines that were missing entirely (`meeting_started` with provider and model, `meeting_stopped`, `enhance_complete` with model and elapsed, `enhance_skipped_too_short`) and demoted per-socket and teardown chatter in `meetings.rs`/`ws.rs` to `debug!`.

  Verified end to end against a real local-STT meeting on `large-v3-turbo`: start, record, stop, enhance now produces 13 lines total.

  Not addressed: the `audio adapter: * lagged` / `whisper_local audio rx lagged` warnings still fire un-throttled. In the verification run they came as one 270 ms burst at startup and nothing for the remaining 2.5 minutes, so they are signal about genuinely dropped audio rather than a firehose. Collapsing them to a periodic count is worth doing if they ever turn sustained.
  </details>

- [x] **LLM-7** Add an OpenCode local-CLI provider preset
  <details>
  <summary>Details</summary>

  Third `adapter: "cli"` preset alongside Claude Code and Cursor Agent, defaulting to `minimax-coding-plan/MiniMax-M3`. Same shape as the other two - no base URL, no key, a `Test` that runs a real completion, and a MODEL field whose `Refresh` asks the binary. `opencode models` returns 370 ids on a live account (every model of every provider the user has configured), so the preset ships one static suggestion and Refresh does the rest.

  Two things made this more than an entry in `PRESETS`.

  `opencode` does not speak the `-p --output-format json` shape. `opencode run --format json` prints JSON LINES - the answer as `type: "text"` parts, thinking as `type: "reasoning"`, a failure as `type: "error"` - so `interpret_output` now dispatches on `CliProgram` rather than assuming one object with `result`/`is_error`. Text is keyed by `part.id` so a future switch to streamed snapshots can't duplicate the answer's prefix; `reasoning` is dropped, since the model's scratchpad is not its reply. `parse_model_list` handles both catalog formats (`<id> - <label>` from cursor-agent, bare `<provider>/<model>` from opencode) with one pass.

  The containment is the part worth remembering. `--auto` is opencode's `--dangerously-skip-permissions` and is the flag a user reaches for to make `run` non-interactive - it must never be passed here. Beyond that, a bare `opencode run` loads the user's global config: verified live, the model was offered `google-workspace_send_gmail_message`, `google-workspace_search_gmail_messages` and Drive writes, which is a working exfiltration path for a transcript that says "email yourself the following". `OPENCODE_CONFIG_CONTENT={}` does NOT close it. `OPENCODE_PERMISSION={"*":"deny"}` does, by removing every tool from the model's schema - 35k input tokens down to 2.2k on the same prompt, and an order to run `echo` comes back as prose about running it rather than a `tool_use` event. `--pure` drops third-party plugins on top.

  Verified end-to-end against the live binary through the real API: preset advertised, provider cloned, Refresh returns 370 with the default present, `Test` returns `{"ok":true,"model":"cli:opencode:minimax-coding-plan/MiniMax-M3"}` in ~4 s, and a bogus `cli_model` surfaces opencode's own error text rather than an exit code.
  </details>

- [x] **LLM-6** Fetch the model list for a CLI provider instead of shipping one static suggestion
  <details>
  <summary>Details</summary>

  The Cursor Agent CLI provider only ever suggested `auto`, so a paid-plan user had no way to discover the ~200 model ids `--model` actually accepts without leaving Settings and running the CLI themselves. `claude`'s four aliases are the whole catalog and stay static; `cursor-agent` has a `--list-models` flag and needs asking.

  `POST /api/settings/providers/{id}/models` now handles `cli` rows by spawning `<binary> --list-models` and parsing its `<id> - <label>` lines, and the CLI branch of `ProviderCard`/`ProviderRow` renders the shared `ModelSelect` (dropdown + `Refresh`) instead of a bare `ComboBox`, so it gets the same affordance the HTTP rows have had.

  A listing does NOT prove entitlement, and the UI deliberately does not pretend otherwise. Verified live: a free-tier Cursor account lists all 204 ids and then refuses every named one at call time with `ActionRequiredError: Named models unavailable Free plans can only use Auto. Switch to Auto or upgrade plans to continue.` So `Test` stays the only signal for "can this account actually use this model", and its error text already tells the user to go back to `auto`. Pre-filtering the dropdown would have meant spending a real completion per Refresh to learn something `Test` reports for free.

  `claude` has no such flag, so Refresh on it returns 422 ("no model catalog to refresh") - distinct from the 502 a real probe failure (not logged in, binary gone) produces, so the UI can keep showing the static aliases in the first case.
  </details>

- [x] **LLM-5** Enhance times out on CLI providers whose model ignores `--effort low`
  <details>
  <summary>Details</summary>

  Selecting `haiku` as the Claude Code CLI provider's model makes every enhance fail with `llm call exceeded 60s timeout`. `sonnet` on the identical prompt is fine. Reproduced against a real 94 s meeting (`01a05a46-d9d0-78c1-b88f-5269955516c0`), running `crates/yogurt-llm/src/cli.rs`'s exact argv by hand:

  | model | wall | output tokens | thinking tokens | result |
  | --- | ---: | ---: | ---: | ---: |
  | sonnet, `--effort low` | 4.2 s | 126 | 0 | 314 chars |
  | haiku, `--effort low` | 94 s | 10100 | 9983 | 305 chars |
  | haiku, no `--effort` | 77 s | 7550 | 7466 | 211 chars |

  Three separate problems, all of which have to be fixed for this to stop biting:

  **1. `--effort low` is assumed to be sufficient, and silently is not.** `build_args` passes it unconditionally, justified in its own comment as "extraction calls, not reasoning tasks - low effort is faster and cheaper with no real quality loss ... there's no scenario here that benefits from spending more". Sonnet honours it and emits 0 thinking tokens. Haiku ignores it and spends ~10k thinking tokens on a prompt whose answer is 300 characters. Dropping the flag changes nothing (row 3), so the flag is not the lever - and there is no thinking-budget flag on the CLI at all (`claude --help` offers only `--effort` and `--max-budget-usd`).

  **2. The CLI adapter borrows a timeout sized for a different job.** `CLI_TIMEOUT` is `crate::HTTP_TIMEOUT`, and `enhance.rs` separately wraps `llm.stream()` in the same 60 s. For an HTTP provider that budget covers *opening* a stream - headers only - and a long healthy generation then runs on the per-chunk idle timeout instead. But `CliClient::stream` does not stream: it runs `complete()` to completion and replays the whole answer as one delta (`// v1 scope (LLM-4): no --output-format stream-json parsing yet`). So the same 60 s has to cover *full generation*. Even sonnet on a 60-minute meeting already burns 17.6 s of it, so the margin is thin for everyone, not just haiku.

  **3. The failure is illegible.** A model that is merely slow presents as a hang, then a generic "LLM timed out" banner naming no model and offering no remedy. Nothing points at the model choice, which is what made this take a full debugging session to pin down.

  Worth noting the cost inversion, because it undercuts the obvious "just pick the cheap fast model" instinct: haiku here is ~20x slower **and** more expensive than sonnet (10100 output tokens vs 126) for an equivalent answer.

  Done 2026-09-01 (#24).
  The budget moved onto the trait as `LlmClient::response_timeout`, because only the implementation knows what its own await points cost: HTTP keeps the 60 s handshake figure, `CliClient` returns `GENERATION_TIMEOUT` (300 s), and `enhance.rs` asks the client instead of hardcoding a constant for either the open or the per-chunk idle wait.
  `CLI_TIMEOUT` stays as the inner bound on a wedged subprocess, now sized off `GENERATION_TIMEOUT` so it cannot preempt the caller; the dead `LLM_HTTP_TIMEOUT` is gone.
  `--effort low` is still sent - it genuinely helps the models that honour it - but is now documented as advisory rather than relied on as a cap.
  The timeout message names the model and points at Settings, since a timeout here is far more often "this model is slow for this prompt" than "the provider is down".
  Verified end to end against the running app with `haiku` still configured, on the meeting that was failing: HTTP 200 in 1:53 (`llm_model: cli:claude:haiku`), where before it died at 60 s every time.
  Still slow, because nothing available can cap haiku's thinking - but slow now reads as slow instead of broken.
  </details>

- [x] **LLM-1** Route LLM calls through a local agent CLI (`claude -p`, `cursor-agent`) when no API endpoint is reachable
  <details>
  <summary>Details</summary>

  `LlmClient` (`crates/yogurt-llm/src/lib.rs:49`) is already the right seam: two methods, `complete` and `stream`, and the crate doc at line 12 anticipates a second implementation. A `CliClient` that spawns the agent CLI and implements the same trait needs no changes above it.

  It cannot be just another provider row with a different base URL. `OpenAiClient::new` (`crates/yogurt-llm/src/lib.rs:90`) assumes HTTP at `{base_url}/chat/completions`; agent CLIs speak stdin/stdout. Streaming maps onto `--output-format stream-json` on both CLIs, so `stream()` would parse that into `ChatChunk` the way `streaming.rs` parses SSE today.

  Open decisions before scoping:

  - **Terms of service.** Both CLIs authenticate against a coding subscription. Driving one as a general-purpose completion backend for meeting notes is a gray area, and it is the user's account that gets suspended if the read is wrong. Settle this before writing code, not after.
  - **Which problem this actually solves.** Not offline: both CLIs still call the cloud. It solves "corporate egress allows Claude Code but not `api.minimax.io`" and "user has no separate API key to pay for". Real, but narrower than it first sounds. True-offline is already covered by Ollama, which is an OpenAI-compatible base URL and works today with no code at all.
  - **Cost shape.** Process spawn per call, no connection reuse, cold start in the hundreds of ms. Fine for note enhancement at meeting end, likely not for anything per-utterance.
  - **Discovery and failure UX.** Settings would need to detect which CLIs are on `$PATH` and fail legibly when one is installed but not logged in. That failure is a non-zero exit with text on stderr, not an HTTP status, so the Test button plumbing needs a verdict path that is not `test_provider`'s.
  </details>

  Built, then decided against, in #17: implemented as an implicit fallback (`CliFallbackClient` wrapping the active `http` provider, retrying once on a connect-class failure to its `base_url`), but silently rerouting a meeting's real content to a different, unvetted backend on a network hiccup is a behavior change a user should choose, not one that happens to them mid-meeting. Ripped back out of the same PR before merge. The actual need - "let me use a local agent CLI as my LLM" - is covered by LLM-4 instead: explicit-only, the user picks the CLI as their active provider, nothing implicit ever substitutes it in. `yogurt_llm::CliClient` and its containment (`--restricted --strict-mcp-config --disable-slash-commands`, scratch cwd) survive as the mechanism LLM-4 uses.

- [x] **LLM-2** Test button for the API key should stay available on provider cards with saved models
  <details>
  <summary>Details</summary>

  On a provider card where a model is already saved in the MODEL field, the API KEY `Test` button should still be usable as a probe - typing a new draft key and clicking `Test` should run the same connection check it runs on a keyless card.
  If the button currently gets disabled, hidden, or stomped by some save/refresh state when a model is present, fix it so `Test` is reachable from every state a provider card can sit in (no key + no model, stored key + no model, no key + saved model, stored key + saved model).
  Likely suspects: `ApiKeyInput.canTest` (`web/src/components/settings/ApiKeyInput.tsx:83`) only looks at `draft || hasStoredKey` and ignores MODEL state, so the bug is probably upstream - a parent renders/hides `ApiKeyInput` based on MODEL state, or `onSaved` tears down the input before the user gets to test.
  Verify by hand: load a provider card with a saved model, paste a draft key, click `Test`, confirm a `✓` / `✗` line appears and the draft survives the click.

  Cause was upstream of `canTest`, as suspected: `ProviderRow` renders the whole `ApiKeyInput` only while `keying` is true, and `onSaved` flips it back to false - so an already-keyed provider (the one most worth probing) had no reachable `Test` at all.
  Fixed by extracting `TestKeyButton` from `ApiKeyInput` and rendering it next to `Replace key` in the collapsed state, where it tests the stored key.
  </details>

- [x] **LLM-3** Add Google Gemini and DeepSeek as built-in LLM provider presets
  <details>
  <summary>Details</summary>

  Both speak the OpenAI `/chat/completions` shape, so this needed no new client code in `yogurt-llm` - just `Preset` entries plus the matching `ENV_PRESETS` rows (`YOGURT_GEMINI_API_KEY`, `YOGURT_DEEPSEEK_API_KEY`) so keys seed into the Keychain on first run.
  Gemini goes through Google's OpenAI-compatible shim at `https://generativelanguage.googleapis.com/v1beta/openai`, not the native `generativeLanguage` REST shape.
  Neither was strictly blocked before this - "Add provider" already accepts a free-form base URL - so the win is one click instead of a URL nobody remembers.
  While here: `tests/bootstrap.rs` hand-listed the env vars it cleared, so adding a preset broke it only on machines that exported the new key.
  It now clears everything `bootstrap::env_key_vars()` reports.
  </details>

- [x] **LLM-4** Let a user pick a local agent CLI as their LLM provider outright
  <details>
  <summary>Details</summary>

  "I can't use any cloud model at all, I want to explicitly run `claude` (or `cursor-agent`) as my LLM" - explicit selection, not an automatic fallback (that shape was tried as LLM-1 and reverted; see its entry). Landed in #17. Two new provider presets, "Claude Code (local CLI)" and "Cursor Agent (local CLI)", appear in the same Settings → Model provider list as MiniMax/OpenAI/Ollama/etc. and can be set active like any other provider - no cloud provider needs to be configured at all.

  Needed a `providers.adapter` column (`'http' | 'cli'`, migration V009) since the existing table assumed every row was HTTP-shaped (`base_url` + `model` + a stored API key). A `cli` row repurposes `model` to hold the `yogurt_llm::CliProgram` id instead of a model name, leaves `base_url` empty, and never has a key - `llm_openai::resolve` branches on `adapter` and calls `yogurt_llm::CliClient::locate` directly. Settings API (`create_provider`, `test_provider`, `list_provider_models`) and the provider card/row components (`ProviderCard.tsx`, `ProviderRow.tsx`) branch the same way: no BASE URL / API KEY UI for a `cli` row, just a one-line note and a `Test` button reachable with no key.

  A separate `providers.cli_model` column (migration V010) holds the `--model` alias override, since `providers.model` is already spoken for by the `CliProgram` id and is never user-edited. The Settings UI shows a MODEL field for `cli` rows too - a bare `ComboBox` (no live `Refresh`, since there's no `/v1/models` catalog to probe) seeded from the preset's `models` list ("haiku" / "sonnet" / "opus" / "fable" for Claude Code; "auto" for Cursor Agent). Empty `cli_model` means "use the CLI's own default".

  The MODEL picker is gated behind a passing `Test`, not shown unconditionally: a freshly-cloned row (`PresetChip` no longer seeds `cli_model` at all) shows only the note and `Test` until the CLI proves it actually connects, since there's nothing to override on a row that might not even resolve on `$PATH`. Once `Test` comes back `ok`, the picker appears and - if `cli_model` is still empty - gets PATCHed to the preset's `default_cli_model` so it opens on a sane value instead of blank. `!!provider.cli_model` is what keeps the picker visible on every later visit (no extra "was it tested" column needed); picking a different model PATCHes on commit, and re-running `Test` after that tests whatever's currently saved.

  `cursor-agent` hit a "Workspace Trust Required" wall in manual testing, since every call gets a fresh never-before-seen scratch `cwd`. Fixed with `--trust`, not `--yolo`/`--force` (cursor-agent's broad auto-approve-commands flag) - same reasoning as never passing `--dangerously-skip-permissions` to `claude`, since meeting-transcript text is untrusted input. `--sandbox enabled` is the closest documented equivalent to claude's `--restricted`. `claude` additionally gets `--effort low`, unconditional and not user-configurable: enhance/chat are extraction calls, not open-ended reasoning, so the lowest effort setting has no real quality cost.

  Manual browser verification against an isolated `HOME` (never the real `~/.yogurt/`) caught a real bug in `CliClient::run`: `claude --output-format json` writes a structured, actionable error to stdout even on a non-zero exit (confirmed live against a real "not logged in" account - `{"is_error":true,"result":"Not logged in · Please run /login"}`), but the code checked `status.success()` first and only looked at stderr, so the Settings `Test` button showed a useless "claude CLI exited with exit status: 1: " instead of the real reason. Fixed by extracting `interpret_output` (pure, parses stdout as JSON before checking exit status) with a unit-test regression covering exactly this shape.

  Two more issues from live manual testing once both CLIs were actually installed: (1) `CliClient::model_name()` returned a bare `"cli:claude"` regardless of any `--model` override, so the Settings `Test` verdict ("answered as cli:claude") looked identical whether `haiku` or the CLI's own default actually ran - fixed by including the override when set (`"cli:claude:haiku"`), which also makes `meetings.llm_model` more informative. (2) The Cursor Agent preset shipped with an empty `models`/`default_cli_model` (the "unverified against a live binary" caveat from earlier in this entry) - live testing against a free-tier account found `cursor-agent --list-models` returns 50+ account-dependent ids, and naming any of them except `"auto"` fails with `ActionRequiredError: Named models unavailable, Free plans can only use Auto`. Fixed by setting the preset's `models` to `["auto"]` and `default_cli_model` to `"auto"` - the one value confirmed to work on every plan, not a guess at the full catalog.
  </details>

- [x] **DX-6** One release procedure: fix the untap order, archive the stale runbooks, split the release log, exact-version formula assert
  <details>
  <summary>Details</summary>

  The skill and `docs/RELEASING.md` disagree on untap order and both are moot: `brew untap` refuses while a model formula is installed, so upgrade-in-place is the real path.
  `git mv` `scripts/release-checklist.md` and `scripts/homebrew/` to `docs/archive/`; `release.yml` never reads the seed formula.
  Move the log table to `docs/RELEASE-LOG.md` and promote its four buried lessons into "When it goes wrong".
  `release.yml`'s formula test asserts `yogurt <version>` exactly instead of the substring.
  Design: `docs/.planning/agent-workflow.md`, section 4C, C4 and C5.
  </details>

  Landed in #53 (2026-09-02). Both the skill and `docs/RELEASING.md` now smoke-test with `brew upgrade`/`brew reinstall` first and a from-scratch `untap`/`uninstall` cycle only as a fallback. `scripts/release-checklist.md` and `scripts/homebrew/` moved to `docs/archive/` unmodified. The release log table moved to the new `docs/RELEASE-LOG.md`; its four buried lessons (false `strings | comm` check, `brew untap` refusal, re-reading `origin/main`'s sha before tagging, `git log <lasttag>..origin/main` for scope) are now in RELEASING.md's "When it goes wrong". `release.yml`'s tap formula test asserts the exact `yogurt <version>` output; unexercised until the next real tag push since the `tap` job never runs on a dry run. Also recorded the one-time GitHub merge settings in RELEASING.md and fixed two stale "log lives in RELEASING.md" pointers (AGENTS.md, README.md). Left `docs/.planning/agent-workflow.md` and its Lavish companion untouched: they describe the pre-fix state as the rationale for this ticket, not a live doc.

- [x] **DX-2** Split DONE out of `docs/TODO.md`; add `just ticket` (list, show, next, done)
  <details>
  <summary>Details</summary>

  85% of this file is the DONE section, and every agent that reads the backlog pays about 16k tokens for it.
  Move DONE to a flat `docs/TODO-DONE.md` (docs-only PR), then `scripts/ticket.sh` behind a `ticket` recipe: list open items, print one block, allocate the next ID across both files, and `done <ID> --note-file` for the checkoff move.
  Block boundary is the next `- [` or `#` line, never the closing details tag; the scanner skips fenced code (the example above uses a real ID today).
  `--check` runs in `just lint`.
  Design: `docs/.planning/agent-workflow.md`, section 4A, A2.

  Landed across two PRs: #52 (docs-only) moved the DONE section out of docs/TODO.md into a flat docs/TODO-DONE.md, and #54 adds scripts/ticket.sh behind a `just ticket` recipe (list, show <ID>, next <PREFIX>, done <ID> --note-file <path>, --check), wired into `just lint`.
  The scanner is BSD awk/sed only, skips fenced code blocks (the "Referencing attachments" example), and treats the next `- [` or `#` line as the block boundary, never `</details>`.
  Verified by hand: `just ticket` lists 18 open items (grep -c '^- \[ \]' docs/TODO.md reports 19, the extra one is the fenced UI-0 example, correctly excluded); `just ticket DX-2` printed this block; `just ticket next DX` printed DX-11 and `just ticket next MTG` printed MTG-12, both matching the true max+1 by hand; `just ticket --check` passed on the real files; the error paths (unknown ID, missing/empty note file, done on an already-closed ID) all exited non-zero with a descriptive message.
  scripts/tests/ticket_test.sh (24 assertions against a synthetic fixture pair, including a no-details-block ticket, a DONE entry with resolution text after </details>, and a fenced-code decoy ID) is wired into `just lint` alongside `--check`, both under a second.
  `just lint` and `just test` (314 web tests + full cargo workspace) pass clean.
  This checkoff itself was done with `just ticket done DX-2 --note-file <path>`, the E2E test for the tool.
  </details>

- [x] **CLI-4** Give agents a `yogurt ctl` instead of a markdown page of curl recipes
  <details>
  <summary>Details</summary>

  `.claude/skills/yogurt-control/SKILL.md` is ~114 lines teaching an agent to hand-roll `curl` against the REST API, including reading the session token out of `~/.yogurt/session-token` and attaching it as a bearer header.
  That is paid for in tokens on every invocation, re-derived from scratch each time, untestable, and silently drifts whenever the API moves - nothing fails when the markdown goes stale, it just quietly describes a shape that no longer exists.
  The skill also states the gap outright: "yogurt has no CLI to start it headlessly."

  Replace the recipes with subcommands on the binary that already exists.
  `yogurt doctor --json` is the precedent - the machine-readable habit is already there, it just stops at diagnostics.

  ```
  yogurt ctl meeting new|start|stop|show --json
  yogurt ctl detect      # what meeting detection currently sees
  yogurt ctl windows     # on-screen windows + each one's match verdict
  ```

  `yogurt ctl windows` is the one with a concrete origin story.
  MTG-11's detection rules were first "verified" against a window titled to match the rule under test - a loop that could not fail, and did not catch that real Google Meet runs as an installed Chrome app (`com.google.Chrome.app.<hash>`) titled `Google Meet - Meet - <code>`.
  The tool that settles it exists today only as `cargo run -p yogurt-audio --example meeting_windows`, discoverable by reading `crates/yogurt-audio/src/detect.rs`.
  A cargo example nobody can find is not infrastructure; promote it.

  Design the surface for an agent, not a human: `--json` output, `--dry-run` on anything destructive, descriptive errors that say what to do instead, subcommands rather than one flat flag soup.
  Once it exists, `yogurt-control/SKILL.md` shrinks to naming the commands, and the behavior becomes testable in `crates/yogurt-cli`.

  Landed in #55 (2026-09-02). Shipped D1's first slice: `yogurt ctl` (status, meeting list/new/start/stop/show/summary/transcript/enhance, detect [dismiss], windows) plus D5 (`/api/health` gains `version`/`mode`, `just dev` prints `YOGURT_PORT=`).
  Discovery is `--port` / `$YOGURT_PORT` / a scan of 7878-7898; read commands fall back to the local SQLite DB (`source: db`) when no server answers; mutations are idempotent (`start`/`stop` no-op on an already-in-that-state meeting); errors print `error: ... / help: ...` on stdout with exit 1, usage errors exit 2 via clap.
  `ctl windows` replaced the `yogurt-audio/examples/meeting_windows.rs` cargo example (moved its logic into `yogurt_audio::detect::scan_windows`, called from the CLI via `spawn_blocking` - no new dependency).
  Verified: `just lint` and `just test` (cargo + vitest) both clean; `crates/yogurt-cli/tests/ctl_smoke.rs` (5 tests: no-server exit 1, status version/mode, meeting new/show/list/last, bare stop no-op, the --help tree and `status` output never mention or leak a key/token); manually ran `yogurt ctl windows` and `yogurt ctl detect` on this Mac (screen recording granted, one on-screen window, no meeting-looking title, verdict `-`); full E2E by hand via `just dev` in the worktree (`ctl status`, `meeting new/show/list/stop`, `detect`), smoke meeting deleted afterward via `DELETE /api/meetings/<id>`.
  Deviation: `enhance` forwards progress as a `phase: sending` / periodic `phase: waiting` heartbeat on stderr rather than the server's `enhance_progress` WS frames - `tokio-tungstenite` isn't a `yogurt-cli` dependency and the spec caps new dependencies at moving `reqwest`, so this is the documented polling fallback the ticket allows.
  `ctl meeting transcript --follow` polls on the same basis rather than opening a WebSocket, for the same reason.
- [x] **DX-4** CI calls `just`; `lint-web`; Playwright in `just test`; `scripts/check-docs.sh`; control-skill dedupe and `--help` drift test
  <details>
  <summary>Details</summary>

  The justfile becomes the single definition of the gates: `lint` stays fmt plus clippy, `lint-web` is the typecheck, `test-web` gains Playwright, `test-rust` matches CI's flags.
  `check-docs.sh` (ubuntu job, no path filter, about 15 seconds): documented `/api` paths exist in the router, backticked `just` recipes exist, relative links resolve, backticked repo paths exist, no em dash in prose paths, size budgets on AGENTS.md and TODO.md.
  Same PR: delete the recipes `yogurt-control/SKILL.md` duplicates from AI-INTEGRATION.md, fix the false "no CLI to start headlessly" sentence in both, and add the `--help` drift test that generates the skill's command block.
  Design: `docs/.planning/agent-workflow.md`, sections 4E (E2, E3) and 4D (D2).
  Closes the cheap half of DX-1.

  Landed in #57 (2026-09-02). CI now calls `just` in both jobs (`extractions/setup-just@v3`, pinned); `lint` runs fmt, clippy and the new `check-docs` recipe; `lint-web` covers the web typecheck; `test-web` gained the Playwright e2e smoke folded in from `test-e2e`; `test-rust` gained `--no-fail-fast`.
  New `scripts/check-docs.sh` (also run by `.github/workflows/docs.yml`, since `ci.yml` skips docs-only PRs) checks documented `/api` paths, backticked `just` recipes, relative links, backticked repo paths, no em dash, and size budgets; the first run's drift (94 em dashes, one stale link in `crates/yogurt-audio/README.md`) was cleaned up in the same PR.
  `.claude/skills/yogurt-control/SKILL.md` had its duplicated recipe bodies replaced with pointers into `docs/AI-INTEGRATION.md`, and both files' false "no CLI to start headlessly" sentence was corrected.
  New `crates/yogurt-cli/tests/skill_help.rs` keeps the skill's generated command block honest against real `--help` output (`YOGURT_UPDATE_DOCS=1` regenerates it) and asserts every `yogurt <word>` mentioned in the skill, AI-INTEGRATION.md and README.md is a real subcommand.
  Verified: `just check-docs`, `just lint`, `just test` (rust + web + Playwright) all green; `cargo test -p yogurt --test skill_help` passes and fails correctly with the update hint when the block is stale; em dash count is zero in the scoped paths.
  Caveat: the repo-path rule (rule 4) is scoped to the same reference-doc set as rules 1-2, excluding docs/TODO.md and docs/.planning/, since those intentionally name not-yet-built paths.
  </details>

- [x] **CLI-7** `yogurt start --data-dir` so a worktree instance stops sharing `db.sqlite` with the running app
  <details>
  <summary>Details</summary>

  One `YOGURT_DATA_DIR` variable threaded into the `db_path` and `app_db_path` seams `RunConfig` already has; `yogurt doctor` reads the same variable.
  Keys, models and notes stay shared: a per-worktree `keys.json` would conflict with the keys-live-in-one-file constraint.
  The hazard is real (two migration runners share one file and "whichever fires first wins") but has not bitten; CLI-3's DONE entry already names this fix conditionally.
  Deferred until it does.
  Design: `docs/.planning/agent-workflow.md`, section 4D, D6.

  Landed in #59 (2026-09-02). One resolver, `crates/yogurt-cli/src/data_dir.rs`, used by `start`, `doctor`, and `ctl meeting`'s local-DB read fallback: `--data-dir <path>` / `$YOGURT_DATA_DIR` (flag wins, pure `precedence` function with a unit test) relocates `RunConfig::db_path` and `app_db_path` to the same `<dir>/db.sqlite`, mirroring the production default where both already resolve to one file.
  Keys, models, and notes are untouched, still under `~/.yogurt`.
  `doctor --json` reports the actual path in use; `ctl meeting`'s `open_db_readonly` (no `--data-dir` flag there, env only) reads the same variable so `ctl meeting list` with no server running can't disagree with a running `--data-dir` instance.
  `CONTRIBUTING.md`'s worktree section and `scripts/run-backend.sh` document `YOGURT_DATA_DIR` for the dev loop (the script needs no code change - unknown flags already forward and env vars already inherit through `exec`).
  Verified: `cargo test -p yogurt` (28 passed, including the new `data_dir.rs` integration suite and the precedence unit tests), `just lint`, `just test` all green.
  By hand: `yogurt start --port 7891 --no-open --data-dir /tmp/yogurt-cli7-smoke` produced `db.sqlite`/`-wal`/`-shm` under the temp dir only, `ctl meeting list` against it returned empty, `YOGURT_DATA_DIR=/tmp/yogurt-cli7-smoke yogurt doctor --json` reported `db_path: /tmp/yogurt-cli7-smoke/db.sqlite`, and `~/.yogurt/db.sqlite`'s mtime was identical before and after (`stat -f %m`).
- [x] **DX-3** `just start <ID> [words]`, `just worktrees`, `just dev-bg`
  <details>
  <summary>Details</summary>

  `start`: fetch, one name for worktree and branch from the ticket ID, `git worktree add` from `origin/main`, `just bootstrap`, print the ticket and the absolute handover line; resumes if the same name already exists.
  `worktrees`: path, branch, PR state, listening ports by process cwd, `dirty`, `removable`.
  `dev-bg`: `just dev` in a tmux window, port pair read back from the pane, health polled, one line of output.
  Design: `docs/.planning/agent-workflow.md`, section 4A, A1, A3, A4.
  Depends on DX-2 for the ticket lookup.

  Landed in #58 (2026-09-02). `scripts/task.sh` behind four new `just` recipes: `start` derives one lowercase-and-dashes name from a ticket ID (or accepts a free-form slug), creates the worktree and branch off `origin/main`, runs `just bootstrap`, and prints the ticket block plus the absolute handover line; it resumes (re-bootstraps, reprints) when the name is already claimed by both a directory and a branch, and refuses with exit 1 naming what's claimed by what when only one of the two exists.
  `worktrees` lists every worktree from `git worktree list --porcelain` with path, branch, PR number/state (one batched `gh pr list`, per-branch fallback, `--no-pr` to skip), listening ports matched by the holder process's cwd (via `lsof`, including the `pnpm --dir web dev` case where Vite's cwd is `<worktree>/web` not the worktree root), `dirty`, and `removable`.
  `dev-bg` opens (or resumes) a `tmux` window running `just dev`, polls the pane for the `YOGURT_PORT=`/`YOGURT_VITE_PORT=` lines and `/api/health`, then prints one line; `dev-stop` kills the window and is idempotent.
  Along the way: the `dev` recipe's trap now also catches `HUP` (what `tmux kill-window` sends the pane), since that alone wasn't reliable through the `just` -> bash -> `exec`'d binary chain, so `dev-stop` also kills any lingering listener under the worktree's cwd as a backstop - caught by actually running `dev-bg` then `dev-stop` twice against the real worktree and finding both the backend and Vite processes still listening after the first stop.
  Design: `docs/.planning/agent-workflow.md`, section 4A, A1, A3, A4.

  Verified by hand against the real repo (from `/Users/rchen/Documents/code/yogurt-worktrees/dx-3-start`): `just start` with no args and `just start XX-999` both exit 2, the second naming `just ticket`; `just start dx-3-smoke` (a slug, standing in for `just start DX-3 smoke` - see caveat) created `/Users/rchen/Documents/code/yogurt-worktrees/dx-3-smoke` on branch `dx-3-smoke` off `origin/main`, ran `just bootstrap` for real (installed `web/node_modules`, built `web/dist`), and printed the handover line and the path; a second run resumed instantly with the same output; the worktree and branch were then removed from the main checkout (`git worktree remove`, `git branch -D`).
  `just worktrees` against the real, currently-running sibling worktrees showed the foreign `~/.treehouse/yogurt-c2d339/...` worktree marked `foreign`, and `llm5-todo-done` (PR #27, merged) marked `removable`; starting `just dev-bg` in this worktree and re-running `just worktrees` showed both `7878` and `5173` on this worktree's row only, and `just dev-stop` cleared them.
  `just dev-bg` printed `backend=http://localhost:7878 vite=5173 tmux=yogurt:dx-3-start`, `curl -sf http://localhost:7878/api/health` succeeded, a second `just dev-bg` resumed without a second window, `just dev-stop` freed both ports (confirmed via `lsof`), and a second `just dev-stop` exited 0.
  `scripts/tests/task_test.sh` (10 assertions against a throwaway bare repo + clone: name derivation with a ticket ID and words, a bare lowercase slug, an invalid slug, both claim-detection directions, and the resume path) passes and is wired into `just lint`; `bash -n scripts/task.sh` is clean; `just lint` and `just test` (314 web tests + full cargo workspace) pass.

  Caveat: the shared main checkout (`/Users/rchen/Documents/code/yogurt`) was 3 commits behind `origin/main` during this work and missing `scripts/ticket.sh` on disk entirely, so `just start DX-3 smoke` (the ticket-ID path) could not be exercised against it without pulling that checkout, which AGENTS.md forbids agents from editing.
  The ticket-ID path itself is exercised end to end by `scripts/tests/task_test.sh` against a synthetic main checkout where it works correctly; the smoke test above used the slug path instead, which shares all the same worktree/branch/bootstrap/resume/handover code and produces the identical name and path.
  Whoever owns that shared checkout should fast-forward it to `origin/main` so `just start <ID>` works for everyone.
  </details>

- [x] **CLI-5** Fixture meetings: `yogurt ctl meeting new --transcript-file` seeds a finished meeting with a known transcript
  <details>
  <summary>Details</summary>

  Today the only ways to get a meeting with a transcript are a live recording or `just eval-play`, which speaks the script through the speaker for five minutes and needs TCC grants; `test_support::seed_meeting` never compiles into a runnable binary.
  So every PR that touches augmented notes or chat is verified by recording a real meeting.
  Extend `POST /api/meetings` with optional `transcript_json` segments and `ended: true`; `ctl meeting new --transcript-file <segments.json>` sends them and `--from-script scripts/eval/conversation.txt` converts the `A:`/`B:` lines so the eval ground truth doubles as the fixture.
  Design: `docs/.planning/agent-workflow.md`, section 4D, D7.
  Depends on CLI-4.

  Landed in #60 (2026-09-02). Extended `POST /api/meetings` with optional `transcript_json` (array of `{ts_ms,channel,text}` segments, validated server-side with a 400 naming the first bad field on a malformed array) and `ended` fields, so a fixture meeting can be seeded with no recording, no `stt_engine`, and `ended_at` stamped from the transcript's own last timestamp. `ctl meeting new --transcript-file <path>` forwards the file's JSON unvalidated so a malformed file's error comes from the server itself; `--from-script <path>` converts an eval-style `A:`/`B:` script into `me`/`them` segments at 4 seconds per line plus any `PAUSE` seconds. Both imply the meeting is created ended and are refused together with `--start` (exit 2, via clap's `conflicts_with`). `ctl meeting show` now also reports a `segments` count.
  Verified: `just lint` and `just test` both green (added 5 `yogurt-cli` integration tests in `ctl_smoke.rs` and 3 `yogurt-server` route tests in `meetings_api.rs`, all passing alongside the existing suite - 314 web tests, full rust workspace, Playwright e2e). Manually ran `just dev`, created a fixture via `--from-script scripts/eval/conversation.txt` (43 segments, matching the script's `A:`/`B:` line count), confirmed `ctl meeting show`/`transcript` output, ran `ctl meeting enhance last` against the real configured MiniMax provider (produced a genuine multi-section summary with action items, not `too_short`), then deleted the fixture via the API and confirmed a 404 on re-fetch.
  </details>

- [x] **DX-8** Feature Map at `docs/FEATURES.md`, with a coverage rule in `check-docs.sh`
  <details>
  <summary>Details</summary>

  One table, about 19 rows: what it does, UI path, API, covering test, source anchor, and the `ctl` command once CLI-4 exists.
  The coverage rule extracts every `.route("...")` literal (spanning lines) and every router path and fails when one has no row and is not in the `internal:` list.
  Design: `docs/.planning/agent-workflow.md`, section 4D, D3.
  After CLI-4 and DX-4.

  Landed in #62 (2026-09-02). docs/FEATURES.md: 19-row table mapping every user-facing feature to UI path, API routes, ctl command, covering test, and source anchor, plus an Internal routes list (health, session-token, the /api/:rest catch-all, and router.tsx's * catch-all). check-docs.sh gained a coverage rule that extracts every .route(...) literal and every router.tsx path: literal, normalizes axum's {param} to the router's :param, and fails if either is missing from FEATURES.md. Deviated from the design doc's internal-route example list: the detection banner and /ws (STT model-download progress) turned out to be real user-facing features once verified against the code, so they got their own rows instead of being marked internal. Verified: just check-docs passes; removing a route from a FEATURES.md cell makes the new rule fail naming that exact route; adding a bogus /api/nope makes the existing rule 1 fail; 38 .route( calls extracted (38 distinct routes) vs 11 router.tsx path: literals, matching grep -c counts; just lint and just test both pass.
  </details>

- [x] **DX-5** `scripts/ship.sh pr | land` and tracked git hooks
  <details>
  <summary>Details</summary>

  `pr` refuses a title without a ticket ID or conventional prefix, a body with attribution or an em dash, a code change without an absolute-path handover line, or a ticket not moved to DONE on the branch.
  `land` waits for CI (skipped for docs-only), squash-merges with `--match-head-commit`, then from the main checkout removes the worktree (refusing on a dirty tree), deletes the branch, and re-prints the handover; every step resumes.
  `.githooks/` rejects agent trailers and commits on `main`, activated by `bootstrap` and `setup.sh`.
  Design: `docs/.planning/agent-workflow.md`, section 4B.

  Landed in #63 (2026-09-02). Adds `scripts/ship.sh pr | land` (design: docs/.planning/agent-workflow.md section 4B) and tracked `.githooks/` (commit-msg, pre-commit) wired via `git config core.hooksPath .githooks` in `just bootstrap` and `scripts/setup.sh`. `pr` validates title, body (attribution/em dash), an absolute-path handover line for code changes, and ticket checkoff before pushing and opening the PR; `land` waits for CI (skipped for docs-only), squash-merges, and cleans up the worktree/branch, resumable at every step. Verified with `scripts/tests/ship_test.sh` (33 cases) and `scripts/tests/hooks_test.sh` (11 cases), both wired into `just lint`; `just lint` and `just test` green; real `just pr --dry-run` and `just land --dry-run` runs and a live commit-msg hook rejection in this worktree.
  </details>

- [x] **DX-7** `scripts/release.sh preflight | ship | verify | finish | untag`, then shrink the release skill
  <details>
  <summary>Details</summary>

  Three PRs: `preflight` (read-only judgment gate, replaces skill steps 1-4 in the same PR), then `verify`/`finish`/`untag` (hash check, tap merge, brew upgrade in place, pre-filled log row with a `NARRATIVE:` slot), then `ship` (bump PR from a throwaway worktree, dry run, tag by merge sha via the GitHub API, watch, verify, finish; resumable from GitHub state).
  Then the skill becomes about 200 words.
  No `just` recipe: `just release` already means "run the release binary".
  Design: `docs/.planning/agent-workflow.md`, section 4C, C1 to C3.
  After DX-6.

  Landed in #64 (2026-09-02). Third PR: `scripts/release.sh ship <version>` orchestrates preflight, the dry-run dispatch, a throwaway-worktree version bump PR (marked with `<!-- yogurt-release: P=<sha> -->` for resumability), the merge, a "main moved" parent-sha assertion, tagging by merge sha via the GitHub API, watching the Release run, then `verify` and `finish`. Every step derives done/not-done from GitHub state so a timed-out run resumes on re-invocation. `-n` runs preflight for real (read-only) and then prints the plan for every mutating step. Verified against the live repo: `ship 0.8.0 -n` prints the real plan (correctly blocking by default on the currently-open #63 doc PR, then showing the full plan with `--allow-open-docs`); `ship 0.7.0 -n` fails preflight (tag exists) and stops before the plan, exit 1; `ship` with no version exits 2. The release skill shrunk from 800 to ~295 words. `ship` itself has not been run for real yet - the next actual release is the test.
  </details>

- [x] **DX-10** `scripts/check-published.sh`: tap formula version, release assets, README formula names and model mirror URLs still resolve
  <details>
  <summary>Details</summary>

  Runnable by hand after a formula edit and weekly from a scheduled ubuntu workflow; opens an issue on failure.
  The one drift a PR-time check cannot see (the v0.3.0 README-versus-tap failure).
  Design: `docs/.planning/agent-workflow.md`, section 4F, F2.
  After DX-7.

  Landed in #65 (2026-09-02). Added `scripts/check-published.sh` (tag vs tap formula version, both tarball URLs + shas vs the release's SHA256SUMS, every README `yogurt-model-*` line vs the tap, every model mirror URL in `crates/yogurt-stt`), wired as `just check-published` (kept out of `just lint`, needs the network) and a new weekly `.github/workflows/check-published.yml` with `workflow_dispatch` as the escape hatch and `--issue` (workflow-only) opening a `gh issue` on failure, skipped if one is already open. Verified: `bash -n` on both scripts; `scripts/tests/check-published_test.sh` (27/27, PATH-stubbed git/gh/curl, wired into `just lint`); `just lint` green end to end; `scripts/check-published.sh` run for real against the live tap and v0.7.0 release - all 14 checks `ok:`, no drift; `scripts/check-published.sh --json | jq .` valid; workflow YAML parses; confirmed by inspection that `--issue`'s `gh issue create`/`gh issue list` path is only ever reached by the test's stubbed `gh`, never by a real run in this session.
  </details>

- [x] **DX-1** `just test` is a weaker gate than CI, and neither exercises the real binary
  <details>
  <summary>Details</summary>

  Two separate problems, worth fixing in that order.

  **The cheap one.** `just test` runs `cargo test` + `pnpm test`. CI additionally runs the Playwright suite (`.github/workflows/ci.yml`, `pnpm --dir web e2e`).
  So the command AGENTS.md points every contributor and agent at is not the gate: you go green locally, open the PR, and find out afterward.
  The suite is Vite-only - no Rust build, no keys, its own port 5199 - so there is no inner-loop cost to justify keeping them apart.
  Add `pnpm --dir web e2e` to the `test` recipe.

  **The one that actually matters.** Even after that, the gate still would not have caught MTG-11.
  `web/playwright.config.ts` is explicit that the specs drive the real React app against a *browser-level mock* of the Rust backend (`page.route`), precisely so they need no keychain and no keys in CI.
  That is a reasonable design for what they cover, but it means nothing in the automated suite runs the real binary, and nothing at all touches macOS-facing behavior like window detection or audio capture.
  Every such check today is a human driving a browser, which is exactly how a fabricated verification got through.

  Worth deciding: a small suite that boots the real debug binary and drives it over REST (which `yogurt ctl` from CLI-4 would make cheap to write), gated behind a feature or an env var so CI can skip the parts needing hardware.
  Not a full E2E rig - just enough that "I verified it" means something a machine can re-run.

  DX-4 already closed the cheap half (Playwright folded into `just test`, CI calling `just`).
  This PR is the D4 half: a real-binary smoke suite and `just test-hw`.
  `crates/yogurt-cli/tests/ctl_smoke.rs` gained hardware-free coverage of the gaps CLI-4/CLI-5 left open (status and detect on a fresh instance, `enhance last` returning `too_short` on an empty meeting, `summary` as front-matter-only before enhance vs. real content after, `stop` on a never-started meeting stamping `ended_at`), plus a hardware path (`ctl windows`, a real `meeting start`/`stop` that opens and closes the SCK + mic pipeline) gated on `#[ignore]` and a `YOGURT_HW_TESTS=1` check each test re-does itself.
  `just test-hw` runs that hardware path plus the two pre-existing `#[ignore]` tests (`yogurt-audio`'s permission smoke, `yogurt-stt`'s whisper smoke), skipping the whisper one with a printed notice if `~/.yogurt/models/ggml-small.en.bin` isn't downloaded rather than failing.
  `ctl meeting mute on` is not covered: no `mute` subcommand exists on `ctl` yet (CLI-6).

  The hardware start/stop test bounds every `ctl` call to 45s (`ctl_run_bounded`, polling `try_wait` instead of assert_cmd's unbounded `.ok()`) so it can never hang the suite; on timeout it kills the child, cleans up via the API, and names the two real causes (a pending TCC prompt for a fresh worktree binary's first run, or a stale capture session).
  While verifying locally the test hung twice (several minutes each) before passing three times in a row; a direct curl against the same setup proved it was not a TCC prompt (the pipeline opened and closed in under a second) but a stale SCK/mic session left by my own SIGKILL of the first hung attempt, which skips `Drop` and its graceful teardown.
  Filed AUD-8 for that finding (a force-killed `yogurt` can leave a stale capture session that blocks the next recording); not fixed here, out of DX-1's scope.

  Verified: `just lint` and `just test` both green (`ctl_smoke.rs` now has 21 tests, 2 of them `#[ignore]`d; full rust workspace 443+ tests, 314 web tests, 2 Playwright specs).
  `cargo test -p yogurt --test ctl_smoke -- --ignored` with no env var confirmed both hardware tests skip cleanly (printed the skip reason, `test result: ok`) and left the real `~/.yogurt` database untouched.
  `just test-hw` run for real on this Mac, three times end to end: `hw_windows_reports_rows_or_denied` passed every time; `hw_meeting_start_stamps_stt_engine_then_stop_closes_pipeline` opened a real local-whisper capture pipeline (symlinking the developer's already-downloaded `small.en` model into the test's temp `HOME` and switching that instance to the local provider via `PATCH /api/settings`), stamped `stt_engine`, stopped, stamped `ended_at`, and deleted the meeting via `DELETE /api/meetings/:id`; the permission smoke test and the whisper smoke test (model present) both passed on every run.
  Real `~/.yogurt/db.sqlite` was unchanged across the whole session: `ctl meeting list --limit 3` matched throughout (30 meetings, same top 3 ids) - all `test-hw` work happens against temp-`HOME` servers, same isolation as every other test in this file - and no yogurt process or listener was left behind.

  Landed in #66 (2026-09-02).
  </details>

- [x] **DX-9** Rewrite AGENTS.md around the six-command lifecycle, with the cloud-session paragraph
  <details>
  <summary>Details</summary>

  About 480 words: constraints, `start`, `ticket`, `dev-bg`, `pr`, `land`, pointers.
  Evicted rationale (build splice, relative paths, port pair) goes to CONTRIBUTING.md's worktree section first.
  One paragraph: tickets under `web/` or `docs/` may run as cloud sessions; Rust stays local with the macos-26 runner as the cloud verifier.
  Only after DX-2 to DX-5 exist.
  Design: `docs/.planning/agent-workflow.md`, sections 4E (E1) and 4F (F1).

  Landed in #68 (2026-09-02). Rewrote AGENTS.md around the six-command lifecycle: what yogurt is, hard constraints (CLI-provider exception cut to one sentence pointing at ARCHITECTURE.md §7.6), the six-command lifecycle (`just start` -> `dev-bg` -> `ticket done` -> `pr` -> `land`, and `scripts/release.sh preflight`/`ship` for releases), repo layout as pointers, conventions, and the F1 cloud-session paragraph. 1558 -> 599 words, 9770 -> 4069 bytes (12 KB budget). Evicted rationale (build-splice, relative-path handover, shared-checkout etiquette) moved into CONTRIBUTING.md's worktree section first. Kept one sentence that release notes are generated by the pipeline, never hand-written, since no CHANGELOG.md has ever existed in this repo but a stale-looking line is cheaper than a surprise. Verified: `just lint` and `just test` both green; `./scripts/check-docs.sh` ok; every named command's `--help`/usage confirmed to exist, including `scripts/release.sh ship` (merged in #64 mid-task). Review round 2 folded back two clauses dropped instead of moved (the CLI-provider no-generalize rule, the .lavish/ not-repo-root steer) and trimmed --help-restating filler to net zero words.
  </details>

- [x] **CLI-6** the control skill rewritten around a generated command block
  <details>
  <summary>Details</summary>

  The `yogurt ctl` second slice - `settings`, `provider`, `models`, `ws`, `meeting mute | search | delete` - landed in #67.
  Remaining scope: `.claude/skills/yogurt-control/SKILL.md` shrinks to about 150 words - the command block between generator markers (kept honest by the `--help` drift test from DX-4), a Feature Map link, and three rules.
  That rewrite waits for the brew release that carries `ctl`: the README's `npx skills add` path installs the skill standalone, so a skill naming `ctl` commands must not precede a binary that has them.
  Design: `docs/.planning/agent-workflow.md`, section 4D, D1 and D2.

  Landed in #67 (ctl second slice) and #72 (2026-09-02). Rewrote `.claude/skills/yogurt-control/SKILL.md` to ~150 words of prose around the generated `<!-- yogurt-cli:start/end -->` block: run `yogurt ctl` first, a stated version floor (needs yogurt 0.8.0 or newer, checked with `yogurt --version`, `brew upgrade jarvisrchen/yogurt/yogurt` if older), a link to docs/FEATURES.md, and three rules (summary before transcript, one recording at a time, never print the session token). Folded docs/AI-INTEGRATION.md's per-recipe curl blocks into a route table (method, path, purpose, `ctl` command) plus one example curl, keeping the auth paragraph, health shape, fixture-meeting fields, and websocket frame list as API contract documentation (1309 -> 634 words). Deleted `scripts/tail-transcript.sh`, replaced by `yogurt ctl meeting transcript --follow` / `yogurt ctl ws`; updated its mentions in docs/DEBUGGING-TRANSCRIPTS.md's two "Watch" sections, `scripts/eval/compare.sh`'s comment, docs/TODO.md's AUD-7 block, and the code caption in docs/.lavish/DEBUGGING-TRANSCRIPTS.html. Trimmed README's skill paragraph to one sentence matching what the skill now teaches.
  Verified: `cargo test -p yogurt --test skill_help --features yogurt-stt/local-stt` passes (2/2) after regenerating the block with `YOGURT_UPDATE_DOCS=1`; skill prose (block stripped) is about 150 words; `just check-docs` and `just lint` both pass; `git grep -n tail-transcript` returns only docs/.planning/agent-workflow.md; ran the skill's commands against the real binary in this worktree (`cargo run -q -p yogurt -- ctl`, `ctl status` correctly error with no server running, `ctl meeting summary last` succeeds via the `source: db` fallback).
  </details>

- [x] **UI-8** No app icon when saving yogurt as a web app (Safari Add to Dock / Chrome install)
  <details>
  <summary>Details</summary>

  `web/index.html` had no favicon or apple-touch-icon links, so saving yogurt as a web app picked up a generic icon instead of the brand mark.

  Added apple-touch-icon, favicon (svg + png), and a web manifest built from the real
  `Logo.tsx` brand mark, and linked them from `web/index.html`. Safari's Add to Dock and
  Chrome's install-as-app now pick up the yogurt logo instead of a generic icon.
  </details>

- [x] **LLM-8** End meeting can show the raw notes as the enhanced summary, and a one-word note comes back grey plus duplicated
  <details>
  <summary>Details</summary>

  Reported 2026-09-02.
  After End meeting the enhanced view was just the typed notes, with no error anywhere; Re-enhance then produced a real summary.
  In that summary `247k` sat grey inside "Base salary: 247k" and appeared again as an orphan paragraph at the end (meeting `01a05a46`).

  Two independent faults, both reproduced headlessly through `yogurt ctl` and pinned with tests.

  **1. An empty model reply was persisted as the summary.**
  `enhance.rs` never checked the scrubbed LLM output; `merge_notes(notes, "")` is exactly the notes, so an empty reply (a CLI provider returning an empty `result`, a reasoning model whose whole answer sat in an unclosed `<think>`) was written to the row and shown as the enhanced document with no error anywhere.
  Enhance now fails with 502 and an `enhance_progress` error frame ("returned no text - try Re-enhance"), and `interpret_single_json` in the CLI provider rejects an empty `result` the way the opencode path already did.

  **2. One-word notes were painted grey and duplicated.**
  `weave::find_user_line` required two words, so `247k` folded into "Base salary: 247k" was never recognised: the bullet went fully grey and the defensive append re-emitted `247k` as an orphan paragraph (meeting `01a05a46`).
  `MIN_WORDS` is now 1; matching is whole-word, so it only lands where the word actually appears.

  Harness: `ctl meeting new --notes-file` seeds notes alongside `--transcript-file`, so `ctl meeting enhance` runs exactly the End meeting path (notes and transcript from the row).
  Tests: `crates/yogurt-notes` (weave, diff), `crates/yogurt-llm/src/cli.rs`, `crates/yogurt-server/tests/enhance_endpoint.rs` (scripted LLM: empty reply, one-word weave), `crates/yogurt-cli/tests/ctl_smoke.rs` (seeded End-meeting path).
  </details>

- [x] **CLI-8** Add a `--markdown` flag to `ctl meeting transcript` for a shareable, seedable export
  <details>
  <summary>Details</summary>

  `ctl meeting transcript` prints plain `<seconds> <channel>: <text>` lines (or `--json` segments), neither of which is markdown.
  Richard wants a `--markdown` flag that formats the transcript as a real markdown doc - title header, `**speaker**` bold, `(mm:ss)` timestamps - so redirecting it to a `.md` file (`yogurt ctl meeting transcript <id> --markdown > out.md`) gives something worth sharing, without a shell one-liner to remember.
  No `--output` flag needed - shell redirection already covers "where to save."
  The existing `--json` output already round-trips into `ctl meeting new --transcript-file`, so that seeding path stays as-is; this ticket is purely the human-readable markdown rendering.

  Added `--markdown` to `ctl meeting transcript`: title header, `**speaker**` bold, `(mm:ss)` timestamps. Works with `--follow` (header printed once) and refuses combination with `--json`. Covered by two new tests in `ctl_smoke.rs`.
  </details>

- [x] **LLM-9** Enhance templates: auto-detected note formats (standup, 1:1, design review...) with a manual picker on Re-enhance
  <details>
  <summary>Details</summary>

  Enhance always produced the same section shape, no matter what kind of meeting it was.
  Bundle a handful of note formats (standup, one-on-one, team meeting, design review, customer call, interview, and a general fallback) as prompt templates.
  Auto-detect the right one from the transcript and notes during the same enhance call, rather than a separate classification pass.
  Add a manual picker beside Re-enhance so a wrong guess, or a shape the user just prefers, is one click away.

  Landed in the enhance-templates PR (2026-09-03).
  Seven formats under `crates/yogurt-prompts/templates/enhance/`, auto-detected in the same LLM call via a `template:` first line, forced via the `template` body field / `--template`, stamped on `meetings.template`, picker beside Re-enhance.
  </details>

- [x] **MTG-12** Don't suggest starting a meeting while one is already in progress

  The server guard now lives in `DetectState::prompt(recording)` with a unit test, and starting a recording from `+ New meeting` invalidates the detection and active-recording queries so the banner drops immediately instead of lingering up to one 5s poll.

- [x] **AUD-8** A force-killed `yogurt` can leave a stale SCK/mic capture session that blocks the next recording
  <details>
  <summary>Details</summary>

  Found while verifying DX-1's hardware smoke test: SIGKILLing a `yogurt` process mid-recording skips `Drop`, so `AudioStream`'s SCK + cpal teardown never runs.
  A subsequent `start()` (same machine, same or a different `yogurt` process) then hangs for several minutes opening its own capture session, apparently waiting for macOS to reclaim the still-held OS-level resources from the killed process; a direct retry once that window passed opened and closed cleanly in under a second.
  Normal shutdown (Cmd-Q, or `ctl meeting stop`) already goes through `Registry::stop()`'s graceful teardown (200ms watchdog), so this only bites a hard kill - Activity Monitor "Force Quit", a crash, `kill -9` - while a meeting is recording.
  Worth understanding whether this is inherent to `SCStream`/`cpal` cleanup timing (nothing to do beyond documenting it) or something `yogurt` could shorten, e.g. detecting an orphaned session on next launch and giving a clear error instead of a silent multi-minute hang.

  AUD-8: detects orphaned SCK capture sessions from a force-killed yogurt process.

  `start_capture()` now writes a PID marker (`<data_dir>/capture.lock`, honoring
  `YOGURT_DATA_DIR`) and removes it on clean teardown. On the next start, a marker
  with a live PID fails fast (AlreadyRecording); a marker with a dead PID (the
  AUD-8 case - a force-killed process) logs a warning, clears the stale marker,
  and fails fast with OrphanedSession instead of hanging silently inside SCK for
  several minutes.

  Verified via unit tests only (no hardware/SCK E2E - no CI hardware available).
- [x] **AUD-9** Filter filler words and misheard noise out of the transcript
  <details>
  <summary>Details</summary>

  Two related clutter sources in the live/stored transcript, both fixable at the same choke point: `relay_transcript_events` (`crates/yogurt-server/src/meetings.rs:874`), the one place every `TranscriptEvent` passes through before reaching both persistence and the WS - it already does one text-level filter there (`EchoDeduper`).

  1. **Backchannel filler.** "mmhmm", "uh-huh", "yeah", "um" etc. get transcribed as real (correct) words but add no value to notes. A blocklist filter dropping short finals that are just filler would slot in next to `EchoDeduper`. Risk: can't distinguish backchannel noise from a genuine one-word answer ("yeah" meaning "yes" to a direct question) - a word-list alone will drop some real utterances.
  2. **Misheard noise.** Coughs, mic bumps, taps, background sound get a guessed word from the STT model. VAD (`crates/yogurt-stt/src/vad.rs`) only gates on energy/spectral "is this voiceish," so noise with speech-like broadband energy passes VAD fine and Whisper/Deepgram then hallucinate a word for it. The unused lever: Deepgram already returns a per-result `confidence` score that's parsed and discarded today (`crates/yogurt-stt/src/deepgram.rs`, near the `is_final`/`speech_final` handling ~line 500); whisper.cpp has the analogous `no_speech_prob`/avg log-prob per segment via whisper-rs, also unused. Gate on that instead of trying to pre-filter audio before STT. Risk: confidence is a blunt instrument - mumbled or accented real speech can also score low, so an aggressive threshold risks silently eating unclear-but-real speech, not just noise.

  Open question before building: drop silently, or surface low-confidence/filler segments as visibly greyed-out first so nothing vanishes without a trace.

  Filtered at `relay_transcript_events` next to `EchoDeduper`: pure-filler segments (non-lexical list, "yeah"/"okay" kept) and segments with engine confidence below 0.4 are dropped with a debug log. `TranscriptEvent.confidence` is populated from Deepgram and from whisper `1 - no_speech_probability`. Open question resolved as drop-with-log; greyed-out UI deferred.
  </details>

- [x] **MTG-13** Live transcript renders every line twice
  <details>
  <summary>Details</summary>

  Loading a page mid-meeting shows every transcript line duplicated in the live dock.
  Root cause: `useTranscriptWs`'s seed effect (`web/src/lib/ws.ts`, ~line 212) latches `seededMeetingIdRef` only when `seedHistory` is non-empty; on page load during a live meeting `seedHistory` starts empty, so the effect returns early without latching and live WS finals accumulate directly in `events`.
  The server persists `transcript_json` mid-meeting and a later TanStack refetch (window focus, etc.) makes `seedHistory` non-empty, so the effect fires again and prepends those same segments on top of the identical live events already present.

  Fixed the live-transcript duplicate-lines bug (MTG-13): the seed effect in `useTranscriptWs` (`web/src/lib/ws.ts`) now dedupes seed entries against the finals already present in `events` before prepending, using the same `${channel} ${text}` signature already used for `seededFinalsRef`, so a mid-meeting `seedHistory` refetch (e.g. from a periodic `transcript_json` persist plus a window-focus TanStack refetch) no longer re-adds finals that live WS delivery already appended. Added a regression test in `web/src/lib/ws.test.ts` that mounts with an empty `seedHistory`, delivers two live finals, then rerenders with a `seedHistory` containing those two plus one older entry, asserting exactly three deduped events in order.
  </details>

- [x] **UI-9** Add a "New label" affordance to the Labels group in the left panel
  <details>
  <summary>Details</summary>

  `SidebarLabelRow` already supports rename and delete, but the only way to create a label is through the per-meeting `LabelPicker` popover.
  Add a create action to the sidebar Labels group (an inline row or "+" button) that reuses `useCreateLabel`, so labels can be managed entirely from the left panel.

  Added a "+" affordance next to the Labels group header in the sidebar.
  Clicking it opens an inline text input matching SidebarLabelRow's rename styling.
  Enter commits (trimmed name, ignores empty/whitespace-only) via `useCreateLabel`; Escape cancels; blur commits like rename does.
  The button and input disable while the create mutation is pending.
  Added vitest coverage in Sidebar.labels.test.tsx for opening the input, Enter creating, and Escape canceling.
- [x] **AUD-10** Add a Test button for the local STT model, matching the cloud one
  <details>
  <summary>Details</summary>

  `STTPicker` renders `TestKeyButton` for the Deepgram card only.
  The local-model pills have no equivalent, so a downloaded model that fails to load is only discovered when a meeting starts.
  Add a Test action for the selected local model that loads it and transcribes a short built-in clip, reporting a verdict line like the cloud button.

  Added a Test action for the local whisper.cpp model, matching the existing Deepgram Test button.

  Backend: `POST /api/settings/stt/local/test` (`test_local_stt` in `crates/yogurt-server/src/api/settings.rs`) resolves the model (defaults to the currently selected `stt_model`), then runs `WhisperLocal::self_test` on `spawn_blocking` - loads the model and decodes a built-in 1s silent clip. Returns the same `{ok, model, error}` shape the LLM provider test endpoint already uses, so the frontend's `TestKeyButton` renders it unchanged.

  `WhisperLocal::self_test` (`crates/yogurt-stt/src/whisper_local.rs`) is a small new public method reusing `load` + the existing private `decode`. No new audio fixture needed - a `vec![0i16; 16_000]` in-memory silent buffer is the clip.

  Frontend: `LocalSTTCard` now renders a `TestKeyButton` under the model picker, testing the currently selected model. Disabled unless that model is downloaded.

  Verified: cargo fmt/clippy -D warnings, cargo test (all pass), web typecheck + vitest (pass), and a real E2E hit against a running dev server with a downloaded small.en model returned `{"ok":true,"model":"small.en · loaded and ran in 7035ms"}`.
  </details>

- [x] **AUD-11** Echo the mic to a second output device during a live meeting
  <details>
  <summary>Details</summary>

  Richard already runs a standalone Python app for this: `/Users/rchen/Documents/code/shadow-voice` (`audio_echo.py`, PortAudio/sounddevice) captures a macOS input device and routes it in real time (sub-50ms, adjustable buffer) to a chosen output device, typically a virtual device like BlackHole so another app (Zoom, OBS, a DAW) can consume the mic.
  Fold that into yogurt so it is one less process to run: when starting or during a live meeting, offer an option to echo the mic into a selectable output device.
  Needs a design pass before implementation, at least: where the toggle and device picker live in the live-meeting UI and Settings (persist the chosen output device), whether the echo taps the existing cpal mic stream in `yogurt-audio` or opens a second one, latency/buffer controls, and how it interacts with the hard constraints (in-process only, no audio leaves the machine; echo to a local output device is fine).
  Use shadow-voice for inspiration on device enumeration, buffer sizing, and the latency/stability trade-off; do not port its Python.

  Two-column block on the live meeting page: mic picker over Mute, echo output picker over an Echo button (E). Backend tees the mic ring into a cpal output stream (`yogurt-audio/src/echo.rs`), hot-swappable, silenced by mute; settings `audio_echo_output_device`, `audio_echo_enabled`, `audio_echo_buffer`. Verified on hardware with a 440 Hz tone through BlackHole 2ch. Also fixed cpal streams never stopping on Drop (cpal 0.15 macOS Arc cycle), which had leaked mic streams on every hot-swap.
  </details>

- [x] **UI-10** Make the Screen Recording failure banner say which app to grant and link to the macOS pane
  <details>
  <summary>Details</summary>

  When start fails with "The user declined TCCs", the banner's "Open Settings" goes to yogurt's own Settings page, which cannot fix it.
  The grant belongs to the app that launched yogurt (the terminal, or Homebrew's binary), not to yogurt, and macOS 26 can leave a stale grant that looks on but is refused; the fix is toggling that app off and on under Privacy & Security > Screen & System Audio Recording.
  Say that in the banner and link `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture` (a plain anchor, no subprocess).

  The banner now detects a Screen Recording / TCC denial and, instead of linking to yogurt's own Settings, explains that the grant belongs to the app that launched yogurt (terminal or Homebrew), that it must be quit and reopened after a grant change, and links `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`.
  </details>
