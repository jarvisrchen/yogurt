/**
 * MicDevicePicker — mic device dropdown in the meeting toolbar (quick task
 * 260709-wnn; extended to the stopped-but-open state per the follow-up UX
 * fix below).
 *
 * Visible whenever a meeting is open, recording or stopped (`Meeting.tsx`
 * gates the mount on `meetingId` alone and passes the live `recording`
 * flag). The two states write through different paths:
 *   - recording: a selection hot-swaps the live capture device via
 *     `POST /:id/audio-device` (quick task 260709-wnn).
 *   - stopped: a selection instead `PATCH`es `settings.audio_input_device`
 *     — the exact field `POST /:id/start` reads to pick the mic for the
 *     *next* recording (same mechanism `AudioSection.tsx` uses on the
 *     Settings page). This is the only way to change the device before a
 *     meeting has ever recorded, or between recordings in the same
 *     meeting.
 * Shares the `["audio-devices"]` query key with `AudioSection.tsx` so the
 * two share one cached `GET /api/audio/devices` fetch. Session-token auth
 * is resolved internally by `http()`/`bearerFetch` — no token prop needed,
 * matching `AudioSection.tsx`'s pattern.
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { audioApi, settingsApi, settingsKey } from "../lib/api/settings";

interface MicDevicePickerProps {
  meetingId: string;
  /** `true` while the meeting is actively recording — selects the
   *  live-hot-swap write path instead of the persisted-setting one. */
  recording: boolean;
}

export function MicDevicePicker({ meetingId, recording }: MicDevicePickerProps) {
  const qc = useQueryClient();
  const devices = useQuery({
    queryKey: ["audio-devices"],
    queryFn: audioApi.devices,
  });

  // Only needed to seed the picker's default selection while stopped (so
  // it shows what will actually be used on the next Start, not just the
  // OS default) — the live hot-swap path never reads it, so it's disabled
  // while recording.
  const settings = useQuery({
    queryKey: settingsKey,
    queryFn: settingsApi.get,
    enabled: !recording,
  });

  // Distinct from whichever device the OS reports as `is_default` — this
  // is set only on a successful write, so the picker reflects the actual
  // chosen device rather than resetting to the default on every re-render.
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
      <span className="text-[12px] font-mono text-mut">
        Loading mics…
      </span>
    );
  }

  if (devices.isError) {
    return (
      <span className="text-[12px] font-mono text-mut">
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
        className="w-full text-[12px] font-mono rounded-md border border-line px-2 py-1"
        value={effectiveValue}
        disabled={setDevice.isPending}
        onChange={(e) => setDevice.mutate(e.target.value)}
      >
        {devices.data?.map((d) => (
          <option key={d.name} value={d.name}>
            {d.name}
            {d.is_default ? " (default)" : ""}
          </option>
        ))}
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
