/**
 * Quick task 260628-g71 — Microphone permission status hook.
 *
 * Parallel to `useScreenRecordingStatus`: same `permissionsKey`, same
 * `fetchPermissions` query fn, same 2s refetch interval. React Query
 * dedupes both hooks onto a single network poll (DD-04 of the plan —
 * separate hooks rather than a combined `usePermissionStatuses`, so the
 * three existing `useScreenRecordingStatus` consumers do not have to
 * change shape).
 *
 * Three callsites consume it (introduced in this task):
 *
 *   1. `<Welcome />` (Step 2 "Microphone") — drives the StepCard state
 *      and the Grant button.
 *   2. `<Library />` — renders `<MicPermissionDenied />` when `denied`
 *      AND screen-recording is already granted (screen-recording denial
 *      takes precedence — it's the more fundamental capture path).
 *   3. `useFirstRunRedirect` — added as a third predicate so the SPA
 *      stays on `/welcome` until mic is granted (alongside the existing
 *      screen-recording + provider gates).
 *
 * Mic permission can take all four `PermissionStatus` values:
 * `granted`, `denied`, `not_determined`, `not_required`. The Welcome UX
 * distinguishes `not_determined` (show "Grant Microphone" button →
 * fires the macOS dialog) from `denied` (show "Open System Settings"
 * button → deep-link into the Privacy_Microphone pane); the macOS
 * system will not re-prompt after a denial, so the recovery path is
 * different.
 */

import { useQuery } from "@tanstack/react-query";
import {
  fetchPermissions,
  permissionsKey,
  type PermissionStatus,
} from "../lib/api/audio";

export interface MicrophoneStatus {
  /** `true` iff backend reports `granted` OR `not_required` (non-macOS). */
  granted: boolean;
  /** Raw status — distinguishes `not_determined` (show Grant button) from
   *  `denied` (show Open System Settings deep link). */
  status: PermissionStatus | undefined;
  isLoading: boolean;
  error: Error | null;
}

export interface UseMicrophoneStatusOptions {
  /**
   * Poll cadence override, in ms. Defaults to the fast 2s cadence
   * (unchanged behavior — the primary consumer is `<Welcome />`'s
   * onboarding gate, which wants near-real-time updates while the user
   * is actively granting permission). Callers where the answer barely
   * ever changes mid-session (Library's gate, the app-wide
   * `useFirstRunRedirect` check) should pass a slower interval — e.g.
   * `60_000` — so the app isn't polling `/api/audio/permission` every 2s
   * on every route (this hook shares `permissionsKey` with
   * `useScreenRecordingStatus`, so whichever mounted observer requests
   * the fastest interval wins for the whole app while it's mounted;
   * dropping the "elsewhere" call sites to 60s means the effective rate
   * is 2s only while onboarding is actually on screen).
   */
  refetchIntervalMs?: number;
}

const DEFAULT_POLL_MS = 2_000;

export function useMicrophoneStatus(
  opts: UseMicrophoneStatusOptions = {},
): MicrophoneStatus {
  const q = useQuery({
    queryKey: permissionsKey,
    queryFn: fetchPermissions,
    refetchInterval: opts.refetchIntervalMs ?? DEFAULT_POLL_MS,
    staleTime: 0,
  });

  const status = q.data?.microphone;
  const granted = status === "granted" || status === "not_required";

  return {
    granted,
    status,
    isLoading: q.isLoading,
    error: q.error as Error | null,
  };
}
