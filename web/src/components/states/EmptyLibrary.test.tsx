/**
 * Phase 7 Plan 07-04 — EmptyLibrary CSS-contract assertion.
 *
 * The PRD §16.5 motion contract locks the floating-logo cadence at 3.5s
 * ease-in-out infinite via the `.float-3500` utility. This test guards
 * against accidental class renames or removal — the class must wrap the
 * logo so the animation actually plays.
 */

import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { EmptyLibrary } from "./EmptyLibrary";

// The component calls `useCreateMeeting` which hits `/api/meetings` —
// stub it out so the test stays hermetic.
vi.mock("../../lib/api/meetings", () => ({
  useCreateMeeting: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
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
    // The logo SVG must live INSIDE the floater so the transform applies.
    expect(floater?.querySelector("svg")).not.toBeNull();
  });

  it("renders the 'No meetings yet' headline + ⌘N affordance", () => {
    const { getByText, getByRole } = renderEmpty();
    expect(getByText("No meetings yet")).toBeInTheDocument();
    expect(
      getByRole("button", { name: /start your first meeting/i }),
    ).toBeInTheDocument();
    // The keyboard hint sits inside the button.
    expect(getByText("⌘N")).toBeInTheDocument();
  });

  it("shows the mono `~/.yogurt/notes/*.md` caption", () => {
    const { getByText } = renderEmpty();
    expect(getByText(/~\/\.yogurt\/notes\/\*\.md/)).toBeInTheDocument();
  });
});
