import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { Preset } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";

/**
 * Preset chip — Phase 5 (Plan 05-03), SET-06.
 *
 * Dashed-border, font-mono, uppercase pill. Clicking instantiates a new
 * inactive provider via `POST /api/settings/providers` with the preset's
 * `name + base_url + default_model`. Activation is a separate explicit
 * action (UI-SPEC §Interaction 6 — no auto-promote).
 *
 * The list itself comes from `GET /api/settings` and is owned by the
 * Rust-side `PRESETS` const (`crates/yogurt-db/src/providers.rs`) — do not
 * enumerate it here, it drifts.
 *
 * `onCreated` surfaces the new provider's id back to the page so the
 * freshly-cloned card can auto-open its API key input — the user came
 * here to paste a key, not to click `Add key` again.
 */

export function PresetChip({
  preset,
  onCreated,
}: {
  preset: Preset;
  onCreated?: (id: string) => void;
}) {
  const qc = useQueryClient();
  const clone = useMutation({
    mutationFn: () =>
      settingsApi.createProvider({
        name: preset.name,
        base_url: preset.base_url,
        model: preset.default_model,
      }),
    onSuccess: (created) => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      onCreated?.(created.id);
    },
  });

  return (
    <button
      type="button"
      onClick={() => clone.mutate()}
      disabled={clone.isPending}
      className="text-xs font-mono uppercase tracking-[0.06em] px-3 py-1.5 rounded-full border border-dashed border-grey text-mut hover:border-[var(--color-blue)] hover:text-[var(--color-blue)] disabled:opacity-50"
    >
      {clone.isPending ? "…" : preset.name}
    </button>
  );
}
