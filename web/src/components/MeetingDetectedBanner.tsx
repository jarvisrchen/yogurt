import { useState } from "react";
import { useNavigate } from "react-router";
import {
  useCreateMeeting,
  useDetectedMeeting,
  useDismissDetectedMeeting,
} from "../lib/api/meetings";

/**
 * Floating "meeting detected" prompt (MTG-11).
 *
 * The server's detection watcher notices a meeting-app window on screen
 * and this offers to record it. It never records on its own: "Start
 * recording" runs the exact same create-then-`autoStart` navigation the
 * sidebar's "+ New meeting" button does, so there is one
 * recording-start flow rather than two.
 *
 * Mounted globally by the router `Shell`, alongside `<RecordingPill>`,
 * whose slot it shares — `GET /api/meetings/detected` returns `null`
 * while a recording is live, so the two are never on screen together.
 *
 * "Not now" dismisses server-side rather than in local state: the
 * decision has to outlive a page reload and apply to every open tab, and
 * it is keyed to the window so the *next* call still prompts.
 *
 * "Start recording" is the mirror image and stays local. The server does
 * stop reporting a detection once recording begins, but the poll can
 * land in the gap between the click and the recording actually starting,
 * which put the prompt back on the screen it had just opened. Remembering
 * the window we acted on closes that race without waiting on any poll.
 *
 * That flag is only set once the create has RETURNED. Setting it first
 * hides the banner, which unmounts this component, which drops the
 * `useMutation` observer and leaves the in-flight `mutateAsync` promise
 * unsettled — the click did nothing at all, silently. The disabled state
 * on the button is what guards the interval in between.
 */
export function MeetingDetectedBanner() {
  const { data } = useDetectedMeeting();
  const dismiss = useDismissDetectedMeeting();
  const createMeeting = useCreateMeeting();
  const navigate = useNavigate();
  /** Window id we already started a recording for, if any. */
  const [startedFor, setStartedFor] = useState<number | null>(null);

  if (!data || data.window_id === startedFor) return null;

  const windowId = data.window_id;
  async function start() {
    try {
      const m = await createMeeting.mutateAsync(undefined);
      setStartedFor(windowId);
      navigate(`/meeting/${m.id}`, { state: { autoStart: true } });
    } catch (e) {
      console.error("create meeting failed", e);
    }
  }

  return (
    <div
      role="status"
      className="fixed top-4 left-1/2 -translate-x-1/2 flex items-center gap-3 pl-3 pr-1.5 py-1.5 bg-card border border-line rounded-full shadow-pop text-[13px] text-ink z-30"
    >
      <span
        aria-hidden="true"
        className="inline-block w-2 h-2 rounded-pill bg-blue animate-recpulse"
      />
      <span className="truncate max-w-[280px]">
        <span className="font-medium">{data.app}</span>
        <span className="text-mut"> meeting detected</span>
      </span>
      <span className="flex items-center gap-1">
        <button
          type="button"
          onClick={() => void start()}
          disabled={createMeeting.isPending}
          className="px-2.5 py-1 rounded-full bg-blue text-white font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
        >
          Start recording
        </button>
        <button
          type="button"
          onClick={() => dismiss.mutate()}
          disabled={dismiss.isPending}
          className="px-2.5 py-1 rounded-full text-mut hover:text-ink transition-colors disabled:opacity-50"
        >
          Not now
        </button>
      </span>
    </div>
  );
}
