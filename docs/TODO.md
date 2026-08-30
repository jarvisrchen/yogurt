# TODO

Open work for yogurt.
Add new items at the bottom of the relevant section, or create a new section.
Keep one item per `- [ ]` line so it's easy to check off.

## Referencing attachments

Drop screenshots and photos in `attachments/`, then reference them inline in the TODO entry.

When the user sends a photo, save it into `attachments/` with a `YYYY-MM-DD-<slug>.<ext>` filename, then add a TODO entry that links to it.

Example entry:

```
- [ ] Settings modal cuts off the API key field on small windows
  <details>
  <summary>Details</summary>

  ![settings modal cutoff](attachments/2026-08-28-settings-modal-cutoff.png)
  </details>
```

## Done items

Check off an item (`- [x]`) when the work lands; move it into the matching subsection below to keep the open work above scannable.

## UI

- [ ] BASE URL and MODEL fields overlap on long URLs in provider cards
  <details>
  <summary>Details</summary>

  On the Settings → LLM providers page, the Google Gemini card (active) has its BASE URL value `https://generativelanguage.googleapis.com/v1beta/openai` running straight through the MODEL value `gemini-2.5-flash`. The two are visually overlaid instead of sitting side-by-side in their two grid columns.

  Root cause is in `web/src/components/settings/ProviderRow.tsx:111` — `<div className="grid grid-cols-2 gap-x-6 gap-y-3">`. Grid items default to `min-width: auto`, so a child with non-wrapping content (here the URL wrapped in `break-all` text but the inner `<code>` is still wider than its column at full URL length) refuses to shrink below its intrinsic content size and overflows into the sibling column.

  Fix: add `min-w-0` to each `Field` child inside the grid (and probably `overflow-hidden` + `truncate` or `break-all` on the `<code>` is already there — keep `break-all`, just unblock the column from shrinking). Verify on Gemini (`/v1beta/openai`), OpenAI (`/v1`), Anthropic (`/v1/messages` is different shape so double-check that case too), and any user-added URL.

  Visual evidence:
  ![BASE URL runs through MODEL field](attachments/2026-08-29-base-url-model-overlap.png)
  </details>

- [ ] Add a "Test" button for the Deepgram key in Settings → Transcription
  <details>
  <summary>Details</summary>

  The LLM providers section has a Test button that does one live round-trip and reports a verdict inline (`web/src/components/settings/TestKeyButton.tsx`, backed by `POST /api/settings/providers/{id}/test`). The Transcription section's Cloud card has no equivalent, so a wrong or expired Deepgram key stays silent until a meeting produces no transcript, which is the worst possible time to find out.

  Two halves:

  1. Backend: a test endpoint for the STT key, alongside the existing `POST /api/settings/stt/key` in `crates/yogurt-server/src/api/settings.rs`. There is no STT equivalent of `test_provider` today. Deepgram's cheapest liveness check is probably a short authenticated request rather than opening a streaming socket; pick one that fails fast on a bad key without burning quota.
  2. Frontend: render the button on `CloudSTTCard` in `web/src/components/settings/STTPicker.tsx`. `TestKeyButton` is close to reusable as-is, but it is currently hardcoded to `settingsApi.testProvider(providerId, ...)`, so it needs the mutation passed in or a sibling component. Keep its two good behaviors: test the stored key when the input is empty, and hide the verdict once the draft no longer matches what was tested, so a green tick never sits next to edited text.

  Match the LLM card's verdict styling (matcha for ok, straw for failure) so the two sections read as one system.
  </details>

## Meetings

## Audio

