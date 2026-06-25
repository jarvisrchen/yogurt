import { type ReactNode } from "react";

type Tone = "neutral" | "blue" | "matcha" | "straw";

/**
 * Live-region behavior for the Pill.
 *
 *   'none'   — decorative / static pill (icon-tag, inline label). Default.
 *   'status' — polite live region: announce content changes without
 *              interrupting. Use for RecordingBadge's ticking timer,
 *              the "Enhancing…" banner, "Local-only · on" toggle.
 *   'alert'  — assertive live region: interrupt. Use only for
 *              high-stakes errors such as permission-denied
 *              (Phase 7 STATE-02).
 */
export type PillStatus = "none" | "status" | "alert";

interface PillProps {
  tone?: Tone;
  children: ReactNode;
  className?: string;
  /**
   * ARIA live-region behavior. See PillStatus above. Defaults to 'none'
   * (no role) so decorative pills don't pollute the AT tree.
   */
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

// Tone token-usage convention (MD-05 / MD-06):
//   - text-straw on bg-strsoft is ~2.9:1 (fails WCAG AA). Strawberry
//     soft pills use text-ink for body copy; text-straw is reserved for
//     the border and the icon dot on those surfaces.
//   - text-mut on bg-paper is ~4.0:1 (just under AA). Acceptable for
//     small *captions* (>=11px metadata) per PRD §16.6, but never for
//     body copy. The neutral tone is a caption-only pill (keyboard
//     hints, mono token labels) — keep text-mut here intentionally.
//     See 01-UI-SPEC.md "Token usage rules".
const TONE: Record<Tone, string> = {
  neutral: "bg-paper text-mut border-line",
  blue:    "bg-blsoft  text-blue   border-blue/20",
  matcha:  "bg-mtsoft  text-matcha border-matcha/25",
  straw:   "bg-strsoft text-ink    border-straw/40",
};

/**
 * Base Pill — a 999-radius badge used for chips, status indicators, and
 * keyboard hints throughout the app. See PRD §16.6.
 *
 * Pass `status="status"` (polite) or `status="alert"` (assertive) when
 * the pill represents a dynamic state that screenreaders should
 * announce — e.g. recording ticking, "Enhancing…" banner appearing,
 * permission-denied error surfacing. Decorative/static pills (icon
 * tags, keyboard hints) omit `status`.
 */
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
      role="status"
      aria-live="polite"
      aria-label={`Recording, ${elapsed}`}
      className={[
        "inline-flex items-center gap-2 rounded-pill px-3 py-1.5",
        "bg-card text-ink border border-straw",
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
