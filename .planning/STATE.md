---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: "meetings::Registry is the canonical in-memory fan-out point (Phase 7 swaps for SQLite behind the same `create/get/start/stop/subscribe` API). Cross-thread !Send bridge owns AudioStream on a dedicated std::thread; supervisor tokio task holds the shutdown oneshot. WS handler serializes `{type:"transcript", payload:{ts_ms,channel,text,is_final}}` exactly matching PRD §10. The server-side half of TRANS-08 < 2s lag is pinned at < 200ms by `e2e_synthetic_audio.rs` (actual is single-digit ms); the remaining budget is the browser side (Plan 03-03) + Deepgram network/processing (manual smoke)."
stopped_at: "Plan 03-02 complete — meetings::Registry wires yogurt-audio (Frame broadcast) → yogurt-stt (AudioChunk → TranscriptEvent broadcast) per meeting; 3 REST + 1 WS route mounted; cross-thread !Send bridge (std::thread + oneshot pair) cleanly owns AudioStream and shuts down via RAII on supervisor abort; < 200ms server-side fan-out lag pinned by e2e_synthetic_audio.rs (actual ≈ 5-10ms). 35 yogurt-server tests pass (11 suites). 3 commits (69ed51f, ca0d4d0, 2dbb398). TRANS-01 + TRANS-02 + TRANS-08 complete. Ready for Plan 03-03 (dock UI: useTranscriptWs hook + TranscriptDock component + slide-in-right motion at 340ms cubic-bezier(.2,.7,.2,1) per PRD §16.5 + library/meeting App.tsx switch). **Note:** Plan 01-03 (style-guide route + React Router 7 setup) still pending from Phase 1; Plan 02-03 (REST endpoints + WAV ear-test) still pending from Phase 2 (audio.rs's `start_meeting_recording()` shim is now wired by Plan 03-02 via Registry::start, so the original 02-03 wire-up requirement is partially absorbed — only the WAV ear-test remains)."
last_updated: "2026-06-26T03:51:50.712Z"
last_activity: "2026-06-26 — Plan 03-02 shipped (3 commits: Registry + AppState + lib wiring; REST + WS routes; E2E < 200ms test + fmt fixup); 4 new tests pass on ports 17890/17891/17892/17893; 35 total yogurt-server tests pass across 11 suites; 3 deviations auto-fixed (AudioStream !Send via std::thread+oneshot bridge; axum 0.8 `{id}` path syntax; build_test_state helper for tempfile AppState); 1 scope deferral (D-INT-02 per-meeting WS auth → Phase 5)."
progress:
  total_phases: 10
  completed_phases: 5
  total_plans: 32
  completed_plans: 18
  percent: 50
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-06-25)

**Core value:** The black-user / grey-AI in-place augmented-notes UX, running fully local on macOS without a meeting bot.
**Current focus:** Phase 1 — Design System (next)

## Current Position

Phase: 3 of 10 (Cloud STT + Live Transcript) — In progress (Plans 01 + 02 of 3 complete)
Plan: 03-02 complete; 03-03 (dock UI: useTranscriptWs hook + TranscriptDock component + slide-in-right motion + library/meeting route switch) next
Status: meetings::Registry is the canonical in-memory fan-out point (Phase 7 swaps for SQLite behind the same `create/get/start/stop/subscribe` API). Cross-thread !Send bridge owns AudioStream on a dedicated std::thread; supervisor tokio task holds the shutdown oneshot. WS handler serializes `{type:"transcript", payload:{ts_ms,channel,text,is_final}}` exactly matching PRD §10. The server-side half of TRANS-08 < 2s lag is pinned at < 200ms by `e2e_synthetic_audio.rs` (actual is single-digit ms); the remaining budget is the browser side (Plan 03-03) + Deepgram network/processing (manual smoke).
Last activity: 2026-07-02 - E2E debug session: fixed backend hang (260701-wjs), capture thread reactor panic (a263faa), and silently-mocked enhance (260701-x3u). Previous: 2026-06-26 — Plan 03-02 shipped (3 commits: Registry + AppState + lib wiring; REST + WS routes; E2E < 200ms test + fmt fixup); 4 new tests pass on ports 17890/17891/17892/17893; 35 total yogurt-server tests pass across 11 suites; 3 deviations auto-fixed (AudioStream !Send via std::thread+oneshot bridge; axum 0.8 `{id}` path syntax; build_test_state helper for tempfile AppState); 1 scope deferral (D-INT-02 per-meeting WS auth → Phase 5).