- [ ] Add NVIDIA Parakeet v3 to the local STT model download
  <details>
  <summary>Details</summary>

  The model registry at `crates/yogurt-stt/src/models.rs` only ships whisper.cpp checkpoints today (tiny.en, small.en, medium.en, large-v3), pulled by `scripts/refresh-model-hashes.sh`.
  Add Parakeet v3 as a downloadable local model - new `ModelSpec` entry, download URL, SHA256 pin, and the engine adapter if Parakeet can't reuse the whisper.cpp runtime.
  Heads-up: Parakeet is an NVIDIA NeMo checkpoint, not a ggml/gguf file - decide the engine first (NeMo ONNX export, whisper.cpp's Parakeet backend, or a new `yogurt-stt` engine next to `WhisperLocal`) before scoping this.
  Scoping research done (2026-08-29), no code yet - engine decision needed first:
  Recommendation: sherpa-onnx with a new `ParakeetLocal` engine next to `WhisperLocal` (official Rust bindings, statically linkable, ships a tested parakeet-tdt-0.6b-v3 int8 archive with a stable URL).
  parakeet.cpp is the cleanest ggml/Metal fit but has no Rust bindings and is pre-1.0 - revisit in 6-12 months.
  Open decisions before scoping: accept Apache-2.0 (sherpa-onnx) alongside MIT; its build.rs downloads a prebuilt static lib at build time; CPU-only inference needs a perf spike on Apple Silicon (no Metal path); Parakeet weights are CC-BY-4.0, so attribution may need surfacing in Settings.
  </details>

## DONE

Closed-out work, kept here for context. Move a `- [x]` item here when the work lands.

<details>
<summary>Click to expand</summary>

### UI

- [x] Chat input loses its pill shape on focus
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

- [x] Thick blue ring around the AI notes section when interacting
  <details>
  <summary>Details</summary>

  In the in-meeting notes panel, typing into "your notes" leaves a heavy blue/purple border wrapped around the AI-generated content on the right.
  Same on post-meeting notes: clicking into the meeting summary section triggers the same thick ring around the whole AI area.
  Nothing is actually focused in those cases - it's a styling bug (likely `:focus-within` or panel-active state styling the wrong target).
  Remove or restyle so it doesn't read as a focus indicator.

  Visual evidence:
  ![notes AI section shows a thick focus ring](attachments/2026-08-28-notes-ai-section-focus-ring.png)
  </details>

### Meetings

- [x] Black "your notes" vs grey AI blocks in the enhanced summary is confusing and misapplies when notes are empty
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

- [x] Add an "enhanced with" pill next to the existing STT pill on meeting headers
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

- [x] LLM pill missing on a live meeting (STT pill shows, LLM pill doesn't)
  <details>
  <summary>Details</summary>

  Starting a new meeting showed the STT pill but no LLM pill.
  Not a bug in the pill: `stt_engine` is stamped on the row at recording start (`routes.rs::start_meeting`), while `llm_model` is only stamped by `enhance.rs` after a successful enhance, so a live meeting has nothing to render and `LlmPill` correctly refuses to guess.

  Fixed 2026-08-30 in the live header only (`web/src/routes/Meeting.tsx`): when the row has no `llm_model`, fall back to the active provider's model from the already-cached `useSettings()`, mirroring the existing `stt_engine ?? activeRecording.stt` fallback.
  `MeetingMetaPills` grew an `llmPending` flag that flips the pill's tooltip to "Will enhance with <model>" so a pre-enhance pill doesn't claim the meeting was already enhanced.
  Post-meeting header and library card are untouched - a stored meeting that was never enhanced still shows no pill.
  Verified E2E against the running backend: "+ New meeting" now renders `[Local · large-v3-turbo] [MiniMax-M3]` while recording.
  </details>

- [x] Short / empty post-meeting meetings should say "too short" and return to the library
  <details>
  <summary>Details</summary>

  When a meeting ends but has nothing meaningful to transcribe or enhance (very short duration, mostly silence, audio captured but no usable content), the post-meeting view runs enhance on near-empty input today.
  Detect this case and surface a clear "Meeting too short" state instead of producing augmented notes from nothing.
  Auto-return to the library (home screen) so the user isn't left in a blank post-meeting view they have to navigate out of manually.
  </details>

- [x] Delete-confirm check should overlay, not shift the post-meeting topbar
  <details>
  <summary>Details</summary>

  In the post-meeting view, clicking the trashcan button shows the confirm checkmark inline, which then pushes the Enhance button and the rest of the top bar around (format/visual layout changes mid-click).
  Make the confirm step overlay-anchored under the trashcan (floating popover) instead of taking up flow space, so the topbar stays put.

  Visual evidence:
  ![post-meeting delete confirm with check](attachments/2026-08-28-delete-confirm-check-appears.png)
  ![post-meeting delete confirm shifts the topbar](attachments/2026-08-28-delete-confirm-shifts-topbar.png)
  </details>

- [x] Live transcript duplicates when audio plays through the machine
  <details>
  <summary>Details</summary>

  During a live meeting, audio played on the Mac (system-audio playback, video with speech, etc.) shows up in the live transcript as duplicates rather than once.
  Root cause likely lives between `crates/yogurt-audio`'s mic and ScreenCaptureKit channels (acoustic echo into the mic plus digital capture, or an internal double-publish) - investigate and de-dupe.

  Visual evidence:
  ![transcript duplicates on machine audio](attachments/2026-08-28-transcript-duplicates-on-machine-audio.png)
  </details>

### Audio

- [x] Add the ability to delete a downloaded local STT model
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

### LLM

- [x] Test button for the API key should stay available on provider cards with saved models
  <details>
  <summary>Details</summary>

  On a provider card where a model is already saved in the MODEL field, the API KEY `Test` button should still be usable as a probe — typing a new draft key and clicking `Test` should run the same connection check it runs on a keyless card.
  If the button currently gets disabled, hidden, or stomped by some save/refresh state when a model is present, fix it so `Test` is reachable from every state a provider card can sit in (no key + no model, stored key + no model, no key + saved model, stored key + saved model).
  Likely suspects: `ApiKeyInput.canTest` (`web/src/components/settings/ApiKeyInput.tsx:83`) only looks at `draft || hasStoredKey` and ignores MODEL state, so the bug is probably upstream — a parent renders/hides `ApiKeyInput` based on MODEL state, or `onSaved` tears down the input before the user gets to test.
  Verify by hand: load a provider card with a saved model, paste a draft key, click `Test`, confirm a `✓` / `✗` line appears and the draft survives the click.

  Cause was upstream of `canTest`, as suspected: `ProviderRow` renders the whole `ApiKeyInput` only while `keying` is true, and `onSaved` flips it back to false — so an already-keyed provider (the one most worth probing) had no reachable `Test` at all.
  Fixed by extracting `TestKeyButton` from `ApiKeyInput` and rendering it next to `Replace key` in the collapsed state, where it tests the stored key.
  </details>

- [x] Add Google Gemini and DeepSeek as built-in LLM provider presets
  <details>
  <summary>Details</summary>

  Both speak the OpenAI `/chat/completions` shape, so this needed no new client code in `yogurt-llm` - just `Preset` entries plus the matching `ENV_PRESETS` rows (`YOGURT_GEMINI_API_KEY`, `YOGURT_DEEPSEEK_API_KEY`) so keys seed into the Keychain on first run.
  Gemini goes through Google's OpenAI-compatible shim at `https://generativelanguage.googleapis.com/v1beta/openai`, not the native `generativeLanguage` REST shape.
  Neither was strictly blocked before this - "Add provider" already accepts a free-form base URL - so the win is one click instead of a URL nobody remembers.
  While here: `tests/bootstrap.rs` hand-listed the env vars it cleared, so adding a preset broke it only on machines that exported the new key.
  It now clears everything `bootstrap::env_key_vars()` reports.
  </details>

</details>