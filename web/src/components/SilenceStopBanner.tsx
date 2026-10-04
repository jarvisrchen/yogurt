import { useEffect, useState } from "react";
import { useKeepRecording } from "../lib/api/meetings";

function format(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/**
 * Warns on the live meeting page that the server is about to stop a silent
 * recording. The countdown is local; the server's `auto_stop_at` is the
 * only deadline that counts.
 */
export function SilenceStopBanner({
  meetingId,
  autoStopAt,
}: {
  meetingId: string;
  autoStopAt: number;
}) {
  const keep = useKeepRecording();
  const [now, setNow] = useState(() => Date.now());

  // The banner mounts once per warning episode, so mount is the moment to
  // nudge a user whose tab is in the background.
  useEffect(() => {
    if (!("Notification" in window) || Notification.permission !== "granted") {
      return;
    }
    const notification = new Notification("No audio for 4 minutes", {
      body: "yogurt will stop recording in 1 minute. Open it to keep recording.",
      tag: "yogurt-silence-warning",
    });
    notification.onclick = () => {
      window.focus();
      notification.close();
    };
    return () => notification.close();
  }, []);

  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);

  return (
    <div
      role="status"
      className="flex items-center justify-between gap-3 rounded-button px-4 py-3 text-[13px] bg-strsoft text-ink border border-straw/40"
    >
      <span>
        No audio for a few minutes. Stopping in{" "}
        <span className="font-semibold tabular-nums">
          {format(autoStopAt - now)}
        </span>
      </span>
      <button
        type="button"
        onClick={() => keep.mutate(meetingId)}
        disabled={keep.isPending}
        className="shrink-0 px-2.5 py-1 rounded-full bg-blue text-white font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
      >
        Keep recording
      </button>
    </div>
  );
}
