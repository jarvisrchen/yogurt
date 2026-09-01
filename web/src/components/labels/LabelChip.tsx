/**
 * `LabelChip` — small colored pill rendering one label's name.
 *
 * Colors come from `LABEL_COLORS`: the three original design-system tones
 * (blue/matcha/straw) reuse the existing `--color-*soft` tokens; the three
 * new palette entries (lilac/honey/slate) don't have design tokens yet, so
 * they're inline hex per the plan spec.
 */

import type { LabelColor } from "../../lib/api/labels";

export const LABEL_COLORS: Record<LabelColor, { bg: string; fg: string }> = {
  blue: { bg: "var(--color-blsoft)", fg: "var(--color-blue)" },
  matcha: { bg: "var(--color-mtsoft)", fg: "var(--color-matcha)" },
  straw: { bg: "var(--color-strsoft)", fg: "var(--color-straw)" },
  lilac: { bg: "var(--color-lilacsoft)", fg: "var(--color-lilac)" },
  honey: { bg: "var(--color-honeysoft)", fg: "var(--color-honey)" },
  slate: { bg: "var(--color-slatesoft)", fg: "var(--color-slate)" },
};

/** Fallback for an unrecognized color key (e.g. a future palette addition
 *  this build doesn't know about yet). */
const FALLBACK = { bg: "var(--color-line)", fg: "var(--color-mut)" };

interface Props {
  label: { name: string; color: LabelColor };
  size?: "sm" | "md";
  onRemove?: () => void;
}

export function LabelChip({ label, size = "md", onRemove }: Props) {
  const tone = LABEL_COLORS[label.color] ?? FALLBACK;
  const textSize = size === "sm" ? "text-[10px]" : "text-[11px]";
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-pill px-2 py-0.5 ${textSize} font-mono leading-none`}
      style={{ background: tone.bg, color: tone.fg }}
    >
      {label.name}
      {onRemove && (
        <button
          type="button"
          aria-label={`Remove ${label.name} label`}
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onRemove();
          }}
          className="ml-0.5 leading-none hover:opacity-70"
        >
          ×
        </button>
      )}
    </span>
  );
}
