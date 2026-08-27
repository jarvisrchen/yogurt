/**
 * Phase 7 Plan 07-04 — first-run redirect (PRD §5.10 onboarding gate).
 *
 * Mount-time hook that lives at the router shell level. On every render
 * (typically a route change or a `useSettings` / permission cache update)
 * it inspects four predicates:
 *
 *   1. `first_run_completed` — has the user finished `/welcome`?
 *   2. `granted` (screen recording) — Screen Recording permission state.
 *   3. `micGranted` — Microphone permission state (added by 260628-g71).
 *   4. `hasActiveProvider`    — does any provider row have `is_active=true`?
 *
 * If the SPA is at `/` AND any of those is false, we `navigate("/welcome",
 * { replace: true })`. While settings / permissions are loading we do
 * nothing (avoid bouncing the user off `/` mid-load).
 *
 * Only `/` is gated. Direct deep-links to `/meeting/:id`, `/settings`,
 * `/style-guide`, etc. continue to work even before onboarding — that's
 * deliberate: power users hitting `/settings` to paste an API key
 * shouldn't be punted to Welcome.
 */

import { useEffect } from "react";
import { useLocation, useNavigate } from "react-router";
import { useSettings } from "../lib/api/settings";
import { useMicrophoneStatus } from "./useMicrophoneStatus";
import { useScreenRecordingStatus } from "./useScreenRecordingStatus";

// This hook is mounted once at the router `<Shell>` level, so it's alive
// on EVERY route — including the post-meeting view, which has nothing to
// do with permissions. Its own gate only checks `pathname === "/"`, and
// macOS only re-reads permission state on next process launch, so a fast
// 2s poll here bought nothing but ~10 idle `/api/audio/permission`
// requests every 20s on every page. 60s keeps `/` correct without the
// background chatter; `<Welcome />`'s own hook calls (unchanged, default
// 2s) still drive the fast onboarding cadence while it's mounted.
const SLOW_POLL_MS = 60_000;

export function useFirstRunRedirect(): void {
  const nav = useNavigate();
  const { pathname } = useLocation();
  const settings = useSettings();
  const screenRec = useScreenRecordingStatus({ refetchIntervalMs: SLOW_POLL_MS });
  const mic = useMicrophoneStatus({ refetchIntervalMs: SLOW_POLL_MS });

  useEffect(() => {
    // Only gate the root route.
    if (pathname !== "/") return;
    // Wait until all probes have data — flickering past Welcome
    // before settings/permissions load would be worse than a one-frame
    // delay. Both screen-recording and mic share a single React Query
    // poll (DD-04), so the two isLoading flags resolve in lockstep.
    if (settings.isLoading || screenRec.isLoading || mic.isLoading) return;
    if (!settings.data) return;

    const firstRunCompleted = settings.data.general.first_run_completed;
    const hasActiveProvider = settings.data.providers.some((p) => p.is_active);
    const granted = screenRec.granted;
    const micGranted = mic.granted;

    if (
      !firstRunCompleted ||
      !granted ||
      !micGranted ||
      !hasActiveProvider
    ) {
      nav("/welcome", { replace: true });
    }
  }, [
    pathname,
    settings.isLoading,
    settings.data,
    screenRec.isLoading,
    screenRec.granted,
    mic.isLoading,
    mic.granted,
    nav,
  ]);
}
