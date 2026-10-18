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
      // TranscriptDock finds the closest line to a scrollTo request via `[data-transcript-ts-sec]`.
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
