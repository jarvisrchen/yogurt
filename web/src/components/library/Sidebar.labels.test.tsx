import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Sidebar } from "./Sidebar";
import {
  useCreateLabel,
  useDeleteLabel,
  useLabels,
  useReorderLabels,
  useUpdateLabel,
} from "../../lib/api/labels";
import { useArchivePreview, useCreateMeeting } from "../../lib/api/meetings";
import { settingsApi } from "../../lib/api/settings";
import { LABEL_SORT_KEY } from "../../lib/labelSort";

vi.mock("../../lib/api/labels", async (importOriginal) => ({
  isArchivedLabel: (await importOriginal<typeof import("../../lib/api/labels")>()).isArchivedLabel,
  useLabels: vi.fn(),
  useUpdateLabel: vi.fn(),
  useDeleteLabel: vi.fn(),
  useCreateLabel: vi.fn(),
  useReorderLabels: vi.fn(),
}));

const archiveMutate = vi.fn(() => Promise.resolve({ count: 2 }));
vi.mock("../../lib/api/meetings", () => ({
  useCreateMeeting: vi.fn(),
  useArchivePreview: vi.fn((scope: unknown) => ({ data: scope ? { count: 2 } : undefined })),
  useArchiveMatching: vi.fn(() => ({ mutateAsync: archiveMutate, reset: vi.fn(), isPending: false, error: null })),
}));

vi.mock("../../lib/api/settings", () => ({
  settingsApi: { get: vi.fn() },
}));

afterEach(() => cleanup());

