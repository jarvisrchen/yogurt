# Pitfalls Research

**Domain:** macOS local-first meeting copilot (ScreenCaptureKit + whisper.cpp + TipTap + single Rust binary + Homebrew)
**Researched:** 2026-06-25
**Confidence:** HIGH for documented gotchas (ScreenCaptureKit/TCC, whisper-rs build, rust-embed/Vite, TipTap collab); MEDIUM for runtime behaviors (broadcast lag, audio sync, model verification); LOW where sourced only from forum threads.

---

## Critical Pitfalls (Project-Killers)

### Pitfall 1: TCC permission re-prompts every dev build (Screen Recording)

**What goes wrong:**
The macOS TCC database keys Screen Recording (and Microphone, Accessibility, etc.) permissions on the binary's *designated requirement* — the code signature identity. Every time the build's signing identity changes (or it's unsigned and the binary path/inode changes), TCC treats it as a brand-new app: the prompt re-appears, the user has to navigate to System Settings → Privacy → Screen Recording, toggle Yogurt on, and **restart**. Worse, on macOS Sequoia (15) the OS re-prompts even stable, signed apps every ~30 days. During development on an unsigned `cargo run` binary, this happens on *every rebuild* — a developer with frequent rebuilds will burn 30 seconds × N restarts per day.

**Why it happens:**
- TCC uses the cdhash of the signed bundle (or the path for unsigned binaries) as the identity key.
- `cargo build` produces unsigned binaries by default; the inode changes; TCC invalidates.
- Even with a stable Developer ID, Sequoia (macOS 15) intentionally re-prompts monthly for all "screen capture" entitlements as a privacy mitigation post-2024 controversy.
- Without notarization + hardened runtime, end-users hit Gatekeeper "damaged" errors that look like Yogurt is broken.

**How to avoid:**
- Sign dev builds with a stable self-signed cert (`codesign --sign "Yogurt Dev" --force --options runtime target/debug/yogurt`). Bake into `cargo make` or a `make dev` recipe so devs don't think about it.
- Use a stable Developer ID Application certificate for releases. Pin it in the GitHub Actions release workflow via secrets.
- Set the bundle ID / `CFBundleIdentifier` *now* (`ai.yogurt.app` or similar) and never change it. Changing it is equivalent to shipping a new app — all permissions reset.
- Notarize releases with `notarytool` (xcrun notarytool submit) and staple. Without this, users see "Yogurt can't be opened because Apple cannot check it for malicious software" *before* Yogurt even reaches the Screen Recording prompt.
- Document the Sequoia 30-day re-prompt in the FAQ — it's not a Yogurt bug.
- Ship `tccutil reset ScreenCapture ai.yogurt.app` in a `yogurt doctor` command for users with stuck permissions.

**Warning signs:**
- "I granted permission but Yogurt still says Screen Recording not granted" → almost always means the binary that asked for permission and the binary that's running are different identities.
- Yogurt missing from the System Settings → Screen Recording list entirely → the binary identity changed between the prompt and the user opening Settings; the entry is orphaned.

**Phase to address:**
Phase 2 (audio capture) for the in-product UX (restart-required screen, "Open System Settings" deep link via `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`). Phase 9 (distribution) for signing + notarization in CI. The PRD's "Restart Yogurt" recovery screen (§5.11) is necessary but not sufficient — without the codesign discipline above, users will hit it constantly.

**Severity:** Project-killer (without codesigning, every dev rebuild and every release re-prompts; users will assume the app is broken).

---

### Pitfall 2: `screencapturekit` Rust crate gaps for audio-only loopback

**What goes wrong:**
The `screencapturekit` crate (doom-fish/svtlabs) wraps Apple's SCK, but its primary design target is video capture; audio-only loopback streams have documented "samples not received / empty buffer" issues and a memory leak that was patched as recently as the 1.x line. On macOS 14 you also need to opt into `SCStreamConfiguration.capturesAudio` *and* set `excludesCurrentProcessAudio` correctly, otherwise Yogurt hears itself (the playback of streaming partials, the UI ding) and feedback-loops into the transcript. On macOS 15+ the API requires `SCContentSharingPicker` for app-specific audio capture in some configurations, which the crate may not yet expose.

**Why it happens:**
- SCK was added to macOS 13 and has had material API additions in 14 and 15; the Rust crate inevitably trails.
- Audio loopback is a less-exercised SCK code path; bugs survive longer.
- The crate's examples are nearly all video; audio configuration is undocumented in the README.

**How to avoid:**
- **Phase 0 / Phase 2 spike (1–2 days, before committing).** Build the smallest possible test: start an SCK stream, capture 30 seconds of system audio while playing a YouTube video, write to WAV, verify in Audacity that the waveform looks right and isn't silent / clipped / channel-swapped. **Do this before designing the audio pipeline architecture.** The PRD §13 already flags this as a risk; treat it as a Phase 2 gate, not a Phase 2 detail.
- Have a fallback ready: a ~150-line Swift binary (`yogurt-audio-helper`) invoked over a Unix domain socket. The PRD already notes this as the mitigation; make sure Phase 2 leaves room for it.
- Set `excludesCurrentProcessAudio = true` from day one. Test by playing audio *from the Yogurt UI itself* (e.g., a notification ding) during capture — if the transcript shows that sound, the flag is wrong.
- Pin the `screencapturekit` crate version. Don't `^` it. New patches have changed audio buffer semantics.
- Convert SCK's CMSampleBuffer to PCM yourself; don't trust the crate's high-level audio helper if there is one (early versions silently dropped channels).

**Warning signs:**
- Capture works for video frames but `audio_sample_handler` fires zero times → `capturesAudio` flag not set or display filter excludes audio.
- Audio captures but is silent for 5–10 seconds at start → buffer alignment / first-chunk drop bug; need to discard the first N samples.
- Yogurt hears its own UI sounds in the transcript → `excludesCurrentProcessAudio` not set.
- Memory grows linearly during a meeting → buffer leak; either the crate's bug or you forgot to release `CMSampleBuffer`s.

**Phase to address:** Phase 2 (audio capture) — explicitly gate Phase 3 (Cloud STT) on Phase 2 passing the spike. PRD §12 already orders these correctly; just make the spike a Phase 2 deliverable, not a Phase 2 assumption.

**Severity:** Project-killer (no audio = no transcript = no augmented notes = the entire product fails).

---

### Pitfall 3: Mic + system audio timestamp drift

**What goes wrong:**
The mic stream (CoreAudio / AVAudioEngine) and the system-audio stream (ScreenCaptureKit) arrive on *different clocks*. Mic timestamps come from the input device's sample clock; SCK audio timestamps come from the display server's media clock. They drift — typically 0.5–2 seconds per hour, but can be worse if the user changes audio devices mid-meeting (AirPods connect/disconnect, USB-C dock plugged in). If Yogurt naively uses `now()` at the moment each buffer arrives in the Rust handler, the drift is amplified by Rust scheduling latency. Result: "Me" turn at `00:11:02` overlaps with "Them" turn at `00:10:55` on the same wall-clock moment, transcript looks like nonsense, the `↳ HH:MM` deep-links jump to the wrong moment.

**Why it happens:**
- Two independent clocks; nobody synchronizes them for you.
- Even Granola has subtle drift visible if you watch long enough; they get away with it because their UI doesn't surface frame-accurate sync.
- Tokio task scheduling can add 10–100ms jitter to "now" measurements on top.

**How to avoid:**
- Establish a single **meeting-relative clock**: at meeting start, capture `Instant::now()` as `t0`. Every audio buffer (from both streams) gets stamped with `chunk_arrival - t0` *plus* the buffer's source-clock duration accumulator. Use the source clock for *relative* offsets within a stream, and the meeting clock to align streams.
- For SCK audio, use `CMSampleBuffer.presentationTimeStamp` — this is the SCK media-clock time, not wall-clock. Convert to a meeting-relative offset by subtracting the first sample's PTS.
- For mic, use CoreAudio's `mSampleTime` from `AudioTimeStamp`, with the same first-sample-as-zero convention.
- Re-sync at every silence gap detected by VAD (Phase 8). If both streams have ~500ms of silence, snap them to the same offset — small drift is visible only across long meetings.
- Test: record a meeting where you talk and the other side talks alternately for 60 minutes. The end-of-meeting drift between mic and system should be < 250ms.

**Warning signs:**
- Long meetings (45 min+) show transcript lines that look temporally out-of-order at the end.
- Clicking `↳ HH:MM` deep-link plays back from a noticeably wrong moment (e.g., the speaker hasn't started yet).
- Drift grows monotonically as the meeting goes on.

**Phase to address:** Phase 2 (audio capture) — design the clock model when wiring the broadcast channels. Adding it later is a refactor of every timestamp in the system.

**Severity:** Project-killer for the deep-link feature (which is the *signature* of augmented notes per PRD §5.3); merely annoying for the transcript panel.

---

### Pitfall 4: TipTap mark loss on markdown round-trip

**What goes wrong:**
The PRD's hero UX (PRD §5.3, §16.7) depends on a TipTap `aiGrey` mark with a `transcriptTs` data attribute applied to LLM-added inline runs. Standard markdown has no syntax for "this run is grey AI text." When you serialize the editor to markdown for storage (`notes_md` / `enriched_md` per PRD §9), the marks are lost — you can re-parse the file and *every* bullet is treated as user-authored (black). Edit-promotes-to-black breaks because there's no grey to demote. Re-enhance breaks because the LLM can't tell what's already AI-written. Worst case: Yogurt overwrites the user's black edits with grey AI rewrites because the round-trip lost the distinction.

This is *also* why PRD §13 already flags "TipTap's mark system may struggle with structural diffing" — but the round-trip problem is downstream of that, and equally fatal.

**Why it happens:**
- Markdown is a lossy serialization for editor state. CommonMark spec has no extension point for inline annotations beyond emphasis, strong, code, links.
- TipTap's built-in markdown extension serializes only the marks it knows about; custom marks need custom `toMarkdown`/`parseMarkdown` rules, and even then non-standard syntax breaks user-grepping (a stated goal in PRD: "markdown is the source of truth for grep").
- The naive fix (HTML-in-markdown like `<span data-ai="true">...</span>`) makes the export ugly when opened in Obsidian or `cat`.

**How to avoid:**
- Persist two artifacts, not one:
  - `notes_md` — pure user markdown, no marks, what `grep` sees, what the export file contains.
  - `enriched_doc_json` — the full TipTap/ProseMirror JSON document, marks intact. This is what the editor loads; this is what diffs against.
- The exported `.md` file is human-friendly only — use a footer comment block like `<!-- yogurt-doc-state: <gzip+base64 JSON> -->` if needed for round-trip, but it's *recovery*, not the primary load path. Document this trade-off.
- Schema-version the ProseMirror JSON. ProseMirror is strict: content that doesn't fit the current schema is *deleted on load*. Use TipTap v3's content migrations (or a hand-written migrator) from day one. Without this, the first time you change the `aiGrey` mark's attrs (e.g., add `model_id`), every existing note's grey content vanishes.
- Test: write a meeting, close Yogurt, hexdump the `.md` to confirm pure markdown; reopen Yogurt, verify grey ranges remain grey, edit a grey range, verify it goes black, re-enhance, verify the now-black range survives.

**Warning signs:**
- After restart, every bullet looks black (or every bullet looks grey).
- After re-enhance, the user's edits get clobbered.
- Schema change ships a release; first run of the new build silently drops marks on old notes.
- ProseMirror console warnings about "Invalid content for node X, dropping."

**Phase to address:** Phase 4 (Augmented notes hero) — design the persistence model *before* writing the mark. PRD §12 Phase 4 already calls out "markdown round-trip" as a deliverable; make `enriched_doc_json` part of the schema (PRD §9 currently only has `enriched_md` — add `enriched_doc_json TEXT`).

**Severity:** Project-killer for the augmented-notes UX, which the PROJECT.md "Core Value" explicitly says is the *single thing* that determines product success.

---

### Pitfall 5: whisper.cpp streaming mode quality cliff vs batch mode

**What goes wrong:**
whisper.cpp has two operating modes: batch (process N seconds, return high-quality transcript) and streaming (process 30ms chunks live). The streaming path is materially worse: skipped audio, latency that *grows* (3s → 10s → 30s) until the process is killed, and on weak hardware (M1 Air, Intel) it falls behind real-time entirely. The PRD's promised "< 3s lag using local `whisper.cpp small.en`" (§5.2) is achievable *only* with careful chunked decode + VAD + sliding context window. The naive "feed every PCM buffer to whisper" approach will produce a transcript noticeably worse than even Deepgram's free tier.

**Why it happens:**
- Whisper was trained on 30-second windows; streaming partials don't have the temporal context the model expects.
- Most "streaming" wrappers re-decode an overlapping window every chunk, multiplying compute and creating non-monotonic partials ("hello world" → "hello world how are you" → "hello where you" if the window slid).
- Metal acceleration helps throughput but doesn't change the model's quality-vs-latency curve.

**How to avoid:**
- Use VAD (voice-activity detection) to segment audio into utterances, then run whisper *batch-mode* on each segment as it ends. This is what whisper.cpp's `stream` example actually does well, and the basis of `whisper_streaming` papers.
- whisper.cpp ships `whisper_full_with_state` for parallel decoding across two `whisper_state`s — use it to run a "settled" decode (final, slower) behind a "preview" decode (partial, fast) per stream. Display the preview as transcript-in-progress, replace with the settled version when it arrives.
- Cap streaming-mode promises in copy. The PRD's "local STT positioned as the privacy escape hatch, not the daily-driver" framing (§13) is the right one — don't let the UI claim parity with cloud.
- Benchmark on an M1 Air, not just an M3 Max. The PRD claims "M1 Air runs small.en at 10–15x real-time" for *batch*; in streaming-mode chunked decode the headroom shrinks dramatically.
- Run two model instances if RAM permits: `tiny.en` for live preview, `small.en` for settled. The latency win is enormous; the cost is ~150MB extra RAM.

**Warning signs:**
- Streaming partials produce visibly different text than the final transcript for the same audio.
- Latency grows during the meeting rather than staying stable.
- Transcript repeats phrases ("the the the cat sat sat sat").
- M1 Air falls behind in the first 5 minutes of a meeting.

**Phase to address:** Phase 8 (Local STT) — but design VAD into Phase 2's audio pipeline (chunking concerns are upstream of STT).

**Severity:** Annoyance for v1 (positioning of local STT as fallback covers it), but project-killer for the OSS-credibility narrative if local mode is unusably worse than cloud.

---

### Pitfall 6: rust-embed serves stale assets in dev / breaks SPA routing in prod

**What goes wrong:**
The PRD's distribution story is "single static Rust binary with embedded web assets via `rust-embed`." Two failure modes are near-universal:

1. **Asset paths.** Vite's default build outputs `<script src="/assets/index-abc123.js">` (absolute paths). Served from `axum`'s embed handler, these work *if* the binary is the root server — but break the moment anyone tries to embed Yogurt under a path prefix, and produce subtle bugs when the cached browser holds an old `index.html` that references a now-renamed hash file.
2. **SPA fallback.** React Router's `/meetings/abc-123` route is unknown to axum; reloading on that URL returns 404. Devs ship a release, do `yogurt start`, navigate to `/library`, refresh — blank page. The fix is `fallback(ServeFile::from(index.html))`, but with `rust-embed` you don't have a file, you have bytes; you need a custom fallback handler that re-serves the embedded `index.html` for any unknown route.
3. **Dev/release divergence.** `--dev` proxies to Vite; release serves embedded. They behave differently for the same code path (CSP headers, MIME types, gzip). A bug only seen in release ships unnoticed because dev is the hot loop.

**Why it happens:**
- Vite assumes deployment to a CDN or static-file server; embedded-asset serving is an afterthought.
- `rust-embed` provides bytes, not paths; SPA fallback patterns assume filesystem.
- Dev/prod parity isn't a concern until release; then it's a release blocker.

**How to avoid:**
- Set `base: './'` in `vite.config.ts` so all asset references are relative. Verify by inspecting `web/dist/index.html` — every `src=` should be `./assets/...`, not `/assets/...`.
- Write the SPA fallback explicitly: an axum route `fallback(get(serve_index_html))` that returns the embedded `index.html` bytes with `content-type: text/html`. Test by curling `/library/123/foo` and confirming HTML returns.
- Make `cargo run --release` (no `--dev`) the *default* CI smoke test, not just `pnpm dev`. Add a `make smoke` recipe that builds Vite, builds release, starts the binary, curls 3 routes, kills it.
- Set proper `Content-Type` headers from the embedded handler — `rust-embed`'s default may not infer correctly for `.wasm`, `.json`, `.map`.
- Cache-bust correctly: Vite hashes asset filenames; *don't* set long cache on `index.html` (or users see the old reference after upgrade). axum's `Cache-Control: no-cache` on `index.html` only.

**Warning signs:**
- "Works in dev, blank screen in release" — almost always an asset path or SPA fallback issue.
- Hard refresh in production fixes things — stale `index.html` in browser cache.
- 404 in network tab for a hashed asset file — old `index.html` references a no-longer-shipping bundle.
- Mobile Safari (for the rare user who proxies localhost to a phone) renders blank — strict MIME-type enforcement caught a wrong `Content-Type`.

**Phase to address:** Phase 0 (skeleton) — the embed + SPA-fallback handlers are foundational. Don't defer to Phase 9.

**Severity:** Project-killer if shipped (every release tag breaks until users hard-refresh), trivial if designed in Phase 0.

---

### Pitfall 7: `keyring` crate silently fails on async drop / on locked Keychain

**What goes wrong:**
The `keyring` crate's macOS backend calls into SecKeychain APIs, which are synchronous but interact with the user's logged-in Keychain. Three failure modes:

1. **Locked Keychain on cold boot.** If the user enabled "require password to unlock Keychain" (uncommon but happens in security-conscious environments — Yogurt's exact target audience), the first `keyring.get_password()` call hangs waiting for a modal that Yogurt-as-a-CLI cannot render. From the user's perspective, `yogurt start` hangs at boot.
2. **Async runtime issues.** Calling `keyring` from inside `tokio::main` works, but calling it from a `spawn_blocking` task on a runtime that's being shut down (e.g., during graceful Ctrl-C) can panic or deadlock as the Keychain TCC prompt races the runtime drop.
3. **Permission prompts surprise the user.** First read of a stored secret triggers macOS's "Yogurt wants to use confidential information stored in your Keychain" dialog, repeated *per process restart* if the user hits "Allow" instead of "Always Allow." Documenting Always Allow in onboarding is essential.

**Why it happens:**
- macOS Keychain is designed for GUI apps with main run loops; CLIs sit awkwardly with it.
- The `keyring` crate's API is sync; users wrap in `spawn_blocking`; the wrap interacts badly with shutdown.
- Default access prompts re-fire if the user doesn't pick "Always Allow."

**How to avoid:**
- **Load all secrets at startup**, eagerly, in a single `spawn_blocking` before the axum server starts. Cache them in an `Arc<RwLock<Secrets>>`. Never call `keyring` on-demand from a request handler.
- Use the `keyring` crate's `Entry::set_attribute_for_access_control` (if exposed) to mark Yogurt's items as "Always Allow" by default — or document the user clicking that checkbox.
- On `get_password` failure, distinguish "not found" (first run, expected) from "access denied" (user cancelled the prompt) and "locked Keychain" (need user interaction). Surface these distinctly in the Settings UI.
- Time-bound the cold-boot Keychain read with a 5-second timeout; if it hangs, fall back to "Settings → re-enter your keys" rather than freezing.
- Test on a fresh macOS user account where Keychain has never been unlocked in this session.

**Warning signs:**
- `yogurt start` hangs with no output, especially after reboot.
- "Yogurt wants to use confidential information…" dialog appears every restart.
- Settings page loads but API key fields are blank until you save them again.
- Tokio runtime panics on Ctrl-C with "blocking thread pool shut down."

**Phase to address:** Phase 5 (LLM client + settings) — design the eager-load pattern when writing the Keychain integration.

**Severity:** Annoyance (workarounds exist), but for the privacy-paranoid target user a hung CLI is a trust-eroding first impression.

---

### Pitfall 8: whisper-rs build failure on contributors' machines

**What goes wrong:**
`whisper-rs` (the Rust binding) builds whisper.cpp from source via CMake at compile time. With the `metal` feature, it links Metal, Accelerate, and Foundation frameworks. Three failure modes that bite OSS contributors:

1. **CMake not installed.** `cargo build` fails with "Failed to run cmake (is CMake installed?): Os { code: 2 }". A new contributor's "5-minute dev environment" turns into a 30-minute Homebrew session.
2. **Wrong CMake version.** Older CMake (< 3.16) can't build modern ggml; some Linux dev containers ship 3.10.
3. **Cargo rebuilds whisper.cpp on every Rust change.** Without `WHISPER_DONT_GENERATE_BINDINGS=1` and proper build script caching, every edit to a Rust file triggers a 60-second whisper.cpp rebuild. Contributors leave.

**Why it happens:**
- `whisper-rs-sys` runs bindgen + CMake on every build by default.
- The build script does not cache aggressively.
- README assumes the contributor is on the maintainer's machine.

**How to avoid:**
- Document CMake + Xcode CLT in README's "5 minutes to dev environment" section. Prove the 5-minute claim on a clean Mac.
- Set `WHISPER_DONT_GENERATE_BINDINGS=1` in `.cargo/config.toml` once you've vendored pre-generated bindings.
- Add `.cargo/config.toml` with the Metal/Accelerate rustflags so contributors don't need to know about them:
  ```
  [target.aarch64-apple-darwin]
  rustflags = ["-C", "link-arg=-framework", "-C", "link-arg=Metal", ...]
  ```
- Gate `whisper-rs` behind a Cargo feature (`local-stt`). Cloud-STT-only contributors should be able to skip the whisper.cpp build entirely. The PRD's pluggable-STT architecture already supports this — exploit it.
- Cache the CMake build dir between CI runs explicitly (sccache or actions/cache).

**Warning signs:**
- New contributor opens issue: "can't build, cmake error."
- Clean `cargo build` takes > 90 seconds even with no source changes.
- CI flakes on transient CMake fetches.

**Phase to address:** Phase 8 (Local STT) — the moment you add `whisper-rs` to a `Cargo.toml`, the contributor onboarding cost goes up; mitigate at the same PR.

**Severity:** Annoyance, but compounds on OSS adoption.

---

### Pitfall 9: Universal binary lipo gotchas + GitHub Actions matrix cost

**What goes wrong:**
The PRD calls for a universal binary (arm64 + x86_64). Building this naively in CI has three traps:

1. **No GHA macOS x86_64 runners** for Apple Silicon-only repos — they're available but expensive ($0.16/min vs $0.08/min for Linux), and the M1/M2 hosted runners *can* cross-compile but the toolchain needs `--target x86_64-apple-darwin` set up, which surprises devs the first time.
2. **`lipo` requires both slices to have matching codesignatures stripped first**, then re-sign the fat binary. Doing this in the wrong order ("sign each slice then lipo") produces a binary that codesign reports as valid but Gatekeeper rejects at first launch.
3. **whisper.cpp / Metal slice for Intel** — Metal is Apple-Silicon-only. The x86_64 slice can't use Metal; you need to compile whisper.cpp twice with different feature flags and lipo only the binary, not the libraries. If you're statically linking, the Metal symbols leak into the Intel slice and crash at first run.

**Why it happens:**
- macOS universal binary tooling is poorly documented outside Xcode.
- Notarization requirements changed in 2022; old StackOverflow advice is wrong.
- Conditional-feature crates are not standard Rust idiom.

**How to avoid:**
- Build two single-arch tarballs first (`yogurt-aarch64-apple-darwin.tar.gz` + `yogurt-x86_64-apple-darwin.tar.gz`). Ship those. Universal binary is *optional polish* — Homebrew already supports per-arch bottles.
- If you want universal: build each slice unsigned, `lipo -create -output yogurt aarch64/yogurt x86_64/yogurt`, then `codesign --options runtime --timestamp --sign "$IDENTITY" yogurt`, then notarize the fat binary, then staple. Order matters.
- Compile-time-feature-gate Metal: `#[cfg(target_arch = "aarch64")] use metal;`. Verify with `otool -L yogurt` on the Intel slice — there should be no Metal references.
- For Phase 0 / dev: just build for your own arch. Universal is a Phase 9 problem.

**Warning signs:**
- "Yogurt is damaged and can't be opened" on a clean macOS install (Gatekeeper rejection — usually signature/notarization order).
- App opens on M-series but crashes immediately on Intel.
- `lipo -info yogurt` shows only one architecture even though you ran lipo.

**Phase to address:** Phase 9 (distribution) — don't pre-optimize.

**Severity:** Annoyance for Apple-Silicon-first ship, project-killer for Intel users if Intel slice is broken (and the PRD does promise Intel best-effort).

---

### Pitfall 10: localhost binding security and port conflict UX

**What goes wrong:**
`yogurt start` binds `127.0.0.1:7878`. Three issues:

1. **Port already in use** — another tool (Phoenix LiveDashboard defaults to 7878 historically, some Jupyter setups, a previous `yogurt start` that didn't shut down cleanly) → axum fails to bind, error is cryptic, user has no clue.
2. **Binding to `0.0.0.0` "for convenience"** — if a contributor changes the bind in dev and accidentally ships it, every laptop on the user's coffee shop WiFi can hit Yogurt and read meeting transcripts. There's *no auth* in v1.
3. **WebSocket Origin / CORS for the embedded UI** — axum's WS upgrade doesn't check Origin by default; a malicious webpage in the same browser can open a WS to `ws://localhost:7878/ws/meetings/:id` and exfiltrate live transcripts. The local-first privacy posture (PROJECT.md) makes this worse than for a typical web app.

**Why it happens:**
- Port collisions are an accepted hazard but rarely UX'd well.
- Local-app-binds-localhost developers forget that `localhost` is reachable from any browser tab.
- WebSocket Origin checks are easy to forget.

**How to avoid:**
- On `bind()` failure, detect `AddrInUse` and print: "Port 7878 is already in use. Try `yogurt start --port 7879` or run `lsof -i :7878` to find the conflicting process." Don't show a raw IO error.
- Hard-code `127.0.0.1` as the only bindable interface in release builds. Add `#[cfg(debug_assertions)]` for any `0.0.0.0` override.
- WebSocket and `/api/*` handlers: check `Origin` header is one of `http://localhost:7878`, `http://127.0.0.1:7878`, and (in dev) `http://localhost:5173`. Reject anything else with 403.
- Generate a session token at server start, write to `~/.yogurt/session-token`, require it as a `Sec-WebSocket-Protocol` or cookie on WS. Belt and braces against malicious local webpages.
- Document the threat model in SECURITY.md: "Yogurt assumes localhost is trusted; do not run `yogurt start --bind 0.0.0.0` on a shared network."

**Warning signs:**
- "yogurt start" fails silently with no message after a crashed previous run.
- Any browser tab to `http://localhost:7878` returns Yogurt's UI without auth.
- Network tab on attacker.example.com shows successful `ws://localhost:7878/...` connection.

**Phase to address:** Phase 0 (skeleton — port + binding) + Phase 3 (live transcript — WS auth/origin).

**Severity:** Project-killer for the privacy positioning if exploited (the whole point of Yogurt is "your transcripts don't leak").

---

### Pitfall 11: SQLite single-writer contention under WAL

**What goes wrong:**
PRD §9 puts everything in `~/.yogurt/db.sqlite`. With multiple writers — transcript-line inserts (~5-20/sec), notes auto-save, chat message inserts, settings updates — they contend on SQLite's single-writer lock. With a typical rusqlite + r2d2 connection pool, multiple "writer" connections fight, each tries to acquire EXCLUSIVE, and the result is ~20x slower than a single dedicated writer connection. Worst case: "database is locked" errors during a meeting, transcript drops chunks, autosave stalls.

**Why it happens:**
- Naive setup: open N connections in a pool, treat them symmetrically. SQLite doesn't work that way — only one can write at a time.
- WAL improves *concurrent read + one write* but doesn't change the single-writer limit.
- High-frequency small writes (transcript lines) are the worst case.

**How to avoid:**
- Two connection pools: one shared multi-connection read pool, one single-connection write "pool" (just a `tokio::sync::Mutex<Connection>`).
- Run rusqlite calls on `spawn_blocking`. rusqlite is sync; calling it from async without spawn_blocking blocks the executor.
- Batch transcript inserts: buffer N lines, write in one transaction every M ms.
- Set pragmas at startup: `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000; PRAGMA wal_autocheckpoint=1000;`
- Consider whether transcript lines need to be persisted *during* the meeting at all — the canonical source is the in-memory broadcast stream + post-meeting flush. Reduce write volume.

**Warning signs:**
- `SQLITE_BUSY` errors in logs during meetings.
- Transcript panel stutters as inserts back up.
- Meeting feels laggy after 20+ minutes of transcript accumulation.

**Phase to address:** Phase 0 (skeleton) — set the pragmas and the pool model day one. Adding it later means rewriting every DB access.

**Severity:** Annoyance at v1 single-user scale, but cheap to prevent.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Use Vite dev server directly in production (skip rust-embed) | No build step, instant changes | Breaks the "single static binary" distribution promise — the entire wedge | Never (load-bearing for PRD §11) |
| Persist only markdown (no ProseMirror JSON) | Simpler schema, smaller files | Marks lost on round-trip → augmented-notes UX broken on every restart | Never (see Pitfall 4) |
| Use `tokio::time::Instant::now()` as audio timestamp | Trivial to wire | Drift between mic/system streams (Pitfall 3) | Phase 0 prototype only |
| Skip code signing in dev | Easier `cargo run` workflow | TCC re-prompts every rebuild; devs burn 30s/restart | Never for any dev who records > 2 meetings/day |
| Single connection pool for SQLite | Fewer moving parts | 20x slower writes, lock contention (Pitfall 11) | Only if write volume stays < 1/sec |
| Skip TipTap content migrations | Faster initial build | Schema bump silently drops marks on existing notes | Never after first user is using Yogurt |
| Eagerly load all whisper models at startup | Predictable latency on first transcription | 3GB RAM + 30s startup if large-v3 selected | Only with explicit user toggle |
| Use unbounded broadcast channel for transcript fan-out | No `Lagged` errors | Memory grows without bound if browser tab is slow/dead | Never in long-meeting scenarios |
| Hard-code localhost:7878 with no port-conflict UX | Less code | Crashes on `yogurt start` with cryptic error after previous unclean exit | Phase 0 only; fix by Phase 9 |
| `getUserMedia` for mic in browser instead of Rust capture | "Easier" frontend | Two mic streams competing for device; double-permission confusion (PRD §13 risk) | Never (PRD already commits to Rust-only) |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| ScreenCaptureKit | Forgetting `excludesCurrentProcessAudio = true`; Yogurt hears its own UI sounds | Set on `SCStreamConfiguration` at init; test by playing audio from the Yogurt UI itself |
| ScreenCaptureKit | Treating SCK audio timestamps as wall-clock time | Use `CMSampleBuffer.presentationTimeStamp` as media-clock; convert to meeting-relative |
| whisper.cpp | Using `whisper_full` streaming-mode for live transcript | Use VAD + batch-mode-per-utterance + dual `whisper_state` for preview/settled |
| whisper-rs build | No CMake → cryptic build failure for OSS contributors | Document in README; gate behind `local-stt` feature so non-local devs skip the build |
| OpenAI-compat clients | Assuming all "OpenAI-compatible" endpoints support `stream: true` correctly | Test against Minimax, Ollama, LM Studio, OpenRouter, Groq individually; some buffer entire response |
| Deepgram | Forgetting to send `keepalive` frames during silence | Stream times out after ~30s of no data; send keepalives |
| macOS Keychain | Calling `keyring` from async handler on demand | Eager-load at startup into `Arc<RwLock>`; never block a request handler on Keychain |
| TipTap collaboration extension | Leaving default `History` enabled alongside Yjs `History` | Disable StarterKit's History when using Collaboration (we're not using collab in v1; but watch this if added) |
| Vite + rust-embed | Default `base: '/'` → absolute asset paths break when index.html is cached cross-version | Set `base: './'` in vite.config.ts |
| axum WebSocket | Not checking Origin → CSRF from malicious local pages | Validate Origin header; reject non-Yogurt origins |
| Homebrew tap | PR'ing tap with new SHA before GitHub Release is fully visible | Sleep or poll the release URL in CI before opening tap PR; otherwise tap fetches a 404 |
| cargo publish | Publishing before tag push → Homebrew points to nonexistent version | Order: tag → release binaries upload → cargo publish → tap PR |
| notarytool | Notarizing the binary after stripping symbols | Strip *before* signing; notarize the signed binary; staple after |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Unbounded broadcast channel for transcript fan-out | Memory grows monotonically during a meeting | Bounded channel (e.g., 256 messages); handle `Lagged` by reconnecting WS | Browser tab in background for > 1 minute of active speech |
| Whisper sliding window without VAD | Latency grows from 1s → 30s; CPU pegged | VAD-segmented utterances + per-utterance batch decode | First 5 minutes on M1 Air; immediately on Intel |
| SQLite single connection pool | "database is locked" errors; transcript chunks dropped | Separate read pool + single writer Mutex<Connection>; batch inserts | ~20 transcript lines/sec sustained |
| TipTap re-render on every transcript update | Editor lags typing; cursor jumps | Throttle transcript-related re-renders to 4 Hz; isolate transcript panel from editor state | Long meetings (1h+) with active note-taking |
| Re-enhance on the full transcript every time | LLM cost balloons; response time grows linearly with meeting length | Diff-based re-enhance: only send changed segments + headings | Meetings > 45 minutes with frequent re-enhance clicks |
| Markdown file write on every keystroke | SSD wear; lag on file open in editor | Debounce file writes to 2-second idle windows | Heavy typing sessions |
| Loading large-v3 model on every meeting start | 30-second meeting startup time | Keep model loaded in memory across meetings; warm at app start if local-STT is selected | Every meeting if local STT selected |
| Holding `CMSampleBuffer`s for too long | Memory leak in audio capture | Convert to PCM and drop the buffer immediately; never store SCK buffers in queues | Long meetings (45 min+) |
| Streaming LLM response without backpressure to client | WS frame queue grows on browser; UI freezes | Use `tokio::select!` with a writability check; drop the WS if it can't keep up | Slow client browser on a busy laptop |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Binding axum to `0.0.0.0` instead of `127.0.0.1` | Coffee-shop WiFi exfiltration of transcripts | Hard-code `127.0.0.1` in release; require explicit flag for `0.0.0.0` with a warning |
| No WebSocket Origin check | Malicious webpage in same browser reads live transcript | Validate Origin against known-good list; 403 otherwise |
| No session token on the local server | Any process on the machine can hit `/api/meetings` | Generate token on start, write to `~/.yogurt/session-token` (mode 0600), require on requests |
| Logging API keys in debug output | Credentials leak to logs that might end up in support bundles | Redact via a `Secret<String>` wrapper that's `Debug`-impl'd to "[redacted]" |
| Storing OAuth tokens (future) in SQLite | DB file is more grep-able than Keychain | All credentials → `keyring` only, including future OAuth |
| Not validating LLM-returned markdown before rendering | XSS via TipTap if a node attribute is unsanitized | Treat LLM output as untrusted; use TipTap's HTML sanitization; render `↳ HH:MM` links as plain text, not HTML |
| Audio file not deleted on crash | PRD promises audio deletion after transcription; a crash leaves it on disk | Write audio to temp file with `O_TMPFILE` (unlinked on open) or to a watcher-cleaned location; explicit cleanup in panic handler |
| Including transcripts in crash reports | Leaks meeting content to crash collector | PRD already says no telemetry; if added later, never include transcript/notes content |
| Loose file permissions on `~/.yogurt/` | Other local users (rare on macOS, common on shared machines) read your meetings | `chmod 700 ~/.yogurt` on first run; warn if dir already exists with looser modes |
| Storing whisper model file from arbitrary URL | Supply-chain attack: malicious model file with embedded ggml exploit | Pin model URLs to Hugging Face's `ggerganov/whisper.cpp`; verify SHA256 from a hardcoded list before loading |

---

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Screen Recording prompt fires before the user understands why | User clicks "Don't Allow" → product fundamentally broken | Onboarding (§5.10) explains *first*, then triggers the prompt only after "Continue" — PRD already does this; resist temptation to fire eagerly for "demo wow" |
| User grants permission, Yogurt doesn't notice it needs a restart | "I granted permission and it still says I haven't" | Detect `CGRequestScreenCaptureAccess` returning denied even though TCC.db says granted → show the macOS-mandated restart screen (§5.11 already designed) |
| Model download with no time estimate | User cancels at 50% thinking it's stuck | Show bytes/sec + ETA + "Most users stay on cloud STT" copy (§5.11 already designed); also pre-fetch in background while user uses cloud |
| "End meeting" → 30-second enhancing wait with no feedback | User assumes the app froze | Streaming character count in the progress banner (§5.11 already designed); plus skeleton placeholders for AI bullets |
| Re-enhance silently clobbers user edits | "Where did my edits go?!" — trust shattered | Display a confirm dialog "Re-enhance will preserve your edits to grey AI text but may add new bullets" the first 3 times |
| Deep-link `↳ HH:MM` jumps but transcript panel is collapsed | Click does nothing visible | Click expands panel + scrolls — PRD §5.3 already specifies; verify in implementation |
| AI grey color too light on cream paper → looks like placeholder | User can't read AI bullets in bright light | Test the `#A89F90` on `#FBF7EF` contrast ratio — it's ~3.0:1, below WCAG AA. Consider providing a "stronger AI contrast" toggle |
| Settings change requires restart but doesn't tell the user | User changes provider, next meeting uses old one | Settings UI should hot-reload provider config; if a setting *truly* needs restart, banner that says so |
| API key field hides after save with no confirmation | User unsure if key was actually stored | Show "✓ stored" + last 4 chars (PRD §5.6 already specifies) |
| Empty transcript panel on first 10 seconds (whisper warmup) | "Is this thing on?" | Show "Listening…" pill with the wave icon; first transcript appears when VAD detects speech, not at second 0 |
| Local STT model unloaded between meetings | Every meeting takes 30s to start | Keep model loaded; document the RAM cost; let user toggle off to save RAM |
| User runs `yogurt start` in two terminals → second fails | Confusing error | First instance detected via lockfile (`~/.yogurt/yogurt.pid`), print "Yogurt is already running at localhost:7878; opening browser" instead of failing |

---

## "Looks Done But Isn't" Checklist

- [ ] **Screen Recording permission flow:** Often missing the *restart* step — verify that after granting, the next launch (not the same launch) succeeds.
- [ ] **Mic + system audio capture:** Often "works" but only one channel is actually populated — verify by inspecting raw PCM amplitude on both channels during a Zoom call.
- [ ] **Transcript timestamps:** Often shown but drift across long meetings — verify with a 60-min recording that mic/system stay within 250ms.
- [ ] **Augmented notes round-trip:** Often "works" the first time but breaks on app restart — verify by closing Yogurt, reopening, confirming grey bullets are still grey.
- [ ] **Re-enhance:** Often "works" but clobbers user edits — verify that editing a grey bullet to black, then re-enhancing, leaves the edited text alone.
- [ ] **TipTap mark persistence:** Often appears to persist via JSON but fails when schema changes — verify by bumping the mark's attribute set and confirming old documents still load correctly.
- [ ] **Deep-link `↳ HH:MM`:** Often renders but scrolls to wrong moment — verify against a meeting with multiple deep-links spanning the full duration.
- [ ] **Whisper model download:** Often "completes" but the file is an HTML error page from Hugging Face — verify file size matches expected (e.g., small.en = 466MB, large-v3 = 3.1GB) and SHA256 matches a hardcoded list.
- [ ] **API key Keychain storage:** Often "saved" but never actually written — verify with `security find-generic-password -s yogurt` from terminal.
- [ ] **Single binary build:** Often works locally but breaks in release because Vite assets path was `/` not `./` — verify `cargo build --release && ./target/release/yogurt start` (no `--dev` flag) loads the UI without 404s.
- [ ] **SPA fallback:** Often missed — verify by navigating to `/library/whatever-fake` and reloading; should serve `index.html`, not 404.
- [ ] **Universal binary:** Often "fat" but with a broken Intel slice — verify on an Intel Mac (or a Rosetta-launched binary on Apple Silicon).
- [ ] **Notarization:** Often "signed" but not notarized — verify with `spctl -a -vv yogurt` on a clean Mac; should say "accepted" not "rejected".
- [ ] **Cold-boot Keychain:** Often "works" because the dev's Keychain is unlocked — verify by logging out and back in on a test account.
- [ ] **Port conflict:** Often unhandled — verify by running two `yogurt start` instances; second should print a helpful error.
- [ ] **WebSocket Origin:** Often unchecked — verify by opening DevTools on `attacker.example.com` and trying to open a WS to `ws://localhost:7878`; should be rejected.
- [ ] **Audio deletion after transcription:** Often "deleted" via Rust but lingers in `/tmp` on crash — verify by SIGKILLing during a meeting and checking the FS.
- [ ] **whisper.cpp on M1 Air:** Often only tested on M3 Max — verify on lowest-spec target.
- [ ] **Folder colors in library sidebar:** Often hardcoded but PRD says they come from the palette — verify all folder color tokens are referenced from `--blue`/`--straw`/`--matcha`.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| TCC permission stuck after rebuild | LOW | Ship `yogurt doctor --reset-permissions` → runs `tccutil reset ScreenCapture ai.yogurt.app`; user restarts and re-grants |
| SCK crate has gaps | MEDIUM | Drop in a ~150-line Swift helper binary, invoked via stdin/stdout JSON; PRD §13 already pre-acknowledges this |
| Markdown round-trip loses marks | MEDIUM | Add `enriched_doc_json` column to schema; backfill from existing markdown by re-running enhance (cheap if cloud LLM, expensive if local) |
| Whisper model download corrupted | LOW | Verify SHA256 on load; if mismatch, delete and re-download. Add `yogurt models redownload` command |
| TipTap schema change breaks old notes | HIGH | Ship a content migration script that runs on first load; keep one in `migrations/` for every schema bump. Cost is high *retroactively* — prevent by versioning from day one |
| Audio drift discovered post-launch | HIGH | Requires rewriting timestamp model in audio + STT + UI deep-links + DB schema. Almost a full rewrite — prevent in Phase 2 |
| WS Origin exploit reported | LOW | Ship a patch release with Origin check; tell users to upgrade; consider a CVE if transcripts known to have leaked |
| Universal binary Intel slice broken | LOW | Drop universal, ship per-arch tarballs (Homebrew handles bottles fine) |
| Keychain hung on cold boot | MEDIUM | Add the 5-second timeout fallback (Pitfall 7) in a hotfix; meanwhile, document `yogurt start --no-keychain` workaround |
| SQLite "database is locked" | MEDIUM | Switch to single-writer pattern (Pitfall 11). Cost depends on how much code already touches the multi-pool. Cheap if caught in Phase 0; expensive in Phase 9 |
| OpenAI-compat provider broken | LOW | Settings UI shows "test connection" button; user re-pastes URL. Document expected endpoint format for each preset |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1. TCC permission re-prompts | Phase 9 (signing/notarization) + Phase 2 (in-app recovery UI) | `spctl -a -vv` clean; fresh-Mac install completes onboarding without TCC error |
| 2. screencapturekit gaps | Phase 2 (audio capture spike *first*) | 30-min Zoom recording produces clean two-channel WAV |
| 3. Audio timestamp drift | Phase 2 (clock model design) | 60-min meeting shows < 250ms drift mic↔system at end |
| 4. TipTap mark loss on round-trip | Phase 4 (notes hero) — design schema before writing the mark | Restart app, grey bullets are still grey; schema bump preserves old marks |
| 5. whisper.cpp streaming quality | Phase 8 (local STT) with VAD design in Phase 2 | small.en on M1 Air keeps up with real speech without latency drift |
| 6. rust-embed/Vite SPA breakage | Phase 0 (skeleton) — fallback handler and `base: './'` | Release build serves `/library/x/y` on reload; hashed assets match |
| 7. Keychain hang / async drop | Phase 5 (settings + Keychain) — eager-load pattern | Cold-boot test account: `yogurt start` succeeds within 5s |
| 8. whisper-rs build complexity | Phase 8 — `local-stt` feature gate + `WHISPER_DONT_GENERATE_BINDINGS=1` | New contributor on fresh Mac runs `cargo run` within 5 minutes |
| 9. Universal binary / notarization | Phase 9 (distribution) | `lipo -info`, `spctl -a -vv`, `otool -L`, fresh-Mac install |
| 10. Localhost binding / WS origin | Phase 0 (port + bind) + Phase 3 (WS auth) | `curl` from `attacker.example.com`-Origin headers rejected; port-conflict UX is friendly |
| 11. SQLite single-writer contention | Phase 0 (db pool design) | Sustained 20 inserts/sec doesn't produce SQLITE_BUSY |

---

## Cross-References to PRD §13 Open Risks

| PRD §13 risk | Expanded as Pitfall | Additional context |
|---|---|---|
| `screencapturekit` Rust crate gaps | Pitfall 2 | + memory leak history, audio loopback specific bugs, `excludesCurrentProcessAudio` |
| TipTap mark system for structural diffing | Pitfall 4 | + markdown round-trip is the underrated half of this risk; ProseMirror strict-schema content drop |
| whisper.cpp streaming partials quality | Pitfall 5 | + VAD-segment-then-batch pattern, dual `whisper_state` for preview/settled, M1 Air vs M3 Max bench gap |
| `getUserMedia` confusion | (Not expanded — PRD's resolution is correct; flagged only to confirm Rust-only capture is right call) | — |
| macOS Screen Recording permission UX | Pitfall 1 | + signing identity = TCC key; Sequoia 30-day re-prompt; `tccutil reset` recovery |
| whisper.cpp model download size | (Covered in UX pitfalls + Pitfall 8) | + SHA256 verification against hardcoded list; HTML-error-page-instead-of-model detection |

New pitfalls beyond PRD §13:
- Audio timestamp drift (Pitfall 3) — not flagged in PRD
- rust-embed/Vite asset/SPA (Pitfall 6) — not flagged in PRD
- Keychain async behavior (Pitfall 7) — partially flagged via constraint, not as a pitfall
- Localhost binding / WS Origin / session token (Pitfall 10) — partially implied by privacy posture, not as a pitfall
- SQLite single-writer (Pitfall 11) — not flagged in PRD
- Universal binary notarization order (Pitfall 9) — implied in §11 but not as a failure mode
- whisper-rs build/onboarding cost (Pitfall 8) — not flagged in PRD

---

## Sources

- [screencapturekit-rs docs and known issues](https://crates.io/crates/screencapturekit)
- [doom-fish/screencapturekit-rs README](https://github.com/svtlabs/screencapturekit-rs/blob/main/README.md)
- [cpal PR #894 — Support ScreenCapture loopback](https://github.com/RustAudio/cpal/pull/894)
- [Apple Developer Forums — TCC permissions on macOS](https://developer.apple.com/forums/thread/730043)
- [Screenify Studio — macOS Screen Recording Permissions Complete Guide](https://www.screenify.studio/blog/2026-04-23-macos-screen-recording-permissions)
- [whisper.cpp Issue #1641 — streaming.exe quality cliff](https://github.com/ggml-org/whisper.cpp/issues/1641)
- [whisper.cpp Discussion #3567 — Android streaming 5x slower than batch](https://github.com/ggml-org/whisper.cpp/discussions/3567)
- [whisper.cpp Discussion #1948 — Improving transcription quality](https://github.com/ggml-org/whisper.cpp/discussions/1948)
- [vite-rs and embedded Vite assets in Rust](https://github.com/Wulf/vite-rs)
- [Single Rust Binary with Vite+Svelte — base path gotchas](https://fdeantoni.medium.com/single-rust-binary-with-vite-svelte-66944f9ac561)
- [axum Discussion #1309 — host SPA files and embed in executable](https://github.com/tokio-rs/axum/discussions/1309)
- [TipTap Markdown Manager API](https://tiptap.dev/docs/editor/markdown/api/markdown-manager)
- [TipTap Collaboration extension docs (history conflict)](https://tiptap.dev/docs/editor/extensions/functionality/collaboration)
- [TipTap V3 Roadmap — Content migrations](https://github.com/ueberdosis/tiptap/discussions/5793)
- [Hocuspocus #443 — Schema Versioning and Migrations](https://github.com/ueberdosis/hocuspocus/discussions/443)
- [xcnotary — macOS app notarization helper in Rust](https://github.com/akeru-inc/xcnotary)
- [Random Errata — Rough guide to notarizing CLI apps for macOS](https://www.randomerrata.com/articles/2024/notarize/)
- [whisper-rs BUILDING.md](https://github.com/tazz4843/whisper-rs/blob/master/BUILDING.md)
- [whisper-rs Discussion #93 — Build on macOS Ventura](https://github.com/tazz4843/whisper-rs/discussions/93)
- [Arpad Voros — Speeding up whisper-rs build times](https://arpadvoros.com/posts/2026/05/05/speeding-up-rust-whisper-rs-build-times/)
- [keyring-rs / keyring-lib crates](https://crates.io/crates/keyring)
- [Evan Schwartz — Your SQLite Connection Pool Might Be Ruining Your Write Performance](https://emschwartz.me/psa-your-sqlite-connection-pool-might-be-ruining-your-write-performance/)
- [rusqlite Issue #697 — Transactions don't work with async/await](https://github.com/rusqlite/rusqlite/issues/697)
- [tenthousandmeters — SQLite concurrent writes and "database is locked"](https://tenthousandmeters.com/blog/sqlite-concurrent-writes-and-database-is-locked-errors/)
- [axum-socket-backpressure docs](https://docs.rs/axum-socket-backpressure/latest/axum_socket_backpressure/)
- [Cherry Studio Issue #11965 — OpenAI SSE timeout configurable](https://github.com/CherryHQ/cherry-studio/issues/11965)
- [async-openai docs](https://docs.rs/async-openai)
- [ggml-large-v3.bin — Complete Guide](https://fazm.ai/blog/ggml-large-v3-bin)
- [Whisper Discussion #1027 — SHA256 checksum mismatch](https://github.com/openai/whisper/discussions/1027)
- [Granola docs — How transcription works](https://docs.granola.ai/help-center/taking-notes/transcription)
- [Granola blog — Back-to-back meetings context (merge-meetings bug)](https://www.granola.ai/blog/meeting-notes-back-to-back-meetings-context)

---
*Pitfalls research for: macOS local-first meeting copilot (yogurt)*
*Researched: 2026-06-25*
