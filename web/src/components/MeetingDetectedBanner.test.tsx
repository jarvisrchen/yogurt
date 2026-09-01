import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";

const state = vi.hoisted(() => ({
  detected: null as { window_id: number; app: string; title: string } | null,
  /** Resolved by the test to control when `POST /api/meetings` "returns". */
  createResolve: null as ((m: { id: string }) => void) | null,
}));

const dismissSpy = vi.hoisted(() => vi.fn());
vi.mock("../lib/api/meetings", () => ({
  useDetectedMeeting: () => ({ data: state.detected }),
  useDismissDetectedMeeting: () => ({ mutate: dismissSpy, isPending: false }),
  useCreateMeeting: () => ({
    isPending: false,
    mutateAsync: () =>
      new Promise<{ id: string }>((resolve) => {
        state.createResolve = resolve;
      }),
  }),
}));

const navigateSpy = vi.hoisted(() => vi.fn());
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>("react-router");
  return { ...actual, useNavigate: () => navigateSpy };
});

import { MeetingDetectedBanner } from "./MeetingDetectedBanner";

afterEach(() => {
  cleanup();
  state.detected = null;
  state.createResolve = null;
  navigateSpy.mockClear();
  dismissSpy.mockClear();
});

function renderBanner() {
  return render(
    <MemoryRouter>
      <MeetingDetectedBanner />
    </MemoryRouter>,
  );
}

describe("MeetingDetectedBanner", () => {
  it("renders nothing when nothing is detected", () => {
    const { container } = renderBanner();
    expect(container.firstChild).toBeNull();
  });

  it("names the detected app", () => {
    state.detected = { window_id: 7, app: "Zoom", title: "Zoom Meeting" };
    renderBanner();
    expect(screen.getByText("Zoom")).toBeInTheDocument();
    expect(screen.getByText(/meeting detected/)).toBeInTheDocument();
  });

  it("stays mounted while the meeting is being created, then navigates with autoStart", async () => {
    // Regression guard: hiding the banner before the create resolves
    // unmounts the component, which drops the `useMutation` observer and
    // leaves `mutateAsync` unsettled — the click silently did nothing.
    state.detected = { window_id: 7, app: "Zoom", title: "Zoom Meeting" };
    renderBanner();
    fireEvent.click(screen.getByText("Start recording"));

    // Create still in flight: the prompt must still be on screen.
    expect(screen.getByText("Zoom")).toBeInTheDocument();
    expect(navigateSpy).not.toHaveBeenCalled();

    state.createResolve?.({ id: "m1" });
    await waitFor(() =>
      expect(navigateSpy).toHaveBeenCalledWith("/meeting/m1", {
        state: { autoStart: true },
      }),
    );
    // ...and only then does it get out of the way.
    expect(screen.queryByText("Zoom")).not.toBeInTheDocument();
  });

  it("dismisses server-side rather than locally", () => {
    state.detected = { window_id: 7, app: "Zoom", title: "Zoom Meeting" };
    renderBanner();
    fireEvent.click(screen.getByText("Not now"));
    expect(dismissSpy).toHaveBeenCalled();
  });
});
