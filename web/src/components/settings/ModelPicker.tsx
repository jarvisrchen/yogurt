/**
 * ModelPicker — pill row of whisper.cpp models (Phase 8 Plan 08-03).
 *
 * One pill per `ModelView`.  Behavior:
 *   - Downloaded models render with a `✓` glyph; clicking calls
 *     `onSelect(name)`.
 *   - Undownloaded models render with a `↓` glyph + tooltip
 *     ("download (NN MB)"); clicking calls `onRequestDownload(name)`
 *     which the parent uses to open the download dialog.
 *   - On Intel hardware (detected via `navigator.userAgent`) a model
 *     with `intel_supported: false` renders an additional "slow"
 *     straw warning chip per PRD §5.8.
 *   - A model whose only copy came from the Homebrew companion formula
 *     (`managed_by_homebrew`) shows a "brew" chip instead of the trash:
 *     the server refuses to delete out of another tool's prefix.
 *   - A downloaded, non-active model also gets a compact trash affordance
 *     (`onDelete`) next to its pill, following `DeleteMeetingConfirm`'s
 *     icon variant: click the trash, an inline `Delete?` / `Cancel` pair
 *     replaces it, confirming calls `onDelete(name)`. Auto-reverts after
 *     3s. Only one model's confirm can be open at a time.
 *
 * Matcha tokens per PRD §16.2:
 *   - selected pill — `var(--color-matcha)` border + `var(--color-mtsoft)` bg.
 *   - unselected pill — neutral line border + white bg.
 */
import { useEffect, useState } from "react";
import clsx from "clsx";
import { Trash2 } from "lucide-react";
import type { ModelView } from "../../lib/api/stt";
import type { DownloadState } from "../../hooks/useModelDownloadProgress";

interface ModelPickerProps {
  models: ModelView[];
  selected: string | null;
  onSelect: (name: string) => void;
  /** Called whether the pill is for a not-yet-downloaded model OR for
   *  the currently-active-download model. In the latter case the parent
   *  reopens the existing dialog rather than starting a new download. */
  onRequestDownload: (name: string) => void;
  /** Name of the model currently being downloaded, if any. Used to
   *  render the `⟳ NN%` overlay on the matching pill. */
  activeDownloadName?: string | null;
  /** Live progress state from `useModelDownloadProgress` for the active
   *  download. `null` when nothing is in flight. */
  activeDownloadProgress?: DownloadState | null;
  /** Called with the model name to delete after the inline confirm.
   *  Omit to hide the trash affordance entirely. */
  onDelete?: (name: string) => void;
  /** The model currently in use - never offered for deletion. */
  activeModelName?: string | null;
}

/** Naive Intel-Mac detection. UA strings on Apple Silicon Macs identify
 *  as "Intel Mac OS X" in some browsers (Safari at least), so this is a
 *  best-effort signal — the PRD calls Intel support "best-effort" anyway
 *  so a false positive here is non-fatal (we'd over-warn). */
function isIntelMac(): boolean {
  if (typeof navigator === "undefined") return false;
  const ua = navigator.userAgent;
  // Safari/Chrome on Apple Silicon report "Intel Mac OS X"; check for an
  // explicit ARM signal first.  This is what the PRD §16.9 reference
  // implementation recommends.
  if (/Mac/.test(ua) && /arm64|Apple Silicon/i.test(ua)) return false;
  // `userAgentData` is more reliable when available (Chromium-based).
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const uaData = (navigator as any).userAgentData;
  if (uaData?.platform && /macOS/i.test(uaData.platform)) {
    // Heuristic: high-entropy hint architecture would be more reliable
    // but requires async; for an "is this Intel?" warning we accept
    // false positives.
    return /Intel/i.test(ua);
  }
  return /Mac/.test(ua) && /Intel/.test(ua);
}

