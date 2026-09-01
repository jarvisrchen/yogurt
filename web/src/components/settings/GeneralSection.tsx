/**
 * GeneralSection — Settings page General section (Phase 5 Plan 05-04, SET-08).
 *
 * - Port input (number, 1024–65535) persists on blur if changed.
 * - "Open browser on start" checkbox persists on toggle.
 * - Both persist via `PATCH /api/settings` and invalidate `['settings']`.
 *
 * Caption explains that port changes apply on the next `yogurt start`.
 * - Appearance (UI-6) is browser-local (see `lib/theme.ts`), not a server
 *   setting, so it applies instantly and survives reload without a flash.
 */
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { settingsApi, type General } from "../../lib/api/settings";
import { getThemePref, setThemePref, type ThemePref } from "../../lib/theme";

const THEME_OPTIONS: { value: ThemePref; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

interface GeneralSectionProps {
  general: General;
}

export function GeneralSection({ general }: GeneralSectionProps) {
  const qc = useQueryClient();
  const patch = useMutation({
    mutationFn: (p: Partial<General>) => settingsApi.patch(p),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  const [theme, setTheme] = useState<ThemePref>(getThemePref);

  return (
    <section className="space-y-5">
      <h2 className="heading-section">General</h2>

      <div className="space-y-1.5">
        <label className="block text-[10px] font-mono uppercase tracking-wider text-mut">
          Appearance
        </label>
        <div
          role="radiogroup"
          aria-label="Appearance"
          className="inline-flex gap-0.5 rounded-[11px] bg-line/60 p-1"
        >
          {THEME_OPTIONS.map((opt) => {
            const selected = theme === opt.value;
            return (
              <button
                key={opt.value}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => {
                  setThemePref(opt.value);
                  setTheme(opt.value);
                }}
                className={`rounded-lg px-4 py-[6px] text-[13px] font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue ${
                  selected
                    ? "bg-card text-ink shadow-[0_1px_3px_rgba(40,30,15,0.1)]"
                    : "text-mut hover:text-ink"
                }`}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>

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
          className="block w-32 rounded-md border border-line bg-card px-3 py-2 text-sm font-mono focus:border-blue focus:outline-none"
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
