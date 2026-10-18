interface AudioWaveIconProps {
  mic: number;
  system: number;
  className?: string;
}

const HEIGHT_PX = 12;
const WIDTH_PX = 2;
const GAP_PX = 1;
const IDLE_FLOOR = 0.2;
const BAR_MULTIPLIERS = [0.7, 1, 0.85];

export function AudioWaveIcon({ mic, system, className }: AudioWaveIconProps) {
  // Raw speech peaks sit around 0.02-0.3 (far-field mic much lower), so a linear map leaves the bars dead.
  // pow(level, 0.35) approximates loudness: 0.01 -> 0.20, 0.05 -> 0.35, 0.15 -> 0.51, 0.5 -> 0.78.
const level = Math.pow(Math.max(mic, system, 0), 0.35);
  // Straw for the mic ("you"), blue for system ("them"), matching the recording accent and equalizer icon.
  const color = mic >= system ? "var(--color-straw)" : "var(--color-blue)";

  return (
    <span
      aria-hidden
      role="presentation"
      className={className}
      style={{
        display: "inline-flex",
        alignItems: "flex-end",
        gap: `${GAP_PX}px`,
        height: `${HEIGHT_PX}px`,
      }}
    >
      {BAR_MULTIPLIERS.map((m, i) => {
        const h = HEIGHT_PX * (IDLE_FLOOR + (1 - IDLE_FLOOR) * Math.min(1, level * m));
        return (
          <span
            key={i}
            style={{
              display: "inline-block",
              width: `${WIDTH_PX}px`,
              height: `${h}px`,
              background: color,
              borderRadius: "1px",
              transformOrigin: "bottom center",
              transition: "height 120ms ease-out, background 120ms ease-out",
            }}
          />
        );
      })}
    </span>
  );
}
