/**
 * AudioWaveIcon — real amplitude-driven wave, replacing the heartbeat-only
 * `AudioWaveBars` glyph next to "Live transcript" (Phase 8+ follow-up:
 * `useAudioLevels` now streams per-chunk peak amplitude over the meeting
 * WS, so the icon can react to what's actually on the wire).
 *
 * Three bars, 2px wide with a 1px gap, height driven by
 * `max(mic, system)` (0..1) with staggered multipliers so it still reads
 * as an organic wave rather than three identical blocks. An idle floor of
 * 20% keeps the bars visible even at silence instead of collapsing to
 * nothing. `height` transitions over 120ms so level updates (throttled to
 * ~100ms server-side) look smooth rather than jumpy.
 */
interface AudioWaveIconProps {
  mic: number;
  system: number;
  className?: string;
}

const HEIGHT_PX = 12;
const WIDTH_PX = 2;
const GAP_PX = 1;
const IDLE_FLOOR = 0.2;
// Stagger so all three bars don't move in lockstep — mirrors AudioWaveBars'
// asymmetric baseH ratios.
const BAR_MULTIPLIERS = [0.7, 1, 0.85];

export function AudioWaveIcon({ mic, system, className }: AudioWaveIconProps) {
  // Perceptual scaling: raw peak amplitudes for normal speech sit around
// 0.02-0.3 (far-field mic audio much lower), so a linear map leaves the
// bars visually dead at the idle floor. pow(level, 0.35) approximates
// loudness perception: 0.01 -> 0.20, 0.05 -> 0.35, 0.15 -> 0.51, 0.5 -> 0.78.
const level = Math.pow(Math.max(mic, system, 0), 0.35);
  // ponytail: color reflects the dominant channel (straw = mic/"you",
  // matching the recording-indicator accent; blue = system/"them",
  // matching the existing equalizer icon) rather than a fixed color —
  // one line, no extra prop needed.
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
