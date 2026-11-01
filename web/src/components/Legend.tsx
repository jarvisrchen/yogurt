// Swatches use tokens, not hex, so they track the editor's ink and `.ai-grey` colors in light and dark.

const INK = "var(--color-ink)";
const AI = "var(--color-ai)";
const MUT = "var(--color-mut)";

/** Sits at the right end of the post page's document tab row. */
export function Legend() {
  const wrapperStyle: React.CSSProperties = {
    display: "inline-flex",
    gap: 14,
    fontSize: 11,
    color: MUT,
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
