/**
 * MeetingPost.tsx — live-recording redirect.
 *
 * A meeting that's still recording server-side must never render the
 * frozen post view: deep links, refresh, and the back button can all land
 * here for a meeting `GET /api/meetings/active` still reports as live.
 * When that's the case, redirect straight to the live capture surface
 * (`/meeting/:id`) instead — it has the Start/Stop controls and live
 * transcript this route doesn't.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../components/TranscriptDock", () => ({
  TranscriptDock: () => null,
}));
vi.mock("../components/AskExperience", () => ({
  AskExperience: () => null,
}));
vi.mock("../lib/session", () => ({
  ensureSessionToken: () => Promise.resolve("test-token"),
}));
vi.mock("../lib/ws", () => ({
  useEnhanceProgress: () => ({
    enhancing: false,
    chars: null,
    errorMessage: null,
  }),
  storedSegmentToEvent: (s: unknown) => s,
}));

const state = vi.hoisted(() => ({
  active: null as { id: string; title: string; started_at: number } | null,
}));

const patchMock = vi.hoisted(() => vi.fn().mockResolvedValue({}));
vi.mock("../lib/api/meetings", () => ({
  meetingsApi: { patch: patchMock },
  useMeeting: () => ({ data: undefined, isError: false }),
  useUpdateMeetingTitle: () => ({ mutate: vi.fn() }),
  useActiveRecording: () => ({ data: state.active, isLoading: false, error: null }),
  useSetMeetingLabels: () => ({ mutate: vi.fn() }),
}));

import { MeetingPost } from "./MeetingPost";

const MEETING_ID = "019f10a8-861c-7c61-b10f-cdf424b02b4d";

function mockMeetingFetch(row: Record<string, unknown>) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: RequestInfo | URL) => {
      const u = String(url);
      if (u.includes(`/api/meetings/${MEETING_ID}`)) {
        return new Response(JSON.stringify(row), { status: 200 });
      }
      return new Response("{}", { status: 200 });
    }),
  );
}

function renderPost() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/meeting/${MEETING_ID}/post`]}>
        <Routes>
          <Route path="/meeting/:id/post" element={<MeetingPost />} />
          <Route
            path="/meeting/:id"
            element={<div data-testid="live-view" />}
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("MeetingPost — live-recording redirect", () => {
  beforeEach(() => {
    state.active = null;
    patchMock.mockClear();
  });

  it("redirects to the live view when this meeting is the active recording", async () => {
    state.active = { id: MEETING_ID, title: "Standup", started_at: Date.now() };
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Standup",
      started_at: 1000,
      ended_at: null,
      notes_md: "- x",
      enriched_md: null,
      transcript_json: "[]",
    });

    renderPost();

    await waitFor(() => {
      expect(screen.getByTestId("live-view")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("meeting-post-route")).toBeNull();
  });

  it("renders normally when this meeting is not the active recording", async () => {
    state.active = null;
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Standup",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "- x",
      enriched_md: "## enriched",
      transcript_json: "[]",
    });

    renderPost();

    await waitFor(() => {
      expect(screen.getByTestId("meeting-post-route")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("live-view")).toBeNull();
  });

  it("does not redirect when a DIFFERENT meeting is the active recording", async () => {
    state.active = { id: "some-other-id", title: "Other", started_at: Date.now() };
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Standup",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "- x",
      enriched_md: "## enriched",
      transcript_json: "[]",
    });

    renderPost();

    await waitFor(() => {
      expect(screen.getByTestId("meeting-post-route")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("live-view")).toBeNull();
  });
});
