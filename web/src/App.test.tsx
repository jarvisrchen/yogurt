import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { App } from "./App";

/**
 * App.test — covers the library-stub root and the navigation contract.
 *
 * Phase 4 (Plan 04-04) replaces the Phase 3 `useState<View>` toggle with a
 * URL-driven route table. The CTA now `navigate("/meeting/new")` rather
 * than swapping in the Meeting component inline — so the test asserts the
 * navigation intent via a `useNavigate` spy.
 */

const navigateSpy = vi.fn();
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>(
    "react-router",
  );
  return {
    ...actual,
    useNavigate: () => navigateSpy,
  };
});

function renderApp() {
  return render(
    <MemoryRouter>
      <App />
    </MemoryRouter>,
  );
}

describe("App", () => {
  beforeEach(() => {
    navigateSpy.mockReset();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("renders the library stub by default", () => {
    renderApp();
    expect(
      screen.getByRole("heading", { name: /yogurt/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Open a new meeting/i }),
    ).toBeInTheDocument();
  });

  it("navigates to /meeting/new on click", () => {
    renderApp();
    const btn = screen.getByRole("button", { name: /Open a new meeting/i });
    act(() => {
      btn.click();
    });
    expect(navigateSpy).toHaveBeenCalledWith("/meeting/new");
  });
});
