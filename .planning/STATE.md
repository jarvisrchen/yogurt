---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: Plan 02-02 complete — real mic + system audio capture on Path A (in-process SCK 8.x); start_capture() orchestrator returns AudioStream with subscribe_mic/subscribe_system; dual_smoke verified 500 mic + 498 system frames over 10s with both peaks > 1000
last_updated: "2026-06-25T19:25:32.000Z"
last_activity: 2026-06-25 — Phase 2 Plan 02 complete: yogurt-audio gains mic capture (cpal + Downmix), system audio capture (SCK 8.x in-process, AUDIO-03 excludes_current_process_audio set), start_capture() orchestrator with 256-cap broadcast channels per AUDIO-04, Instant::now() per-FrameChunker baseline per AUDIO-05; 4 commits (build.rs rpath fix, mic, system, orchestrator); 8 new tests (yogurt-audio total 16+1 ignored, workspace total 44+1 ignored); dual_smoke hardware-verified on Apple Silicon macOS 15.6
progress:
  total_phases: 10
  completed_phases: 1
  total_plans: 32
  completed_plans: 7
  percent: 22
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-06-25)

**Core value:** The black-user / grey-AI in-place augmented-notes UX, running fully local on macOS without a meeting bot.
**Current focus:** Phase 1 — Design System (next)

## Current Position

Phase: 2 of 10 (Audio Capture — HIGHEST RISK) — In progress (Plan 02 of 3 complete)
Plan: 02-02 complete; 02-03 (REST endpoints + WAV ear-test checkpoint) next
Status: yogurt-audio has real mic + system capture wired end-to-end on Apple Silicon (cpal default input → Downmix → broadcast; SCK 8.x audio-only stream → Downmix → broadcast). start_capture() returns AudioStream with subscribe_mic/subscribe_system; AUDIO-02/03/04/05 satisfied; AUDIO-06 plumbing in place via RAII Drop on owned MicCapture + SystemCapture fields. dual_smoke hardware-verified.
Last activity: 2026-06-25 — Plan 02-02 shipped (4 commits: build.rs rpath fix + mic capture + system capture + start_capture orchestrator); 8 new Rust tests; 44 workspace tests green; mic_smoke 249 frames/5s, system_smoke 248 frames/5s peak −5221 with Glass.aiff loop, dual_smoke 500 mic + 498 system over 10s

Progress: [██░░░░░░░░] 22%

## Performance Metrics

**Velocity:**

- Total plans completed: 7
- Average duration: ~29 min
- Total execution time: ~204 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 0. Skeleton & Foundations | 3 | ~66 min | ~22 min |
| 1. Design System | 2 | ~6 min | ~3 min |
| 2. Audio Capture (HIGHEST RISK) | 2 | ~132 min | ~66 min |

**Recent Trend:**

