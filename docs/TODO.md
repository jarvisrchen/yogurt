# TODO

Open work for yogurt.
Add new items at the bottom of the relevant section, or create a new section.
Keep one item per `- [ ]` line so it's easy to check off.

## Ticket IDs

Every item carries a `**<PREFIX>-<N>**` ID right after the checkbox so it can be referenced in conversation, commits, and PR titles.

| Prefix | Section |
| --- | --- |
| `UI` | UI |
| `MTG` | Meetings |
| `AUD` | Audio |
| `LLM` | LLM |
| `CLI` | CLI |
| `DX` | Developer experience |

Numbers are per prefix and allocated once: run `just ticket next <PREFIX>` to get the next ID; it is the highest existing number for that prefix plus one, counting `docs/TODO-DONE.md` too.
Never reuse or renumber an ID, including when an item moves to DONE.
A new section gets a new short prefix added to the table above.

## Referencing attachments

Drop screenshots and photos in `attachments/`, then reference them inline in the TODO entry.

When the user sends a photo, save it into `attachments/` with a `YYYY-MM-DD-<slug>.<ext>` filename, then add a TODO entry that links to it.

Example entry:

```
- [ ] **UI-0** Settings modal cuts off the API key field on small windows
  <details>
  <summary>Details</summary>

  ![settings modal cutoff](attachments/2026-08-28-settings-modal-cutoff.png)
  </details>
```

## Done items

Run `just ticket done <ID> --note-file <path>`: it checks off the item (`- [x]`), moves the whole block to the bottom of `docs/TODO-DONE.md`, and appends the note file's content as a paragraph inside the block, for example `Landed in #N (date). ...`.
The note is a file, not a shell argument, because real resolution notes contain backticks, dollar signs and JSON literals that do not survive shell quoting.

## UI


## Meetings

