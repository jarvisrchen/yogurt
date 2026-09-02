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
  `scripts/tail-transcript.sh` and the raw WS frames (see [docs/DEBUGGING-TRANSCRIPTS.md](DEBUGGING-TRANSCRIPTS.md)) already expose the pieces; this needs turning into something an agent can watch continuously and reason about live, not just a one-off dump.
  </details>

## LLM

## CLI

- [ ] **CLI-6** `yogurt ctl` second slice, and the control skill rewritten around a generated command block
  <details>
  <summary>Details</summary>

  `settings`, `provider`, `models`, `ws`, `meeting mute | search | delete`, once CLI-4's client and port discovery are proven.
  Then `.claude/skills/yogurt-control/SKILL.md` shrinks to about 150 words: the command block between generator markers (kept honest by the `--help` drift test from DX-4), a Feature Map link, and three rules.
  Only after the brew release that carries `ctl`: the README's `npx skills add` path installs the skill standalone.
  Design: `docs/.planning/agent-workflow.md`, section 4D, D1 and D2.
  </details>

## Developer experience

- [ ] **DX-1** `just test` is a weaker gate than CI, and neither exercises the real binary
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
  </details>

- [ ] **DX-9** Rewrite AGENTS.md around the six-command lifecycle, with the cloud-session paragraph
  <details>
  <summary>Details</summary>

  About 480 words: constraints, `start`, `ticket`, `dev-bg`, `pr`, `land`, pointers.
  Evicted rationale (build splice, relative paths, port pair) goes to CONTRIBUTING.md's worktree section first.
  One paragraph: tickets under `web/` or `docs/` may run as cloud sessions; Rust stays local with the macos-26 runner as the cloud verifier.
  Only after DX-2 to DX-5 exist.
  Design: `docs/.planning/agent-workflow.md`, sections 4E (E1) and 4F (F1).
  </details>

- [ ] **DX-10** `scripts/check-published.sh`: tap formula version, release assets, README formula names and model mirror URLs still resolve
  <details>
  <summary>Details</summary>

  Runnable by hand after a formula edit and weekly from a scheduled ubuntu workflow; opens an issue on failure.
  The one drift a PR-time check cannot see (the v0.3.0 README-versus-tap failure).
  Design: `docs/.planning/agent-workflow.md`, section 4F, F2.
  After DX-7.
  </details>
