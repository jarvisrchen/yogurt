/**
 * Phase 7 Plan 07-04 — Screen-Recording permission status hook.
 *
 * Polls `GET /api/audio/permission` (the Rust handler at
 * `crates/yogurt-server/src/audio.rs::get_permission`) and surfaces a
 * `{ granted, isLoading, error }` shape to the UI. Three callsites
 * consume it:
 *
 *   1. `<Welcome />` (Step 1 "Screen Recording") — flips to ✓ once granted.
 *   2. `<Library />` — renders `<PermissionDenied />` when `denied`.
 *   3. `useFirstRunRedirect` — one of the three predicates that decides
 *      whether the SPA lands on `/welcome` vs `/`.
 *
 * The endpoint is dirt-cheap (a single `CGPreflightScreenCaptureAccess`
 * call), so we poll on a 2s interval while the user is still in
 * onboarding. macOS only re-reads permission state on next process
 * launch (TCC quirk — see `permission.rs` doc-comment) so the poll
 * mainly catches the user toggling System Settings while Yogurt is
 * running; the actual grant takes effect after restart.
 *
 * `PermissionStatus` mirrors the Rust enum (`granted`, `denied`,
 * `not_required`). On non-macOS the backend returns `not_required` and
 * we treat that as `granted` (mic capture works without TCC).
 */

import { useQuery } from "@tanstack/react-query";
import { bearerFetch } from "../lib/session";

type PermissionStatus = "granted" | "denied" | "not_required";

interface PermissionResponse {
  status: PermissionStatus;
}

async function fetchPermission(): Promise<PermissionResponse> {
  // `/api/audio/permission` lives behind `require_session_token` (Phase 0
  // WR-06) — bare fetch would 403 with "no session token presented" and
  // gate every onboarding poll. `bearerFetch` attaches the bootstrap token.
  const res = await bearerFetch("/api/audio/permission");
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText}`);
  }
  return (await res.json()) as PermissionResponse;
}

export interface ScreenRecordingStatus {
  /** `true` iff backend reports `granted` OR `not_required`. */
  granted: boolean;
  /** Raw status — useful for the §5.11 recovery card to distinguish denied
   *  from "permission state still loading". */
  status: PermissionStatus | undefined;
  isLoading: boolean;
  error: Error | null;
}

export const screenRecordingKey = ["audio", "permission"] as const;

export function useScreenRecordingStatus(): ScreenRecordingStatus {
  const q = useQuery({
    queryKey: screenRecordingKey,
    queryFn: fetchPermission,
    // Poll while onboarding is open — macOS doesn't push permission events.
    refetchInterval: 2_000,
    staleTime: 0,
  });

  const status = q.data?.status;
  const granted = status === "granted" || status === "not_required";

  return {
    granted,
    status,
    isLoading: q.isLoading,
    error: q.error as Error | null,
  };
}
