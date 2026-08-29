/**
 * ComboBox — text input + clickable dropdown of suggestions.
 *
 * Replaces the `<input list>` + `<datalist>` pattern that ships with
 * every browser but renders inconsistently on Safari/macOS (no dropdown
 * arrow, click does nothing in some versions). The combo-box gives the
 * user a real "click to see options" affordance with the same free-text
 * escape hatch as the native autocomplete.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { ComboBox } from "./ComboBox";

describe("ComboBox", () => {
  const options = ["gpt-4o", "gpt-4o-mini", "gpt-4.1", "o4-mini", "o3"];

  it("renders the current value into the input", () => {
    render(
      <ComboBox
        value="gpt-4o-mini"
        onChange={() => {}}
        options={options}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    expect(screen.getByLabelText("Model")).toHaveValue("gpt-4o-mini");
  });

  it("does not show the popup before any interaction", () => {
    render(
      <ComboBox
        value=""
        onChange={() => {}}
        options={options}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("opens the popup when the trigger button is clicked", () => {
    render(
      <ComboBox
        value=""
        onChange={() => {}}
        options={options}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Show models" }));
    const listbox = screen.getByRole("listbox");
    expect(listbox).toBeInTheDocument();
    expect(within(listbox).getAllByRole("option")).toHaveLength(5);
  });

  it("opens the popup when the input is focused", () => {
    render(
      <ComboBox
        value=""
        onChange={() => {}}
        options={options}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    fireEvent.focus(screen.getByLabelText("Model"));
    expect(screen.getByRole("listbox")).toBeInTheDocument();
  });

  it("calls onChange with the picked option when one is clicked", () => {
    const onChange = vi.fn();
    render(
      <ComboBox
        value=""
        onChange={onChange}
        options={options}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Show models" }));
    fireEvent.mouseDown(screen.getByRole("option", { name: "gpt-4.1" }));
    expect(onChange).toHaveBeenCalledWith("gpt-4.1");
  });

  it("calls onChange on every keystroke so the parent's draft / PATCH flow keeps working", () => {
    const onChange = vi.fn();
    render(
      <ComboBox
        value=""
        onChange={onChange}
        options={options}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    fireEvent.change(screen.getByLabelText("Model"), {
      target: { value: "gemini-3" },
    });
    expect(onChange).toHaveBeenCalledWith("gemini-3");
  });

  it("renders a free-text hint when the option set is empty (after a Refresh that found nothing)", () => {
    render(
      <ComboBox
        value=""
        onChange={() => {}}
        options={[]}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Show models" }));
    expect(
      screen.getByText(/no saved suggestions/i),
    ).toBeInTheDocument();
  });
});
