/**
 * Pure selection-state transitions for the Library's multi-select mode
 * (MTG-14). No React here so the range-selection math is unit-testable
 * without mounting anything - same split as `DateGroup`'s pure
 * `groupMeetingsByDay` next to the component that renders it.
 */

export interface SelectionState {
  selected: ReadonlySet<string>;
  /** Last plain-clicked id - the anchor a shift-click ranges from. */
  anchor: string | null;
}

export const EMPTY_SELECTION: SelectionState = { selected: new Set(), anchor: null };

/**
 * A checkbox click. Plain click toggles just `id` and moves the anchor
 * there. Shift-click (with a prior anchor still present in `orderedIds`)
 * unions in the inclusive range between the anchor and `id`, so a
 * shift-click after a few individual picks doesn't drop them.
 */
export function clickSelect(
  state: SelectionState,
  id: string,
  orderedIds: readonly string[],
  shiftKey: boolean,
): SelectionState {
  if (shiftKey && state.anchor != null) {
    const from = orderedIds.indexOf(state.anchor);
    const to = orderedIds.indexOf(id);
    if (from !== -1 && to !== -1) {
      const [lo, hi] = from <= to ? [from, to] : [to, from];
      const next = new Set(state.selected);
      for (let i = lo; i <= hi; i++) next.add(orderedIds[i]!);
      return { selected: next, anchor: id };
    }
  }
  const next = new Set(state.selected);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return { selected: next, anchor: id };
}

/** "Select all" in the action bar - every currently-visible id. */
export function selectAll(orderedIds: readonly string[]): SelectionState {
  return { selected: new Set(orderedIds), anchor: null };
}

/** "Clear" in the action bar, and the Escape shortcut. */
export function clearSelection(): SelectionState {
  return EMPTY_SELECTION;
}
