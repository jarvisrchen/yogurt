/**
 * Quick task 260628-g71 — shared audio-permission API surface.
 *
 * Centralizes the wire shape of `GET /api/audio/permission` and the
 * request endpoints `POST /api/audio/microphone/request` and
 * `POST /api/audio/screen-recording/request` (quick task 260701-vjb) so
 * both `useScreenRecordingStatus` and `useMicrophoneStatus` (DD-04)
 * consume the same fetch + query key. React Query dedupes on
 * `permissionsKey`, so the two hooks share a single 2s network poll.
 *
 * Backend handler: `crates/yogurt-server/src/audio.rs::get_permission`
 * + `::request_microphone` + `::request_screen_recording`.
 */

import { bearerFetch } from "../session";

/**
 * Mirrors the Rust `PermissionStatus` enum
 * (`crates/yogurt-audio/src/permission.rs`). `not_determined` is only
 * produced by the microphone path — the screen-recording path can only
 * yield `granted`, `denied`, or `not_required` (non-macOS).
 */
export type PermissionStatus =
  | "granted"
  | "denied"
  | "not_determined"
  | "not_required";

/**
 * Wire shape of both `GET /api/audio/permission` and
 * `POST /api/audio/microphone/request`. The two endpoints intentionally
 * return the same combined snapshot so the SPA can update both hooks on
 * a single round-trip.
 */
export interface PermissionsResponse {
  screen_recording: PermissionStatus;
  microphone: PermissionStatus;
}

/**
 * Shared React-Query cache key. Both `useScreenRecordingStatus` and
 * `useMicrophoneStatus` reuse this so the query is deduped at the
 * React Query layer — one network poll per 2s interval, two hook
 * surfaces deriving from it via `select`-style accessors.
 */
export const permissionsKey = ["audio", "permission"] as const;

/**
 * Fetch the combined screen-recording + microphone permission snapshot.
 *
 * `/api/audio/permission` lives behind `require_session_token` (Phase 0
 * WR-06) — bare `fetch` would 403. `bearerFetch` attaches the bootstrap
 * token.
 */
export async function fetchPermissions(): Promise<PermissionsResponse> {
  const res = await bearerFetch("/api/audio/permission");
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText}`);
  }
  return (await res.json()) as PermissionsResponse;
}

/**
 * Fire the macOS microphone permission system dialog. Idempotent on the
 * backend — calling when already granted / denied is a cheap no-op.
 *
 * Returns the *current* (likely still `not_determined`) snapshot; the
 * 2s `fetchPermissions` poll picks up the eventual user response on the
 * next tick. Callers should `qc.invalidateQueries({ queryKey: permissionsKey })`
 * after this resolves to surface the new state immediately.
 */
export async function requestMicrophonePermission(): Promise<PermissionsResponse> {
  const res = await bearerFetch("/api/audio/microphone/request", {
    method: "POST",
  });
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText}`);
  }
  return (await res.json()) as PermissionsResponse;
}

/**
 * Fire the macOS Screen Recording TCC prompt (quick task 260701-vjb).
 * Fire-and-forget on the backend - the prompt only appears if the app
 * has never asked. macOS will not re-prompt after a denial, so the
 * caller offers a System Settings deep link for that case.
 *
 * Returns the *current* snapshot; the 2s `fetchPermissions` poll picks
 * up the eventual user response. Callers should
 * `qc.invalidateQueries({ queryKey: permissionsKey })` after this resolves.
 */
export async function requestScreenRecordingPermission(): Promise<PermissionsResponse> {
  const res = await bearerFetch("/api/audio/screen-recording/request", {
    method: "POST",
  });
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText}`);
  }
  return (await res.json()) as PermissionsResponse;
}
