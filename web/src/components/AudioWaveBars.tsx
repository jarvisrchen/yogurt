/**
 * AudioWaveBars — three animated bars indicating live audio activity.
 *
 * Pure CSS via the `--animate-wave` keyframe from PRD §16.5 (1s ease-in-out
 * infinite). Three bars are staggered by 0.15s so the pattern looks
 * organic rather than synchronized. Matches the Granola / Claude
 * "Live transcript" pill design ref.
 *
 * The bars do NOT react to actual audio amplitude — Phase 8 doesn't
 * surface a frame-level RMS stream to the browser. They're a heartbeat,
 * not a meter: they tell the user "the recording pipeline is alive and
 * something is on the wire." A real amplitude-driven meter is v1.1
 * (would need a new WS frame type and ~10 ms cadence).
 *
 * Sizing: `size="sm"` (12 px tall, fits inline with text) and `size="md"`
 * (16 px, comfortable as a standalone indicator next to a label).
 *
 * Color defaults to `currentColor` so the caller controls it via parent
 * text color — drops in cleanly next to recording-state text without an
 * extra style override.
 */

interface AudioWaveBarsProps {
  size?: "sm" | "md";
  /** Hide the animation when the recording is paused/stopped — bars
   *  render at their rest height but don't bounce. Useful for the
   *  Library card indicator where you want layout stability without
   *  pretending audio is still flowing. */
  paused?: boolean;
  /** Override color; defaults to currentColor. */
  color?: string;
  className?: string;
}

const SIZE_PX: Record<"sm" | "md", number> = {
  sm: 12,
  md: 16,
};

export function AudioWaveBars({
  size = "sm",
  paused = false,
  color,
  className,
}: AudioWaveBarsProps) {
  const h = SIZE_PX[size];
  const w = Math.round(h * 0.18);
  const gap = Math.round(h * 0.12);
  const bg = color ?? "currentColor";

  // Stagger the bars by negative animation delays so each starts at a
  // different phase of the keyframe — no two bars peak in sync.
  const bars: Array<{ delay: string; baseH: number }> = [
    { delay: "-0.00s", baseH: Math.round(h * 0.5) },
    { delay: "-0.20s", baseH: Math.round(h * 0.85) },
    { delay: "-0.40s", baseH: Math.round(h * 0.65) },
  ];

  return (
    <span
      aria-hidden
      role="presentation"
      className={className}
      style={{
        display: "inline-flex",
        alignItems: "flex-end",
        gap: `${gap}px`,
        height: `${h}px`,
      }}
    >
      {bars.map((b, i) => (
        <span
          key={i}
          style={{
            display: "inline-block",
            width: `${w}px`,
            height: `${b.baseH}px`,
            background: bg,
            borderRadius: `${Math.max(1, Math.round(w * 0.5))}px`,
            transformOrigin: "bottom center",
            animation: paused
              ? "none"
              : `wave 1s ease-in-out ${b.delay} infinite`,
          }}
        />
      ))}
    </span>
  );
}
