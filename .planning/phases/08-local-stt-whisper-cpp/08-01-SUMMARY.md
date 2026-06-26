---
phase: 08-local-stt-whisper-cpp
plan: 01
subsystem: stt
tags: [rust, whisper-rs, whisper.cpp, metal, tokio, spawn_blocking, sha256, vad, feature-flag]

# Dependency graph
requires:
  - phase: 03-cloud-stt-live-transcript
    provides: "Stt trait surface (AudioChunk / AudioRx / TranscriptEvent / TranscriptTx) — WhisperLocal mirrors it verbatim for drop-in swap with DeepgramStt"
provides:
  - "yogurt-stt::WhisperLocal — local STT adapter implementing Stt trait, dual-state Metal-accelerated decode"
  - "yogurt-stt::sha256::{hash_file, hash_bytes} — streaming 64 KB-chunked SHA256 helper for Plan 08-02 model verification"
  - "yogurt-stt::vad::{Segmenter, SegmenterEvent} — type-signature placeholder Plan 08-02 replaces with the real webrtc-vad implementation"
  - "yogurt-stt `local-stt` Cargo feature — gates the entire whisper.cpp/Metal/VAD toolchain pull so default builds stay fast (1.3 s)"
  - "tokio::spawn_blocking invariant (LOCAL-05) — every whisper.cpp decode call site wrapped, grep shows 9 occurrences"
affects: [08-02-model-download, 08-03-meeting-supervisor-wiring, 09-polish-distribution]

# Tech tracking
tech-stack:
  added:
    - "whisper-rs 0.16 (Metal feature) — Rust bindings to whisper.cpp; vendored via workspace dep with default-features = false"
    - "webrtc-vad 0.4 — workspace pin for Plan 08-02 VAD segmenter (placeholder lands here)"
    - "sha2 0.10 — streaming SHA256 for model integrity verification"
    - "hex 0.4 — lowercase hex digest encoding (matches `shasum -a 256` style)"
  patterns:
    - "feature-gate-then-share — workspace dep declared with default-features = false, leaf crate opts into vendor features via `features = [...]` + `optional = true`"
    - "dual whisper_state pattern — same WhisperContext (Arc'd), per-decode WhisperState, Greedy/preview vs BeamSearch/settled"
    - "spawn_blocking-or-deadlock — every synchronous C++ STT call wrapped in tokio::task::spawn_blocking; the convention is documented at every call site"
    - "placeholder-module-with-API-contract — vad.rs ships type signatures so a dependent (whisper_local.rs) can compile against the contract while a follow-up plan (08-02) lands the body"

key-files:
  created:
    - "crates/yogurt-stt/src/whisper_local.rs — WhisperLocal struct + impl Stt with mic-final, sys-final, partial-ticker workers"
    - "crates/yogurt-stt/src/sha256.rs — hash_file (streaming) + hash_bytes (in-memory) + 3 unit tests"
    - "crates/yogurt-stt/src/vad.rs — Segmenter / SegmenterEvent placeholder for Plan 08-02"
    - "crates/yogurt-stt/tests/whisper_smoke.rs — triple-gated (#[cfg], #[ignore], RUN_WHISPER_SMOKE env) end-to-end smoke"
  modified:
    - "Cargo.toml — workspace deps: whisper-rs 0.16, webrtc-vad 0.4, sha2 0.10, hex 0.4"
    - "crates/yogurt-stt/Cargo.toml — `local-stt` feature flag, optional deps, dev-deps (tempfile + directories)"
    - "crates/yogurt-stt/src/lib.rs — feature-gated module wiring for sha256, vad, whisper_local + WhisperLocal re-export"

