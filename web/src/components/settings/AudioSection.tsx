/**
 * AudioSection — Settings page Audio section (Phase 5 Plan 05-04, SET-08).
 *
 * Renders a `<select>` of audio input devices fetched from Phase 2's
 * `GET /api/audio/devices`. Persists selection via
 * `PATCH /api/settings { audio_input_device }` and invalidates the
 * `['settings']` query so the rest of the page re-reads.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { audioApi, settingsApi, type General } from "../../lib/api/settings";

interface AudioSectionProps {
  general: General;
}

export function AudioSection({ general }: AudioSectionProps) {
  const qc = useQueryClient();
  const devices = useQuery({
    queryKey: ["audio-devices"],
    queryFn: audioApi.devices,
  });

  const patch = useMutation({
    mutationFn: (audio_input_device: string) =>
      settingsApi.patch({ audio_input_device }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  return (
    <section className="space-y-4">
      <h2 className="font-serif text-[28px] leading-none">Audio</h2>

      <div className="space-y-1.5">
        <label className="text-[10px] font-mono uppercase tracking-wider text-mut">
          Input device
        </label>
        <select
          className="block w-full rounded-md border border-line bg-white px-3 py-2 text-sm focus:border-blue focus:outline-none"
          defaultValue={general.audio_input_device}
          onChange={(e) => patch.mutate(e.target.value)}
          disabled={devices.isLoading || patch.isPending}
        >
          <option value="">System default</option>
          {devices.data?.map((d) => (
            <option key={d.name} value={d.name}>
              {d.name}
              {d.is_default ? " (default)" : ""}
            </option>
          ))}
        </select>
      </div>

      <p className="text-xs font-mono text-mut">
        System audio is captured via ScreenCaptureKit — no extra setup.
      </p>
    </section>
  );
}
