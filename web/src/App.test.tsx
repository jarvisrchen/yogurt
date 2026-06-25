import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { App } from "./App";

vi.mock("./lib/api", () => ({
  fetchHealth: vi
    .fn()
    .mockResolvedValue({ status: "ok", service: "yogurt-server" }),
}));

function renderApp() {
  return render(
    <MemoryRouter>
      <App />
    </MemoryRouter>,
  );
}

describe("App", () => {
  it("renders the yogurt headline", async () => {
    renderApp();
    expect(
      await screen.findByRole("heading", { name: /yogurt/i }),
    ).toBeInTheDocument();
  });

  it("shows the health response once fetched", async () => {
    renderApp();
    await waitFor(() => {
      expect(screen.getByText(/yogurt-server ok/)).toBeInTheDocument();
    });
  });

  it("links to the style guide", async () => {
    renderApp();
    const link = await screen.findByRole("link", { name: /style-guide/i });
    expect(link).toHaveAttribute("href", "/style-guide");
  });
});
