import { afterEach, describe, expect, it, vi } from "vitest";
import { applyTheme, getThemePref, resolveTheme, setThemePref } from "./theme";

function mockScheme(dark: boolean) {
  vi.stubGlobal("matchMedia", (q: string) => ({
    matches: dark && q.includes("dark"),
    addEventListener: () => {},
  }));
}

describe("theme", () => {
  afterEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
    delete document.documentElement.dataset.theme;
  });

  it("defaults to system and follows the OS scheme", () => {
    mockScheme(true);
    expect(getThemePref()).toBe("system");
    expect(resolveTheme("system")).toBe("dark");
    mockScheme(false);
    expect(resolveTheme("system")).toBe("light");
  });

  it("persists an explicit choice and stamps <html data-theme>", () => {
    mockScheme(false);
    setThemePref("dark");
    expect(localStorage.getItem("yogurt-theme")).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    // Back to system clears the key so a later OS flip is honored.
    setThemePref("system");
    expect(localStorage.getItem("yogurt-theme")).toBeNull();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("ignores garbage in storage", () => {
    mockScheme(false);
    localStorage.setItem("yogurt-theme", "neon");
    expect(getThemePref()).toBe("system");
    applyTheme();
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
