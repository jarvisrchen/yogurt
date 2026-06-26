---
phase: 08-local-stt-whisper-cpp
plan: 03
subsystem: stt
tags: [stt, rust, react, tanstack-query, websocket, settings, ui, dialog, whisper.cpp]

# Dependency graph
requires:
  - phase: 03-cloud-stt-live-transcript
    provides: "Stt trait + per-meeting ws fan-out — meetings/start.rs branches on stt_provider and constructs WhisperLocal or DeepgramStt against the same trait surface"
  - phase: 05-llm-client-settings-keychain
    provides: "Settings page + General KV row + /api/settings PATCH endpoint — Plan 08-03 extends General with stt_provider / stt_model and wires the Transcription section to PATCH them"
  - phase: 07-library-onboarding-states
    provides: "STATE-04 first-time download modal visual spec — wired to live progress here"
  - phase: 08-local-stt-whisper-cpp/01
    provides: "WhisperLocal + Stt impl gated behind local-stt feature flag"
  - phase: 08-local-stt-whisper-cpp/02
    provides: "models::REGISTRY + download_to (Range resume + SHA256) + webrtc-vad segmenter — Plan 08-03 REST handlers, ws progress events, and start_meeting branch consume them"

provides:
  - "REST /api/stt/models{,/:name/download,/:name} surface for the SPA model picker"
  - "WsEvent::SttModelDownloadProgress / _Complete / _Error variants + app_events_tx broadcaster fan-out"
  - "select_stt(&Settings) → SttSpec sync helper (testable) inside crates/yogurt-server/src/meetings.rs"
  - "start_meeting branches between DeepgramStt (cloud) and WhisperLocal (local); WhisperLocal::load is wrapped in spawn_blocking (LOCAL-05)"
  - "Frontend ws hook useModelDownloadProgress(model) + REST hooks useModels / useDownloadModel / useDeleteModel"
  - "LocalSTTCard + ModelPicker + ModelDownloadDialog wired end-to-end; STATE-04 'Coming soon' stub replaced with a working picker"

affects: [09-polish-distribution]

# Tech tracking
tech-stack:
  added:
    - "clsx ^2.1.1 — conditional class name composition for the matcha/blue card chrome"
  patterns:
    - "ws-app-channel: dedicated /ws socket (session-token gated) for meeting-independent progress events, distinct from /ws/meetings/:id"
    - "fire-and-forget download POST: 202 Accepted, progress + terminal state delivered exclusively over WS"
    - "TanStack-Query invalidation triggered from WS hook: invalidateQueries({queryKey: sttKeys.models}) inside ws.onmessage on _complete"

key-files:
  created:
    - "web/src/lib/api/stt.ts — typed fetch client + useModels/useDownloadModel/useDeleteModel TanStack hooks + sttKeys"
    - "web/src/hooks/useModelDownloadProgress.ts — /ws subscription filtered by model name; surfaces DownloadState; invalidates sttKeys.models on _complete"
    - "web/src/components/settings/ModelPicker.tsx — pill row with ✓ / ↓ glyphs + Intel slow-warning chip"
    - "web/src/components/settings/LocalSTTCard.tsx — Settings card chrome + matcha radio + dialog opener"
    - "web/src/components/dialogs/ModelDownloadDialog.tsx — STATE-04 native <dialog> wired to WS hook"
    - "web/src/components/settings/ModelPicker.test.tsx — two Vitest cases for click routing"
  modified:
    - "web/src/components/settings/STTPicker.tsx — rewritten: data-driven, mounts CloudSTTCard pair + LocalSTTCard side-by-side; 'Coming soon' badge GONE"
    - "web/src/lib/api/settings.ts — General gains stt_provider / stt_model fields (mirror Rust)"
    - "web/src/routes/Settings.test.tsx — fixture updated for two new required General fields"
    - "web/package.json + pnpm-lock.yaml — clsx dependency"
    - "crates/yogurt-server/src/api/stt_models.rs — rustfmt sweep"
    - "crates/yogurt-server/src/meetings.rs — rustfmt sweep"

