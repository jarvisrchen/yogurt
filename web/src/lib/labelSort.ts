// Sidebar Labels sort mode. Persisted in localStorage like `theme.ts` (client-side display preference).
// "Custom" order itself lives in the server-side `position` column; this file only remembers the mode.
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
