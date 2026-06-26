import type { TranscriptEvent } from "../lib/ws";

/**
 * Format a transcript event timestamp (ms-since-meeting-start) as `HH:MM:SS`.
 *
 *   formatTs(3_661_000) === "01:01:01"
 *   formatTs(11_020)    === "00:00:11"
 *
 * Zero-padded via String.padStart so the column reads as a clean monospace
 * gutter even at low times.
 */
export function formatTs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

const INK = "#211D18";  // Phase 1 --color-ink — "Me" label + final body text.
const GREY = "#A89F90"; // Phase 1 --color-grey — "Them" + timestamp + partial.

/**
 * One transcript line: channel label · monospace timestamp · spoken text.
 *
 * Visual contract (PRD §5.2 + Phase 3 D-16 / D-18):
 *   - First column: "Me" (mic) / "Them" (system) channel label, colored ink
 *     for mic, grey for system. Bold, fixed-width gutter so the text columns
 *     line up.
 *   - Second column: HH:MM:SS timestamp, JetBrains Mono, grey.
 *   - Third column: the spoken text. Color matches channel. Non-final
 *     (partial) events render at opacity 0.7 to signal "still listening"
 *     (TRANS-07).
 *
 * Data attributes (`data-channel`, `data-final`) exist for the test layer
 * and future selectors (e.g. Phase 4 transcript deep-links).
 */
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
      // NOTES-11: enables `yogurt:transcript:scrollTo` lookup (TranscriptDock
      // queries `[data-transcript-ts-sec]` and picks the closest match by
      // absolute distance).
      data-transcript-ts-sec={Math.floor(ev.ts_ms / 1000)}
    >
      <span
        className="inline-block w-10 font-semibold"
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
