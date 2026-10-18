/**
 * Native `<dialog>` for free focus trap, Escape-to-close and backdrop semantics.
 * Cancel only closes the dialog: the background download keeps running, since there is no cancel token into `download_to` yet.
 * On `state.complete` it auto-closes after 600ms so the user sees 100% briefly.
 */
import { useEffect, useRef } from "react";
import { useDownloadModel } from "../../lib/api/stt";
import type { DownloadState } from "../../hooks/useModelDownloadProgress";

interface ModelDownloadDialogProps {
  model: string | null;
  sizeMb: number | null;
  /** Owned by `LocalSTTCard` so the subscription survives dialog open/close. */
  progress: DownloadState | null;
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
  const state = progress;

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (model) {
      if (!d.open) d.showModal();
      // POST /download once per browser session and model: reopening for an already-running download must not re-POST.
      if (startedRef.current !== model) {
        startedRef.current = model;
        dl.mutate(model);
      }
    } else {
      if (d.open) d.close();
      // Do NOT reset startedRef here. The parent keeps the download alive after Run-in-background,
      // and a re-POST on reopen would spawn a duplicate worker racing on the same file.
    }
  }, [model, dl]);

  useEffect(() => {
    if (!state?.complete) return;
    const t = window.setTimeout(onClose, 600);
    return () => window.clearTimeout(t);
  }, [state?.complete, onClose]);

  if (!model) {
    // Empty `<dialog>` keeps the ref stable before `model` is first non-null.
    return (
      <dialog
        ref={ref}
        className="rounded-panel p-0 backdrop:bg-black/30"
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
      className="rounded-panel p-0 backdrop:bg-black/30 w-[420px] bg-[var(--color-card)] text-ink shadow-window"
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
            <h3 className="heading-wordmark leading-tight">
              {title}
            </h3>
            <p className="text-[11px] text-mut">
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
            <p className="text-[11px] text-mut">
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
