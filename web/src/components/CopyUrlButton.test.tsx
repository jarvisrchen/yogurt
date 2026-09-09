import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { CopyUrlButton } from "./CopyUrlButton";

describe("CopyUrlButton", () => {
  const writeText = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    writeText.mockClear();
    Object.assign(navigator, { clipboard: { writeText } });
  });

  it("copies the current URL and swaps to the checkmark, then reverts after 1.5s", async () => {
    vi.useFakeTimers();
    try {
      render(<CopyUrlButton />);

      const button = screen.getByRole("button", { name: /copy meeting url/i });
      await act(async () => {
        fireEvent.click(button);
      });

      expect(writeText).toHaveBeenCalledWith(window.location.href);
      expect(button.querySelector("svg.lucide-check")).toBeInTheDocument();

      act(() => {
        vi.advanceTimersByTime(1500);
      });

      expect(button.querySelector("svg.lucide-link-2")).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("clears the revert timer on unmount instead of firing against a discarded instance", async () => {
    vi.useFakeTimers();
    try {
      const { unmount } = render(<CopyUrlButton />);

      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: /copy meeting url/i }));
      });

      unmount();

      // No React "state update on an unmounted component" warning should
      // fire when the 1.5s timer would otherwise have run.
      expect(() => {
        act(() => {
          vi.advanceTimersByTime(1500);
        });
      }).not.toThrow();
    } finally {
      vi.useRealTimers();
    }
  });
});
