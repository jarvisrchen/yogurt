/**
 * Phase 7 Plan 07-04 — first-run redirect (PRD §5.10 onboarding gate).
 *
 * Mount-time hook that lives at the router shell level. On every render
 * (typically a route change or a `useSettings`/`useScreenRecordingStatus`
 * cache update) it inspects three predicates:
 *
 *   1. `first_run_completed` — has the user finished `/welcome`?
 *   2. `granted`              — Screen Recording permission state.
 *   3. `hasActiveProvider`    — does any provider row have `is_active=true`?
 *
 * If the SPA is at `/` AND any of those is false, we `navigate("/welcome",
 * { replace: true })`. While settings / permission are loading we do
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
import { useScreenRecordingStatus } from "./useScreenRecordingStatus";

export function useFirstRunRedirect(): void {
  const nav = useNavigate();
  const { pathname } = useLocation();
  const settings = useSettings();
  const screenRec = useScreenRecordingStatus();

  useEffect(() => {
    // Only gate the root route.
    if (pathname !== "/") return;
    // Wait until both probes have data — flickering past Welcome
    // before settings load would be worse than a one-frame delay.
    if (settings.isLoading || screenRec.isLoading) return;
    if (!settings.data) return;

    const firstRunCompleted = settings.data.general.first_run_completed;
    const hasActiveProvider = settings.data.providers.some((p) => p.is_active);
    const granted = screenRec.granted;

    if (!firstRunCompleted || !granted || !hasActiveProvider) {
      nav("/welcome", { replace: true });
    }
  }, [
    pathname,
    settings.isLoading,
    settings.data,
    screenRec.isLoading,
    screenRec.granted,
    nav,
  ]);
}
