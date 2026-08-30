# Design: stream the enhanced summary into the editor

Status: approved 2026-08-29, implementation on branch `feat/enhance-streaming`.
Interactive companion: [../.lavish/enhance-streaming-design.html](../.lavish/enhance-streaming-design.html).

## Goal

When the user presses End meeting or Re-enhance, the Enhanced tab should fill with text as the LLM produces it, the way Granola does.
Today the editor stays blank (or keeps the stale summary) behind a lilac banner until the whole completion returns, which for a long meeting is 10 to 40 seconds of nothing.

## Current state (verified in code)

- `crates/yogurt-server/src/enhance.rs` calls `llm.complete()` (non-streaming) inside a 60 s `tokio::time::timeout`, then emits exactly one `enhance_progress { phase: "streaming", chars }` frame after the full text has arrived.
  The `streaming` phase name is a placeholder left from Phase 4 and does not stream anything.
- After the LLM returns, the handler runs `fix_model_mojibake` -> `strip_model_reasoning` -> `strip_prompt_scaffolding` -> `merge_notes` -> `render::to_markdown` -> `sanitize_enriched_md`, persists, and emits `done`.
  `merge_notes` diffs the whole LLM document against the user's notes to decide which blocks are `User` (black) and which are `AiGrey` (grey, with a transcript timestamp), so the black/grey wire format can only exist once the full document is known.
- `yogurt_llm::LlmClient::stream()` already exists and is production-proven by chat (`crates/yogurt-server/src/api/chat.rs::run_stream`), including `ThinkStripper` for `<think>` blocks split across chunks.
  `MockLlm::stream()` also exists.
- Frontend: `useEnhanceProgress` (`web/src/lib/ws.ts`) reads `phase` / `chars` / `errorMessage` from `enhance_progress` frames and drives `EnhancingBanner`.
  `YogurtEditor` (`web/src/editor/index.tsx`) replaces its content whenever the `enrichedMarkdown` prop changes, via `setContent(html, false)` so no `onChange` fires.
- End meeting (`web/src/routes/Meeting.tsx::endMeeting`) awaits the whole POST before navigating to `/meeting/{id}/post`, so the in-meeting screen is what the user stares at during the wait.
  Re-enhance (`ReEnhanceButton`) runs on the post view and awaits the same POST.

## Proposed behavior

1. Click End meeting: notes flush, recording stops, and the app navigates to the post view immediately.
   The post view fires the enhance POST itself.
2. Within a second the Enhanced tab starts filling with grey text, top to bottom, with a small caret at the end of the growing document.
   The lilac banner stays and its character count is now live.
3. The editor is read-only while text is streaming.
   The Enhanced / My notes tabs stay clickable; My notes is unaffected.
4. When the stream finishes, the server runs the existing merge + sanitize pipeline and the final document replaces the preview in place.
   The user's own lines turn black and `↳ MM:SS` transcript links appear on the AI lines.
   Text does not move because the preview and the final document share the same blocks; only styling and link atoms change.
5. Re-enhance behaves the same way on the post view: the old summary is replaced by the growing preview on the first chunk.
6. Errors mid-stream keep today's contract: `enhance_progress { phase: "error", message }`, the strawberry banner with Retry, and the previous `enriched_md` left untouched in SQLite.

## Approaches considered

### A. Stream the raw markdown as a preview, settle to the merged document on done (recommended)

Server streams the accumulated raw LLM markdown; the browser renders it as an all-grey preview; the existing merge pipeline runs once on the full text and the POST response swaps in the final document.

- Reuses `LlmClient::stream()` and the chat loop shape verbatim.
- The merge pipeline, sanitizer, persistence, and response shape are untouched.
- The "settle" moment (grey to black, links appear) is a legible cue that the AI finished and the user's notes were preserved.
- Cost: the preview cannot show black/grey or transcript links until the end.

### B. Server-side incremental merge on every tick

Server re-runs mojibake fix + merge + render + sanitize on the partial text every ~80 ms and streams the wire-format snapshot, so the preview already has black/grey and links.

- Best-looking preview, no settle step.
- Cost: `strip_model_reasoning` and `strip_prompt_scaffolding` are written for complete documents and can misfire on a half-written tag; the trailing partial block gets a bogus timestamp guess on every tick; the merge runs 100+ times per enhance instead of once.
  Nothing here is expensive on a 5 KB document, but it moves complexity into the hot loop for a cosmetic gain.
- Upgrade path from A if the settle step ever feels wrong: the frame shape does not change, only what `text` contains.

### C. Client-side fake typing of the completed response

Keep the non-streaming call and animate the final text in.

- Rejected: the user still waits the full time to first paint, which is the actual complaint.

## Design (approach A)

### Wire contract

One frame type, backward compatible.
`enhance_progress { phase: "streaming" }` gains a `text` field carrying the full accumulated raw markdown so far.

```json
{ "type": "enhance_progress", "phase": "streaming", "chars": 1234, "text": "# Standup\n\n- ..." }
```

Snapshots instead of deltas, on purpose.
`events_tx` is a `tokio::sync::broadcast` with no replay, so a subscriber that connects late (the post view mounting after End meeting) or reconnects after a drop would silently miss deltas and render a corrupted document.
With snapshots every frame is self-contained, the client needs no accumulator, and the worst case after a hiccup is one stale tick.
Over localhost a 5 KB document at ~12 frames per second is ~60 KB/s for the duration of the stream, which is not worth optimising.

`sending`, `done`, and `error` frames are unchanged.

### Server: `enhance.rs`

Replace step 4's `llm.complete()` with the chat-style loop.

