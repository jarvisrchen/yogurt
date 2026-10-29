/** Custom `#rrggbb` colors have no design token, so the soft background is the hex plus low alpha. The lilac/honey/slate palette entries are inline hex for the same reason. */

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

/** What a color input shows before a custom color is picked. */
export const DEFAULT_CUSTOM_COLOR = "#8a8fa3";

const HEX_COLOR_RE = /^#[0-9a-f]{6}$/i;

export function isHexColor(color: string): boolean {
  return HEX_COLOR_RE.test(color);
}

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
