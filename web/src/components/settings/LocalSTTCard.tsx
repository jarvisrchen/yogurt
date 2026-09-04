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
 *   - a mono footer noting the models directory
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
 *
 * "Use Local" guard (fast task) — activating local STT with a model that
 * was never downloaded used to be possible from this card even though
 * recording would then fail at meeting start. The "Use Local" radio is
 * now disabled unless the currently-selected model's `downloaded` flag
 * (from `useModels()`, already fetched for the pill row) is true, with a
 * "Download the model first" hint taking the place of the radio.
 *
 * Delete affordance - `ModelPicker` renders a trash icon next to every
 * downloaded, non-active model pill. Confirming wires to
 * `useDeleteModel()`; the active model (whichever `stt_model` is
 * selected while local is in use) is never offered, since the server
 * 409s that case anyway. A 409/other error renders as a `role="alert"`
 * line via `deleteErrorMessage`, which strips the `http()` helper's
 * `"<status> <statusText>: "` prefix; a success renders a transient
 * "Deleted <name> - freed <size>" line for 4s.
 */
import { useEffect, useState } from "react";
import clsx from "clsx";
import { useModels, useDeleteModel, formatBytes } from "../../lib/api/stt";
import { settingsApi } from "../../lib/api/settings";
import { useModelDownloadProgress } from "../../hooks/useModelDownloadProgress";
import { ModelPicker } from "./ModelPicker";
import { ModelDownloadDialog } from "../dialogs/ModelDownloadDialog";
import { TestKeyButton } from "./TestKeyButton";

/** `http()` throws `Error("<status> <statusText>: <raw body>")`. The
 *  delete endpoint's 409 body is a plain sentence (not JSON) - strip the
 *  status prefix so the user sees just that sentence. */
function deleteErrorMessage(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err);
  // `[^:]*` not `+`: HTTP/2 has no reason phrase, so the prefix can be "409 : ".
  return raw.replace(/^\d+\s+[^:]*:\s*/, "");
}

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
  const del = useDeleteModel();
  // Success line clears itself 4s after a delete resolves; tracked
  // separately from `del.isSuccess` so a later error doesn't leave a
  // stale "Deleted…" line hanging around forever.
  const [showFreed, setShowFreed] = useState(false);
  useEffect(() => {
    if (!del.isSuccess) return;
    setShowFreed(true);
    const t = window.setTimeout(() => setShowFreed(false), 4000);
    return () => window.clearTimeout(t);
  }, [del.isSuccess, del.data]);
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

  // The model backing `selectedModel` must actually be on disk — otherwise
  // recording fails at meeting start with a backend error the user can't
  // self-serve from here. Guard-gap fix: this used to be `!active &&
  // !selectedModelDownloaded`, so once `stt_provider` was ALREADY "local"
  // the `!active` short-circuit made the guard permanently false — a
  // persisted-but-broken combo (e.g. the active model got deleted, or a
  // bad row predates this guard) rendered as if nothing were wrong: no
  // warning, and the radio looked normally enabled. Dropping `!active`
  // makes the warning + disabled state track disk truth regardless of
  // which provider is currently active.
  const selectedModelDownloaded =
    q.data?.find((m) => m.name === selectedModel)?.downloaded ?? false;
  const activateBlocked = !selectedModelDownloaded;

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
        "rounded-xl p-5 space-y-3 bg-card transition-colors",
        active
          ? "border-[1.5px] border-[var(--color-matcha)]"
          : "border border-line",
      )}
    >
      <header className="flex items-center justify-between">
        <h3 className="heading-card">Local · whisper.cpp</h3>
        <div className="flex flex-col items-end gap-1">
          <label
            className={clsx(
              "inline-flex items-center gap-2 text-[11px] font-mono uppercase tracking-wider",
              activateBlocked ? "cursor-not-allowed opacity-50" : "cursor-pointer",
            )}
          >
            <input
              type="radio"
              name="stt-provider"
              checked={active}
              disabled={activateBlocked}
              // `disabled` alone is a UI nicety, not enforcement — guard
              // in the handler too so the activation can't fire via a
              // synthetic/programmatic change event on a disabled input.
              onChange={() => {
                if (!activateBlocked) onActivate();
              }}
              className="accent-[var(--color-matcha)]"
            />
            <span>Use Local</span>
          </label>
          {activateBlocked && (
            <span className="text-[10px] font-mono text-[var(--color-straw)]">
              Download the model first
            </span>
          )}
        </div>
      </header>

      <p className="text-[13px] text-mut">
        Whisper.cpp with Metal acceleration. Audio never leaves this Mac.
        Local is free.
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
          onDelete={(name) => del.mutate(name)}
          activeModelName={active ? selectedModel : null}
        />
      )}

      {q.data && (
        <div className="flex flex-wrap items-center gap-2 pt-1 border-t border-line">
          <TestKeyButton
            providerName={selectedModel}
            hasStoredKey={selectedModelDownloaded}
            testFn={() => settingsApi.testLocalStt(selectedModel)}
          />
        </div>
      )}

      {del.isError && (
        <p role="alert" className="text-[11px] font-mono text-[var(--color-straw)]">
          {deleteErrorMessage(del.error)}
        </p>
      )}
      {showFreed && del.data && (
        <p
          data-testid="model-freed"
          className="text-[11px] font-mono text-[var(--color-matcha)]"
        >
          Deleted {del.variables}
          {del.data.freed_bytes > 0
            ? ` - freed ${formatBytes(del.data.freed_bytes)}`
            : ""}
        </p>
      )}

      <p className="text-[11px] font-mono text-mut pt-1">
        Models download on first use · stored in{" "}
        <code className="text-ink">~/.yogurt/models/</code>
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
