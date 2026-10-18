/**
 * While recording, a selection hot-swaps the live device (`POST /:id/audio-device`).
 * Otherwise it PATCHes `settings.audio_input_device`, which the next `POST /:id/start` reads.
 * Shares the `["audio-devices"]` query key with `AudioSection.tsx` for one cached fetch.
 */
import { useState } from "react";
import { DeviceOptions } from "./DeviceOptions";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { audioApi, settingsApi, settingsKey } from "../lib/api/settings";

interface MicDevicePickerProps {
  meetingId: string;
  /** True while recording: hot-swap instead of persisting the setting. */
  recording: boolean;
}

export function MicDevicePicker({ meetingId, recording }: MicDevicePickerProps) {
  const qc = useQueryClient();
  const devices = useQuery({
    queryKey: ["audio-devices"],
    queryFn: audioApi.devices,
  });

  // Seeds the default selection while stopped, so it shows what the next Start will use. Disabled while recording, where hot-swap never reads it.
  const settings = useQuery({
    queryKey: settingsKey,
    queryFn: settingsApi.get,
    enabled: !recording,
  });

  // Set only on a successful write, so the picker keeps the chosen device instead of reverting to the OS default.
  const [activeDevice, setActiveDevice] = useState<string | null>(null);

  const setDevice = useMutation({
    mutationFn: async (deviceId: string) => {
      if (recording) {
        const res = await audioApi.switchMeetingDevice(meetingId, deviceId);
        return res.device;
      }
      const res = await settingsApi.patch({ audio_input_device: deviceId });
      void qc.invalidateQueries({ queryKey: settingsKey });
      return res.audio_input_device;
    },
    onSuccess: (device) => setActiveDevice(device),
  });

  if (devices.isLoading) {
    return (
      <span className="text-[13px] text-mut">
        Loading mics…
      </span>
    );
  }

  if (devices.isError) {
    return (
      <span className="text-[13px] text-mut">
        Mic list unavailable
      </span>
    );
  }

  const persistedDevice = !recording
    ? settings.data?.general.audio_input_device
    : undefined;
  const effectiveValue =
    activeDevice ??
    (persistedDevice || undefined) ??
    devices.data?.find((d) => d.is_default)?.name ??
    devices.data?.[0]?.name ??
    "";

  return (
    <div className="flex items-center gap-1.5 w-full">
      <select
        aria-label="Microphone"
        className="w-full text-[13.5px] text-ink rounded-button border border-line bg-paper px-3 py-2"
        value={effectiveValue}
        disabled={setDevice.isPending}
        onChange={(e) => setDevice.mutate(e.target.value)}
      >
        <DeviceOptions devices={devices.data} selected={effectiveValue} />
      </select>
      {setDevice.isPending && (
        <span className="text-[13px] text-mut">
          {recording ? "Switching…" : "Saving…"}
        </span>
      )}
      {setDevice.isError && (
        <span className="text-[13px] text-[var(--color-straw)]">
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