- Open the stream inside `tokio::time::timeout(LLM_HTTP_TIMEOUT, llm.stream(req))`.
- Consume chunks with `tokio::time::timeout(LLM_HTTP_TIMEOUT, stream.next())` per chunk, so the constant becomes an idle timeout rather than a wall-clock cap.
  A 60 s wall-clock cap would cut off a long summary that is streaming healthily; an idle cap still catches a hung provider.
- Accumulate `delta` into a `String`.
- Emit a `streaming` snapshot on the first non-empty delta, then at most every 80 ms, then once more after the loop ends if the last emit is older than the last delta.
  80 ms keeps TipTap `setContent` under ~13 Hz on the browser side without visible stutter.
- On chunk error: emit `phase: "error"` and return 502, same as today's `llm complete failed` branch.
- Everything from step 6 onward (mojibake, reasoning strip, scaffolding strip, merge, render, sanitize, persist, `done`) runs unchanged on the accumulated text.
- Remove the placeholder `streaming { chars }` emit after the call since the loop now owns that phase.

`MockLlm::stream()` stays a single-delta stream, which is enough for the endpoint test to assert that at least one `streaming` frame carries `text`.
A second mock behavior that yields the content in three deltas with small sleeps would exercise the throttle; add it only if the throttle logic grows a branch worth testing.

### Frontend

`useEnhanceProgress` (`web/src/lib/ws.ts`):

- Add `text: string | null` to the result, set from `streaming` frames that carry `text`, reset to `null` on `sending`, `done`, and `error`.
- `EnhanceProgressEvent` gains `text?: string`.

`MeetingPost.tsx`:

- `const streaming = ws.phase === "streaming" && ws.text != null`.
- While `streaming`, force `activeDocument` to `"enhanced"` on the first frame, pass `enrichedMarkdown={ws.text}` and `editable={false}` to `YogurtEditor`, and set `data-streaming` on the editor wrapper.
- `handleEnhanced(response)` is unchanged: it sets `enrichedMd` from the POST response, which becomes the editor content once `streaming` flips false on `done`.
  Because `done` and the POST response arrive within the same tick, the swap is a single `setContent`.
- On a `done` frame with no local POST in flight (the page was reloaded mid-stream), refetch `GET /api/meetings/{id}` through the existing generation-guarded loader so the final document still lands.
- Autosave: `setContent(html, false)` never fires `onChange`, so `enrichedEditedRef` cannot flip during streaming and no preview text is ever persisted.
- Accept `location.state.autoEnhance: { notes_md, title }` and fire `postEnhance` once on mount when present, mirroring the `autoStart` pattern already in `Meeting.tsx`.
  `too_short` from that response drives the existing too-short screen instead of coming through `location.state.tooShort`.

`Meeting.tsx::endMeeting`:

- Flush notes and stop recording as today, then `navigate(`/meeting/${id}/post`, { state: { autoEnhance: { notes_md: notesMd, title } } })` without awaiting an enhance POST.

`ReEnhanceButton`: no change.
It still awaits the POST for the final document; the preview arrives through the WS in the meantime.

Styling (`web/src/index.css`):

- `[data-streaming] .ProseMirror { color: var(--color-grey); }` so the whole preview reads as AI text, matching the Legend.
- `[data-streaming] .ProseMirror > :last-child::after` renders a 2 px blueberry caret with the existing `recpulse` keyframes.
- No layout animation on settle; the swap is instant and text stays in place.

### Failure modes

| Situation | Behavior |
| --- | --- |
| Provider rejects the stream open (bad key, 429) | `error` frame + 502, banner shows message + Retry, old summary untouched. Same as today. |
| Provider stalls mid-stream | Idle timeout after 60 s of silence: `error` frame + 504. Preview text stays visible but grey until the user retries or navigates. |
| Chunk parse error | `error` frame + 502, same as chat's `stream chunk error` branch but without persisting partial text. |
| WS not yet connected when the first snapshot fires | Next snapshot repaints the whole document. At most 80 ms lost. |
| Page reload during stream | Post view remounts, WS reconnects, snapshots resume. `done` triggers a refetch for the final document. |
| Two enhances for the same meeting overlap | Already possible today; last `done` wins in SQLite. Snapshots interleave visibly. Not addressed here; a per-meeting in-flight guard is a separate ticket. |
| Model emits `<think>` reasoning | `ThinkStripper` inside `yogurt-llm` already removes it stream-side; `strip_model_reasoning` still runs on the full text as the second layer. |
| Model emits prompt scaffolding tags | Visible in the preview for the duration of the stream, stripped in the final document. Acceptable; it is rare and clearly temporary. |

### Testing

- `crates/yogurt-server/tests/enhance_endpoint.rs`: assert the WS sees `sending`, at least one `streaming` frame with a non-empty `text` equal to the mock content, then `done`; assert the persisted `enriched_md` is unchanged from today's expectations.
- `web/src/lib/ws.test.ts` (or the existing `useEnhanceProgress` tests): `text` is exposed on `streaming` and cleared on `done` / `error`.
- `web/src/routes/MeetingPost.test.tsx`: a `streaming` frame renders the preview grey and read-only on the Enhanced tab; the POST response replaces it; `autoEnhance` state fires exactly one POST.
- E2E against the real binary with a real provider: End meeting, confirm text appears within ~2 s, confirm the settle preserves user lines in black, confirm Re-enhance replaces the old summary on the first chunk.
- `docs/ARCHITECTURE.md` §5 sequence diagram updates to show the streaming loop; §11 call-count note changes "non-streaming" to "streaming".

## Decisions (confirmed 2026-08-29)

1. `streaming` frames carry snapshots of the full accumulated text, not deltas.
2. Preview style is approach A: all grey while streaming, merged document on done.
   B stays documented as the upgrade path.
3. End meeting navigates to the post view immediately and the post view streams.
4. The lilac banner stays during streaming with a live character count.
5. The editor is read-only while streaming.
