import { type ReactNode } from "react";

type Tone = "neutral" | "blue" | "matcha" | "straw";

interface PillProps {
  tone?: Tone;
  children: ReactNode;
  className?: string;
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

const TONE: Record<Tone, string> = {
  neutral: "bg-paper text-mut border-line",
  blue:    "bg-blsoft  text-blue   border-blue/20",
  matcha:  "bg-mtsoft  text-matcha border-matcha/25",
  straw:   "bg-strsoft text-straw  border-straw/25",
};

/**
 * Base Pill — a 999-radius badge used for chips, status indicators, and
 * keyboard hints throughout the app. See PRD §16.6.
 */
export function Pill({ tone = "neutral", children, className = "" }: PillProps) {
  return <span className={`${BASE} ${TONE[tone]} ${className}`.trim()}>{children}</span>;
}

/**
 * RecordingBadge — pulsing strawberry dot + JetBrains-Mono timer. Always
 * uses a white surface with a strawberry hairline border to match PRD §16.6.
 */
interface RecordingBadgeProps {
  /** Pre-formatted elapsed time, e.g. "12:04". */
  elapsed: string;
}

export function RecordingBadge({ elapsed }: RecordingBadgeProps) {
  return (
    <span
      className={[
        "inline-flex items-center gap-2 rounded-pill px-3 py-1.5",
        "bg-card text-ink border border-straw",
        "text-[12px] leading-none",
      ].join(" ")}
    >
      <span
        data-testid="recording-dot"
        className="inline-block w-2 h-2 rounded-pill bg-straw animate-recpulse"
        aria-hidden
      />
      <span className="font-mono text-[12px] tracking-tight">{elapsed}</span>
    </span>
  );
}

/**
 * ProviderChip — used in settings (§5.6) and onboarding (§5.10) to surface
 * LLM/STT providers. `active` paints blueberry, `local` paints matcha,
 * default is dashed-border neutral for "preset to clone".
 */
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
        aria-hidden
        className={[
          "inline-block w-1.5 h-1.5 rounded-pill",
          active ? "bg-blue" : local ? "bg-matcha" : "bg-mut",
        ].join(" ")}
      />
      {name}
    </Pill>
  );
}
