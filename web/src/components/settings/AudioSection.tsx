/**
 * AudioSection — Settings page Audio section (Phase 5 Plan 05-04, SET-08).
 *
 * Renders a `<select>` of audio input devices fetched from Phase 2's
 * `GET /api/audio/devices`. Persists selection via
 * `PATCH /api/settings { audio_input_device }` and invalidates the
 * `['settings']` query so the rest of the page re-reads.
 */
import { DeviceOptions } from "../DeviceOptions";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { audioApi, settingsApi, type General } from "../../lib/api/settings";
import { EchoTestButton } from "../EchoTestButton";

interface AudioSectionProps {
  general: General;
}

const ECHO_BUFFERS = [128, 256, 512, 1024, 2048] as const;

export function AudioSection({ general }: AudioSectionProps) {
  const qc = useQueryClient();
  const devices = useQuery({
    queryKey: ["audio-devices"],
    queryFn: audioApi.devices,
  });
  const outputDevices = useQuery({
    queryKey: ["audio-output-devices"],
    queryFn: audioApi.outputDevices,
  });

  const patch = useMutation({
    mutationFn: (audio_input_device: string) =>
      settingsApi.patch({ audio_input_device }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  const patchEchoDevice = useMutation({
    mutationFn: (audio_echo_output_device: string) =>
      settingsApi.patch({ audio_echo_output_device }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  const patchEchoBuffer = useMutation({
    mutationFn: (audio_echo_buffer: number) =>
      settingsApi.patch({ audio_echo_buffer }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  return (
    <section className="space-y-4">
      <h2 className="heading-section">Audio</h2>

      <div className="space-y-1.5">
        <label className="text-[10px] font-mono uppercase tracking-wider text-mut">
          Input device
        </label>
        <select
          className="block w-full rounded-md border border-line bg-card px-3 py-2 text-sm focus:border-blue focus:outline-none"
          value={general.audio_input_device}
          onChange={(e) => patch.mutate(e.target.value)}
          disabled={devices.isLoading || patch.isPending}
        >
          <option value="">System default</option>
          <DeviceOptions devices={devices.data} selected={general.audio_input_device} />
        </select>
      </div>

      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <label className="text-[10px] font-mono uppercase tracking-wider text-mut">
            Echo output device
          </label>
          <EchoTestButton device={general.audio_echo_output_device} />
        </div>
        <select
          className="block w-full rounded-md border border-line bg-card px-3 py-2 text-sm focus:border-blue focus:outline-none"
          value={general.audio_echo_output_device}
          onChange={(e) => patchEchoDevice.mutate(e.target.value)}
          disabled={outputDevices.isLoading || patchEchoDevice.isPending}
        >
          <option value="">System default</option>
          <DeviceOptions devices={outputDevices.data} selected={general.audio_echo_output_device} />
        </select>
        <p className="text-xs font-mono text-mut">
          Also changeable from the meeting page. Use a virtual device such as
          BlackHole to hand your mic to Zoom or OBS while yogurt records.
        </p>
      </div>

      <div className="space-y-1.5">
        <label className="block text-[10px] font-mono uppercase tracking-wider text-mut">
          Echo buffer
        </label>
        <div className="inline-flex rounded-md border border-line overflow-hidden">
          {ECHO_BUFFERS.map((size) => (
            <button
              key={size}
              type="button"
              onClick={() => patchEchoBuffer.mutate(size)}
              disabled={patchEchoBuffer.isPending}
              aria-pressed={general.audio_echo_buffer === size}
              className={`px-3 py-1.5 text-sm border-r border-line last:border-r-0 ${
                general.audio_echo_buffer === size
                  ? "bg-blsoft text-blue font-semibold"
                  : "text-mut"
              }`}
            >
              {size}
            </button>
          ))}
        </div>
        <p className="text-xs font-mono text-mut">
          Frames per callback. 512 is about 10.7 ms at 48 kHz. Larger is
          safer against dropouts.
        </p>
      </div>

      <p className="text-xs font-mono text-mut">
        System audio is captured via ScreenCaptureKit — no extra setup.
      </p>
    </section>
  );
}
