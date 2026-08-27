/**
 * MicDevicePicker — live mic device dropdown in the meeting toolbar (quick
 * task 260709-wnn).
 *
 * Visible only while a meeting is recording (`Meeting.tsx` gates the mount).
 * Shares the `["audio-devices"]` query key with `AudioSection.tsx` so the
 * two share one cached `GET /api/audio/devices` fetch. Session-token auth
 * is resolved internally by `http()`/`bearerFetch` — no token prop needed,
 * matching `AudioSection.tsx`'s pattern.
 */
import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { audioApi } from "../lib/api/settings";

interface MicDevicePickerProps {
  meetingId: string;
}

export function MicDevicePicker({ meetingId }: MicDevicePickerProps) {
  const devices = useQuery({
    queryKey: ["audio-devices"],
    queryFn: audioApi.devices,
  });

  // Distinct from whichever device the OS reports as `is_default` — this
  // is set only on a successful switch, so the picker reflects the actual
  // active device rather than resetting to the default on every re-render.
  const [activeDevice, setActiveDevice] = useState<string | null>(null);

  const switchDevice = useMutation({
    mutationFn: (deviceId: string) =>
      audioApi.switchMeetingDevice(meetingId, deviceId),
    onSuccess: (data) => setActiveDevice(data.device),
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

  const effectiveValue =
    activeDevice ??
    devices.data?.find((d) => d.is_default)?.name ??
    devices.data?.[0]?.name ??
    "";

  return (
    <div className="flex items-center gap-1.5">
      <select
        aria-label="Microphone"
        className="text-[12px] font-mono rounded-md border border-line px-2 py-1"
        value={effectiveValue}
        disabled={switchDevice.isPending}
        onChange={(e) => switchDevice.mutate(e.target.value)}
      >
        {devices.data?.map((d) => (
          <option key={d.name} value={d.name}>
            {d.name}
            {d.is_default ? " (default)" : ""}
          </option>
        ))}
      </select>
      {switchDevice.isPending && (
        <span className="text-[12px] font-mono text-mut">
          Switching…
        </span>
      )}
      {switchDevice.isError && (
        <span className="text-[12px] font-mono text-[var(--color-straw)]">
          {switchDevice.error instanceof Error
            ? switchDevice.error.message
            : "Switch failed"}
        </span>
      )}
    </div>
  );
}
