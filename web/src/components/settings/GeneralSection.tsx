/**
 * GeneralSection — Settings page General section (Phase 5 Plan 05-04, SET-08).
 *
 * - Port input (number, 1024–65535) persists on blur if changed.
 * - "Open browser on start" checkbox persists on toggle.
 * - Both persist via `PATCH /api/settings` and invalidate `['settings']`.
 *
 * Caption explains that port changes apply on the next `yogurt start`.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { settingsApi, type General } from "../../lib/api/settings";

interface GeneralSectionProps {
  general: General;
}

export function GeneralSection({ general }: GeneralSectionProps) {
  const qc = useQueryClient();
  const patch = useMutation({
    mutationFn: (p: Partial<General>) => settingsApi.patch(p),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  return (
    <section className="space-y-5">
      <h2 className="heading-section">General</h2>

      <div className="space-y-1.5">
        <label className="text-[10px] font-mono uppercase tracking-wider text-mut">
          Port
        </label>
        <input
          type="number"
          min={1024}
          max={65535}
          defaultValue={general.port}
          onBlur={(e) => {
            const next = Number.parseInt(e.target.value, 10);
            if (
              Number.isFinite(next) &&
              next >= 1024 &&
              next <= 65535 &&
              next !== general.port
            ) {
              patch.mutate({ port: next });
            }
          }}
          className="block w-32 rounded-md border border-line bg-white px-3 py-2 text-sm font-mono focus:border-blue focus:outline-none"
        />
      </div>

      <label className="flex items-center gap-2 text-sm text-ink">
        <input
          type="checkbox"
          defaultChecked={general.open_browser_on_start}
          onChange={(e) =>
            patch.mutate({ open_browser_on_start: e.target.checked })
          }
          className="h-4 w-4 accent-blue"
        />
        <span>Open browser on start</span>
      </label>

      <p className="text-xs font-mono text-mut">
        Port change applies on next `yogurt start`.
      </p>
    </section>
  );
}
