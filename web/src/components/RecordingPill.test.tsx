import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter, Routes, Route } from "react-router";

const state = vi.hoisted(() => ({
  active: null as { id: string; title: string; started_at: number } | null,
}));

vi.mock("../lib/api/meetings", () => ({
  useActiveRecording: () => ({ data: state.active, isLoading: false, error: null }),
}));

const navigateSpy = vi.hoisted(() => vi.fn());
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>("react-router");
  return { ...actual, useNavigate: () => navigateSpy };
});

import { RecordingPill } from "./RecordingPill";

afterEach(() => {
  cleanup();
  state.active = null;
  navigateSpy.mockClear();
});

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="*" element={<RecordingPill />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("RecordingPill", () => {
  it("renders nothing when the API reports no active recording", () => {
    state.active = null;
    const { container } = renderAt("/");
    expect(container.firstChild).toBeNull();
  });

  it("shows the meeting title and navigates to the live view on click", () => {
    state.active = { id: "abc-123", title: "Standup", started_at: Date.now() };
    renderAt("/");

    const btn = screen.getByRole("button", { name: "Return to recording" });
    expect(btn).toHaveTextContent("Standup");

    fireEvent.click(btn);
    expect(navigateSpy).toHaveBeenCalledWith("/meeting/abc-123");
  });

  it("falls back to 'Recording' when the title is empty", () => {
    state.active = { id: "abc-123", title: "", started_at: Date.now() };
    renderAt("/");
    expect(
      screen.getByRole("button", { name: "Return to recording" }),
    ).toHaveTextContent("Recording");
  });

  it("hides itself on the live view for that same recording", () => {
    state.active = { id: "abc-123", title: "Standup", started_at: Date.now() };
    const { container } = renderAt("/meeting/abc-123");
    expect(container.firstChild).toBeNull();
  });

  it("still shows on the post-meeting view for that same recording", () => {
    state.active = { id: "abc-123", title: "Standup", started_at: Date.now() };
    renderAt("/meeting/abc-123/post");
    expect(
      screen.getByRole("button", { name: "Return to recording" }),
    ).toBeInTheDocument();
  });

  it("still shows on an unrelated meeting's page", () => {
    state.active = { id: "abc-123", title: "Standup", started_at: Date.now() };
    renderAt("/meeting/other-id");
    expect(
      screen.getByRole("button", { name: "Return to recording" }),
    ).toBeInTheDocument();
  });
});
