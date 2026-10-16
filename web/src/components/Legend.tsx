// Swatches use tokens, not hex, so they track the editor's ink and `.ai-grey` colors in light and dark.

const INK = "var(--color-ink)";
const AI = "var(--color-ai)";
const MUT = "var(--color-mut)";

export interface LegendProps {
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
            borderRadius: 3,
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
            borderRadius: 3,
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
