/**
 * MicMuteToggle — icon button next to `MicDevicePicker` in the meeting
 * toolbar (AUD-6). Pauses `Channel::Mic` only via `POST
 * /:id/mic-muted` — `Channel::System` keeps recording the whole time.
 *
 * Only meaningful while recording; `Meeting.tsx` gates the mount on
 * `recording` the same way it gates `MicDevicePicker` on `meetingId`.
 *
 * Sourced from the shared `["meetings", "active"]` query (`useActiveRecording`,
 * already polled every 5s by `Meeting.tsx` for the STT engine badge) rather
 * than local component state, so a page reload or a second tab shows the
 * true mute state — see `docs/.planning/aud6-mic-mute-design.md`. A
 * successful mutation invalidates that query so the toggle itself doesn't
 * have to wait out the poll interval.
 */
import { Mic, MicOff } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { audioApi } from "../lib/api/settings";
import { activeRecordingKey, useActiveRecording } from "../lib/api/meetings";

const STRAW = "#E07A66";

interface MicMuteToggleProps {
  meetingId: string;
}

export function MicMuteToggle({ meetingId }: MicMuteToggleProps) {
  const qc = useQueryClient();
  const active = useActiveRecording();
  const muted = active.data?.mic_muted ?? false;

  const setMuted = useMutation({
    mutationFn: (next: boolean) => audioApi.setMicMuted(meetingId, next),
    onSuccess: () => void qc.invalidateQueries({ queryKey: activeRecordingKey }),
  });

  return (
    <div className="flex items-center gap-1.5">
      <button
        type="button"
        aria-label={muted ? "Resume mic" : "Pause mic"}
        title={
          muted
            ? "Mic paused — click to resume"
            : "Pause mic — system audio keeps recording"
        }
        disabled={setMuted.isPending}
        onClick={() => setMuted.mutate(!muted)}
        className="inline-flex items-center justify-center w-7 h-7 rounded-md border transition-colors disabled:opacity-60"
        style={
          muted
            ? {
                backgroundColor: "var(--color-strsoft)",
                borderColor: "var(--color-strsoft)",
                color: STRAW,
              }
            : {
                backgroundColor: "var(--color-card)",
                borderColor: "var(--color-line)",
                color: "var(--color-mut)",
              }
        }
      >
        {muted ? <MicOff size={15} /> : <Mic size={15} />}
      </button>
      {muted && (
        <span
          className="text-[11px] font-mono inline-flex items-center gap-1"
          style={{ color: STRAW }}
        >
          <span
            aria-hidden
            className="inline-block w-1.5 h-1.5 rounded-full"
            style={{ backgroundColor: STRAW }}
          />
          Mic paused
        </span>
      )}
      {setMuted.isError && (
        <span className="text-[12px] font-mono text-[var(--color-straw)]">
          {setMuted.error instanceof Error
            ? setMuted.error.message
            : "Couldn't update mic"}
        </span>
      )}
    </div>
  );
}
