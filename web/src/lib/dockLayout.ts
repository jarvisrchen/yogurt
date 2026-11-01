export const DOCK_DEFAULT = 330;
export const DOCK_MIN = 320;
export const DOCK_MAX = 600;
/** Window kept beside the dock so its resize handle and edge tabs stay reachable. */
const KEEP_VISIBLE = 80;
/** The edge tabs' width, which the page wrapper reserves while the dock is closed. */
const GUTTER = 28;
const NOTES_MAX = 660;
/** Below this the notes column stops narrowing and the dock overlaps it instead. */
const NOTES_MIN = 480;

export const TEXT_SIZES = [
  { label: "Small", px: 13 },
  { label: "Default", px: 14 },
  { label: "Large", px: 16 },
  { label: "Extra large", px: 18 },
] as const;
export const TEXT_DEFAULT = 14;

const WIDTH_KEY = "yogurt-dock-width";
const TEXT_KEY = "yogurt-dock-text";

/** The saved width as shown: a small window shrinks the dock without changing what is saved. */
export function clampDockWidth(saved: number, windowWidth: number): number {
  return Math.round(Math.max(DOCK_MIN, Math.min(saved, DOCK_MAX, windowWidth - KEEP_VISIBLE)));
}

/**
 * The page wrapper's right padding while the dock is open. The centered notes column shifts left just far
 * enough to clear the dock and its tabs, then narrows, and holds at NOTES_MIN with the dock overlapping it.
 */
export function dockLayout(width: number, windowWidth: number): { pad: number; clear: number; overlaps: boolean } {
  const clear = width + GUTTER;
  // A centered column of width N in [0, W - P] ends at (W - P + N) / 2, which must not pass W - clear.
  const pad = Math.min(
    clear,
    Math.max(GUTTER, 2 * clear + NOTES_MAX - windowWidth),
    Math.max(GUTTER, windowWidth - NOTES_MIN),
  );
  const notes = Math.min(NOTES_MAX, windowWidth - pad);
  return { pad, clear, overlaps: (windowWidth - pad + notes) / 2 > windowWidth - clear };
}

function readNumber(key: string): number | null {
  try {
    const v = Number(localStorage.getItem(key));
    return Number.isFinite(v) && v > 0 ? v : null;
  } catch {
    return null;
  }
}

function writeNumber(key: string, v: number) {
  try {
    localStorage.setItem(key, String(v));
  } catch {
    // Private mode or blocked storage: the choice lasts until reload.
  }
}

export const loadDockWidth = () => readNumber(WIDTH_KEY) ?? DOCK_DEFAULT;
export const saveDockWidth = (px: number) => writeNumber(WIDTH_KEY, px);

export function loadDockText(): number {
  const v = readNumber(TEXT_KEY);
  return TEXT_SIZES.some((s) => s.px === v) ? v! : TEXT_DEFAULT;
}
export const saveDockText = (px: number) => writeNumber(TEXT_KEY, px);
