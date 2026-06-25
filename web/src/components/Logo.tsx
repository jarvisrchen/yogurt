interface LogoProps {
  /** Render size in CSS pixels. Default 44 (matches the brand mark). */
  size?: number;
  /** Accessible name for the SVG. Omit for decorative use. */
  ariaLabel?: string;
  /** Optional className for spacing/positioning. */
  className?: string;
}

/**
 * Yogurt logo — the "spoon & swirl" mark (PRD §16.1).
 * Blueberry circle, white spoon-curve, strawberry dot at the spoon tip.
 * The viewBox is locked to 44×44; pass `size` to scale.
 */
export function Logo({ size = 44, ariaLabel, className }: LogoProps) {
  const a11yProps = ariaLabel
    ? { role: "img" as const, "aria-label": ariaLabel }
    : { "aria-hidden": true as const };

  return (
    <svg
      viewBox="0 0 44 44"
      width={size}
      height={size}
      className={className}
      style={{ flex: "none" }}
      {...a11yProps}
    >
      <circle cx="22" cy="22" r="22" fill="#5B4FC7" />
      <path
        d="M13 31 C 13 22, 31 25, 31 17 C 31 11, 19 12.5, 19 19"
        fill="none"
        stroke="#fff"
        strokeWidth="3.4"
        strokeLinecap="round"
      />
      <circle cx="19" cy="19" r="1.7" fill="#E07A66" />
    </svg>
  );
}
