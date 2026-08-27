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
 *
 * Matcha tokens per PRD §16.2:
 *   - selected pill — `var(--color-matcha)` border + `var(--color-mtsoft)` bg.
 *   - unselected pill — neutral line border + white bg.
 */
import clsx from "clsx";
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
}: ModelPickerProps) {
  const intel = isIntelMac();

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
                "inline-flex items-center gap-1.5",
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
            </button>
            {isDownloading ? (
              <span
                aria-hidden
                className="text-[10px] font-mono text-[var(--color-matcha)] font-semibold"
                title="click pill to reopen progress dialog"
              >
                {pct != null ? `${pct}%` : "…"}
              </span>
            ) : !m.downloaded ? (
              <span
                aria-hidden
                className="text-[10px] font-mono text-mut"
                title={`${m.size_mb} MB download from huggingface.co`}
              >
                {m.size_mb < 1000
                  ? `${m.size_mb} MB`
                  : `${(m.size_mb / 1024).toFixed(1)} GB`}
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
          </span>
        );
      })}
    </div>
  );
}