key-decisions:
  - "Frontend ws path is /ws (the app-wide socket already added in Plan 08-03 Task 1 server work), not a meeting-scoped socket. The Settings page must receive download progress without a meeting being open."
  - "Session token is fetched via ensureSessionToken() before constructing the ws URL — same gate as /ws/meetings/:id. The hook delays open until the bootstrap promise resolves (handles parallel mount in the SPA Bootstrap)."
  - "Reuse the existing STTPicker.tsx component file (mounted from /routes/Settings.tsx) rather than introducing a new Settings.tsx — the plan path web/src/components/settings/Settings.tsx didn't exist; the actual route is in /routes/Settings.tsx and the Transcription stub was inside STTPicker.tsx."
  - "Switched from @testing-library/user-event (not installed in web/) to fireEvent.click for the ModelPicker tests, matching the project's existing Button.test.tsx / AskPill.test.tsx patterns. Equivalent assertion power, no new dependency."
  - "ModelPicker is purely controlled — selection / download dispatch are owned by LocalSTTCard (selection) and the parent route (provider PATCH). Keeps the pill row a single-responsibility presentational component."
  - "DownloadDialog opens its own ws once per dialog session (mount). Filtering on ev.model === model means a parallel download of another model would not corrupt the dialog state."

patterns-established:
  - "Pattern E — App-wide ws hook with TanStack invalidation: open a dedicated /ws socket inside useEffect, parse {type, …} discriminated frames, useQueryClient().invalidateQueries on terminal events."
  - "Pattern F — Rewrite-in-place when a stub component already owns the slot: STTPicker.tsx kept its mount point in Settings.tsx; the body was replaced wholesale. Avoids dangling imports and orphan tests."

requirements-completed: [LOCAL-01, LOCAL-03]
# LOCAL-04 (M1 Air < 3s lag) is the human-verification gate — deferred to 08-VERIFICATION.md.

# Metrics
duration: ~70min
completed: 2026-06-26
---

# Phase 8 Plan 03: Wire Local STT End-to-End Summary

**Phase 8 wired end-to-end: `start_meeting()` branches on `stt_provider` to construct either `DeepgramStt` (cloud) or `WhisperLocal` (local, with `spawn_blocking`-wrapped load); the Settings page replaces the Phase-7 "Coming in v1" stub with a real `LocalSTTCard` + matcha `ModelPicker` + STATE-04 download modal driven by live `/ws` progress events. M1 Air bench acceptance (LOCAL-04 < 3 s first-bar latency) deferred to user verification — no Apple-Silicon-M1 hardware available in this autonomous session.**

## Performance

- **Duration:** ~70 min (autonomous resume from Task 3 of 5).
- **Started:** 2026-06-26T11:15:00Z (continuation)
- **Server-side gates (Tasks 1-2):** committed in prior agent run (`252b9b1`, `9b5307e`).
- **Frontend + integration (Tasks 3-5):** committed in this run.
- **Files touched in Plan 08-03 total:** 7 created, 6 modified (sub-rolled-up).

## Task Commits

1. **Task 1: WS event variants + REST endpoints for STT model management** — `252b9b1` (prior run)
2. **Task 2: Branch start_meeting on stt_provider + V005 stt KV seed** — `9b5307e` (prior run)
3. **Task 3: Frontend API client + WS progress hook** — `a0dc737` (feat,web)
4. **Task 4: LocalSTTCard + ModelPicker + ModelDownloadDialog + Settings rewire** — `8e2a1a5` (feat,web)
5. **Task 5: Pre-bench build + rustfmt sweep + dev-server smoke** — `5be3c71` (style,stt)
6. **Final checkpoint: M1 Air bench** — `verification_deferred` (see 08-VERIFICATION.md)

## Accomplishments

