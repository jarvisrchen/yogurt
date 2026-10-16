import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import {
  useCreateMeeting,
  useDetectedMeeting,
  useDismissDetectedMeeting,
} from "../lib/api/meetings";

/**
 * "Not now" dismisses server-side so the decision survives reloads and applies to every tab.
 * "Start recording" tracks the acted-on window locally: the detection poll can land between the click and the recording starting, and would re-show the prompt.
 * That flag is set only after the create has returned. Setting it first hides the banner, which unmounts this component and drops the `useMutation` observer,
 * leaving the `mutateAsync` promise unsettled. The button's disabled state guards the interval.
 * The notification is keyed on window id in a ref, not state: the poll re-renders every 5s and must not re-fire it.
 */
export function MeetingDetectedBanner() {
  const { data } = useDetectedMeeting();
  const dismiss = useDismissDetectedMeeting();
  const createMeeting = useCreateMeeting();
  const navigate = useNavigate();
  /** Window id we already started a recording for, if any. */
  const [startedFor, setStartedFor] = useState<number | null>(null);
  const notifiedWindowIds = useRef(new Set<number>());
  const openNotification = useRef<Notification | null>(null);

  useEffect(() => {
    if (!data) {
      openNotification.current?.close();
      openNotification.current = null;
      return;
    }
    if (!("Notification" in window) || Notification.permission !== "granted") {
      return;
    }
    if (notifiedWindowIds.current.has(data.window_id)) return;
    notifiedWindowIds.current.add(data.window_id);

    const notification = new Notification(`${data.app} meeting detected`, {
      body: "Open yogurt to start recording",
      tag: "yogurt-meeting-detected",
    });
    notification.onclick = () => {
      window.focus();
      notification.close();
    };
    openNotification.current = notification;
  }, [data]);

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
