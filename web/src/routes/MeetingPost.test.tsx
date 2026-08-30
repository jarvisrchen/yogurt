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
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  // The header's delete button mounts DeleteMeetingConfirm, which
  // calls this hook on render even though these suites never click it.
  useDeleteMeeting: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

import { MeetingPost } from "./MeetingPost";

const MEETING_ID = "019f10a8-861c-7c61-b10f-cdf424b02b4d";

function mockMeetingFetch(row: Record<string, unknown>) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const u = String(url);
      if (u.includes(`/api/meetings/${MEETING_ID}`)) {
        if (init?.method === "POST") {
          return new Response(
            JSON.stringify({
              enriched_md: "## regenerated",
              notes_file: "/tmp/notes.md",
              too_short: false,
              llm_model: "gpt-5-mini",
            }),
            { status: 200 },
          );
        }
        return new Response(JSON.stringify(row), { status: 200 });
      }
      return new Response("{}", { status: 200 });
    }),
  );
}

function renderPost(routerState?: Record<string, unknown>) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter
        initialEntries={[
          { pathname: `/meeting/${MEETING_ID}/post`, state: routerState },
        ]}
      >
        <Routes>
          <Route path="/meeting/:id/post" element={<MeetingPost />} />
          <Route
            path="/meeting/:id"
            element={<div data-testid="live-view" />}
          />
          <Route path="/" element={<div data-testid="library-view" />} />
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

  it("keeps raw notes separate and sends only them to Re-enhance", async () => {
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Standup",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "- private raw note",
      enriched_md: "## Enhanced summary",
      transcript_json: "[]",
      llm_model: "MiniMax-M3",
    });

    renderPost();
    await screen.findByText("Enhanced summary");
    expect(screen.getByText("Enhanced · MiniMax-M3")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "My notes" }));
    expect(await screen.findByText(/private raw note/)).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("re-enhance-button"));
    await waitFor(() => {
      const calls = vi.mocked(fetch).mock.calls;
      const post = calls.find(([, init]) => init?.method === "POST");
      expect(post).toBeDefined();
      expect(JSON.parse(String(post?.[1]?.body))).toMatchObject({
        notes_md: "- private raw note",
      });
    });
    // The header pill follows the model the re-enhance actually used.
    expect(await screen.findByText("Enhanced · gpt-5-mini")).toBeInTheDocument();
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

describe("MeetingPost — too-short meeting", () => {
  // Reproduces the docs/TODO.md "too short" case: `endMeeting()` navigates
  // here with `tooShort: true` in router state when the server's enhance
  // response reported `too_short` (empty notes + trivial transcript, see
  // `enhance.rs`'s `TOO_SHORT_TRANSCRIPT_WORDS` gate). The route must show
  // the brief state instead of the editor, then auto-return to the library.
  beforeEach(() => {
    state.active = null;
    patchMock.mockClear();
  });

  it("shows a 'Meeting too short' state instead of the editor, then returns to the library", async () => {
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Accidental tap",
      started_at: 1000,
      ended_at: 1500,
      notes_md: "",
      enriched_md: null,
      transcript_json: "[]",
    });

    renderPost({ tooShort: true });

    expect(screen.getByTestId("meeting-too-short")).toBeInTheDocument();
    expect(screen.queryByTestId("meeting-post-route")).toBeNull();

    await waitFor(
      () => {
        expect(screen.getByTestId("library-view")).toBeInTheDocument();
      },
      { timeout: 3000 },
    );
  });
});