Progress: [██▊░░░░░░░] 28%

## Performance Metrics

**Velocity:**

- Total plans completed: 9
- Average duration: ~26 min
- Total execution time: ~232 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 0. Skeleton & Foundations | 3 | ~66 min | ~22 min |
| 1. Design System | 2 | ~6 min | ~3 min |
| 2. Audio Capture (HIGHEST RISK) | 2 | ~132 min | ~66 min |
| 3. Cloud STT + Live Transcript | 2 | ~28 min | ~14 min |

**Recent Trend:**

- Last 8 plans: 00-01 (~20m), 00-02 (~28m), 00-03 (~18m), 01-01 (~2m), 01-02 (~4m), 02-01 (~72m), 02-02 (~60m), 03-01 (~20m), 03-02 (~8m)
- Trend: Plan 03-02 landed at ~8 min — the fastest non-design plan to date. Reasons: (1) 03-01 had already pre-staged uuid + async-trait + the Stt trait shape so Cargo.toml deltas were trivial; (2) the only real architectural surprise was AudioStream !Send (caught in ~30s by cargo check and fixed with the std::thread+oneshot pattern); (3) axum 0.8 path syntax error was caught immediately by the pre-existing health test rather than later. Each prior plan paid down a tax that 03-02 didn't have to.

*Updated after each plan completion*
| Phase 00-skeleton-foundations P02 | 28 | 3 tasks | 18 files |
| Phase 00-skeleton-foundations P03 | 18 | 3 tasks | 15 files |
| Phase 01-design-system P01 | 2 | 3 tasks | 4 files |
| Phase 01-design-system P02 | 4 | 6 tasks | 10 files |
| Phase 02-audio-capture-highest-risk P01 | 72 | 3 tasks | 13 files |
| Phase 02-audio-capture-highest-risk P02 | 60 | 3 tasks |  8 files |
| Phase 03-cloud-stt-live-transcript P01  | 20 | 3 tasks |  6 files |
| Phase 03-cloud-stt-live-transcript P02  |  8 | 3 tasks |  5 files |

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
- [Phase 3]: AppState extended (NOT replaced) with `meetings: Arc<Registry>` alongside Phase 0's mode/storage/session/bind_port — preserves existing auth + storage surface (Plan 03-02)
- [Phase 3]: AudioStream is !Send (cpal::Stream); owned on a dedicated std::thread with two `oneshot::channel`s (readiness in: ships subscribed `broadcast::Receiver<Frame>` pair; shutdown out: tokio supervisor drops the sender to wake `blocking_recv` → drops AudioStream → RAII stops cpal+SCK). Pattern reusable for any future !Send native resource (Phase 5 Keychain prompts?) (Plan 03-02)
- [Phase 3]: axum 0.8 path syntax is `{id}` not `:id` — the superpowers source predated the axum 0.7→0.8 bump; routes must use `{...}` form going forward (Plan 03-02)
- [Phase 3]: WS handler at `/ws/meetings/{id}` does NOT yet enforce session-token / Origin auth (D-INT-02 in deferred-items); planner tests dial it raw and v1 trust posture is single-user localhost. Phase 5 hardening will fold under the Phase 0 gate (Plan 03-02)
- [Phase 3]: `__test_router(AppState) -> axum::Router` is the `#[doc(hidden)]` test seam; integration tests build full AppState via `Storage::init_at(tempdir)` + `session::load_or_create(tempdir)` + `meetings::Registry::new()` so the dev's ~/.yogurt/ is never touched (Plan 03-02)
- [Phase 3]: Frame.monotonic_micros → AudioChunk.ts_ms via integer divide by 1000 in the adapter loop — precision drop acceptable because downstream UI uses minute-resolution `↳ HH:MM` deep-links (Plan 03-02)
- [Phase 3]: Server-side fan-out budget pinned at < 200ms by e2e_synthetic_audio.rs (actual ≈ 5-10ms). The remaining TRANS-08 < 2s budget is browser-side (Plan 03-03) + Deepgram round-trip (manual smoke) (Plan 03-02, CONTEXT D-19)

### Pending Todos

None yet.

### Blockers/Concerns

