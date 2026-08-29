/**
 * Sidebar — Labels section coverage: labels render with their meeting
 * counts and link to `/label/:id`.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Sidebar } from "./Sidebar";
import { useDeleteLabel, useLabels, useUpdateLabel } from "../../lib/api/labels";
import { useCreateMeeting } from "../../lib/api/meetings";
import { settingsApi } from "../../lib/api/settings";

vi.mock("../../lib/api/labels", () => ({
  useLabels: vi.fn(),
  useUpdateLabel: vi.fn(),
  useDeleteLabel: vi.fn(),
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
    vi.mocked(settingsApi.get).mockResolvedValue({ providers: [] } as never);

    renderSidebar();

    expect(screen.getByText("No labels yet")).toBeInTheDocument();
  });
});
