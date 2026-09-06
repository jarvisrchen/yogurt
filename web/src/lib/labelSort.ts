/**
 * UI-12: sidebar Labels sort mode.
 *
 * Persisted in localStorage rather than server settings - same call as
 * `theme.ts`: a purely client-side display preference with no reason to
 * round-trip through SQLite. "Custom" mode itself is backed by the
 * server-side `position` column (set via `useReorderLabels`); this file
 * only remembers which *mode* the sidebar is in.
 */
import type { LabelWithCount } from "./api/labels";

export type LabelSortMode = "alpha" | "updated" | "custom";

export const LABEL_SORT_KEY = "yogurt-label-sort";

export function getLabelSortMode(): LabelSortMode {
  try {
    const v = localStorage.getItem(LABEL_SORT_KEY);
    return v === "updated" || v === "custom" ? v : "alpha";
  } catch {
    return "alpha";
  }
}

export function setLabelSortMode(mode: LabelSortMode): void {
  try {
    localStorage.setItem(LABEL_SORT_KEY, mode);
  } catch {
    // Private mode / storage disabled: still applies for this page load.
  }
}

type Sortable = Pick<LabelWithCount, "name" | "position" | "updated_at">;

/** Pure sort - same list, ordered per `mode`. Never mutates `labels`. */
export function sortLabels<T extends Sortable>(labels: T[], mode: LabelSortMode): T[] {
  const sorted = [...labels];
  switch (mode) {
    case "updated":
      sorted.sort((a, b) => b.updated_at - a.updated_at);
      return sorted;
    case "custom":
      sorted.sort((a, b) => a.position - b.position || compareNames(a.name, b.name));
      return sorted;
    default:
      sorted.sort((a, b) => compareNames(a.name, b.name));
      return sorted;
  }
}

function compareNames(a: string, b: string): number {
  return a.localeCompare(b, undefined, { sensitivity: "base" });
}