key-decisions:
  - "default = [] (not [\"local-stt\"]) — defeats the whole point of the feature flag if default-on. CLAUDE.md tech stack pins whisper.cpp's ~17 s incremental rebuild; we keep CI/test loops fast by requiring opt-in."
  - "whisper-rs 0.16 (not source-plan 0.13) — followed CLAUDE.md tech stack pin. Required an API-drift fix: 0.16 replaced `full_get_segment_text(i)` with `get_segment(i)?.to_str_lossy()`."
  - "Mirrored Phase 3's actual Stt trait — not the source plan's hypothesized AudioFrame/TranscriptEvent::{Partial,Final} enum. Phase 3 shipped a flat TranscriptEvent { ts_ms, channel, text, is_final } so WhisperLocal mirrors that for genuine drop-in compatibility with DeepgramStt."
  - "VAD placeholder as separate module (vad.rs) rather than stubbed inside whisper_local.rs — Plan 08-02 only needs to replace one file; git diff stays clean."
  - "Smoke test triple-gated (#[cfg(feature)] + #[ignore] + env var) — wider than the source plan's two gates. Even contributors who pass `--ignored` shouldn't pay multi-second whisper.cpp inference cost unless they explicitly opted in via RUN_WHISPER_SMOKE=1."
  - "WhisperLocal::load is synchronous (no spawn_blocking inside) — documented contract is that the caller wraps. Tests + Plan 08-02 download tests need synchronous setup, and meetings/start.rs (Plan 08-03) already wraps."

patterns-established:
  - "Pattern A — `local-stt` feature flag: gate whisper.cpp's CMake toolchain (~17 s incremental, ~2 min cold) behind an explicit opt-in so the default workflow stays at 1.3 s. Plans 02-09 use the flag, not a default-on dependency."
  - "Pattern B — dual whisper_state: Greedy { best_of: 1 } + no_context=true for partial-window (5 s rolling, 1 s tick); BeamSearch { beam_size: 5, patience: 1.0 } + no_context=false for VAD-bounded finals."
  - "Pattern C — spawn_blocking-or-deadlock: every synchronous C++ STT/LLM call wrapped, with a doc-comment at the call site naming LOCAL-05. Grep proof: ≥3 occurrences in the file."
  - "Pattern D — placeholder-with-API-contract: a follow-up plan inherits the file; today's plan ships type signatures + module docs that name the inheriting plan."

requirements-completed: [LOCAL-01, LOCAL-02, LOCAL-05]

# Metrics
duration: ~45min
completed: 2026-06-26
---

# Phase 8 Plan 01: WhisperLocal Adapter Scaffold + Metal + Dual State Summary

**`yogurt-stt::WhisperLocal` — drop-in `Stt` impl backed by whisper.cpp via whisper-rs 0.16 (Metal), gated behind `local-stt` Cargo feature, dual `whisper_state` (Greedy/preview + BeamSearch/settled), every decode wrapped in `spawn_blocking`.**

## Performance

- **Duration:** ~45 min
- **Started:** 2026-06-26T07:18:00Z
- **Completed:** 2026-06-26T08:04:00Z
- **Tasks:** 3 (+ fmt sweep)
- **Files modified:** 7 (4 created, 3 modified)

## Accomplishments

- `WhisperLocal` implements the Phase-3 `Stt` trait verbatim — `Box<dyn Stt>` drop-in for `DeepgramStt`.
- `local-stt` Cargo feature flag gates the entire whisper.cpp/Metal/VAD/SHA toolchain pull. Default build stays at ~1.3 s; `--features local-stt` adds ~17 s of incremental whisper.cpp + Metal compilation. CLAUDE.md success criterion #5 (LOCAL-05-adjacent) satisfied.
- Dual `whisper_state` pattern shipped: `SamplingStrategy::Greedy { best_of: 1 }` for partial-window decode (rolling 5 s mic buffer, 1 s tick); `SamplingStrategy::BeamSearch { beam_size: 5, patience: 1.0 }` for VAD-bounded finals on mic + system channels.
- Every whisper.cpp decode call site wrapped in `tokio::task::spawn_blocking` (LOCAL-05). Three call sites: mic-final-decoder, sys-final-decoder, partial-ticker. `grep -c spawn_blocking crates/yogurt-stt/src/whisper_local.rs` → 9 (call + doc references).
- Streaming SHA256 helper (`hash_file`, 64 KB chunks) ready for Plan 08-02 model verification. Three unit tests cover empty, short ("hello"), and 200 KB temp-file roundtrip (>64 KB to exercise the chunk loop).
- `#[ignore]`-gated smoke test compiles with `--features local-stt`. Triple-gated so it never runs accidentally.
- Workspace test suite green: 171 default + 175 with `--features yogurt-stt/local-stt` (+4 = sha256 ×3 + object-safety ×1).