- Last 7 plans: 00-01 (~20m), 00-02 (~28m), 00-03 (~18m), 01-01 (~2m), 01-02 (~4m), 02-01 (~72m), 02-02 (~60m)
- Trend: spike-first plans take longer (Phase 2's 72 min is dominated by the SCK 8.x API-discovery + runtime-rpath debugging; the actual crate scaffolding was ~15 min). Real-capture wire-up (02-02) landed at ~60m — most time in rubato/cpal/SCK API surface mapping; the actual code is ~970 LoC across 7 new files. Plans without spikes should still land at the ~20 min average.

*Updated after each plan completion*
| Phase 00-skeleton-foundations P02 | 28 | 3 tasks | 18 files |
| Phase 00-skeleton-foundations P03 | 18 | 3 tasks | 15 files |
| Phase 01-design-system P01 | 2 | 3 tasks | 4 files |
| Phase 01-design-system P02 | 4 | 6 tasks | 10 files |
| Phase 02-audio-capture-highest-risk P01 | 72 | 3 tasks | 13 files |
| Phase 02-audio-capture-highest-risk P02 | 60 | 3 tasks |  8 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table. Roadmap-shaping decisions reflected in the phase structure:

- Phase 0: Foundational pitfall mitigations baked in (WAL + dual pool, SPA fallback, localhost bind, WS Origin + session token, port-conflict UX) — not deferred
- Phase 2: Audio capture spike (30s dual-channel PCM ear-test) is a phase gate, not a checkbox; Swift sidecar fallback path documented
- Phase 4: Ships minimal hardcoded `OpenAiCompatClient` (~50 LOC); Phase 5 promotes to trait-bounded
- Phase 4: Schema migration adds `enriched_doc_json TEXT` so TipTap marks survive restart (research-flagged hero risk)
- Phase 7: Absorbs 4 table-stakes adds (FTS5 search, copy/reveal-in-finder, inline-editable title, delete-card UI)
- Phase 8: Gated behind `local-stt` Cargo feature; M1 Air benchmark required
- Phase 9: Per-arch tarballs first (universal binary optional); strict notarization order; `yogurt doctor` subcommand
- [Phase ?]: Used defineConfig from vite with as UserConfig cast in vite.config.ts to bridge vite 6 + vitest 2.1 vite-5 peer types (Plan 00-02)
- [Phase 0]: SQLite read pool sized at 4 (D-22 left to discretion; covers Phase 7 library + Phase 3 transcript + Phase 6 chat fan-out without blowing fds) (Plan 00-03)
- [Phase 0]: WS token transport supports BOTH `?token=` query param AND `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header — D-21 listed both, implemented both (Plan 00-03)
- [Phase 0]: RunConfig struct exposes optional db_path + session_token_path overrides; all server integration tests use tempdir injection rather than CLI spawn (Plan 00-03)
- [Phase 0]: Port-conflict suggestion uses `port.wrapping_add(1)` — message is a hint, not a binding decision (Plan 00-03)
- [Phase 4]: SQLite schema-version tracking deferred — migrations module is single-statement bare for Phase 0 simplicity; Plan 4-N should add `PRAGMA user_version` bookkeeping when the second migration lands (Plan 00-03 forward note)
- [Phase 1]: Single Blueberry token block in @theme; Strawberry + Matcha-dark themes deferred per PRD §15 (Plan 01-01)
- [Phase 1]: All 7 motion tokens + matching @keyframes declared centrally in Phase 1; runtime state binding happens in Phases 2 (recpulse → recording), 3 (slide-in-right → transcript dock), 4 (shimmer → enhance stream) (Plan 01-01)
- [Phase 1]: Fonts shipped via @fontsource/* side-effect imports — no Google Fonts CDN egress, satisfies privacy posture (Plan 01-01)
- [Phase 1]: Components colocated as `Foo.tsx` + `Foo.test.tsx` under `web/src/components/` (no shared `__tests__/` directory); RED/GREEN landed in a single atomic `feat(web): …` commit per component for cleaner `git log` (Plan 01-02)
- [Phase 1]: RecordingBadge and ProviderChip are named exports that compose the base Pill primitive — avoids one mega-component with twelve flags (Plan 01-02)
- [Phase 1]: Card is polymorphic via `as?: ElementType` (default div) — supports `<article>` for meeting cards downstream (Plan 01-02)
- [Phase 1]: Button `'ink'` variant deferred to Phase 4 (live meeting top-bar end-meeting CTA); documented inline in the Button source (Plan 01-02)
- [Phase 2]: SCK 8.x audio-loopback spike PASSED on Apple Silicon macOS 15.6 → Path A (in-process SCK) confirmed; Path B (Swift sidecar) NOT needed (Plan 02-01)
- [Phase 2]: Pin `screencapturekit = "8"` (NOT 0.3 as plan said) with `macos_13_0` feature — crate has had multiple major version bumps since plan was authored; full 8.x API quirks documented in spike-result note (Plan 02-01)
- [Phase 2]: Permission FFI uses bare `extern "C"` + `#[link(name="CoreGraphics", kind="framework")]` (3 lines, zero crate deps) — skipped objc2 + objc2-foundation that the plan called for (Plan 02-01)
- [Phase 2]: Audio-only SCStream still needs valid video dims `with_width(2).with_height(2)` — SCK quirk documented for Plan 02-XX (Plan 02-01)
- [Phase 2]: Sample format constants nailed down — `SAMPLE_RATE_HZ=16_000`, `FRAME_SAMPLES=320` exported from `yogurt_audio::frame`; downstream Phase 3 STT consumers MUST `use` these, never hardcode (Plan 02-01)
- [Phase 2]: build.rs in yogurt-audio bakes `/usr/lib/swift` into LC_RPATH (cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift); Xcode toolchain path deliberately NOT added — combining the two causes duplicate-class warnings and spurious TCC denial per spike note (Plan 02-02)
- [Phase 2]: `FrameChunker` is `pub(crate)` in `mic.rs` and reused by `system.rs` as a sibling-module import — single chunking implementation, single Instant::now() baseline pattern (Plan 02-02)
- [Phase 2]: SCK audio_buffer_list() yields parallel mono buffers (one per channel), NOT one interleaved buffer — system.rs handler interleaves L/R before feeding through the shared Downmix (Plan 02-02)
- [Phase 2]: Both broadcast channels are created with `BROADCAST_CAPACITY = 256` const — exactly the AUDIO-04 minimum, ~5 seconds of buffered audio per channel (Plan 02-02)
- [Phase 2]: cpal F32 callback path is the production case on this MacBook Pro (mono 48 kHz F32); I16 path supported for hardware variance but not exercised in dev-machine smokes (Plan 02-02)
- [Phase 2]: rubato SincFixedIn has a warm-up delay (148 output samples on first call for 480 input; ~160 thereafter) — test asserts ≥290 across two chunks rather than tight per-call bound (Plan 02-02)

### Pending Todos

None yet.

### Blockers/Concerns

- ~~Phase 2 has the highest project-killer risk concentration (audio loopback maturity, timestamp drift). Plan-phase 2 must treat the spike as a gate.~~ **RESOLVED (Plan 02-01)** — SCK 8.x spike PASSED; Path A confirmed. Drift verification deferred to Phase 3 per CONTEXT D-22.
- ~~Phase 2 (forward note): Plan 02-XX MUST add a `build.rs` emitting `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift`...~~ **RESOLVED (Plan 02-02)** — `crates/yogurt-audio/build.rs` now emits the Swift Concurrency rpath link arg on macOS; only `/usr/lib/swift` is added (not Xcode's toolchain path) per spike note.
- Phase 2 (forward note): Plan 02-03 must NOT introduce `--features synthetic` regressions. `yogurt-audio/tests/synthetic.rs` references `yogurt_audio::synthetic::*` which is `#[cfg(feature = "synthetic")]`-gated; workspace test invocations need `--features yogurt-audio/synthetic`. Could be fixed by gating the integration test file with `#![cfg(feature = "synthetic")]` but out of 02-02 scope.
- Phase 4 is highest payoff but also highest UX risk (TipTap mark + AST diff round-trip). Plan-phase 4 must design `enriched_doc_json` schema before writing the mark.

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-06-25T19:25:32.000Z
Stopped at: Plan 02-02 complete — real mic + system audio capture wired end-to-end via in-process SCK 8.x (Path A); start_capture() returns AudioStream with subscribe_mic/subscribe_system; AUDIO-02/03/04/05 satisfied; AUDIO-06 plumbing in place. dual_smoke hardware-verified on Apple Silicon (500 mic + 498 system frames over 10s, both peaks > 1000). **Note:** Plan 01-03 (style-guide route + React Router 7 setup) is still pending from Phase 1.
Resume file: None
