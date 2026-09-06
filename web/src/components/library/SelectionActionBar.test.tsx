/**
 * `SelectionActionBar` - MTG-14 floating bulk-action bar. Covers the
 * rendering the ticket calls out explicitly: the "N selected" count,
 * Clear, Select all, and the Delete confirm step (irreversible action -
 * must not fire on the first click). API hooks are mocked the same way
 * `LabelPicker.test.tsx` mocks them - no real network calls.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { SelectionActionBar } from "./SelectionActionBar";
import { useLabels, useCreateLabel } from "../../lib/api/labels";
import { useBulkDeleteMeetings, useBulkSetLabel, type Meeting } from "../../lib/api/meetings";

vi.mock("../../lib/api/labels", () => ({
  useLabels: vi.fn(),
  useCreateLabel: vi.fn(),
}));

vi.mock("../../lib/api/meetings", () => ({
  useBulkDeleteMeetings: vi.fn(),
  useBulkSetLabel: vi.fn(),
}));

function meetings(n: number): Pick<Meeting, "id" | "labels">[] {
  return Array.from({ length: n }, (_, i) => ({ id: `m${i}`, labels: [] }));
}

const bulkDeleteMutateAsync = vi.fn();

function renderBar(count: number) {
  const onSelectAll = vi.fn();
  const onClear = vi.fn();
  render(
    <SelectionActionBar meetings={meetings(count)} onSelectAll={onSelectAll} onClear={onClear} />,
  );
  return { onSelectAll, onClear };
}

afterEach(() => cleanup());

describe("SelectionActionBar", () => {
  beforeEach(() => {
    vi.mocked(useLabels).mockReturnValue({ data: [] } as unknown as ReturnType<typeof useLabels>);
    vi.mocked(useCreateLabel).mockReturnValue({
      mutateAsync: vi.fn(),
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useBulkSetLabel).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useBulkSetLabel>);
    bulkDeleteMutateAsync.mockReset();
    bulkDeleteMutateAsync.mockResolvedValue(undefined);
    vi.mocked(useBulkDeleteMeetings).mockReturnValue({
      mutateAsync: bulkDeleteMutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof useBulkDeleteMeetings>);
  });

  it("shows the selected count", () => {
    renderBar(3);
    expect(screen.getByText("3 selected")).toBeInTheDocument();
  });

  it("Clear invokes onClear", () => {
    const { onClear } = renderBar(2);
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(onClear).toHaveBeenCalledTimes(1);
  });

  it("Select all invokes onSelectAll", () => {
    const { onSelectAll } = renderBar(2);
    fireEvent.click(screen.getByRole("button", { name: "Select all" }));
    expect(onSelectAll).toHaveBeenCalledTimes(1);
  });

  it("Delete requires a confirm step before the bulk mutation fires", () => {
    renderBar(2);
    expect(screen.queryByTestId("bulk-delete-confirm-popover")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.getByTestId("bulk-delete-confirm-popover")).toBeInTheDocument();
    expect(bulkDeleteMutateAsync).not.toHaveBeenCalled();
  });

  it("confirming Delete? fires the bulk mutation with every selected id", async () => {
    renderBar(2);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    // ConfirmPanel's buttons carry `role="menuitem"` (see
    // DeleteMeetingConfirm.test.tsx) - same reused component here.
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete?" }));
    expect(bulkDeleteMutateAsync).toHaveBeenCalledWith({
      ids: ["m0", "m1"],
      deleteFile: true,
    });
  });

  it("Cancel on the confirm step reverts to the plain Delete button", () => {
    renderBar(1);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Cancel" }));
    expect(screen.queryByTestId("bulk-delete-confirm-popover")).toBeNull();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });

  it("Add label opens the bulk label picker", () => {
    renderBar(2);
    fireEvent.click(screen.getByRole("button", { name: "Add label" }));
    expect(screen.getByPlaceholderText("Search or create label")).toBeInTheDocument();
  });
});
