import { describe, it, expect, beforeEach } from "vitest";
import { clampDockWidth, dockLayout, loadDockText, loadDockWidth, saveDockText, saveDockWidth } from "./dockLayout";

describe("clampDockWidth", () => {
  it("keeps a saved width that fits", () => {
    expect(clampDockWidth(330, 1440)).toBe(330);
    expect(clampDockWidth(600, 680)).toBe(600);
  });

  it("caps at 600 px and at the window minus 80 px", () => {
    expect(clampDockWidth(900, 1680)).toBe(600);
    expect(clampDockWidth(600, 600)).toBe(520);
  });

  it("never goes under 320 px, even on a tiny window", () => {
    expect(clampDockWidth(100, 1440)).toBe(320);
    expect(clampDockWidth(600, 350)).toBe(320);
  });
});

describe("dockLayout", () => {
  it("leaves the notes alone when the default dock fits beside them", () => {
    expect(dockLayout(330, 1440)).toEqual({ pad: 28, clear: 358, overlaps: false });
  });

  it("shifts the notes left just enough to clear the dock", () => {
    expect(dockLayout(330, 1280)).toMatchObject({ pad: 96, overlaps: false });
    expect(dockLayout(600, 1440)).toMatchObject({ pad: 476, overlaps: false });
  });

  it("narrows the notes once they cannot shift further", () => {
    // 1180 - 628 leaves a 552 px column.
    expect(dockLayout(600, 1180)).toMatchObject({ pad: 628, overlaps: false });
  });

  it("holds the notes at 480 px and lets the dock overlap past that", () => {
    expect(dockLayout(330, 800)).toMatchObject({ pad: 320, overlaps: true });
    expect(dockLayout(600, 900)).toMatchObject({ pad: 420, overlaps: true });
  });
});

describe("dock preferences", () => {
  beforeEach(() => localStorage.clear());

  it("falls back to the defaults when nothing or garbage is stored", () => {
    expect(loadDockWidth()).toBe(330);
    expect(loadDockText()).toBe(14);
    localStorage.setItem("yogurt-dock-width", "wide");
    localStorage.setItem("yogurt-dock-text", "15");
    expect(loadDockWidth()).toBe(330);
    expect(loadDockText()).toBe(14);
  });

  it("round-trips a saved width and text size", () => {
    saveDockWidth(480);
    saveDockText(18);
    expect(loadDockWidth()).toBe(480);
    expect(loadDockText()).toBe(18);
  });
});
