---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: Plan 03-01 complete — yogurt-stt crate scaffolded with Stt trait + Channel/AudioChunk/TranscriptEvent types (PRD §10 shape); DeepgramStt hand-rolled tokio-tungstenite 0.24 adapter (nova-2, linear16, dual-channel mic+system WS); mock-WS integration test proves end-to-end audio→transcript mapping in ~160ms
last_updated: "2026-06-26T00:55:00.000Z"
last_activity: 2026-06-25 — Phase 3 Plan 01 complete: new yogurt-stt crate added to workspace, Stt trait + TranscriptEvent matches PRD §10 verbatim, DeepgramStt opens two parallel WS sessions per meeting (preserves Me/Them without diarization), CloseStream-on-mpsc-drop clean shutdown, 4 unit tests + 1 mock integration test (6 total passing, clippy clean, fmt clean); 4 commits (165c251, 521b91b, c052c75, 9a712d6); 2 Rule-1 auto-fixes (clippy useless_conversion + mock determinism); TRANS-01 + TRANS-02 complete
progress:
  total_phases: 10
  completed_phases: 2
  total_plans: 32
  completed_plans: 8
  percent: 25
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-06-25)

**Core value:** The black-user / grey-AI in-place augmented-notes UX, running fully local on macOS without a meeting bot.
**Current focus:** Phase 1 — Design System (next)

## Current Position

Phase: 3 of 10 (Cloud STT + Live Transcript) — In progress (Plan 01 of 3 complete)
Plan: 03-01 complete; 03-02 (meetings registry + REST endpoints + WS route) next
Status: yogurt-stt crate ships Stt trait + DeepgramStt adapter. yogurt-stt has ZERO dep on yogurt-audio (verified) — server crate will be the wirer in 03-02. Two parallel WS sessions per meeting (one per Channel) preserves Me/Them label without diarization (PRD §2 v1 anti-goal). TranscriptEvent matches PRD §10 wire shape verbatim. Mock-WS pattern (TcpListener:0 + accept_async) proven and reusable for 03-02 WS handler tests.
Last activity: 2026-06-25 — Plan 03-01 shipped (4 commits: bootstrap crate + types, Deepgram adapter, fmt fixup, mock integration test); 6 tests passing in yogurt-stt; 2 Rule-1 auto-fixes; tokio-tungstenite 0.24 + async-trait + url + uuid added as workspace deps with rustls-tls-webpki-roots feature (no OpenSSL link, preserves single-binary distribution).

Progress: [██▌░░░░░░░] 25%

## Performance Metrics

**Velocity:**

- Total plans completed: 8
- Average duration: ~28 min
- Total execution time: ~224 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 0. Skeleton & Foundations | 3 | ~66 min | ~22 min |
| 1. Design System | 2 | ~6 min | ~3 min |
| 2. Audio Capture (HIGHEST RISK) | 2 | ~132 min | ~66 min |
| 3. Cloud STT + Live Transcript | 1 | ~20 min | ~20 min |

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
| Phase 03-cloud-stt-live-transcript P01  | 20 | 3 tasks |  6 files |

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
- [Phase 3]: yogurt-stt is a dedicated crate with ZERO dep on yogurt-audio — defines its own AudioChunk type and accepts broadcast::Receiver. Server crate is the wirer in 03-02. Decouples Phase 8 whisper.cpp adapter from audio crate (Plan 03-01, CONTEXT D-02)
- [Phase 3]: Hand-rolled tokio-tungstenite 0.24 client over the community `deepgram` crate — pre-1.0 churn risk; ~200 LOC behind the Stt trait keeps swap-out clean (Plan 03-01, CONTEXT D-04)
- [Phase 3]: Two parallel Deepgram WS sessions per meeting (one per Channel mic+system) — preserves Me/Them label without speaker diarization (PRD §2 explicit v1 anti-goal). Costs 2× Deepgram seconds but is the only correct option (Plan 03-01, CONTEXT D-05)
- [Phase 3]: tokio-tungstenite uses `rustls-tls-webpki-roots` feature (NOT native-tls) — preserves single-binary distribution by avoiding OpenSSL link (Plan 03-01)
- [Phase 3]: TranscriptEvent serde shape matches PRD §10 verbatim — snake_case field names, lowercase Channel via rename_all. The wire format is locked here so 03-02 WS handler can `serde_json::to_string` directly (Plan 03-01)
- [Phase 3]: Mock-WS test pattern (TcpListener on 127.0.0.1:0 + tokio_tungstenite::accept_async) is reusable for 03-02 WS handler tests; asymmetric per-channel response avoids broadcast recv ordering nondeterminism (Plan 03-01)
- [Phase 3]: `uuid 1` (v7 + serde) added as workspace dep — yogurt-stt doesn't use it directly, but ready for 03-02 MeetingId type (Plan 03-01)

### Pending Todos

None yet.

### Blockers/Concerns

- ~~Phase 2 has the highest project-killer risk concentration (audio loopback maturity, timestamp drift). Plan-phase 2 must treat the spike as a gate.~~ **RESOLVED (Plan 02-01)** — SCK 8.x spike PASSED; Path A confirmed. Drift verification deferred to Phase 3 per CONTEXT D-22.
- ~~Phase 2 (forward note): Plan 02-XX MUST add a `build.rs` emitting `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift`...~~ **RESOLVED (Plan 02-02)** — `crates/yogurt-audio/build.rs` now emits the Swift Concurrency rpath link arg on macOS; only `/usr/lib/swift` is added (not Xcode's toolchain path) per spike note.
- Phase 2 (forward note): Plan 02-03 must NOT introduce `--features synthetic` regressions. `yogurt-audio/tests/synthetic.rs` references `yogurt_audio::synthetic::*` which is `#[cfg(feature = "synthetic")]`-gated; workspace test invocations need `--features yogurt-audio/synthetic`. Could be fixed by gating the integration test file with `#![cfg(feature = "synthetic")]` but out of 02-02 scope. **Re-confirmed pre-existing during Plan 03-01 verification** — `cargo test --workspace` fails on this; tracked at `.planning/phases/03-cloud-stt-live-transcript/deferred-items.md` D-INT-01. Scoped `cargo test -p yogurt-stt` is green.
- Phase 4 is highest payoff but also highest UX risk (TipTap mark + AST diff round-trip). Plan-phase 4 must design `enriched_doc_json` schema before writing the mark.

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-06-26T00:55:00.000Z
Stopped at: Plan 03-01 complete — yogurt-stt crate ships Stt trait + DeepgramStt adapter (hand-rolled tokio-tungstenite 0.24, nova-2, linear16, dual-channel mic+system WS). TranscriptEvent matches PRD §10 verbatim. 6 tests passing (1 lib serialize + 4 deepgram unit + 1 mock integration). Clippy clean, fmt clean. TRANS-01 + TRANS-02 complete. Ready for Plan 03-02 (meetings::Registry + REST endpoints + WS route). **Note:** Plan 01-03 (style-guide route + React Router 7 setup) is still pending from Phase 1; Plan 02-03 (REST endpoints + WAV ear-test) also still pending from Phase 2.
Resume file: None