export function ModelPicker({
  models,
  selected,
  onSelect,
  onRequestDownload,
  activeDownloadName = null,
  activeDownloadProgress = null,
  onDelete,
  activeModelName = null,
}: ModelPickerProps) {
  const intel = isIntelMac();
  const [confirming, setConfirming] = useState<string | null>(null);

  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(null), 3000);
    return () => clearTimeout(t);
  }, [confirming]);

  return (
    <div className="flex flex-wrap gap-2">
      {models.map((m) => {
        const isSelected = selected === m.name;
        const showSlow = intel && !m.intel_supported;
        const isDownloading = activeDownloadName === m.name && !m.downloaded;
        const pct =
          isDownloading &&
          activeDownloadProgress &&
          activeDownloadProgress.totalBytes > 0
            ? Math.min(
                100,
                Math.round(
                  (activeDownloadProgress.bytesDownloaded /
                    activeDownloadProgress.totalBytes) *
                    100,
                ),
              )
            : null;
        const glyph = m.downloaded ? "✓" : isDownloading ? "⟳" : "↓";
        const title = m.downloaded
          ? `${m.name}${isSelected ? " · selected" : ""}`
          : isDownloading
            ? pct != null
              ? `downloading… ${pct}%`
              : "downloading…"
            : `download (${m.size_mb} MB)`;
        return (
          <span key={m.name} className="inline-flex items-center gap-1.5">
            <button
              type="button"
              title={title}
              aria-pressed={isSelected}
              aria-label={
                m.downloaded
                  ? `Select ${m.name}`
                  : isDownloading
                    ? `Show download progress for ${m.name}`
                    : `Download ${m.name} (${m.size_mb} MB)`
              }
              onClick={() =>
                m.downloaded ? onSelect(m.name) : onRequestDownload(m.name)
              }
              className={clsx(
                "text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full transition-colors",
                // nowrap + shrink-0: when the delete confirm pair appears next
                // to the pill, the pill must wrap to the next row as a unit,
                // not squeeze its size chip onto a second line.
                "inline-flex items-center gap-1.5 whitespace-nowrap shrink-0",
                isSelected
                  ? "border-[1.5px] border-[var(--color-matcha)] bg-[var(--color-mtsoft)] text-[var(--color-matcha)]"
                  : m.downloaded
                    ? "border border-line bg-white text-ink hover:border-[var(--color-matcha)]"
                    : isDownloading
                      ? "border border-[var(--color-matcha)] bg-[var(--color-mtsoft)] text-[var(--color-matcha)] hover:opacity-90"
                      : "border border-dashed border-line bg-white text-mut hover:border-[var(--color-matcha)] hover:text-[var(--color-matcha)]",
              )}
            >
              <span aria-hidden>{m.name}</span>
              <span aria-hidden className={isDownloading ? "animate-spin" : ""}>
                {glyph}
              </span>
              <span
                aria-hidden
                className="text-[10px] font-mono normal-case tracking-normal"
                title={
                  m.managed_by_homebrew
                    ? `${m.size_mb} MB installed by Homebrew`
                    : `${m.size_mb} MB on huggingface.co`
                }
              >
                {m.size_mb < 1000
                  ? `${m.size_mb} MB`
                  : `${(m.size_mb / 1024).toFixed(1)} GB`}
              </span>
            </button>
            {isDownloading ? (
              <span
                aria-hidden
                className="text-[10px] font-mono text-[var(--color-matcha)] font-semibold"
                title="click pill to reopen progress dialog"
              >
                {pct != null ? `${pct}%` : "…"}
              </span>
            ) : null}
            {showSlow && (
              <span
                title="Slower than real-time on Intel"
                className="text-[10px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded bg-strsoft text-ink border border-straw/40"
              >
                slow
              </span>
            )}
            {m.downloaded && m.managed_by_homebrew ? (
              <span
                title="Installed by Homebrew - remove it with brew uninstall"
                className="text-[10px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded bg-line/40 text-mut border border-line"
              >
                brew
              </span>
            ) : m.downloaded && onDelete && m.name !== activeModelName ? (
              confirming === m.name ? (
                <span className="inline-flex items-center gap-1">
                  <button
                    type="button"
                    autoFocus
                    aria-label={`Confirm delete ${m.name}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirming(null);
                      onDelete(m.name);
                    }}
                    className="text-[10px] font-mono uppercase tracking-wider px-2 py-0.5 rounded-full bg-strsoft text-ink border border-straw/40 font-semibold whitespace-nowrap hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-straw/50"
                  >
                    Delete?
                  </button>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirming(null);
                    }}
                    className="text-[10px] font-mono text-mut hover:text-ink"
                  >
                    Cancel
                  </button>
                </span>
              ) : (
                <button
                  type="button"
                  aria-label={`Delete ${m.name}`}
                  title="Delete downloaded model"
                  onClick={(e) => {
                    e.stopPropagation();
                    setConfirming(m.name);
                  }}
                  className="p-1 rounded-full text-mut hover:text-straw hover:bg-line/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-straw/40"
                >
                  <Trash2 size={13} aria-hidden />
                </button>
              )
            ) : null}
          </span>
        );
      })}
    </div>
  );
}
