/**
 * Phase 7 Plan 07-04 — Screen-Recording permission status hook.
 *
 * Polls the combined permission endpoint
 * (`GET /api/audio/permission` — see `crates/yogurt-server/src/audio.rs::get_permission`)
 * and surfaces a `{ granted, status, isLoading, error }` shape to the UI.
 * Three callsites consume it:
 *
 *   1. `<Welcome />` (Step 1 "Screen Recording") — flips to ✓ once granted.
 *   2. `<Library />` — renders `<PermissionDenied />` when `denied`.
 *   3. `useFirstRunRedirect` — one of the predicates that decides whether
 *      the SPA lands on `/welcome` vs `/`.
 *
 * Quick task 260628-g71 (DD-03 + DD-04) refactored the endpoint shape
 * from `{status}` to `{screen_recording, microphone}` and introduced a
 * parallel `useMicrophoneStatus` hook. Both hooks share the same
 * `permissionsKey` (`["audio", "permission"]`) and `queryFn` so React
 * Query dedupes — one 2s network poll, two derived hook surfaces.
 *
 * The endpoint is dirt-cheap (a single `CGPreflightScreenCaptureAccess`
 * call), so we poll on a 2s interval while the user is still in
 * onboarding. macOS only re-reads permission state on next process
 * launch (TCC quirk — see `permission.rs` doc-comment) so the poll
 * mainly catches the user toggling System Settings while Yogurt is
 * running; the actual grant takes effect after restart.
 *
 * `PermissionStatus` mirrors the Rust enum (`granted`, `denied`,
 * `not_determined`, `not_required`). The screen-recording path never
 * produces `not_determined` today (CGPreflight only returns bool) but
 * the union accepts it so a future code path doesn't blow up the type.
 * On non-macOS the backend returns `not_required` and we treat that as
 * `granted` (mic capture works without TCC).
 */

import { useQuery } from "@tanstack/react-query";
import {
  fetchPermissions,
  permissionsKey,
  type PermissionStatus,
} from "../lib/api/audio";

export interface ScreenRecordingStatus {
  /** `true` iff backend reports `granted` OR `not_required`. */
  granted: boolean;
  /** Raw status — useful for the §5.11 recovery card to distinguish denied
   *  from "permission state still loading". */
  status: PermissionStatus | undefined;
  isLoading: boolean;
  error: Error | null;
}

// Re-export the shared key so existing import-site behavior stays
// stable for callers that referenced `screenRecordingKey` directly.
// The canonical name now lives in `../lib/api/audio` as `permissionsKey`.
export const screenRecordingKey = permissionsKey;

export function useScreenRecordingStatus(): ScreenRecordingStatus {
  const q = useQuery({
    queryKey: permissionsKey,
    queryFn: fetchPermissions,
    // Poll while onboarding is open — macOS doesn't push permission events.
    refetchInterval: 2_000,
    staleTime: 0,
  });

  const status = q.data?.screen_recording;
  const granted = status === "granted" || status === "not_required";

  return {
    granted,
    status,
    isLoading: q.isLoading,
    error: q.error as Error | null,
  };
}