## Task Commits

1. **Task 1: Add local-stt feature flag + whisper-rs/webrtc-vad/sha2 deps** — `fe178d8` (feat)
2. **Task 2: Streaming SHA256 helper with inline tests** — `b44b9aa` (feat)
3. **Task 3: WhisperLocal adapter with dual whisper_state + spawn_blocking** — `19fc529` (feat)
4. **Cargo fmt sweep** — `ecc52ad` (style)

## Files Created/Modified

- `Cargo.toml` — workspace deps: whisper-rs 0.16 (no default features), webrtc-vad 0.4, sha2 0.10, hex 0.4.
- `crates/yogurt-stt/Cargo.toml` — `local-stt` feature gates `dep:whisper-rs`/`dep:webrtc-vad`/`dep:sha2`/`dep:hex`; whisper-rs enables `features = ["metal"]` at crate level; tempfile + directories moved to dev-dependencies.
- `crates/yogurt-stt/src/lib.rs` — feature-gated `pub mod sha256/vad/whisper_local` + `pub use whisper_local::WhisperLocal`.
- `crates/yogurt-stt/src/whisper_local.rs` (NEW) — `WhisperLocal::load`, `decode(ctx, pcm, fast)`, `impl Stt::start` with three workers (mic-final, sys-final, partial-ticker) + main audio pump.
- `crates/yogurt-stt/src/sha256.rs` (NEW) — `hash_file` (64 KB streaming) + `hash_bytes` + 3 unit tests.
- `crates/yogurt-stt/src/vad.rs` (NEW) — `Segmenter::new(sample_rate_hz)` + `Segmenter::push(pcm, on_event)` + `SegmenterEvent::Segment { pcm, start_ms, end_ms }`. `push` is no-op; Plan 08-02 implements.
- `crates/yogurt-stt/tests/whisper_smoke.rs` (NEW) — `#[cfg(feature = "local-stt")]` + `#[ignore]` + `RUN_WHISPER_SMOKE=1` triple-gated end-to-end smoke (3 s silence + 2 s 440 Hz tone).

## Decisions Made

- **whisper-rs 0.16 (CLAUDE.md tech stack) — not source-plan 0.13.** Required an API-drift fix: 0.16 replaced `full_get_segment_text(i)` with `get_segment(i)?.to_str_lossy()`. Lossy decoding is appropriate because invalid UTF-8 must never escape into a browser DOM.
- **`default = []`, not `default = ["local-stt"]`.** The source plan's draft snippet said default-on, which defeats the entire flag's purpose. Default-off means `cargo check -p yogurt-stt` stays at 1.3 s and only consumers that need the local path pay whisper.cpp's build cost.
- **Mirrored Phase 3's actual Stt trait signatures.** Source plan hypothesized `AudioFrame`/`TranscriptEvent::{Partial,Final}` enum; Phase 3 actually shipped flat `AudioChunk` + flat `TranscriptEvent { ts_ms, channel, text, is_final: bool }`. WhisperLocal mirrors what shipped — drop-in compatibility with `DeepgramStt` was the stated goal.
- **VAD placeholder in its own `vad.rs` module.** Plan 08-02 replaces one file (clean diff) rather than threading edits across `whisper_local.rs`. The placeholder's `push` is intentionally a no-op so 08-01 compiles without producing fake transcripts.
- **`WhisperLocal::load` is synchronous.** Doc comment names the contract: callers must wrap in `spawn_blocking`. Tests + Plan 08-02 download verification need synchronous setup; Plan 08-03 `meetings/start.rs` already wraps in `spawn_blocking`.
- **Smoke test triple-gated** (`#[cfg]` + `#[ignore]` + env var). Source plan used two gates; we added the env-var guard so even contributors who pass `-- --ignored` don't pay multi-second inference cost without intent.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] Installed `cmake` via Homebrew**

