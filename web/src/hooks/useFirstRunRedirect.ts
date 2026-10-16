import { useEffect } from "react";
import { useLocation, useNavigate } from "react-router";
import { useSettings } from "../lib/api/settings";
import { useMicrophoneStatus } from "./useMicrophoneStatus";
import { useScreenRecordingStatus } from "./useScreenRecordingStatus";

// Mounted on every route but only `/` is gated, so a slow poll avoids idle permission requests elsewhere.
const SLOW_POLL_MS = 60_000;

export function useFirstRunRedirect(): void {
  const nav = useNavigate();
  const { pathname } = useLocation();
  const settings = useSettings();
  const screenRec = useScreenRecordingStatus({ refetchIntervalMs: SLOW_POLL_MS });
  const mic = useMicrophoneStatus({ refetchIntervalMs: SLOW_POLL_MS });

  useEffect(() => {
    // Only `/` is gated so deep links like /settings work before onboarding.
    if (pathname !== "/") return;
    // Wait for all probes so the user does not flash past Welcome mid-load.
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
