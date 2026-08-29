/**
 * `LabelPicker` — covers the three core interactions: rendering existing
 * labels with the correct checked state, creating a brand-new label via
 * Enter, and toggling an existing label on/off.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { LabelPicker } from "./LabelPicker";
import { useCreateLabel, useLabels } from "../../lib/api/labels";
import { useSetMeetingLabels } from "../../lib/api/meetings";

vi.mock("../../lib/api/labels", () => ({
  useLabels: vi.fn(),
  useCreateLabel: vi.fn(),
}));

vi.mock("../../lib/api/meetings", () => ({
  useSetMeetingLabels: vi.fn(),
}));

const LABELS = [
  { id: "l1", name: "Sales", color: "blue" as const, meeting_count: 2 },
  { id: "l2", name: "Support", color: "matcha" as const, meeting_count: 0 },
];

const setMutate = vi.fn();
const createMutateAsync = vi.fn();

function renderPicker(selected = [{ id: "l1", name: "Sales", color: "blue" as const }]) {
  return render(
    <LabelPicker
      meetingId="meeting-1"
      selected={selected}
      open
      onClose={() => {}}
    />,
  );
}

afterEach(() => cleanup());

describe("LabelPicker", () => {
  beforeEach(() => {
    vi.mocked(useLabels).mockReturnValue({ data: LABELS } as ReturnType<typeof useLabels>);
    vi.mocked(useCreateLabel).mockReturnValue({
      mutateAsync: createMutateAsync,
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useSetMeetingLabels).mockReturnValue({
      mutate: setMutate,
    } as unknown as ReturnType<typeof useSetMeetingLabels>);
    setMutate.mockClear();
    createMutateAsync.mockReset();
    createMutateAsync.mockResolvedValue({ id: "new-1", name: "Marketing", color: "honey" });
  });

  it("renders existing labels and marks the selected one checked", () => {
    renderPicker();
    const sales = screen.getByRole("option", { name: /Sales/ });
    const support = screen.getByRole("option", { name: /Support/ });
    expect(sales).toHaveAttribute("aria-selected", "true");
    expect(support).toHaveAttribute("aria-selected", "false");
  });

  it('shows Create "…" for a query with no match, and Enter creates then applies it', async () => {
    renderPicker();
    const input = screen.getByPlaceholderText("Search or create label");
    fireEvent.change(input, { target: { value: "Marketing" } });
    const createRow = await screen.findByText('Create "Marketing"');
    expect(createRow).toBeInTheDocument();

    fireEvent.keyDown(input, { key: "Enter" });

    expect(createMutateAsync).toHaveBeenCalledWith({ name: "Marketing" });
    await waitFor(() =>
      expect(setMutate).toHaveBeenCalledWith({
        id: "meeting-1",
        label_ids: ["l1", "new-1"],
      }),
    );
  });

  it("adds an unselected label and removes a selected one on click", () => {
    renderPicker();
    fireEvent.click(screen.getByRole("option", { name: /Support/ }));
    expect(setMutate).toHaveBeenCalledWith({
      id: "meeting-1",
      label_ids: ["l1", "l2"],
    });

    fireEvent.click(screen.getByRole("option", { name: /Sales/ }));
    expect(setMutate).toHaveBeenCalledWith({
      id: "meeting-1",
      label_ids: [],
    });
  });
});
