/**
 * LocalSTTCard — Settings → Transcription "Local · whisper.cpp" card
 * (Phase 8 Plan 08-03, PRD §5.6 / D-12).
 *
 * Replaces the Phase-7 STATE-04 "Coming in v1" stub.  Renders:
 *   - card chrome with matcha tinting when `active`
 *   - header with the title + a "Use Local" radio button
 *   - explanatory body copy
 *   - a `<ModelPicker />` row driven by `useModels()`, with live `⟳ NN%`
 *     progress on the pill of any currently-downloading model
 *   - inline `<ModelDownloadDialog />` opened by `↓`-pill clicks (or by
 *     re-clicking an in-progress pill to see byte/rate/ETA detail)
 *   - a mono footer noting `~/.yogurt/models` storage
 *
 * State strategy:
 *   - `activeDownload` is THE truth about "is a download in flight?".
 *     Set when a download starts, cleared 600 ms after complete/error so
 *     the user sees 100% momentarily. Survives the dialog closing —
 *     that's the entire point of "Run in background".
 *   - `dialogOpen` is JUST about whether the modal is currently visible.
 *     Closing the modal does NOT cancel the download; it just hides the
 *     modal. The pill keeps showing `⟳ NN%` until completion.
 *   - The WS hook lives at this level (not inside the dialog) so the
 *     subscription persists across dialog open/close cycles. On
 *     `_complete` it invalidates `sttKeys.models` and the picker pill
 *     flips from `↓` to `✓` automatically.
 *
 * V1 limitation: only one concurrent download is tracked in the UI. The
 * server happily runs N parallel downloads (each `tokio::spawn`), but
 * clicking a second pill while one is in flight replaces the tracked
 * download — the original still completes server-side and its pill
 * flips to ✓ via cache invalidation.
 */
import { useEffect, useState } from "react";
import clsx from "clsx";
import { useModels } from "../../lib/api/stt";
import { useModelDownloadProgress } from "../../hooks/useModelDownloadProgress";
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
  // The download we're currently tracking (survives dialog close).
  const [activeDownload, setActiveDownload] = useState<{
    name: string;
    sizeMb: number;
  } | null>(null);
  // Whether the modal is currently open.  Closes via Cancel / Run in
  // background / Escape; reopens when the user re-clicks the pill.
  const [dialogOpen, setDialogOpen] = useState(false);

  // Subscribe at THIS level so progress keeps flowing even when the
  // dialog is closed.  Subscription drops when activeDownload is null,
  // i.e. nothing in flight.
  const progress = useModelDownloadProgress(activeDownload?.name ?? null);

  // 600 ms after the download completes successfully, clear
  // activeDownload so the pill stops showing `⟳ NN%`. The models cache
  // has already been invalidated by the hook, so the pill will read
  // `downloaded:true` by the time this fires and renders as ✓.
  //
  // DO NOT auto-clear on error — the user needs to SEE the error.  On
  // error, we keep activeDownload alive so:
  //   - the pill stays `⟳ NN%` styled (a follow-up commit could flip
  //     to a `⚠ failed` look but for v1 leaving the pill as-is is fine)
  //   - re-clicking the pill reopens the dialog with the error visible
  //   - the user can read the SHA mismatch / network error message and
  //     decide whether to retry (which resumes from the partial file)
  //     or report it.
  // The user dismisses the error explicitly via the dialog's Cancel
  // button, which clears activeDownload via the onClose path below.
  useEffect(() => {
    if (!progress?.complete) return;
    const t = window.setTimeout(() => setActiveDownload(null), 600);
    return () => window.clearTimeout(t);
  }, [progress?.complete]);

  const startOrReopenDownload = (name: string) => {
    const m = q.data?.find((x) => x.name === name);
    if (!m) return;
    // If this pill is already the active download, just reopen the dialog.
    if (activeDownload?.name === name) {
      setDialogOpen(true);
      return;
    }
    // Otherwise start a new download (replaces the tracked one).
    setActiveDownload({ name: m.name, sizeMb: m.size_mb });
    setDialogOpen(true);
  };

  const handleDialogClose = () => {
    setDialogOpen(false);
    // If the user is closing the dialog while an error is visible, treat
    // that as dismissing the failed-download state — clear activeDownload
    // so the pill returns to its idle ↓ form. Without this the pill is
    // stuck in the ⟳ state forever after a failure.
    // Running download with no error: leave activeDownload alive so the
    // pill keeps ticking and the user can reopen the dialog for detail.
    if (progress?.error) {
      setActiveDownload(null);
    }
  };

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
          onRequestDownload={startOrReopenDownload}
          activeDownloadName={activeDownload?.name ?? null}
          activeDownloadProgress={progress}
        />
      )}

      <p className="text-[11px] font-mono text-mut pt-1">
        Models download on first use · stored in{" "}
        <code className="text-ink">~/.yogurt/models</code>
      </p>

      <ModelDownloadDialog
        model={dialogOpen ? activeDownload?.name ?? null : null}
        sizeMb={activeDownload?.sizeMb ?? null}
        progress={progress}
        onClose={handleDialogClose}
      />
    </article>
  );
}
