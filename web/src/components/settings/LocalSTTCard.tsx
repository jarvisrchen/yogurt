/**
 * LocalSTTCard — Settings → Transcription "Local · whisper.cpp" card
 * (Phase 8 Plan 08-03, PRD §5.6 / D-12).
 *
 * Replaces the Phase-7 STATE-04 "Coming in v1" stub.  Renders:
 *   - card chrome with matcha tinting when `active`
 *   - header with the title + a "Use Local" radio button
 *   - explanatory body copy
 *   - a `<ModelPicker />` row driven by `useModels()`
 *   - inline `<ModelDownloadDialog />` opened by `↓`-pill clicks
 *   - a mono footer noting `~/.yogurt/models` storage
 *
 * State strategy: model-list comes from TanStack Query; selection +
 * `stt_provider="local"` flip are owned by the parent (Settings.tsx).
 * Download-dialog visibility is local UI state on this card.
 */
import { useState } from "react";
import clsx from "clsx";
import { useModels } from "../../lib/api/stt";
import { ModelPicker } from "./ModelPicker";
import { ModelDownloadDialog } from "../dialogs/ModelDownloadDialog";

interface LocalSTTCardProps {
  /** `true` when `settings.stt_provider === "local"`. */
  active: boolean;
  /** Currently selected model name (e.g. "small.en"). */
  selectedModel: string;
  /** Called when the user clicks a downloaded model pill. */
  onSelectModel: (name: string) => void;
  /** Called when the user clicks the "Use Local" radio. */
  onActivate: () => void;
}

export function LocalSTTCard({
  active,
  selectedModel,
  onSelectModel,
  onActivate,
}: LocalSTTCardProps) {
  const q = useModels();
  const [downloading, setDownloading] = useState<{
    name: string;
    sizeMb: number;
  } | null>(null);

  return (
    <article
      data-testid="local-stt-card"
      className={clsx(
        "rounded-xl p-5 space-y-3 bg-white transition-colors",
        active
          ? "border-[1.5px] border-[var(--color-matcha)]"
          : "border border-neutral-300",
      )}
    >
      <header className="flex items-center justify-between">
        <h3 className="font-serif text-xl">Local · whisper.cpp</h3>
        <label className="inline-flex items-center gap-2 text-[11px] font-mono uppercase tracking-wider cursor-pointer">
          <input
            type="radio"
            name="stt-provider"
            checked={active}
            onChange={onActivate}
            className="accent-[var(--color-matcha)]"
          />
          <span>Use Local</span>
        </label>
      </header>

      <p className="text-[13px] text-mut">
        Fully on-device transcription via Metal-accelerated whisper.cpp.
        Most users stay on Cloud — Local is the privacy escape hatch when
        audio can't leave the machine.
      </p>

      {q.isLoading && (
        <p className="text-[11px] font-mono text-mut">Loading models…</p>
      )}
      {q.isError && (
        <p className="text-[11px] font-mono text-[var(--color-straw)]">
          Failed to load models: {String(q.error)}
        </p>
      )}
      {q.data && (
        <ModelPicker
          models={q.data}
          selected={selectedModel}
          onSelect={onSelectModel}
          onRequestDownload={(name) => {
            const m = q.data.find((x) => x.name === name);
            if (!m) return;
            setDownloading({ name: m.name, sizeMb: m.size_mb });
          }}
        />
      )}

      <p className="text-[11px] font-mono text-mut pt-1">
        Models download on first use · stored in{" "}
        <code className="text-ink">~/.yogurt/models</code>
      </p>

      <ModelDownloadDialog
        model={downloading?.name ?? null}
        sizeMb={downloading?.sizeMb ?? null}
        onClose={() => setDownloading(null)}
      />
    </article>
  );
}
