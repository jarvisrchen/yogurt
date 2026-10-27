import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_PAGE_SIZE,
  pageCount,
  parsePage,
  rangeLabel,
  rememberPageSize,
  resolvePageSize,
} from "./pagination";

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("rangeLabel", () => {
  it("shows the 1-based range with thousands separators", () => {
    expect(rangeLabel(2, 25, 1412)).toBe("26–50 of 1,412");
    expect(rangeLabel(1, 25, 1412)).toBe("1–25 of 1,412");
  });

  it("clamps the last page to the total", () => {
    expect(rangeLabel(57, 25, 1412)).toBe("1,401–1,412 of 1,412");
  });

  it("handles an empty library", () => {
    expect(rangeLabel(1, 25, 0)).toBe("0 of 0");
  });
});

describe("pageCount", () => {
  it("rounds up and never drops below one", () => {
    expect(pageCount(25, 51)).toBe(3);
    expect(pageCount(25, 50)).toBe(2);
    expect(pageCount(25, 0)).toBe(1);
  });
});

describe("parsePage", () => {
  it("falls back to page 1 for anything but a positive integer", () => {
    expect(parsePage("3")).toBe(3);
    for (const bad of [null, "", "0", "-2", "1.5", "abc"]) expect(parsePage(bad)).toBe(1);
  });
});

describe("resolvePageSize", () => {
  it("prefers the URL, then the remembered size, then the default", () => {
    expect(resolvePageSize(null)).toBe(DEFAULT_PAGE_SIZE);
    rememberPageSize(50);
    expect(resolvePageSize(null)).toBe(50);
    expect(resolvePageSize("10")).toBe(10);
  });

  it("ignores sizes the server would not offer", () => {
    rememberPageSize(50);
    expect(resolvePageSize("7")).toBe(50);
    localStorage.setItem("yogurt.library.pageSize", "999");
    expect(resolvePageSize(null)).toBe(DEFAULT_PAGE_SIZE);
  });

  it("survives storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(resolvePageSize(null)).toBe(DEFAULT_PAGE_SIZE);
    expect(() => rememberPageSize(10)).not.toThrow();
  });
});
