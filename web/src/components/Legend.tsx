// Legend — top-right swatch contract per PRD §5.3 + NOTES-02.
//
// Two rows:
//
//   ■ your notes      (ink #211D18 — what the user typed)
//   ■ AI              (ai #7A7264 - what the LLM added)
//
// Tokens, not hex: the swatches must track the editor's `.ai-grey` /
// ink colors in both light and dark (UI-6).

const INK = "var(--color-ink)"; // user
const AI = "var(--color-ai)"; // AI
const MUT = "var(--color-mut)"; // muted caption text

export interface LegendProps {
  /**
   * When the Legend lives inside a `position: relative` host, default
   * absolute placement (top-right) is correct. Override with `inline` to
   * render in normal flow (useful for tests / inspectors).
   */
  inline?: boolean;
}

export function Legend({ inline = false }: LegendProps) {
  const wrapperStyle: React.CSSProperties = inline
    ? {
        display: "inline-flex",
        gap: 14,
        fontSize: 11,
        color: MUT,
      }
    : {
        position: "absolute",
        top: 18,
        right: 24,
        display: "flex",
        gap: 14,
        fontSize: 11,
        color: MUT,
        // Stay clickable-through so the legend never blocks the editor's
        // top-right region; the swatches are decorative.
        pointerEvents: "none",
        // Above the editor's max-width column but below the EnhancingBanner.
        zIndex: 10,
      };

  return (
    <div
      data-testid="legend"
      aria-label="Swatch legend: black is your notes, grey is AI"
      style={wrapperStyle}
    >
      <span style={{ display: "inline-flex", alignItems: "center" }}>
        <i
          aria-hidden="true"
          style={{
            display: "inline-block",
            width: 9,
            height: 9,
            background: INK,
            marginRight: 6,
            verticalAlign: "middle",
          }}
        />
        your notes
      </span>
      <span style={{ display: "inline-flex", alignItems: "center" }}>
        <i
          aria-hidden="true"
          style={{
            display: "inline-block",
            width: 9,
            height: 9,
            background: AI,
            marginRight: 6,
            verticalAlign: "middle",
          }}
        />
        AI
      </span>
    </div>
  );
}
