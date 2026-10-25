import { type ReactNode } from "react";

type Tone = "neutral" | "blue" | "matcha" | "straw" | "honey";

/** Live-region mode: 'none' decorative (default), 'status' polite (timers, banners), 'alert' assertive (permission-denied errors only). */
export type PillStatus = "none" | "status" | "alert";

interface PillProps {
  tone?: Tone;
  children: ReactNode;
  className?: string;
  status?: PillStatus;
}

const BASE = [
  "inline-flex",
  "items-center",
  "gap-1.5",
  "rounded-pill",
  "px-2.5",
  "py-1",
  "text-[12px]",
  "font-medium",
  "leading-none",
  "border",
  "whitespace-nowrap",
].join(" ");

// text-straw on bg-strsoft is ~2.9:1 (fails WCAG AA), so strawberry-soft pills use text-ink for body copy and keep text-straw for the border and icon dot.
// text-mut on bg-card is ~4.0:1 (just under AA). Fine for caption-only pills (>=11px metadata, keyboard hints, mono token labels), never for body copy.
const TONE: Record<Tone, string> = {
  neutral: "bg-card text-mut border-line",
  blue:    "bg-blsoft  text-blue border-transparent",
  matcha:  "bg-mtsoft  text-matcha border-transparent",
  straw:   "bg-strsoft text-ink border-transparent",
  honey:   "bg-honeysoft text-honeyink border-transparent",
};

export function Pill({
  tone = "neutral",
  children,
  className = "",
  status = "none",
}: PillProps) {
  const liveProps =
    status === "status"
      ? { role: "status" as const, "aria-live": "polite" as const }
      : status === "alert"
        ? { role: "alert" as const, "aria-live": "assertive" as const }
        : {};
  return (
    <span className={`${BASE} ${TONE[tone]} ${className}`.trim()} {...liveProps}>
      {children}
    </span>
  );
}

interface RecordingBadgeProps {
  elapsed: string;
}

export function RecordingBadge({ elapsed }: RecordingBadgeProps) {
  return (
    <span
      role="status"
      aria-live="polite"
      aria-label={`Recording, ${elapsed}`}
      className={[
        "inline-flex items-center gap-2 rounded-pill px-3 py-1.5",
        "bg-strsoft text-ink border border-transparent",
        "text-[12px] leading-none",
      ].join(" ")}
    >
      <span
        data-testid="recording-dot"
        className="inline-block w-2 h-2 rounded-pill bg-straw animate-recpulse"
        aria-hidden="true"
      />
      <span className="font-mono text-[12px] tracking-tight">{elapsed}</span>
    </span>
  );
}

interface ProviderChipProps {
  name: string;
  active?: boolean;
  local?: boolean;
}

export function ProviderChip({ name, active, local }: ProviderChipProps) {
  let tone: Tone = "neutral";
  if (active) tone = "blue";
  else if (local) tone = "matcha";

  return (
    <Pill tone={tone} className="px-3 py-1.5 text-[12px]">
      <span
        aria-hidden="true"
        className={[
          "inline-block w-1.5 h-1.5 rounded-pill",
          active ? "bg-blue" : local ? "bg-matcha" : "bg-mut",
        ].join(" ")}
      />
      {name}
    </Pill>
  );
}