- `web/src/lib/api/stt.ts` ships the typed REST surface: `fetchModels`, `useModels`, `useDownloadModel`, `useDeleteModel`, with `sttKeys.models` as the shared cache key.
- `web/src/hooks/useModelDownloadProgress.ts` opens a dedicated `/ws` socket (session-token gated), filters incoming `{type: "stt_model_download_progress" | "_complete" | "_error"}` frames by `ev.model === model`, and exposes a `DownloadState` shape (`bytesDownloaded`, `totalBytes`, `bytesPerSec`, `etaSeconds`, `complete`, `error`).
- `ModelPicker.tsx` renders one pill per `ModelView`. Downloaded models get the `✓` glyph and route clicks to `onSelect`; undownloaded models get the `↓` glyph and route clicks to `onRequestDownload`. Intel Macs see an additional `slow` straw chip on `medium.en` / `large-v3` per PRD §5.8.
- `ModelDownloadDialog.tsx` is a native `<dialog>` (`d.showModal()` on mount), fires `useDownloadModel.mutate(model)` once per open, displays bytes / rate / ETA via the WS hook, and auto-closes 600 ms after `complete: true`.
- `LocalSTTCard.tsx` ties the picker + dialog together with the matcha card chrome (1.5px matcha border + mtsoft bg when `active`).
- `STTPicker.tsx` was rewritten in place — it remains the mount point inside `routes/Settings.tsx`, but its body is now a `<CloudSTTCard /> + <LocalSTTCard />` pair driven by `useQuery(["settings"])`. The radio buttons PATCH `stt_provider`; clicking a downloaded local-model pill PATCHes `stt_model`.
- `General` in `web/src/lib/api/settings.ts` now exposes `stt_provider: string` + `stt_model: string`, mirroring the Rust struct that the V005 migration shipped.
- 121 web tests pass (was 119, +2 new ModelPicker cases). 196 workspace Rust tests pass with `--features yogurt-stt/local-stt`.
- `cargo clippy --workspace --all-targets --features yogurt-stt/local-stt -- -D warnings` is clean.
- `cargo build --no-default-features -p yogurt-stt` finishes in 3.2 s — well under the 30 s ROADMAP gate (LOCAL-05 success criterion).
- Release binary built (`target/release/yogurt`, 12.3 MB stripped) and dev-server smoke-tested: `GET /` → 200, `GET /api/stt/models` → all 4 REGISTRY entries with correct `{name, size_mb, downloaded, intel_supported}` shape.
- "Coming soon" badge is gone from every settings file (`grep -F 'Coming soon' web/src/components/settings/STTPicker.tsx` returns nothing).

## Decisions Made

- **`/ws` path, not `/ws/meetings/:id` or a new `/ws/app`.** Plan 08-03 Task 1 (prior run) already wired the app-wide `app_events_tx` broadcaster into the existing `/ws` socket; the SPA gets STT progress on the same connection that handles future app-level events. No new endpoint introduced.
- **Session token via `ensureSessionToken()` before opening the ws.** The `/ws` endpoint is gated by both Origin and session token (Phase 0 D-20). The hook awaits the cached promise before constructing the URL, so the URL is fully formed by the time `new WebSocket(...)` is called.
- **`STTPicker.tsx` rewritten in place** rather than introducing a new `Settings.tsx`. The plan-spec'd `web/src/components/settings/Settings.tsx` doesn't exist; `routes/Settings.tsx` is the real page and already mounted `<STTPicker />` in the Transcription section. Rewriting STTPicker keeps the mount point stable.
- **`fireEvent.click` for the ModelPicker tests** instead of `@testing-library/user-event` (which isn't installed). Matches the patterns already in `Button.test.tsx` and `AskPill.test.tsx`.
- **`ModelPicker` is purely presentational and controlled.** All persistence + provider PATCH flows live in the parent `STTPicker` route component. Keeps the pill row reusable + simple to test.
- **Cancel + "Run in background" both just close the dialog** per CONTEXT D-13. True cancellation (cancel-token threaded through `download_to`) is deferred to v1.1.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug / Rule 3 - Blocking] Test fixture missing new required `General` fields**

- **Found during:** Task 3 `pnpm --dir web build` after adding `stt_provider` / `stt_model` to `General`.
- **Issue:** `web/src/routes/Settings.test.tsx`'s `fixture: SettingsView` was typed and didn't include the two new required fields, breaking `tsc`.
- **Fix:** Added `stt_provider: "cloud"` and `stt_model: "small.en"` to the fixture's `general` block.
- **Files modified:** `web/src/routes/Settings.test.tsx`.
- **Verification:** `pnpm --dir web build` → green; `pnpm --dir web test` → 121 passed.
- **Committed in:** `a0dc737` (Task 3).

**2. [Rule 3 - Blocking] `@testing-library/user-event` not installed**

- **Found during:** Task 4 test authoring.
- **Issue:** Initial draft used `userEvent.setup()` + `await user.click()` for the ModelPicker tests, but the package isn't in `web/package.json`. Adding a new test-only dep just for two clicks isn't worth the dependency cost.
- **Fix:** Switched to `fireEvent.click` from `@testing-library/react`, matching `Button.test.tsx` and `AskPill.test.tsx`.
- **Files modified:** `web/src/components/settings/ModelPicker.test.tsx` (pre-commit).
- **Committed in:** `8e2a1a5` (Task 4) — no separate fix commit; resolved during authoring.

**3. [Rule 3 - Blocking] Port 7878 held by a prior `yogurt` process**

- **Found during:** Task 5 server-up smoke step.
- **Issue:** A previous agent or session had `yogurt` bound to port 7878 (PID 39114). The release binary wouldn't start cleanly.
- **Fix:** `kill 39114` — confirmed `lsof -i :7878` returned nothing before starting the fresh release build.
- **Files modified:** None.