function renderSidebar(path = "/") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Sidebar />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Sidebar — Labels section", () => {
  it("renders labels with counts and links to /label/:id", () => {
    vi.mocked(useLabels).mockReturnValue({
      data: [
        { id: "l1", name: "Sales", color: "blue", meeting_count: 3, archived_count: 0 },
        { id: "l2", name: "Support", color: "matcha", meeting_count: 0, archived_count: 0 },
      ],
    } as unknown as ReturnType<typeof useLabels>);
    vi.mocked(useUpdateLabel).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateMeeting>);
    vi.mocked(useCreateLabel).mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useReorderLabels).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useReorderLabels>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    expect(screen.getByText("Sales")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("Support")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Sales/ })).toHaveAttribute(
      "href",
      "/label/l1",
    );
  });

  it("opens the row menu from the kebab without navigating", () => {
    vi.mocked(useLabels).mockReturnValue({
      data: [{ id: "l1", name: "Sales", color: "blue", meeting_count: 3, archived_count: 0 }],
    } as unknown as ReturnType<typeof useLabels>);
    vi.mocked(useUpdateLabel).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateMeeting>);
    vi.mocked(useCreateLabel).mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useReorderLabels).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useReorderLabels>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Sales label options" }));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
    // Parent container must not clip the absolutely-positioned menu.
    const menu = screen.getByRole("menu");
    expect(menu.parentElement?.parentElement?.className).not.toMatch(/overflow/);

    // Archive meetings counts the label's meetings, then archives them on confirm.
    expect(useArchivePreview).toHaveBeenLastCalledWith(null);
    fireEvent.click(screen.getByRole("menuitem", { name: "Archive meetings" }));
    expect(useArchivePreview).toHaveBeenLastCalledWith({ labelId: "l1" });
    expect(screen.getByText("Archives 2 meetings")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Archive?" })).toHaveFocus();
    fireEvent.click(screen.getByRole("menuitem", { name: "Archive?" }));
    expect(archiveMutate).toHaveBeenCalledWith({ labelId: "l1" });
  });

  it("commits a valid custom hex on Enter but not an invalid one on blur", () => {
    vi.mocked(useLabels).mockReturnValue({
      data: [{ id: "l1", name: "Sales", color: "blue", meeting_count: 3, archived_count: 0 }],
    } as unknown as ReturnType<typeof useLabels>);
    const mutate = vi.fn();
    vi.mocked(useUpdateLabel).mockReturnValue({
      mutate,
    } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateMeeting>);
    vi.mocked(useCreateLabel).mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Sales label options" }));
    const hexInput = screen.getByRole("textbox", { name: "Custom label color hex" });

    fireEvent.change(hexInput, { target: { value: "not-a-color" } });
    fireEvent.blur(hexInput);
    expect(mutate).not.toHaveBeenCalled();

    fireEvent.change(hexInput, { target: { value: "#A3C9FF" } });
    fireEvent.keyDown(hexInput, { key: "Enter" });
    expect(mutate).toHaveBeenCalledWith({ id: "l1", color: "#A3C9FF" });
  });

  it("shows a muted line when there are no labels", () => {
    vi.mocked(useLabels).mockReturnValue({ data: [] } as unknown as ReturnType<
      typeof useLabels
    >);
    vi.mocked(useUpdateLabel).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateMeeting>);
    vi.mocked(useCreateLabel).mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useReorderLabels).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useReorderLabels>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    expect(screen.getByText("No labels yet")).toBeInTheDocument();
  });

  it("creates a label via the + affordance", () => {
    vi.mocked(useLabels).mockReturnValue({ data: [] } as unknown as ReturnType<
      typeof useLabels
    >);
    vi.mocked(useUpdateLabel).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateMeeting>);
    const mutate = vi.fn();
    vi.mocked(useCreateLabel).mockReturnValue({
      mutate,
      isPending: false,
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useReorderLabels).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useReorderLabels>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    expect(screen.queryByPlaceholderText("Label name")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "New label" }));
    const input = screen.getByPlaceholderText("Label name");

    fireEvent.change(input, { target: { value: "  Sales  " } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(mutate).toHaveBeenCalledWith({ name: "Sales" }, expect.anything());
    expect(screen.queryByPlaceholderText("Label name")).not.toBeInTheDocument();
  });

  it("closes the new-label input on Escape without creating", () => {
    vi.mocked(useLabels).mockReturnValue({ data: [] } as unknown as ReturnType<
      typeof useLabels
    >);
    vi.mocked(useUpdateLabel).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateMeeting>);
    const mutate = vi.fn();
    vi.mocked(useCreateLabel).mockReturnValue({
      mutate,
      isPending: false,
    } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useReorderLabels).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useReorderLabels>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    fireEvent.click(screen.getByRole("button", { name: "New label" }));
    const input = screen.getByPlaceholderText("Label name");
    fireEvent.change(input, { target: { value: "Draft" } });
    fireEvent.keyDown(input, { key: "Escape" });

    expect(mutate).not.toHaveBeenCalled();
    expect(screen.queryByPlaceholderText("Label name")).not.toBeInTheDocument();
  });

  it("drags a label to a new position in Custom mode and calls reorder", () => {
    localStorage.setItem(LABEL_SORT_KEY, "custom");
    vi.mocked(useLabels).mockReturnValue({
      data: [
        { id: "l1", name: "Sales", color: "blue", meeting_count: 3, archived_count: 0, position: 0, updated_at: 1 },
        { id: "l2", name: "Support", color: "matcha", meeting_count: 0, archived_count: 0, position: 1, updated_at: 2 },
        { id: "l3", name: "Zeta", color: "straw", meeting_count: 0, archived_count: 0, position: 2, updated_at: 3 },
        { id: "l4", name: "Old", color: "slate", meeting_count: 2, archived_count: 2, position: 3, updated_at: 4 },
      ],
    } as unknown as ReturnType<typeof useLabels>);
    vi.mocked(useUpdateLabel).mockReturnValue({
      mutate: vi.fn(),
    } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({
      mutateAsync: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateMeeting>);
    vi.mocked(useCreateLabel).mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof useCreateLabel>);
    const reorder = vi.fn();
    vi.mocked(useReorderLabels).mockReturnValue({
      mutate: reorder,
    } as unknown as ReturnType<typeof useReorderLabels>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    const wrapperOf = (name: string) =>
      screen.getByText(name).closest('[draggable="true"]') as HTMLElement;

    fireEvent.dragStart(wrapperOf("Zeta"));
    fireEvent.dragOver(wrapperOf("Sales"));
    fireEvent.drop(wrapperOf("Sales"));

    expect(reorder).toHaveBeenCalledWith(["l3", "l1", "l2", "l4"]);

    localStorage.removeItem(LABEL_SORT_KEY);
  });

  it("files a label with only archived meetings under Archived labels, and counts and links by view", () => {
    vi.mocked(useLabels).mockReturnValue({
      data: [
        { id: "l1", name: "Sales", color: "blue", meeting_count: 3, archived_count: 1 },
        { id: "l2", name: "Old clients", color: "honey", meeting_count: 2, archived_count: 2 },
      ],
    } as unknown as ReturnType<typeof useLabels>);
    vi.mocked(useUpdateLabel).mockReturnValue({ mutate: vi.fn() } as unknown as ReturnType<typeof useUpdateLabel>);
    vi.mocked(useDeleteLabel).mockReturnValue({ mutateAsync: vi.fn(), isPending: false } as unknown as ReturnType<typeof useDeleteLabel>);
    vi.mocked(useCreateMeeting).mockReturnValue({ mutateAsync: vi.fn(), isPending: false } as unknown as ReturnType<typeof useCreateMeeting>);
    vi.mocked(useCreateLabel).mockReturnValue({ mutate: vi.fn(), isPending: false } as unknown as ReturnType<typeof useCreateLabel>);
    vi.mocked(useReorderLabels).mockReturnValue({ mutate: vi.fn() } as unknown as ReturnType<typeof useReorderLabels>);
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();
    const sales = screen.getByRole("link", { name: /Sales/ });
    expect(sales).toHaveAttribute("href", "/label/l1");
    expect(sales).toHaveTextContent("Sales2");
    expect(screen.queryByText("Old clients")).not.toBeInTheDocument();
    const group = screen.getByRole("button", { name: /Archived labels/ });
    expect(group).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(group);
    const old = screen.getByRole("link", { name: /Old clients/ });
    expect(old).toHaveAttribute("href", "/archive/label/l2");
    expect(old).toHaveTextContent("Old clients2");
    fireEvent.click(screen.getByRole("button", { name: "Old clients label options" }));
    expect(screen.queryByRole("menuitem", { name: "Archive meetings" })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
    cleanup();

    // In the Archive view every label opens and counts its archived meetings, and the group opens on its own label.
    renderSidebar("/archive/label/l2");
    expect(screen.getByRole("link", { name: /Sales/ })).toHaveAttribute("href", "/archive/label/l1");
    expect(screen.getByRole("link", { name: /Sales/ })).toHaveTextContent("Sales1");
    expect(screen.getByRole("button", { name: /Archived labels/ })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("link", { name: /Old clients/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Archive" })).not.toHaveAttribute("aria-current");
    fireEvent.click(screen.getByRole("button", { name: /Archived labels/ }));
    expect(screen.getByRole("button", { name: /Archived labels/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Old clients")).not.toBeInTheDocument();
  });
});
