---
phase: quick
plan: 260701-vjb
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/yogurt-server/src/audio.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-server/tests/audio_api.rs
  - web/src/lib/api/audio.ts
  - web/src/routes/Welcome.tsx
autonomous: true
requirements: [QUICK-260701-VJB]

must_haves:
  truths:
    - "A first-run user with no permissions granted sees a working action button on Step 1 (Screen Recording), not a dead card"
    - "Clicking Grant Screen Recording fires the macOS TCC prompt via the backend without starting a capture"
    - "A denied user can deep-link to the Screen Recording pane of System Settings from Step 1"
    - "Step 3 and Step 4 each offer a link into /settings so the user can finish provider and transcription setup"
    - "Step 4 no longer looks permanently stuck at pending once steps 1-3 are done"
  artifacts:
    - path: "crates/yogurt-server/src/audio.rs"
      provides: "request_screen_recording handler returning combined PermissionResponse"
      contains: "request_screen_recording"
    - path: "crates/yogurt-server/src/routes.rs"
      provides: "POST /api/audio/screen-recording/request registration behind require_session_token"
      contains: "screen-recording/request"
    - path: "web/src/lib/api/audio.ts"
      provides: "requestScreenRecordingPermission() POST wrapper"
      exports: ["requestScreenRecordingPermission"]
    - path: "web/src/routes/Welcome.tsx"
      provides: "Actionable Step 1/3/4 controls"
  key_links:
    - from: "web/src/routes/Welcome.tsx"
      to: "/api/audio/screen-recording/request"
      via: "useMutation(requestScreenRecordingPermission)"
      pattern: "requestScreenRecordingPermission"
    - from: "crates/yogurt-server/src/audio.rs"
      to: "yogurt_audio::request_screen_recording_permission"
      via: "handler call"
      pattern: "request_screen_recording_permission"
---

<objective>
Make every incomplete step on `/welcome` actionable. Today Step 1 (Screen Recording) renders no control at all - the TCC prompt only fires implicitly when SCK capture starts, but the CTA is gated on the permission, so first-run users are deadlocked. Steps 3 and 4 are display-only. Only Step 2 (Microphone) works; its Grant-button + mutation pattern is the template to mirror.

Purpose: a first-run user completes all four steps directly from this page without guessing.
Output: new `POST /api/audio/screen-recording/request` endpoint, `requestScreenRecordingPermission()` API wrapper, and Welcome.tsx controls for Steps 1, 3, and 4.
</objective>

<context>
@web/src/routes/Welcome.tsx
@web/src/lib/api/audio.ts
@crates/yogurt-server/src/audio.rs
@crates/yogurt-server/src/routes.rs
@crates/yogurt-server/tests/audio_api.rs
@crates/yogurt-audio/src/permission.rs

Ground truth from orchestrator investigation (do not re-verify):
- `yogurt_audio::request_screen_recording_permission()` already exists (calls `CGRequestScreenCaptureAccess`), is exported from `crates/yogurt-audio/src/lib.rs` line 37, and is fire-and-forget like the mic variant. It is never exposed over HTTP.
- CGPreflight cannot distinguish never-asked from denied - screen recording status is only `granted` / `denied` / `not_required`. So the UI must always offer BOTH the Grant button (fires the prompt if never asked) and an Open System Settings link (recovery after denial), since we cannot tell which case we're in.
- Route registration: `crates/yogurt-server/src/routes.rs` lines 27-41, the `audio_routes` router behind `require_session_token`.
- Settings route path is `/settings` (`web/src/router.tsx` line 56). Settings.tsx has no per-section anchors; link to `/settings` plainly for both Steps 3 and 4.
- Mic flow pattern in Welcome.tsx: `useMutation` + `qc.invalidateQueries({ queryKey: permissionsKey })` on success; the 2s poll picks up the eventual state change.
- Existing integration tests: `crates/yogurt-server/tests/audio_api.rs` - `it_request_microphone_returns_combined_snapshot` (line 167) is the template for the new endpoint's test.

Constraints:
- Minimal diff; mirror existing patterns, no new abstractions.
- Never use em dash in any authored copy or comments; use plain dash.
</context>

<tasks>

