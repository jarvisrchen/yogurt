import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router";
import { routes } from "./router";

// App calls fetchHealth() on mount; mock it locally so the test doesn't
// hit a real (or absent) backend. vi.mock does not leak across files.
vi.mock("./lib/api", () => ({
  fetchHealth: vi
    .fn()
    .mockResolvedValue({ status: "ok", service: "yogurt-server" }),
}));

describe("router", () => {
  it("renders the App at /", async () => {
    const router = createMemoryRouter(routes, { initialEntries: ["/"] });
    render(<RouterProvider router={router} />);
    expect(
      await screen.findByRole("heading", { name: /yogurt/i }),
    ).toBeInTheDocument();
  });

  it("renders the StyleGuide at /style-guide", async () => {
    const router = createMemoryRouter(routes, {
      initialEntries: ["/style-guide"],
    });
    render(<RouterProvider router={router} />);
    expect(
      await screen.findByRole("heading", { name: /style guide/i }),
    ).toBeInTheDocument();
  });
});
