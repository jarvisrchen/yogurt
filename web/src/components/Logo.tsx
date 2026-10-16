interface LogoProps {
  size?: number;
  /** Omit when adjacent visible text already says "yogurt", so the SVG is aria-hidden. */
  ariaLabel?: string;
  className?: string;
}

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
      focusable="false"
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