- **Found during:** Task 1 verification (`cargo check -p yogurt-stt --features local-stt`)
- **Issue:** whisper-rs-sys's build.rs invokes CMake to compile whisper.cpp. CMake was not installed on the host (`which cmake` → not found); the build panicked with `is cmake not installed?`. Xcode CLT was already present (`/Applications/Xcode.app/Contents/Developer`), but CMake is a separate prerequisite.
- **Fix:** `brew install cmake` (cmake 4.3.4 from Homebrew bottles).
- **Files modified:** None in repo — host-machine dependency.
- **Verification:** `cargo check -p yogurt-stt --features local-stt` succeeded in 17.5 s after install; `cargo build` and `cargo clippy` also clean.
- **Committed in:** noted in `fe178d8` commit message.
- **Follow-up:** The plan's pre-task instruction said to document `xcode-select --install` as a build prerequisite. CMake is a second prerequisite. Plan 08-02 (or the Phase 9 distribution work) should document `brew install cmake` in CONTRIBUTING.md / README setup section.

**2. [Rule 1 — API drift fix] whisper-rs 0.16 segment-getter API change**

- **Found during:** Task 3 implementation.
- **Issue:** Source plan called `state.full_get_segment_text(i)` which no longer exists in whisper-rs 0.16 (the version mandated by CLAUDE.md tech stack — the source plan was written against 0.13).
- **Fix:** Use the typed `state.get_segment(i)?.to_str_lossy()` chain. Lossy because invalid UTF-8 must never escape into a browser DOM (the Phase 4 ammonia sanitizer would also catch it, but defense-in-depth).
- **Files modified:** `crates/yogurt-stt/src/whisper_local.rs::decode`.
- **Verification:** Compiles + clippy clean; smoke-test smoke layer compiles too.
- **Committed in:** `19fc529` (Task 3 commit).

**3. [Rule 1 — Lint fix] `while let` instead of `loop` + `match`**

- **Found during:** Task 3 clippy run (`-D warnings`).
- **Issue:** `clippy::while-let-loop` rejected the event-drain loop in `whisper_smoke.rs` (`loop { match timeout { Ok(Ok(ev)) => events.push(ev), _ => break } }`).
- **Fix:** Rewrote as `while let Ok(Ok(ev)) = timeout(...).await { events.push(ev); }`.
- **Files modified:** `crates/yogurt-stt/tests/whisper_smoke.rs`.
- **Verification:** Clippy clean across `cargo clippy --workspace --all-targets --features yogurt-stt/local-stt -- -D warnings`.
- **Committed in:** `19fc529`.

**4. [Rule 1 — Lint fix] Replaced `if let` on single-variant enum with `match`**

- **Found during:** Task 3 `cargo build` warnings + clippy.
- **Issue:** `SegmenterEvent` (the placeholder) has only one variant today, making `if let SegmenterEvent::Segment { .. } = e` an irrefutable pattern.
- **Fix:** Used `match e { SegmenterEvent::Segment { .. } => ... }` so adding variants in Plan 08-02 is a compile error rather than a silent drop.
- **Files modified:** `crates/yogurt-stt/src/whisper_local.rs`.
- **Verification:** No warnings; clippy clean.
- **Committed in:** `19fc529`.

**5. [Rule 3 — Blocking] Cleared `target/debug/incremental` (3.1 GB) to free disk for workspace clippy**

