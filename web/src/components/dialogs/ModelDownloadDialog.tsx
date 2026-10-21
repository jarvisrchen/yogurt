/**
 * Native `<dialog>` for free focus trap, Escape-to-close and backdrop semantics.
 * Mounted once in the Shell and driven by `useModelDownload`, so it reopens from any page.
 * On `complete` it auto-closes after 600ms so the user sees 100% briefly.
 */
import { useEffect, useRef } from "react";
import {
  beginDownload,
  cancelActiveDownload,
  hideDownloadDialog,
  useModelDownload,
} from "../../hooks/useModelDownload";

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

export function ModelDownloadDialog() {
  const ref = useRef<HTMLDialogElement | null>(null);
  const dl = useModelDownload();
  const open = dl?.dialogOpen ?? false;
  const complete = dl?.progress.complete ?? false;

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  useEffect(() => {
    if (!open || !complete) return;
    const t = window.setTimeout(hideDownloadDialog, 600);
    return () => window.clearTimeout(t);
  }, [open, complete]);

  const state = dl?.progress;
  const total = state?.totalBytes ?? 0;
  const downloaded = state?.bytesDownloaded ?? 0;
  const pct =
    total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
  const error = state?.error ?? null;

  const title = error
    ? "Couldn't download"
    : complete
      ? "Downloaded"
      : "Downloading…";

  const secondary =
    "text-[13px] px-3 py-1.5 rounded border border-line hover:bg-paper";
  const primary =
    "text-[13px] px-3 py-1.5 rounded bg-[var(--color-matcha)] text-[var(--color-card)] hover:opacity-90";

  return (
    <dialog
      ref={ref}
      onClose={hideDownloadDialog}
      data-testid="model-download-dialog"
      className="m-auto rounded-panel p-0 backdrop:bg-black/30 w-[420px] bg-[var(--color-card)] text-ink shadow-window"
    >
      {dl && (
        <div className="p-6 space-y-4">
          <header className="flex items-start gap-3">
            <div
              aria-hidden
              className="w-10 h-10 rounded-full bg-[var(--color-mtsoft)] text-[var(--color-matcha)] flex items-center justify-center text-lg"
            >
              ↓
            </div>
            <div className="flex-1 min-w-0">
              <h3 className="heading-wordmark leading-tight">{title}</h3>
              <p className="text-[11px] text-mut">{dl.model} · whisper.cpp</p>
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
            Stored in <code className="font-mono text-ink">~/.yogurt/models/</code>.
            {!error && " The download keeps going if you close this or leave the page."}
          </p>

          <div className="flex justify-end gap-2 pt-2">
            {error ? (
              <>
                <button type="button" onClick={hideDownloadDialog} className={secondary}>
                  Close
                </button>
                <button
                  type="button"
                  onClick={() => beginDownload(dl.model)}
                  className={primary}
                >
                  Retry
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  onClick={cancelActiveDownload}
                  disabled={complete}
                  className={secondary}
                >
                  Cancel download
                </button>
                <button type="button" onClick={hideDownloadDialog} className={primary}>
                  Run in background
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </dialog>
  );
}
