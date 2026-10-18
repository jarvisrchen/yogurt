import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { Preset } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";

// The preset list is owned by the Rust `PRESETS` const (crates/yogurt-db/src/providers.rs).
// Do not enumerate it here, it drifts.

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
        adapter: preset.adapter,
        // A `cli` row leaves `cli_model` empty until a Test passes, so the MODEL picker stays hidden.
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
      className="inline-flex items-center gap-1.5 rounded-pill px-2.5 py-1 text-[12px] font-medium leading-none border bg-card text-mut border-line hover:text-ink hover:border-grey disabled:opacity-50"
    >
      {clone.isPending ? "…" : `+ ${preset.name}`}
    </button>
  );
}
