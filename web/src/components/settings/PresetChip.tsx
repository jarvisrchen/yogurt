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
 * v1 ships: Minimax, OpenAI, Ollama (local), LM Studio (local),
 * OpenRouter (defined by the Rust-side `PRESETS` const).
 */

export function PresetChip({ preset }: { preset: Preset }) {
  const qc = useQueryClient();
  const clone = useMutation({
    mutationFn: () =>
      settingsApi.createProvider({
        name: preset.name,
        base_url: preset.base_url,
        model: preset.default_model,
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
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
