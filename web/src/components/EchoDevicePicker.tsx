/**
 * "Echo to" device dropdown on the meeting page.
 * While recording, a selection hot-swaps the live device; otherwise it writes the setting used for the next recording.
 */
import { useState } from "react";
import { DeviceOptions } from "./DeviceOptions";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { audioApi, settingsApi, settingsKey } from "../lib/api/settings";

interface EchoDevicePickerProps {
  meetingId: string;
  recording: boolean;
}

export function EchoDevicePicker({ meetingId, recording }: EchoDevicePickerProps) {
  const qc = useQueryClient();
  const devices = useQuery({
    queryKey: ["audio-output-devices"],
    queryFn: audioApi.outputDevices,
  });

  const settings = useQuery({
    queryKey: settingsKey,
    queryFn: settingsApi.get,
  });

  const [activeDevice, setActiveDevice] = useState<string | null>(null);

  const setDevice = useMutation({
    mutationFn: async (device: string) => {
      if (recording) {
        const res = await audioApi.setEcho(meetingId, { device });
        return res.device;
      }
      const res = await settingsApi.patch({ audio_echo_output_device: device });
      void qc.invalidateQueries({ queryKey: settingsKey });
      return res.audio_echo_output_device;
    },
    onSuccess: (device) => setActiveDevice(device),
  });

  if (devices.isLoading) {
    return <span className="text-[12px] font-mono text-mut">Loading devices…</span>;
  }

  if (devices.isError) {
    return <span className="text-[12px] font-mono text-mut">Device list unavailable</span>;
  }

  const effectiveValue =
    activeDevice ?? settings.data?.general.audio_echo_output_device ?? "";

  return (
    <div className="flex items-center gap-1.5 w-full">
      <select
        aria-label="Echo to"
        className="w-full text-[12px] font-mono rounded-md border border-line px-2 py-1"
        value={effectiveValue}
        disabled={setDevice.isPending}
        onChange={(e) => setDevice.mutate(e.target.value)}
      >
        <option value="">System default</option>
        <DeviceOptions devices={devices.data} selected={effectiveValue} />
      </select>
      {setDevice.isPending && (
        <span className="text-[12px] font-mono text-mut">
          {recording ? "Switching…" : "Saving…"}
        </span>
      )}
      {setDevice.isError && (
        <span className="text-[12px] font-mono text-[var(--color-straw)]">
          {setDevice.error instanceof Error
            ? setDevice.error.message
            : recording
              ? "Switch failed"
              : "Save failed"}
        </span>
      )}
    </div>
  );
}
