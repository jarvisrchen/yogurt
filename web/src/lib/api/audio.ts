import { bearerFetch } from "../session";

/** `not_determined` only comes from the microphone path. */
export type PermissionStatus =
  | "granted"
  | "denied"
  | "not_determined"
  | "not_required";

/** `GET /api/audio/permission` and the microphone request endpoint return the same combined snapshot. */
export interface PermissionsResponse {
  screen_recording: PermissionStatus;
  microphone: PermissionStatus;
}

/** Shared by both permission hooks so React Query dedupes them into one poll. */
export const permissionsKey = ["audio", "permission"] as const;

export async function fetchPermissions(): Promise<PermissionsResponse> {
  const res = await bearerFetch("/api/audio/permission");
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText}`);
  }
  return (await res.json()) as PermissionsResponse;
}

/**
 * Returns the current snapshot (likely still `not_determined`). Callers should
 * invalidate `permissionsKey` to surface the new state immediately.
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
 * The prompt only appears if the app has never asked, and macOS will not re-prompt after a denial,
 * so callers offer a System Settings deep link. Callers should invalidate `permissionsKey` afterwards.
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
