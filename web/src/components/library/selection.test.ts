import { describe, expect, it } from "vitest";
import { EMPTY_SELECTION, clickSelect, clearSelection, selectAll } from "./selection";

const IDS = ["a", "b", "c", "d", "e"];

describe("clickSelect", () => {
  it("plain click selects an unselected id and sets the anchor", () => {
    const s = clickSelect(EMPTY_SELECTION, "b", IDS, false);
    expect([...s.selected]).toEqual(["b"]);
    expect(s.anchor).toBe("b");
  });

  it("plain click on an already-selected id deselects it", () => {
    const s1 = clickSelect(EMPTY_SELECTION, "b", IDS, false);
    const s2 = clickSelect(s1, "b", IDS, false);
    expect(s2.selected.size).toBe(0);
  });

  it("shift-click with no prior anchor behaves like a plain click", () => {
    const s = clickSelect(EMPTY_SELECTION, "c", IDS, true);
    expect([...s.selected]).toEqual(["c"]);
    expect(s.anchor).toBe("c");
  });

  it("shift-click selects the inclusive range from the anchor forward", () => {
    const s1 = clickSelect(EMPTY_SELECTION, "b", IDS, false);
    const s2 = clickSelect(s1, "d", IDS, true);
    expect([...s2.selected].sort()).toEqual(["b", "c", "d"]);
    expect(s2.anchor).toBe("d");
  });

  it("shift-click selects the inclusive range from the anchor backward", () => {
    const s1 = clickSelect(EMPTY_SELECTION, "d", IDS, false);
    const s2 = clickSelect(s1, "b", IDS, true);
    expect([...s2.selected].sort()).toEqual(["b", "c", "d"]);
  });

  it("shift-click unions the range into the existing selection", () => {
    const s1 = clickSelect(EMPTY_SELECTION, "a", IDS, false);
    const withB = clickSelect(s1, "d", IDS, false); // plain click moves anchor to d, drops nothing
    const ranged = clickSelect(withB, "e", IDS, true);
    // "a" from the first plain click stays selected even though it's
    // outside the d..e range.
    expect([...ranged.selected].sort()).toEqual(["a", "d", "e"]);
  });

  it("ignores a shift-click whose anchor is no longer in orderedIds", () => {
    const s1 = clickSelect(EMPTY_SELECTION, "b", IDS, false);
    const s2 = clickSelect(s1, "d", ["c", "d", "e"], true);
    // "b" isn't in the current order, so this falls back to a plain toggle.
    expect([...s2.selected].sort()).toEqual(["b", "d"]);
  });
});

describe("selectAll / clearSelection", () => {
  it("selects every id and drops the anchor", () => {
    const s = selectAll(IDS);
    expect([...s.selected].sort()).toEqual([...IDS].sort());
    expect(s.anchor).toBeNull();
  });

  it("clears back to the empty selection", () => {
    expect(clearSelection()).toEqual(EMPTY_SELECTION);
  });
});
