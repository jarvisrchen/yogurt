/**
 * Phase 7 Plan 07-04 — `useFirstRunRedirect` redirects the SPA from `/`
 * to `/welcome` when ANY of the three onboarding predicates fails:
 *   • `first_run_completed=false`
 *   • Screen Recording denied
 *   • No active provider
 *
 * The test renders a tiny shell that mounts the hook and prints
 * `useLocation().pathname` via a `<Probe>`. We mock the two upstream
 * hooks (`useSettings`, `useScreenRecordingStatus`) so each case can
 * dial in the exact predicate combination without touching the
 * network or the React-Query cache.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route, useLocation } from "react-router";
import { useFirstRunRedirect } from "./useFirstRunRedirect";

// ─── Hoisted mock state ─────────────────────────────────────────────────────
// `vi.mock` factories run before the module imports, so we expose a couple
// of `vi.hoisted` slots that each test can overwrite per-case.

const state = vi.hoisted(() => ({
  settings: undefined as unknown,
  screenRec: undefined as unknown,
}));

vi.mock("../lib/api/settings", () => ({
  useSettings: () => state.settings,
}));

vi.mock("./useScreenRecordingStatus", () => ({
  useScreenRecordingStatus: () => state.screenRec,
}));

// ─── Helpers ────────────────────────────────────────────────────────────────

function Probe() {
  const { pathname } = useLocation();
  return <div data-testid="pathname">{pathname}</div>;
}

function Shell() {
  useFirstRunRedirect();
  return <Probe />;
}

function renderAt(initialPath: string) {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route path="/" element={<Shell />} />
        <Route path="/welcome" element={<Shell />} />
        <Route path="/settings" element={<Shell />} />
      </Routes>
    </MemoryRouter>,
  );
}

interface FakeSettings {
  isLoading: boolean;
  data?: {
    general: { first_run_completed: boolean };
    providers: Array<{ is_active: boolean }>;
  };
}

function settingsLoaded(opts: {
  firstRunCompleted: boolean;
  hasActiveProvider: boolean;
}): FakeSettings {
  return {
    isLoading: false,
    data: {
      general: { first_run_completed: opts.firstRunCompleted },
      providers: opts.hasActiveProvider
        ? [{ is_active: true }]
        : [{ is_active: false }],
    },
  };
}

function screenRecLoaded(granted: boolean) {
  return { granted, status: granted ? "granted" : "denied", isLoading: false, error: null };
}

// ─── Cases ──────────────────────────────────────────────────────────────────

describe("useFirstRunRedirect", () => {
  beforeEach(() => {
    state.settings = undefined;
    state.screenRec = undefined;
  });

  it("redirects to /welcome when first_run_completed is false", async () => {
    state.settings = settingsLoaded({
      firstRunCompleted: false,
      hasActiveProvider: true,
    });
    state.screenRec = screenRecLoaded(true);

    const { getByTestId } = renderAt("/");
    await waitFor(() => {
      expect(getByTestId("pathname").textContent).toBe("/welcome");
    });
  });

  it("redirects to /welcome when Screen Recording is denied", async () => {
    state.settings = settingsLoaded({
      firstRunCompleted: true,
      hasActiveProvider: true,
    });
    state.screenRec = screenRecLoaded(false);

    const { getByTestId } = renderAt("/");
    await waitFor(() => {
      expect(getByTestId("pathname").textContent).toBe("/welcome");
    });
  });

  it("redirects to /welcome when no active provider", async () => {
    state.settings = settingsLoaded({
      firstRunCompleted: true,
      hasActiveProvider: false,
    });
    state.screenRec = screenRecLoaded(true);

    const { getByTestId } = renderAt("/");
    await waitFor(() => {
      expect(getByTestId("pathname").textContent).toBe("/welcome");
    });
  });

  it("stays on / when fully set up", async () => {
    state.settings = settingsLoaded({
      firstRunCompleted: true,
      hasActiveProvider: true,
    });
    state.screenRec = screenRecLoaded(true);

    const { getByTestId } = renderAt("/");
    // Give effects a tick to potentially redirect — they shouldn't.
    await new Promise((r) => setTimeout(r, 20));
    expect(getByTestId("pathname").textContent).toBe("/");
  });

  it("does not redirect on routes other than /", async () => {
    state.settings = settingsLoaded({
      firstRunCompleted: false,
      hasActiveProvider: false,
    });
    state.screenRec = screenRecLoaded(false);

    const { getByTestId } = renderAt("/settings");
    await new Promise((r) => setTimeout(r, 20));
    expect(getByTestId("pathname").textContent).toBe("/settings");
  });

  it("does nothing while settings are still loading (no flicker)", async () => {
    state.settings = { isLoading: true };
    state.screenRec = screenRecLoaded(false);

    const { getByTestId } = renderAt("/");
    await new Promise((r) => setTimeout(r, 20));
    expect(getByTestId("pathname").textContent).toBe("/");
  });
});
