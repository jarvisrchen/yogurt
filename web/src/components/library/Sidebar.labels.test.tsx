/**
 * Sidebar — Labels section coverage: labels render with their meeting
 * counts and link to `/label/:id`.
 */
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
  useUpdateLabel,
} from "../../lib/api/labels";
import { useCreateMeeting } from "../../lib/api/meetings";
import { settingsApi } from "../../lib/api/settings";

vi.mock("../../lib/api/labels", () => ({
  useLabels: vi.fn(),
  useUpdateLabel: vi.fn(),
  useDeleteLabel: vi.fn(),
  useCreateLabel: vi.fn(),
}));

vi.mock("../../lib/api/meetings", () => ({
  useCreateMeeting: vi.fn(),
}));

vi.mock("../../lib/api/settings", () => ({
  settingsApi: { get: vi.fn() },
}));

afterEach(() => cleanup());

function renderSidebar() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Sidebar — Labels section", () => {
  it("renders labels with counts and links to /label/:id", () => {
    vi.mocked(useLabels).mockReturnValue({
      data: [
        { id: "l1", name: "Sales", color: "blue", meeting_count: 3 },
        { id: "l2", name: "Support", color: "matcha", meeting_count: 0 },
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
      data: [{ id: "l1", name: "Sales", color: "blue", meeting_count: 3 }],
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
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Sales label options" }));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
    // Parent container must not clip the absolutely-positioned menu.
    const menu = screen.getByRole("menu");
    expect(menu.parentElement?.parentElement?.className).not.toMatch(/overflow/);
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
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    expect(screen.queryByPlaceholderText("Label name")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "New label" }));
    const input = screen.getByPlaceholderText("Label name");

    fireEvent.change(input, { target: { value: "  Sales  " } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(mutate).toHaveBeenCalledWith({ name: "Sales" });
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
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    fireEvent.click(screen.getByRole("button", { name: "New label" }));
    const input = screen.getByPlaceholderText("Label name");
    fireEvent.change(input, { target: { value: "Draft" } });
    fireEvent.keyDown(input, { key: "Escape" });

    expect(mutate).not.toHaveBeenCalled();
    expect(screen.queryByPlaceholderText("Label name")).not.toBeInTheDocument();
  });
});