- [ ] **MTG-10** Enhanced summary visibly flashes while streaming on longer meetings
  <details>
  <summary>Details</summary>

  Richard sees the enhanced summary pane flash and sometimes appear to get rewritten while enhance is streaming, worse the longer the meeting.

  Root cause: `enhance.rs` emits an `enhance_progress { phase: "streaming" }` WS frame roughly every 80ms (`STREAM_SNAPSHOT_INTERVAL`, `crates/yogurt-server/src/enhance.rs:388`), and each frame carries the *entire* accumulated markdown generated so far, not a delta - a deliberate choice per `docs/archive/.lavish/enhance-streaming-design.html` (self-healing on reconnect).
  On the client, `useEnhanceProgress` (`web/src/lib/ws.ts:460-479`) applies every frame with no throttling/coalescing, feeding it straight into `YogurtEditor`'s `enrichedMarkdown` prop.
  The editor's effect (`web/src/editor/index.tsx:103-112`) responds to every change by re-running `markdownToHtml` (full `MarkdownIt.render()` + `DOMPurify.sanitize()`, `web/src/editor/markdown.ts:51-55`) over the whole accumulated string, then replaces the entire ProseMirror doc via `editor.commands.setContent()` - a full parse-and-rebuild, not an append.

  Per-tick cost scales with total text generated so far, not with the size of the new chunk. For a short meeting this is cheap enough to look smooth; for a long one it eventually blows through the 80ms budget, frames queue up and land in bursts, and each full-document swap reads as a visible pop (the `[data-streaming] .ProseMirror` grey recolor and the pulsing-caret `::after`, `web/src/index.css:174-210`, also restart on every replace).

  Two directions, both revisit tradeoffs the archived design doc already weighed:
  1. Client-side: throttle/coalesce applied frames by elapsed time or text length instead of applying every WS frame, and/or move to incremental append (diff old vs new markdown, patch the ProseMirror doc) instead of full `setContent` each time.
  2. Server-side: send deltas instead of full snapshots (the design doc's rejected "Option B"), trading away the reconnect self-healing property unless deltas are paired with a periodic full resync.
  </details>

## Audio

- [ ] **AUD-2** Add NVIDIA Parakeet v3 to the local STT model download
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

- [ ] **AUD-3** Capture more than one audio input at a time (multiple mics, or a mic plus one specific app)
  <details>
  <summary>Details</summary>

  Capture is exactly two fixed channels today: one mic device and whole-system loopback.
  `Channel` (`crates/yogurt-audio/src/frame.rs:19`) has precisely `Mic` and `System`, `spawn_mic_capture` (`crates/yogurt-audio/src/mic.rs:175`) takes a single `Option<&str>` device name, and the mid-recording hot-swap (`Registry::switch_mic_device`, `crates/yogurt-server/src/meetings.rs:709`) changes *which* single device is live, never adds a second.

  The headphones-plus-Zoom case in the ask is already covered, though: the SCK content filter is display-rooted with no window exclusions (`crates/yogurt-audio/src/system.rs:165`), so Zoom, Chrome, and everything else already land on `Channel::System` while the headphone mic lands on `Channel::Mic`. What is genuinely missing is N mics (two people at one laptop, or a USB interface alongside the built-in) and per-app system audio instead of all-or-nothing.

  Two independent halves, scope them separately:

  1. **N mic devices.** `Channel` becomes indexed rather than a two-variant enum, which ripples into the `tokio::select!` fan-in, the broadcast-sender-per-channel model, and speaker attribution in the transcript. Cheaper alternative: keep two channels and mix N devices down into `Channel::Mic` before the ring, which costs nothing downstream but throws away per-device separation. Decide which before writing code.
  2. **Per-app system audio.** `SCContentFilter` can be built app-rooted instead of display-rooted, so "just Zoom" is a filter change rather than an architecture change. Check the app-filter API is available at the macOS 13 deployment target.

  Zero-code answer that exists today: a macOS aggregate device (Audio MIDI Setup) presents N inputs to cpal as one device. Worth documenting in the README before building anything.
  </details>

- [ ] **AUD-7** Give an agent a live way to debug which channel (mic vs system) picked up which audio during a real meeting
  <details>
  <summary>Details</summary>

  It's not obvious today which of "me" (mic) and "them" (system) is actually capturing what, especially when Chrome is playing a video/call to simulate the other side of a meeting.
  Richard wants a workflow where he says "help me debug this meeting," an agent starts actively tailing the relevant logs/transcript, he starts the recording and plays audio through Chrome to simulate someone talking in Zoom/Slack, and the agent can narrate what it's seeing (which channel picked up the audio, timing, drops) while he narrates what he sees in the UI.
  `yogurt ctl meeting transcript --follow` and `yogurt ctl ws` (see [docs/DEBUGGING-TRANSCRIPTS.md](DEBUGGING-TRANSCRIPTS.md)) already expose the pieces; this needs turning into something an agent can watch continuously and reason about live, not just a one-off dump.
  </details>

- [ ] **AUD-8** A force-killed `yogurt` can leave a stale SCK/mic capture session that blocks the next recording
  <details>
  <summary>Details</summary>

  Found while verifying DX-1's hardware smoke test: SIGKILLing a `yogurt` process mid-recording skips `Drop`, so `AudioStream`'s SCK + cpal teardown never runs.
  A subsequent `start()` (same machine, same or a different `yogurt` process) then hangs for several minutes opening its own capture session, apparently waiting for macOS to reclaim the still-held OS-level resources from the killed process; a direct retry once that window passed opened and closed cleanly in under a second.
  Normal shutdown (Cmd-Q, or `ctl meeting stop`) already goes through `Registry::stop()`'s graceful teardown (200ms watchdog), so this only bites a hard kill - Activity Monitor "Force Quit", a crash, `kill -9` - while a meeting is recording.
  Worth understanding whether this is inherent to `SCStream`/`cpal` cleanup timing (nothing to do beyond documenting it) or something `yogurt` could shorten, e.g. detecting an orphaned session on next launch and giving a clear error instead of a silent multi-minute hang.
  </details>

## LLM

## CLI

- [ ] **CLI-6** the control skill rewritten around a generated command block
  <details>
  <summary>Details</summary>

  The `yogurt ctl` second slice - `settings`, `provider`, `models`, `ws`, `meeting mute | search | delete` - landed in #67.
  Remaining scope: `.claude/skills/yogurt-control/SKILL.md` shrinks to about 150 words - the command block between generator markers (kept honest by the `--help` drift test from DX-4), a Feature Map link, and three rules.
  That rewrite waits for the brew release that carries `ctl`: the README's `npx skills add` path installs the skill standalone, so a skill naming `ctl` commands must not precede a binary that has them.
  Design: `docs/.planning/agent-workflow.md`, section 4D, D1 and D2.
  </details>

## Developer experience

