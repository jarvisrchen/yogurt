import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { useActiveRecording } from "../lib/api/meetings";

/**
 * Floating "Return to recording" pill (Granola-style discoverability).
 *
 * Recording continues server-side across navigation — the supervisor task
 * lives in the backend `Registry`, not tied to any route — so this widget's
 * only job is making sure the user always sees a way back to it. Mounted
 * globally by the router `Shell` (see `router.tsx`).
 *
 * Hides itself:
 *   - while `GET /api/meetings/active` reports nothing recording
 *   - on the live meeting view for that same recording (`/meeting/:id`) —
 *     "return to recording" is noise on the screen that already IS the
 *     recording. It still shows on `/meeting/:id/post` and everywhere else.
 */
export function RecordingPill() {
  const { data } = useActiveRecording();
  const location = useLocation();
  const navigate = useNavigate();
  const [now, setNow] = useState(() => Date.now());

  // Tick the elapsed timer once a recording is known. Torn down when there's
  // nothing to time so hidden instances don't run a background interval.
  useEffect(() => {
    if (!data) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(id);
  }, [data]);

  if (!data) return null;
  if (location.pathname === `/meeting/${data.id}`) return null;

  const elapsedSec = Math.max(0, Math.floor((now - data.started_at) / 1000));
  const mm = String(Math.floor(elapsedSec / 60)).padStart(2, "0");
  const ss = String(elapsedSec % 60).padStart(2, "0");

  return (
    <button
      type="button"
      onClick={() => navigate(`/meeting/${data.id}`)}
      aria-label="Return to recording"
      className="fixed top-4 left-1/2 -translate-x-1/2 flex items-center gap-2 px-3 py-1.5 bg-card border border-line rounded-full shadow-pop text-[13px] text-ink hover:border-straw transition-colors z-30"
    >
      <span
        aria-hidden="true"
        className="inline-block w-2 h-2 rounded-pill bg-straw animate-recpulse"
      />
      <span className="font-medium truncate max-w-[220px]">
        {data.title || "Recording"}
      </span>
      <span className="font-mono text-[12px] tracking-tight text-mut">
        {mm}:{ss}
      </span>
    </button>
  );
}
