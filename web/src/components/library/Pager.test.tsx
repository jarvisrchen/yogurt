import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { Pager } from "./Pager";

afterEach(() => cleanup());

function setup(page: number, size = 25, total = 1412) {
  const onPage = vi.fn();
  const onSize = vi.fn();
  render(<Pager page={page} size={size} total={total} onPage={onPage} onSize={onSize} />);
  return { onPage, onSize };
}

describe("Pager", () => {
  it("shows the range and steps to the neighbouring pages", () => {
    const { onPage } = setup(2);
    expect(screen.getByText("26–50 of 1,412")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Previous" }));
    expect(onPage.mock.calls).toEqual([[3], [1]]);
  });

  it("disables Previous on the first page and Next on the last", () => {
    setup(1, 25, 40);
    expect(screen.getByRole("button", { name: "Previous" })).toBeDisabled();
    cleanup();
    setup(2, 25, 40);
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  });

  it("offers 10, 25, 50, and 100 per page", () => {
    const { onSize } = setup(1);
    const select = screen.getByRole("combobox", { name: "Meetings per page" });
    expect([...select.querySelectorAll("option")].map((o) => o.value)).toEqual([
      "10",
      "25",
      "50",
      "100",
    ]);
    fireEvent.change(select, { target: { value: "50" } });
    expect(onSize).toHaveBeenCalledWith(50);
  });
});
