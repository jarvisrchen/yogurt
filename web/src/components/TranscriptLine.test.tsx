import { afterEach, describe, expect, it, vi } from "vitest";
import { scrollToTranscriptTime } from "./TranscriptLine";

function lines(...secs: (number | string)[]) {
  const root = document.createElement("div");
  root.innerHTML = secs.map((s) => `<div data-transcript-ts-sec="${s}">${s}</div>`).join("");
  return root;
}

describe("scrollToTranscriptTime", () => {
  afterEach(() => vi.useRealTimers());

  it("centers and tints the closest line, then clears the tint", () => {
    vi.useFakeTimers();
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    const root = lines(10, 60, "oops", 125);
    expect(scrollToTranscriptTime(root, 100)).toBe(true);
    const hit = root.querySelector(".transcript-highlight");
    expect(hit?.textContent).toBe("125");
    expect(scroll).toHaveBeenCalledWith({ behavior: "smooth", block: "center" });
    vi.advanceTimersByTime(1500);
    expect(root.querySelector(".transcript-highlight")).toBeNull();
  });

  it("returns false when there are no lines", () => {
    expect(scrollToTranscriptTime(document.createElement("div"), 5)).toBe(false);
  });
});
