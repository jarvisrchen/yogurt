/**
 * `LabelChip` — small colored pill rendering one label's name.
 *
 * Colors come from `LABEL_COLORS`: the three original design-system tones
 * (blue/matcha/straw) reuse the existing `--color-*soft` tokens; the three
 * new palette entries (lilac/honey/slate) don't have design tokens yet, so
 * they're inline hex per the plan spec. A custom `#rrggbb` label color (see
 * `isHexColor`) has no design token at all, so its soft background is
 * synthesized as the hex plus low alpha.
 */

import type { LabelColor, PaletteColor } from "../../lib/api/labels";

export const LABEL_COLORS: Record<PaletteColor, { bg: string; fg: string }> = {
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

const HEX_COLOR_RE = /^#[0-9a-f]{6}$/i;

/** True for a custom `#rrggbb` label color, false for a palette key. */
export function isHexColor(color: string): boolean {
  return HEX_COLOR_RE.test(color);
}

/** Resolves any `LabelColor` (palette key or custom hex) to a fg/bg pair. */
export function labelTone(color: LabelColor): { bg: string; fg: string } {
  if (isHexColor(color)) return { bg: `${color}22`, fg: color };
  return LABEL_COLORS[color as PaletteColor] ?? FALLBACK;
}

interface Props {
  label: { name: string; color: LabelColor };
  size?: "sm" | "md";
  onRemove?: () => void;
}

export function LabelChip({ label, size = "md", onRemove }: Props) {
  const tone = labelTone(label.color);
  const textSize = size === "sm" ? "text-[11px]" : "text-[12px]";
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-pill px-2.5 py-1 ${textSize} font-medium leading-none`}
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
