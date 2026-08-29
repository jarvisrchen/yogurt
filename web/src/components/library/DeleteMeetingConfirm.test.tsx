/**
 * DeleteMeetingConfirm tests.
 *
 * Covers:
 *   - icon-variant confirm overlays the trashcan instead of replacing it
 *     (the post-meeting topbar used to reflow when the confirm UI expanded)
 *   - menuitem-variant confirm still replaces the menuitem (existing behavior)
 *   - Cancel reverts without mutating
 *   - Delete? calls the mutation with the right args
 *
 * Note: the confirm-panel buttons carry `role="menuitem"` from the original
 * menu-context implementation. The post-meeting topbar technically isn't a
 * menu, but the role is harmless outside a `role="menu"` parent (screen
 * readers ignore orphaned menuitems). Tests query them via that role since
 * it's the most reliable discriminator between the panel and the trashcan.
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
    // Confirm panel is not present until the trashcan is clicked.
    expect(screen.queryByRole("menuitem", { name: /^delete\?$/i })).toBeNull();
  });

  it("keeps the trashcan in the DOM after clicking it (so the topbar does not reflow)", () => {
    renderIcon();
    fireEvent.click(screen.getByRole("button", { name: /delete meeting/i }));

    // Trashcan still present - confirm UI must not replace it.
    expect(
      screen.getByRole("button", { name: /delete meeting/i }),
    ).toBeInTheDocument();
    // Confirm panel reachable.
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
    // Back to rest state: trashcan only, no confirm panel.
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
  });
});
