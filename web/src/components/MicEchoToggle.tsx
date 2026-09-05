/**
 * MicEchoToggle — full-width "Echo" button next to MicMuteToggle (AUD-11).
 *
 * Mirrors `MicMuteToggle.tsx`: sourced from the shared
 * `["meetings", "active"]` query (`useActiveRecording`) rather than local
 * state, disabled with an explanatory tooltip while not recording, and
 * bound to the `E` hotkey via `useKeyboardShortcut`.
 *
 * Toggling calls `POST /:id/echo { enabled }` — the device itself is
 * chosen by the neighboring `EchoDevicePicker`, not this button.
 */
import { Volume2 } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "./Button";
import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";
import { audioApi } from "../lib/api/settings";
import { activeRecordingKey, useActiveRecording } from "../lib/api/meetings";

interface MicEchoToggleProps {
  meetingId: string;
  recording: boolean;
}

export function MicEchoToggle({ meetingId, recording }: MicEchoToggleProps) {
  const qc = useQueryClient();
  const active = useActiveRecording();
  const enabled = recording ? (active.data?.echo_enabled ?? false) : false;

  const setEcho = useMutation({
    mutationFn: (next: boolean) => audioApi.setEcho(meetingId, { enabled: next }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: activeRecordingKey }),
  });

  const toggle = () => {
    if (!recording || setEcho.isPending) return;
    setEcho.mutate(!enabled);
  };

  useKeyboardShortcut({ key: "e", ignoreWhenTyping: true, enabled: recording }, toggle);

  return (
    <div>
      <Button
        variant={enabled ? "on" : "secondary"}
        disabled={!recording || setEcho.isPending}
        onClick={toggle}
        aria-label={enabled ? "Stop echo" : "Echo mic"}
        title={
          !recording
            ? "Echo is available once recording starts"
            : enabled
              ? "Stop echoing the mic (E)"
              : "Echo the mic to the selected output device (E)"
        }
        className="w-full py-3.5 text-[15px]"
      >
        <Volume2 size={18} />
        Echo
        {recording && (
          <kbd className="bg-black/10 text-current text-[11px] font-mono px-1.5 py-0.5 rounded ml-1">
            E
          </kbd>
        )}
      </Button>
      {setEcho.isError && (
        <p className="mt-1.5 text-[12px] font-mono text-[var(--color-straw)]">
          {setEcho.error instanceof Error ? setEcho.error.message : "Couldn't update echo"}
        </p>
      )}
    </div>
  );
}