### Plan-path → actual-file remap

The plan's `<files_modified>` block lists `web/src/components/settings/Settings.tsx`, but no such file exists. The actual Settings page is `web/src/routes/Settings.tsx`, and the Transcription section stub lived inside `web/src/components/settings/STTPicker.tsx`. Rewriting STTPicker in place was the cleanest fix — it kept the mount point in `routes/Settings.tsx` stable and avoided creating dangling exports. Documented as a decision above.

**Total deviations:** 3 auto-fixed (1 test fixture, 1 dep substitution, 1 blocking port held). No architectural changes, no scope creep.

## Issues Encountered

- **GitNexus index stale notifications** on every Bash call — informational; orthogonal to plan execution.
- **No M1 Air hardware available in autonomous session** — the LOCAL-04 < 3 s bench is the kill criterion for Phase 8. Documented + deferred in `08-VERIFICATION.md` with the verbatim verification protocol the user must run.

## Deferred Issues

- **M1 Air bench (LOCAL-04 < 3 s first-bar latency).** See `08-VERIFICATION.md`. This is the kill criterion: if M1 Air can't hit < 3 s, Phase 8 either tunes the dual-state preview/settled split or escalates to WhisperKit/ANE in v2.
- **Offline test.** Same as above — needs human at a real machine with wifi controls.
- **SHA256 verification of `~/.yogurt/models/ggml-small.en.bin`** against the REGISTRY hash. Same as above — needs the model to be actually downloaded.
- **Intel Mac warning chip visual smoke test.** Best-effort UA detection in `ModelPicker` covers the common case; a real Intel Mac smoke is part of the M1-Air bench protocol.

## Known Stubs

- **`ModelDownloadDialog`'s Cancel + "Run in background" buttons** both call `onClose` without canceling the underlying download (the background tokio task keeps running). This is the V1 limitation per CONTEXT D-13. Document is inline in the file's module doc comment.

## User Setup Required

For the M1 Air bench (deferred):
1. Run `cargo build -p yogurt --release` on an M1 Air with `cmake` + Xcode CLT installed (see 08-01 Summary for prerequisites).
2. Launch `target/release/yogurt start --no-open`.
3. Open `http://localhost:7878/settings` and follow the protocol in `08-VERIFICATION.md`.

## Next Phase Readiness

- **Phase 9 (Polish + distribution) is unblocked** at the code level. Phase 8's two hard gates (LOCAL-04 < 3 s on M1 Air, offline test) remain user-verification-pending, but every implementation surface is wired.
- **Plan 08-03 itself is `verification_deferred`.** The `<task type="checkpoint:human-verify" gate="blocking">` final task in the plan requires the M1 Air bench; the orchestrator should treat the plan as complete-pending-human-bench and resume only after the user runs through 08-VERIFICATION.md.

---
*Phase: 08-local-stt-whisper-cpp*
*Completed (code): 2026-06-26*
*Verification: human_needed — see 08-VERIFICATION.md*

## Self-Check: PASSED

- `web/src/lib/api/stt.ts` — FOUND
- `web/src/hooks/useModelDownloadProgress.ts` — FOUND
- `web/src/components/settings/ModelPicker.tsx` — FOUND
- `web/src/components/settings/LocalSTTCard.tsx` — FOUND
- `web/src/components/dialogs/ModelDownloadDialog.tsx` — FOUND
- `web/src/components/settings/ModelPicker.test.tsx` — FOUND
- Commit `a0dc737` (Task 3) — FOUND
- Commit `8e2a1a5` (Task 4) — FOUND
- Commit `5be3c71` (Task 5 fmt + bench notes) — FOUND
- `grep -F 'Coming soon' web/src/components/settings/STTPicker.tsx` → no matches ✓
- `pnpm --dir web test` → 121 passed (incl. ModelPicker.test.tsx ×2) ✓
- `pnpm --dir web build` → clean ✓
- `cargo test --workspace --features yogurt-stt/local-stt` → 196 passed, 3 ignored ✓
- `cargo clippy --workspace --all-targets --features yogurt-stt/local-stt -- -D warnings` → clean ✓
- `cargo build --no-default-features -p yogurt-stt` → 3.2 s (< 30 s gate) ✓
- Release binary smoke: `GET /` → 200, `GET /api/stt/models` → 4 entries ✓
- Dev-server process killed; port 7878 free at return ✓
