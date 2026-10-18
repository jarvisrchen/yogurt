/**
 * Pauses `Channel::Mic` only (`POST /:id/mic-muted`); `Channel::System` keeps recording.
 * Always mounted and disabled outside recording, since muting only makes sense mid-meeting.
 * The `M` hotkey has no modifier so it works as a reflex; `ignoreWhenTyping` keeps it from firing while notes, title or chat have focus.
 * State comes from the shared `["meetings", "active"]` query, not local state, so a reload or second tab shows the true mute state.
 * A successful mutation invalidates it so the toggle doesn't wait out the 5s poll.
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
        <p className="mt-1.5 text-[12px] text-[var(--color-straw)]">
          {setMuted.error instanceof Error
            ? setMuted.error.message
            : "Couldn't update mic"}
        </p>
      )}
    </div>
  );
}