- ~~Phase 2 has the highest project-killer risk concentration (audio loopback maturity, timestamp drift). Plan-phase 2 must treat the spike as a gate.~~ **RESOLVED (Plan 02-01)** — SCK 8.x spike PASSED; Path A confirmed. Drift verification deferred to Phase 3 per CONTEXT D-22.
- ~~Phase 2 (forward note): Plan 02-XX MUST add a `build.rs` emitting `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift`...~~ **RESOLVED (Plan 02-02)** — `crates/yogurt-audio/build.rs` now emits the Swift Concurrency rpath link arg on macOS; only `/usr/lib/swift` is added (not Xcode's toolchain path) per spike note.
- Phase 2 (forward note): Plan 02-03 must NOT introduce `--features synthetic` regressions. `yogurt-audio/tests/synthetic.rs` references `yogurt_audio::synthetic::*` which is `#[cfg(feature = "synthetic")]`-gated; workspace test invocations need `--features yogurt-audio/synthetic`. Could be fixed by gating the integration test file with `#![cfg(feature = "synthetic")]` but out of 02-02 scope. **Re-confirmed pre-existing during Plan 03-01 verification** — `cargo test --workspace` fails on this; tracked at `.planning/phases/03-cloud-stt-live-transcript/deferred-items.md` D-INT-01. Scoped `cargo test -p yogurt-stt` is green.
- Phase 4 is highest payoff but also highest UX risk (TipTap mark + AST diff round-trip). Plan-phase 4 must design `enriched_doc_json` schema before writing the mark.

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260628-g71 | Add microphone permission detection + UI surface symmetrical to Screen Recording | 2026-06-28 | f7313d6, 0f0b1e7 | .planning/quick/260628-g71-add-microphone-permission-detection-ui-s |
| 260701-vjb | Make /welcome onboarding steps actionable (Grant Screen Recording button + settings links) | 2026-07-02 | 99fc11f, 88fd68d | [260701-vjb-make-welcome-onboarding-steps-actionable](./quick/260701-vjb-make-welcome-onboarding-steps-actionable/) |
| 260701-wjs | Fix backend-wide hang: is_downloaded() hashed multi-GB models per call; sidecar .sha256 marker + spawn_blocking | 2026-07-02 | 1405c82, d60af23, 41ea94b | [260701-wjs-fix-sha256-hashing-hang-in-is-downloaded](./quick/260701-wjs-fix-sha256-hashing-hang-in-is-downloaded/) |
| (fast) | Fix "no reactor running" panic on capture thread: enter runtime Handle before start_capture | 2026-07-02 | a263faa | - |
| (fast) | Allow Vite origin :5173 for WS upgrades in dev mode; dock showed permanent "offline" from 403 bad origin | 2026-07-02 | 505cc58 | - |
| 260701-x3u | Wire enhance to configured LLM provider (env -> Keychain provider -> mock); was silently MockLlm always | 2026-07-02 | ff84d43, da8f2e0, ca126eb | [260701-x3u-wire-enhance-endpoint-to-configured-llm-](./quick/260701-x3u-wire-enhance-endpoint-to-configured-llm-/) |

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-06-26T00:58:00.000Z
Stopped at: Plan 03-02 complete — meetings::Registry wires yogurt-audio (Frame broadcast) → yogurt-stt (AudioChunk → TranscriptEvent broadcast) per meeting; 3 REST + 1 WS route mounted; cross-thread !Send bridge (std::thread + oneshot pair) cleanly owns AudioStream and shuts down via RAII on supervisor abort; < 200ms server-side fan-out lag pinned by e2e_synthetic_audio.rs (actual ≈ 5-10ms). 35 yogurt-server tests pass (11 suites). 3 commits (69ed51f, ca0d4d0, 2dbb398). TRANS-01 + TRANS-02 + TRANS-08 complete. Ready for Plan 03-03 (dock UI: useTranscriptWs hook + TranscriptDock component + slide-in-right motion at 340ms cubic-bezier(.2,.7,.2,1) per PRD §16.5 + library/meeting App.tsx switch). **Note:** Plan 01-03 (style-guide route + React Router 7 setup) still pending from Phase 1; Plan 02-03 (REST endpoints + WAV ear-test) still pending from Phase 2 (audio.rs's `start_meeting_recording()` shim is now wired by Plan 03-02 via Registry::start, so the original 02-03 wire-up requirement is partially absorbed — only the WAV ear-test remains).
Resume file: None
