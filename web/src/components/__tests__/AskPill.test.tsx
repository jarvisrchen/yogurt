import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { AskPill } from "../AskPill";

describe("AskPill", () => {
  it("renders the 'Ask this meeting…' placeholder and ⌘K hint", () => {
    render(<AskPill onExpand={() => {}} />);
    expect(screen.getByText(/ask this meeting/i)).toBeInTheDocument();
    expect(screen.getByText("⌘K")).toBeInTheDocument();
  });

  it("calls onExpand when clicked", () => {
    const onExpand = vi.fn();
    render(<AskPill onExpand={onExpand} />);
    const btn = screen.getByRole("button", { name: /ask this meeting/i });
    fireEvent.click(btn);
    expect(onExpand).toHaveBeenCalledTimes(1);
  });

  it("calls onExpand when ⌘K is pressed", () => {
    const onExpand = vi.fn();
    render(<AskPill onExpand={onExpand} />);
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(onExpand).toHaveBeenCalledTimes(1);
  });
});
