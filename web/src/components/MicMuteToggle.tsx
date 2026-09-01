/**
 * MicMuteToggle — full-width button between the mic-picker row and the
 * notes card (AUD-6). Pauses `Channel::Mic` only via `POST
 * /:id/mic-muted` — `Channel::System` keeps recording the whole time.
 *
 * Always mounted whenever `meetingId` is known (mirrors the notes editor
 * below it), disabled with an explanatory tooltip while not recording —
 * muting only makes sense mid-meeting, so it reads as unavailable rather
 * than disappearing. A core action deserves a big, always-findable target,
 * not a small icon tucked into the toolbar.
 *
 * The `M` hotkey (no modifier — this needs to be a reflex, not a chord)
 * mirrors it; `useKeyboardShortcut`'s `ignoreWhenTyping` keeps it from
 * firing while notes/title/chat have focus, and it's `enabled` only while
 * `recording` so it can't fire before there's anything to mute.
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
import { Button } from "./Button";
import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";
import { audioApi } from "../lib/api/settings";
import { activeRecordingKey, useActiveRecording } from "../lib/api/meetings";

interface MicMuteToggleProps {
  meetingId: string;
  recording: boolean;
}

export function MicMuteToggle({ meetingId, recording }: MicMuteToggleProps) {
  const qc = useQueryClient();
  const active = useActiveRecording();
  const muted = recording ? (active.data?.mic_muted ?? false) : false;

  const setMuted = useMutation({
    mutationFn: (next: boolean) => audioApi.setMicMuted(meetingId, next),
    onSuccess: () => void qc.invalidateQueries({ queryKey: activeRecordingKey }),
  });

  const toggle = () => {
    if (!recording || setMuted.isPending) return;
    setMuted.mutate(!muted);
  };

  useKeyboardShortcut({ key: "m", ignoreWhenTyping: true, enabled: recording }, toggle);

  return (
    <div>
      <Button
        variant={muted ? "warn" : "secondary"}
        disabled={!recording || setMuted.isPending}
        onClick={toggle}
        aria-label={muted ? "Resume mic" : "Pause mic"}
        title={
          !recording
            ? "Mic muting is available once recording starts"
            : muted
              ? "Mic paused — click to resume (M)"
              : "Pause mic — system audio keeps recording (M)"
        }
        className="w-full py-3.5 text-[15px]"
      >
        {muted ? <MicOff size={18} /> : <Mic size={18} />}
        {!recording ? "Mute mic" : muted ? "Mic paused — tap to resume" : "Mute mic"}
        {recording && (
          <kbd className="bg-black/10 text-current text-[11px] font-mono px-1.5 py-0.5 rounded ml-1">
            M
          </kbd>
        )}
      </Button>
      {setMuted.isError && (
        <p className="mt-1.5 text-[12px] font-mono text-[var(--color-straw)]">
          {setMuted.error instanceof Error
            ? setMuted.error.message
            : "Couldn't update mic"}
        </p>
      )}
    </div>
  );
}
