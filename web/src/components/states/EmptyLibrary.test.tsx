import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { EmptyLibrary } from "./EmptyLibrary";

// Stub `useCreateMeeting` (it hits `/api/meetings`).
vi.mock("../../lib/api/meetings", () => ({
  useCreateMeeting: () => ({
    mutateAsync: vi.fn(),
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

function renderEmpty() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <EmptyLibrary />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("EmptyLibrary", () => {
  it("wraps the swirl logo in `.float-3500` (PRD §16.5 cadence contract)", () => {
    const { container } = renderEmpty();
    const floater = container.querySelector(".float-3500");
    expect(floater).not.toBeNull();
    expect(floater?.querySelector("svg")).not.toBeNull();
  });

  it("renders the 'No meetings yet' headline + ⌘N affordance", () => {
    const { getByText, getByRole } = renderEmpty();
    expect(getByText("No meetings yet")).toBeInTheDocument();
    expect(
      getByRole("button", { name: /start your first meeting/i }),
    ).toBeInTheDocument();
    expect(getByText("⌘N")).toBeInTheDocument();
  });

  it("names the server's notes folder in the caption", () => {
    const { getByText } = renderEmpty();
    expect(getByText("/tmp/scratch/notes/*.md")).toBeInTheDocument();
  });
});
