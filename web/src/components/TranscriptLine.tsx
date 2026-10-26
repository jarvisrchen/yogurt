import type { TranscriptEvent } from "../lib/ws";

/** Formats ms since meeting start as `HH:MM:SS`. */
export function formatTs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

const INK = "var(--color-ink)";
const GREY = "var(--color-grey)";

/** Scrolls the line closest to `sec` (seconds) into view and tints it briefly. False when `root` holds no lines. */
export function scrollToTranscriptTime(root: ParentNode, sec: number): boolean {
  let best: HTMLElement | null = null;
  let bestDist = Number.POSITIVE_INFINITY;
  for (const line of root.querySelectorAll<HTMLElement>("[data-transcript-ts-sec]")) {
    const dist = Math.abs(Number(line.getAttribute("data-transcript-ts-sec")) - sec);
    if (dist < bestDist) {
      bestDist = dist;
      best = line;
    }
  }
  if (!best) return false;
  best.scrollIntoView({ behavior: "smooth", block: "center" });
  // Brief tint (CSS keyframe) so the eye lands on the line mid-smooth-scroll.
  best.classList.add("transcript-highlight");
  const el = best;
  setTimeout(() => el.classList.remove("transcript-highlight"), 1500);
  return true;
}

/** Partial (non-final) events render at opacity 0.7 to signal "still listening". */
export function TranscriptLine({ ev }: { ev: TranscriptEvent }) {
  const isMic = ev.channel === "mic";
  const labelColor = isMic ? INK : GREY;
  const textColor = isMic ? INK : GREY;
  const opacity = ev.is_final ? 1 : 0.7;

  return (
    <div
      className="py-2 text-[14px] leading-snug"
      data-channel={ev.channel}
      data-final={ev.is_final}
      // scrollToTranscriptTime finds the closest line via `[data-transcript-ts-sec]`.
      data-transcript-ts-sec={Math.floor(ev.ts_ms / 1000)}
    >
      <span
        className="inline-block w-12 font-semibold"
        style={{ color: labelColor }}
      >
        {isMic ? "Me" : "Them"}
      </span>
      <span
        className="text-[12px] mr-2"
        style={{
          fontFamily: "JetBrains Mono, ui-monospace, monospace",
          color: GREY,
        }}
      >
        {formatTs(ev.ts_ms)}
      </span>
      <span style={{ color: textColor, opacity }}>{ev.text}</span>
    </div>
  );
}
