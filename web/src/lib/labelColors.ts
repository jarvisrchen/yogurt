// The label palette, shared by the Mac app and the Cloud webapp, so it imports nothing.
// A custom `#rrggbb` color has no design token, so its soft background is the hex plus low alpha.

export type PaletteColor = "blue" | "matcha" | "straw" | "lilac" | "honey" | "slate";

/** A palette key or a custom `#rrggbb` hex string. `string & {}` keeps palette autocomplete. */
export type LabelColor = PaletteColor | (string & {});

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
