import { afterEach, describe, expect, it } from "vitest";
import {
  LABEL_SORT_KEY,
  getLabelSortMode,
  setLabelSortMode,
  sortLabels,
} from "./labelSort";

const labels = [
  { name: "Zebra", position: 2, updated_at: 100 },
  { name: "apple", position: 0, updated_at: 300 },
  { name: "Mango", position: 1, updated_at: 200 },
];

describe("sortLabels", () => {
  it("sorts alpha case-insensitively", () => {
    expect(sortLabels(labels, "alpha").map((l) => l.name)).toEqual([
      "apple",
      "Mango",
      "Zebra",
    ]);
  });

  it("sorts by updated_at descending", () => {
    expect(sortLabels(labels, "updated").map((l) => l.name)).toEqual([
      "apple",
      "Mango",
      "Zebra",
    ]);
  });

  it("sorts by position ascending, tie-broken by name", () => {
    expect(sortLabels(labels, "custom").map((l) => l.name)).toEqual([
      "apple",
      "Mango",
      "Zebra",
    ]);
    const tied = [
      { name: "Bravo", position: 0, updated_at: 0 },
      { name: "Alpha", position: 0, updated_at: 0 },
    ];
    expect(sortLabels(tied, "custom").map((l) => l.name)).toEqual([
      "Alpha",
      "Bravo",
    ]);
  });

  it("does not mutate the input array", () => {
    const copy = [...labels];
    sortLabels(labels, "alpha");
    expect(labels).toEqual(copy);
  });
});

describe("label sort mode persistence", () => {
  afterEach(() => localStorage.clear());

  it("defaults to alpha", () => {
    expect(getLabelSortMode()).toBe("alpha");
  });

  it("persists an explicit choice", () => {
    setLabelSortMode("custom");
    expect(localStorage.getItem(LABEL_SORT_KEY)).toBe("custom");
    expect(getLabelSortMode()).toBe("custom");
  });

  it("ignores garbage in storage", () => {
    localStorage.setItem(LABEL_SORT_KEY, "neon");
    expect(getLabelSortMode()).toBe("alpha");
  });
});