<task type="auto">
  <name>Task 1: Expose POST /api/audio/screen-recording/request on the backend</name>
  <files>crates/yogurt-server/src/audio.rs, crates/yogurt-server/src/routes.rs, crates/yogurt-server/tests/audio_api.rs</files>
  <action>
    In `crates/yogurt-server/src/audio.rs`:
    - Add `request_screen_recording_permission` to the `yogurt_audio` import list.
    - Add `pub async fn request_screen_recording() -> Json<PermissionResponse>` directly below `request_microphone`, mirroring it exactly: fire-and-forget call `let _ = request_screen_recording_permission();` then return `Json(PermissionResponse::snapshot())`. Doc-comment should note: fires the macOS Screen Recording TCC prompt if the app has never asked; if already denied, macOS will not re-prompt and the SPA offers a System Settings deep link instead; returns the full combined snapshot so the SPA updates both permission hooks in one round-trip.
    - Update the module doc-comment (lines 10-18): remove the sentence claiming we do not expose a parallel screen-recording request endpoint, and instead list `POST /api/audio/screen-recording/request` as a fourth endpoint that fires the prompt explicitly from the Welcome Step 1 button (implicit prompting via `start_capture` was a deadlock on first run because the Welcome CTA is gated on the permission). Also update the `PermissionResponse` struct doc-comment if it enumerates which endpoints return this shape.

    In `crates/yogurt-server/src/routes.rs`:
    - Register `.route("/api/audio/screen-recording/request", post(audio::request_screen_recording))` immediately after the microphone/request route, inside the same `audio_routes` router (so it inherits `require_session_token`). Brief comment mirroring the mic route's comment style, referencing quick task 260701-vjb.

    In `crates/yogurt-server/tests/audio_api.rs`:
    - Add `it_request_screen_recording_returns_combined_snapshot`, a copy of `it_request_microphone_returns_combined_snapshot` (line 167) targeting `POST /api/audio/screen-recording/request` and asserting the same combined `{screen_recording, microphone}` snapshot shape with known status strings.
  </action>
  <verify>
    <automated>cargo test -p yogurt-server --test audio_api</automated>
  </verify>
  <done>New endpoint compiles, is registered behind session-token auth, integration test asserting the combined snapshot passes, and the audio.rs module doc no longer claims the endpoint does not exist.</done>
</task>

<task type="auto">
  <name>Task 2: Frontend API wrapper + actionable Welcome steps</name>
  <files>web/src/lib/api/audio.ts, web/src/routes/Welcome.tsx</files>
  <action>
    In `web/src/lib/api/audio.ts`:
    - Add `export async function requestScreenRecordingPermission(): Promise<PermissionsResponse>` mirroring `requestMicrophonePermission` exactly, POSTing to `/api/audio/screen-recording/request` via `bearerFetch`. Doc-comment notes fire-and-forget semantics and that macOS will not re-prompt after denial (caller offers a System Settings deep link for that case). Update the module doc-comment's endpoint list.

    In `web/src/routes/Welcome.tsx`:
    - Import `requestScreenRecordingPermission` and `Link` from `react-router`.
    - Add a `screenRequest` mutation mirroring `micRequest` (mutationFn `requestScreenRecordingPermission`, invalidate `permissionsKey` on success).
    - Step 1 (Screen Recording), when `!granted`: render children with TWO controls -
      1. A primary "Grant Screen Recording" button, same styling/disabled pattern as the mic Grant button, calling `screenRequest.mutate()`.
      2. A secondary "Open System Settings" text link/button setting `window.location.href = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"`.
      Both are always shown together because CGPreflight cannot distinguish never-asked from denied (unlike the mic path, which has `not_determined`). Add a short comment stating exactly that. Keep the existing restart-after-grant footer note untouched.
    - Step 3 (Connect your model), when `step3State === "current"` (no active provider): below the existing chips, add a "Set up in Settings →" link (`<Link to="/settings">`) styled as a small secondary action consistent with the card's typography, so the user can paste an API key. Chips stay as non-clickable spans.
    - Step 4 (Pick transcription): replace the hardcoded `step4State = "pending"` with logic that makes it `"current"` once steps 1-3 are all done (`granted && micGranted && hasProvider`), otherwise `"pending"`. When current, render a "Choose in Settings →" link to `/settings` as children (Settings has no per-section anchors, so a plain `/settings` link is correct). Transcription choice does not gate `ready` - leave the `ready` computation unchanged.
    - Update the file's top doc-comment: note quick task 260701-vjb made Steps 1, 3, and 4 actionable (Step 1 Grant button hits `POST /api/audio/screen-recording/request`; Steps 3/4 link to `/settings`; Step 4 state is derived, not hardcoded).
    - Do not use em dashes in any new copy or comments; use plain dashes.
  </action>
  <verify>
    <automated>cd web && pnpm exec tsc --noEmit && pnpm test</automated>
  </verify>
  <done>Welcome renders a working Grant Screen Recording button plus System Settings link on Step 1 when not granted, a Settings link on Step 3 when no provider is active, and a Settings link on Step 4 which becomes current after steps 1-3 complete. Typecheck and existing web tests pass.</done>
</task>

</tasks>

<verification>
- `cargo test -p yogurt-server --test audio_api` passes including the new screen-recording request test.
- `cd web && pnpm exec tsc --noEmit && pnpm test` passes.
- Manual smoke (executor, if server runnable): `curl -X POST -H "Authorization: Bearer $TOKEN" localhost:7878/api/audio/screen-recording/request` returns `{"screen_recording":..., "microphone":...}`.
</verification>

<success_criteria>
- First-run user can trigger the Screen Recording TCC prompt from Step 1 without starting a capture, and can recover from denial via the System Settings deep link.
- Steps 3 and 4 each provide a navigation path to `/settings`.
- Step 4 state is derived from steps 1-3 instead of hardcoded pending.
- No new abstractions; all changes mirror the existing mic-permission patterns.
</success_criteria>

<output>
Create `.planning/quick/260701-vjb-make-welcome-onboarding-steps-actionable/260701-vjb-SUMMARY.md` when done.
</output>
