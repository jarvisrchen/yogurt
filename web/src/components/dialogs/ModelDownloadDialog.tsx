/**
 * ModelDownloadDialog — STATE-04 first-time model download modal
 * (Phase 8 Plan 08-03, PRD §5.11).
 *
 * Renders a native `<dialog>` element so we get correct focus trap,
 * Escape-to-close, and backdrop semantics for free.  Wires
 * `useModelDownloadProgress(model)` for live byte / rate / ETA from the
 * `/ws` channel, and fires `POST /api/stt/models/:name/download` once
 * on open.
 *
 * Lifecycle:
 *   - `model === null` → dialog is closed (`d.close()`).
 *   - `model !== null` → `d.showModal()` and call `startDownload(model)`
 *     the first time we observe this `model` value.
 *   - On `state.complete` → `setTimeout(onClose, 600)` so the user sees
 *     100% momentarily before the dialog auto-closes.
 *   - On `state.error` → keep the dialog open with the error text; user
 *     can Cancel to dismiss, or hit Retry to re-fire the download.
 *
 * V1 limitation (CONTEXT D-13): the Cancel button just closes the
 * dialog — the background download keeps running.  True cancellation
 * (plumbing a cancel-token into `download_to`) is deferred to v1.1.
 */
import { useEffect, useRef } from "react";
import { useDownloadModel } from "../../lib/api/stt";
import type { DownloadState } from "../../hooks/useModelDownloadProgress";

interface ModelDownloadDialogProps {
  /** Model name to download (e.g. "small.en"). `null` → dialog closed. */
  model: string | null;
  /** Total size in MB to display in the mono caption. */
  sizeMb: number | null;
  /** Live progress state owned by `LocalSTTCard` (so the subscription
   *  survives dialog open/close cycles). */
  progress: DownloadState | null;
  /** Called when the user clicks Cancel / Run in background, or when
   *  the download completes (auto-close after a 600 ms peek at 100%). */
  onClose: () => void;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatRate(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return "—";
  if (bytesPerSec < 1024 * 1024) {
    return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
  }
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

function formatEta(seconds: number | null): string {
  if (seconds == null) return "—";
  if (seconds < 60) return `${seconds}s left`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}m${s.toString().padStart(2, "0")}s left`;
}

export function ModelDownloadDialog({
  model,
  sizeMb,
  progress,
  onClose,
}: ModelDownloadDialogProps) {
  const ref = useRef<HTMLDialogElement | null>(null);
  const startedRef = useRef<string | null>(null);
  const dl = useDownloadModel();
  // Progress is owned by the parent (LocalSTTCard) so it survives the
  // dialog closing. Local alias kept for readability with the original
  // render code below.
  const state = progress;

  // Open / close the native dialog as `model` flips, and kick off the
  // download POST once per model open.
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (model) {
      if (!d.open) d.showModal();
      // POST /download only once per (browser session × model). If the
      // user reopens the dialog for an already-running download (clicked
      // the ⟳ pill while progress was streaming), don't re-POST — the
      // worker is already running and the WS state is already current.
      if (startedRef.current !== model) {
        startedRef.current = model;
        dl.mutate(model);
      }
    } else {
      if (d.open) d.close();
      // INTENTIONALLY do NOT reset startedRef here. The parent
      // (LocalSTTCard) keeps the underlying download alive when the
      // dialog closes via Run-in-background, so reopening the dialog
      // for the same model must NOT re-POST to /download — that would
      // spawn a duplicate server-side worker that races on the same
      // file path. startedRef only flips when the parent actually
      // switches to a different model (the `!== model` check above).
    }
  }, [model, dl]);

  // Auto-close 600 ms after completion so the user sees 100% momentarily.
  useEffect(() => {
    if (!state?.complete) return;
    const t = window.setTimeout(onClose, 600);
    return () => window.clearTimeout(t);
  }, [state?.complete, onClose]);

  if (!model) {
    // Render an empty `<dialog>` so the ref is stable; this branch
    // matters on first mount before `model` is ever non-null.
    return (
      <dialog
        ref={ref}
        className="rounded-lg p-0 backdrop:bg-black/30"
        onClose={onClose}
      />
    );
  }

  const total = state?.totalBytes ?? 0;
  const downloaded = state?.bytesDownloaded ?? 0;
  const pct =
    total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
  const error = state?.error ?? null;
  const complete = state?.complete ?? false;

  const title = error
    ? "Couldn't download"
    : complete
      ? "Downloaded"
      : "Downloading…";

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      data-testid="model-download-dialog"
      className="rounded-lg p-0 backdrop:bg-black/30 w-[420px] bg-[var(--color-card)] text-ink shadow-xl"
    >
      <div className="p-6 space-y-4">
        <header className="flex items-start gap-3">
          <div
            aria-hidden
            className="w-10 h-10 rounded-full bg-[var(--color-mtsoft)] text-[var(--color-matcha)] flex items-center justify-center text-lg"
          >
            ↓
          </div>
          <div className="flex-1 min-w-0">
            <h3 className="font-serif text-[22px] leading-tight">
              {title}
            </h3>
            <p className="text-[11px] font-mono uppercase tracking-wider text-mut">
              whisper.cpp · {sizeMb ?? "—"} MB
            </p>
          </div>
        </header>

        {error ? (
          <p className="text-[13px] text-[var(--color-straw)]">{error}</p>
        ) : (
          <>
            <div
              className="h-2 w-full rounded-full bg-[var(--color-mtsoft)] overflow-hidden"
              role="progressbar"
              aria-valuenow={pct}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div
                className="h-full bg-[var(--color-matcha)] transition-[width] duration-200"
                style={{ width: `${pct}%` }}
              />
            </div>
            <p className="text-[11px] font-mono text-mut">
              {formatBytes(downloaded)} / {total > 0 ? formatBytes(total) : "—"}
              {" · "}
              {formatRate(state?.bytesPerSec ?? 0)}
              {" · "}
              {formatEta(state?.etaSeconds ?? null)}
            </p>
          </>
        )}

        <p className="text-[13px] text-mut">
          Stored in <code className="font-mono text-ink">~/.yogurt/models/</code>. You
          can keep this dialog open or run the download in the background —
          either way it'll finish on its own.
        </p>

        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onClose}
            className="text-[13px] px-3 py-1.5 rounded border border-line hover:bg-paper"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onClose}
            className="text-[13px] px-3 py-1.5 rounded bg-[var(--color-matcha)] text-white hover:opacity-90"
          >
            Run in background
          </button>
        </div>
      </div>
    </dialog>
  );
}
