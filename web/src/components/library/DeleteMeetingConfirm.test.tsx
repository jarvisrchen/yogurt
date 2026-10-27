/**
 * The confirm-panel buttons carry `role="menuitem"`, which is harmless outside a
 * `role="menu"` parent. Tests query by it to tell the panel from the trashcan.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DeleteMeetingConfirm } from "./DeleteMeetingConfirm";

const mutateAsync = vi.fn();

vi.mock("../../lib/api/meetings", () => ({
  useDeleteMeeting: () => ({
    mutateAsync,
    isPending: false,
  }),
}));

vi.mock("../../lib/api/settings", () => ({
  useDataPaths: () => ({
    data_dir: "/tmp/scratch",
    keys: "/tmp/scratch/keys.json",
    notes: "/tmp/scratch/notes",
  }),
}));

function renderIcon(onDeleted = vi.fn()) {
  return render(
    <DeleteMeetingConfirm id="m1" variant="icon" onDeleted={onDeleted} />,
  );
}

function renderMenuItem(onDeleted = vi.fn()) {
  return render(
    <DeleteMeetingConfirm id="m2" variant="menuitem" onDeleted={onDeleted} />,
  );
}

describe("DeleteMeetingConfirm — icon variant", () => {
  beforeEach(() => {
    mutateAsync.mockClear();
    mutateAsync.mockResolvedValue({});
  });

  it("renders the trashcan button at rest", () => {
    renderIcon();
    expect(
      screen.getByRole("button", { name: /delete meeting/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /^delete\?$/i })).toBeNull();
  });

  it("keeps the trashcan in the DOM after clicking it (so the topbar does not reflow)", () => {
    renderIcon();
    fireEvent.click(screen.getByRole("button", { name: /delete meeting/i }));

    expect(
      screen.getByRole("button", { name: /delete meeting/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /^delete\?$/i })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /^cancel$/i })).toBeInTheDocument();
  });

  it("anchors the confirm popover with position: absolute (does not occupy flow space)", () => {
    renderIcon();
    fireEvent.click(screen.getByRole("button", { name: /delete meeting/i }));

    const popover = screen.getByTestId("delete-confirm-popover");
    expect(popover.className).toMatch(/\babsolute\b/);
  });

  it("Cancel reverts to trashcan without calling the mutation", () => {
    renderIcon();
    fireEvent.click(screen.getByRole("button", { name: /delete meeting/i }));
    fireEvent.click(screen.getByRole("menuitem", { name: /^cancel$/i }));

    expect(mutateAsync).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: /delete meeting/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /^delete\?$/i })).toBeNull();
  });

  it("Delete? invokes the mutation with { id, deleteFile: true } and the onDeleted callback", async () => {
    const onDeleted = vi.fn();
    mutateAsync.mockResolvedValueOnce({});
    renderIcon(onDeleted);
    fireEvent.click(screen.getByRole("button", { name: /delete meeting/i }));
    fireEvent.click(screen.getByRole("menuitem", { name: /^delete\?$/i }));

    expect(mutateAsync).toHaveBeenCalledTimes(1);
    expect(mutateAsync).toHaveBeenCalledWith({ id: "m1", deleteFile: true });
    await vi.waitFor(() => expect(onDeleted).toHaveBeenCalledTimes(1));
  });
});

describe("DeleteMeetingConfirm — menuitem variant", () => {
  beforeEach(() => {
    mutateAsync.mockClear();
    mutateAsync.mockResolvedValue({});
  });

  it("renders the menuitem Delete entry at rest", () => {
    renderMenuItem();
    expect(screen.getByRole("menuitem", { name: /^delete$/i })).toBeInTheDocument();
  });

  it("replaces the menuitem with the confirm panel when clicked (existing behavior preserved)", () => {
    renderMenuItem();
    fireEvent.click(screen.getByRole("menuitem", { name: /^delete$/i }));
    expect(screen.queryByRole("menuitem", { name: /^delete$/i })).toBeNull();
    expect(screen.getByRole("menuitem", { name: /^delete\?$/i })).toBeInTheDocument();
    expect(screen.getByText("/tmp/scratch/notes")).toBeInTheDocument();
  });
});