- **Found during:** Workspace-wide `cargo clippy --features yogurt-stt/local-stt` after Task 3 commit.
- **Issue:** Host disk hit 100% (`No space left on device`) during the workspace clippy run. `df -h` showed 126 MB free on `/dev/disk3s5`.
- **Fix:** `rm -rf target/debug/incremental` — incremental compilation caches are safe to delete; cargo regenerates them on next build. Freed 3.1 GB.
- **Files modified:** None in repo.
- **Verification:** Workspace clippy then completed clean.
- **Committed in:** N/A (out-of-tree).

**Total deviations:** 5 auto-fixed (2 blocking infra, 2 lint, 1 API drift). All necessary for correctness / build success / clippy compliance.

**Impact on plan:** No scope creep. Every deviation was a precondition for the task's verify step to pass.

## Issues Encountered

- **GitNexus index stale notifications** appeared on every Bash call. These are informational and orthogonal to plan execution; not addressed in this plan.
- **Disk pressure** noted above (deviation #5) — worth a Phase 9 distribution-time follow-up to document target/ cleanup or set `CARGO_TARGET_DIR` outside the user's home if the build is expected to run in a constrained environment.

## Deferred Issues

- **Real-model smoke verification (RUN_WHISPER_SMOKE=1).** Requires `~/.yogurt/models/ggml-small.en.bin` (~487 MB download). That model download is Plan 08-02's job. Once 08-02 lands, `RUN_WHISPER_SMOKE=1 cargo test -p yogurt-stt --features local-stt --test whisper_smoke -- --ignored --nocapture` is the canonical sanity check.
- **M1 Air / Intel performance bench acceptance.** Plan 08-03 territory — those benches require both 08-01 (this) and 08-02 (model download) to land first.
- **CONTRIBUTING.md / README update** documenting `brew install cmake` as a prerequisite for `--features local-stt`. Phase 9 distribution work.

## User Setup Required

None for this plan in isolation. **However**, anyone building with `--features local-stt` on a fresh machine needs:
1. Xcode Command Line Tools (`xcode-select --install`) — usually already present.
2. CMake (`brew install cmake`) — was missing on this host; install was the first deviation.

Both prerequisites are macOS-only and one-time. Documenting in the project README is a Phase 9 follow-up.

## Next Phase Readiness

- **Plan 08-02 ready to start.** `vad.rs` has a one-file replacement target with documented type-signature contract; `sha256::hash_file` is ready for the download integrity-verify step; `WhisperLocal::load` is the synchronous-by-design entry point for the Settings → SQLite "preferred model" flow.
- **Plan 08-03 ready to wire after 08-02.** `WhisperLocal` is `dyn Stt`-safe (compile-time-checked in the inline test), so `meetings/start.rs` can branch on `stt_provider` with no further surface changes.
- **No blockers** for Phase 8 continuation. CMake + Xcode CLT install paths are documented above for any developer machine that hasn't done it yet.

---
*Phase: 08-local-stt-whisper-cpp*
*Completed: 2026-06-26*

## Self-Check: PASSED

- `crates/yogurt-stt/src/whisper_local.rs` — FOUND
- `crates/yogurt-stt/src/sha256.rs` — FOUND
- `crates/yogurt-stt/src/vad.rs` — FOUND
- `crates/yogurt-stt/tests/whisper_smoke.rs` — FOUND
- Commit `fe178d8` (Task 1) — FOUND
- Commit `b44b9aa` (Task 2) — FOUND
- Commit `19fc529` (Task 3) — FOUND
- Commit `ecc52ad` (fmt sweep) — FOUND
- `grep -c 'spawn_blocking' crates/yogurt-stt/src/whisper_local.rs` → 9 (≥ 3 required) ✓
- `cargo test --workspace` → 171 passed (baseline preserved) ✓
- `cargo test --workspace --features yogurt-stt/local-stt` → 175 passed, 3 ignored ✓
- `cargo clippy --workspace --all-targets --features yogurt-stt/local-stt -- -D warnings` → clean ✓
