import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";

vi.mock("./lib/api", () => ({
  fetchHealth: vi
    .fn()
    .mockResolvedValue({ status: "ok", service: "yogurt-server" }),
}));

describe("App", () => {
  it("renders the yogurt headline", async () => {
    render(<App />);
    // Use findBy to wait for the React 19 effect cycle to settle before asserting.
    expect(
      await screen.findByRole("heading", { name: /yogurt/i })
    ).toBeInTheDocument();
  });

  it("shows the health response once fetched", async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText(/yogurt-server ok/)).toBeInTheDocument();
    });
  });
});
