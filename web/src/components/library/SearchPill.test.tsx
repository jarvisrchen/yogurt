import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { SearchPill } from "./SearchPill";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("SearchPill", () => {
  it("keeps a keystroke typed before its last search lands, and follows a cleared search", () => {
    vi.useFakeTimers();
    const onChange = vi.fn();
    const { rerender } = render(<SearchPill value="" onChange={onChange} />);
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "fram" } });
    act(() => vi.advanceTimersByTime(200));
    expect(onChange).toHaveBeenLastCalledWith("fram");
    fireEvent.change(box, { target: { value: "frami" } });
    rerender(<SearchPill value="fram" onChange={onChange} />);
    expect(box).toHaveValue("frami");
    rerender(<SearchPill value="" onChange={onChange} />);
    expect(box).toHaveValue("");
  });
});
