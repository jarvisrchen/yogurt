import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { SelectionActionBar } from "./SelectionActionBar";
import { useLabels, useCreateLabel } from "../../lib/api/labels";
import {
  exportMeetingFile,
  useBulkDeleteMeetings,
  useBulkSetLabel,
  useSetArchived,
  type Meeting,
} from "../../lib/api/meetings";

vi.mock("../../lib/api/labels", () => ({
  useLabels: vi.fn(),
  useCreateLabel: vi.fn(),
}));

vi.mock("../../lib/api/meetings", () => ({
  exportMeetingFile: vi.fn(),
  useBulkDeleteMeetings: vi.fn(),
  useBulkSetLabel: vi.fn(),
  useSetArchived: vi.fn(),
}));

vi.mock("../../lib/api/settings", () => ({
  useDataPaths: () => ({
    data_dir: "/tmp/scratch",
    keys: "/tmp/scratch/keys.json",
    notes: "/tmp/scratch/notes",
  }),
}));

function meetings(n: number): Pick<Meeting, "id" | "title" | "labels">[] {
  return Array.from({ length: n }, (_, i) => ({ id: `m${i}`, title: `Meeting ${i}`, labels: [] }));
}

const bulkDeleteMutateAsync = vi.fn();
const archiveMutateAsync = vi.fn();

function renderBar(count: number, archived = false, activeId: string | null = null) {
  const onSelectAll = vi.fn();
  const onClear = vi.fn();
  render(
    <SelectionActionBar
      meetings={meetings(count)}
      onSelectAll={onSelectAll}
      onClear={onClear}
      archived={archived}
      activeId={activeId}
    />,
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
    archiveMutateAsync.mockReset();
    archiveMutateAsync.mockResolvedValue(undefined);
    vi.mocked(useSetArchived).mockReturnValue({
      mutateAsync: archiveMutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof useSetArchived>);
    vi.mocked(exportMeetingFile).mockReset();
    vi.mocked(exportMeetingFile).mockResolvedValue(undefined);
  });

  it("Export saves one file per selected meeting, skipping the recording one, then clears", async () => {
    const { onClear } = renderBar(3, false, "m1");
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await waitFor(() => expect(onClear).toHaveBeenCalledTimes(1));
    expect(vi.mocked(exportMeetingFile).mock.calls).toEqual([
      ["m0", "Meeting 0"],
      ["m2", "Meeting 2"],
    ]);
  });

  it("Export keeps the selection and says how many files failed", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(exportMeetingFile).mockRejectedValueOnce(new Error("404 Not Found"));
    const { onClear } = renderBar(2);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    expect(await screen.findByRole("button", { name: "Couldn't export 1" })).toBeInTheDocument();
    expect(exportMeetingFile).toHaveBeenCalledTimes(2);
    expect(onClear).not.toHaveBeenCalled();
  });

  it("Archive archives every selected meeting with no confirmation, then clears", async () => {
    const { onClear } = renderBar(2);
    fireEvent.click(screen.getByRole("button", { name: "Archive" }));
    expect(archiveMutateAsync).toHaveBeenCalledWith({ ids: ["m0", "m1"], archived: true });
    await waitFor(() => expect(onClear).toHaveBeenCalledTimes(1));
  });

  it("Archive leaves out the meeting being recorded, which the server refuses", () => {
    renderBar(3, false, "m1");
    fireEvent.click(screen.getByRole("button", { name: "Archive" }));
    expect(archiveMutateAsync).toHaveBeenCalledWith({ ids: ["m0", "m2"], archived: true });
  });

  it("the Archive view offers Unarchive instead", () => {
    renderBar(1, true);
    expect(screen.queryByRole("button", { name: "Archive" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Unarchive" }));
    expect(archiveMutateAsync).toHaveBeenCalledWith({ ids: ["m0"], archived: false });
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
    // ConfirmPanel buttons carry `role="menuitem"`.
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
