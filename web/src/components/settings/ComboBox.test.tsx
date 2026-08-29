/**
 * ComboBox - text input + clickable dropdown of suggestions.
 *
 * Replaces the `<input list>` + `<datalist>` pattern that ships with
 * every browser but renders inconsistently on Safari/macOS (no dropdown
 * arrow, click does nothing in some versions). The combo-box gives the
 * user a real "click to see options" affordance with the same free-text
 * escape hatch as the native autocomplete.
 *
 * `onChange` fires on every keystroke (the parent's draft state); `onCommit`
 * fires once when the user actually commits to a value - a pick, an Enter
 * that isn't opening the popup, or a blur outside the component.
 */
import { describe, it, expect, vi } from "vitest";
import { useState } from "react";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { ComboBox } from "./ComboBox";

/** Stateful wrapper so `value` actually reflects what the user typed, the
 *  way a real parent (`ModelSelect`) does - needed for any test that types
 *  and then expects a commit to see the typed text. */
function ControlledComboBox({
  onChange,
  onCommit,
  options,
  initialValue = "",
}: {
  onChange: (v: string) => void;
  onCommit: (v: string) => void;
  options: string[];
  initialValue?: string;
}) {
  const [value, setValue] = useState(initialValue);
  return (
    <ComboBox
      value={value}
      onChange={(v) => {
        setValue(v);
        onChange(v);
      }}
      onCommit={onCommit}
      options={options}
      ariaLabel="Model"
      triggerLabel="Show models"
    />
  );
}

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

  it("opens the popup showing every option when the trigger button is clicked", () => {
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

  it("calls onChange and onCommit with the picked option when one is clicked", () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();
    render(
      <ComboBox
        value=""
        onChange={onChange}
        onCommit={onCommit}
        options={options}
        ariaLabel="Model"
        triggerLabel="Show models"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Show models" }));
    fireEvent.mouseDown(screen.getByRole("option", { name: "gpt-4.1" }));
    expect(onChange).toHaveBeenCalledWith("gpt-4.1");
    expect(onCommit).toHaveBeenCalledWith("gpt-4.1");
  });

  it("calls onChange on every keystroke so the parent's draft state keeps working", () => {
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

  it("typing free text then Enter keeps the typed value and commits it once", () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();
    render(
      <ControlledComboBox
        onChange={onChange}
        onCommit={onCommit}
        options={options}
      />,
    );
    const input = screen.getByLabelText("Model");
    fireEvent.change(input, { target: { value: "gemini-3-pro" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onChange).toHaveBeenLastCalledWith("gemini-3-pro");
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith("gemini-3-pro");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("ArrowDown then Enter picks option 0 and calls onCommit with it", () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();
    render(
      <ControlledComboBox
        onChange={onChange}
        onCommit={onCommit}
        options={options}
      />,
    );
    const input = screen.getByLabelText("Model");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onChange).toHaveBeenCalledWith(options[0]);
    expect(onCommit).toHaveBeenCalledWith(options[0]);
  });

  it('typing "4.1" narrows the listbox to matching options only', () => {
    render(
      <ControlledComboBox
        onChange={() => {}}
        onCommit={() => {}}
        options={options}
      />,
    );
    const input = screen.getByLabelText("Model");
    fireEvent.change(input, { target: { value: "4.1" } });

    const listbox = screen.getByRole("listbox");
    const shown = within(listbox)
      .getAllByRole("option")
      .map((el) => el.textContent);
    expect(shown).toEqual(["gpt-4.1"]);
  });

  it("blur outside the component calls onCommit with the current value", () => {
    const onCommit = vi.fn();
    render(
      <div>
        <ComboBox
          value="typed-value"
          onChange={() => {}}
          onCommit={onCommit}
          options={options}
          ariaLabel="Model"
          triggerLabel="Show models"
        />
        <button type="button">elsewhere</button>
      </div>,
    );
    const input = screen.getByLabelText("Model");
    fireEvent.focus(input);
    fireEvent.blur(input, { relatedTarget: screen.getByText("elsewhere") });

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith("typed-value");
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

  it("renders a no-match hint when typing filters out every option", () => {
    render(
      <ControlledComboBox
        onChange={() => {}}
        onCommit={() => {}}
        options={options}
      />,
    );
    fireEvent.change(screen.getByLabelText("Model"), {
      target: { value: "does-not-exist" },
    });
    expect(screen.getByText(/no matches/i)).toBeInTheDocument();
  });
});
