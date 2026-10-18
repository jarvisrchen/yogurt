export interface SelectionState {
  selected: ReadonlySet<string>;
  anchor: string | null;
}

export const EMPTY_SELECTION: SelectionState = { selected: new Set(), anchor: null };

// Shift-click unions the anchor..id range into the selection, keeping earlier picks.
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

export function selectAll(orderedIds: readonly string[]): SelectionState {
  return { selected: new Set(orderedIds), anchor: null };
}

export function clearSelection(): SelectionState {
  return EMPTY_SELECTION;
}
