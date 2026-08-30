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
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
// Stateful, test-controllable mock for the streaming-preview tests below —
// the default matches the old static shape (no streaming activity) so every
// pre-existing test in this file is unaffected.
const wsState = vi.hoisted(() => ({
  phase: null as "sending" | "streaming" | "done" | "error" | null,
  chars: null as number | null,
  text: null as string | null,
  errorMessage: null as string | null,
}));
vi.mock("../lib/ws", () => ({
  useEnhanceProgress: () => ({
    enhancing: wsState.phase === "sending" || wsState.phase === "streaming",
    phase: wsState.phase,
    chars: wsState.chars,
    text: wsState.text,
    errorMessage: wsState.errorMessage,
  }),
  storedSegmentToEvent: (s: unknown) => s,
}));

const state = vi.hoisted(() => ({
  active: null as { id: string; title: string; started_at: number } | null,
}));

const patchMock = vi.hoisted(() => vi.fn().mockResolvedValue({}));
vi.mock("../lib/api/meetings", () => ({
  meetingsApi: { patch: patchMock },
  meetingsKey: ["meetings"],
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

// Shared `qc` (rather than one per `buildTree` call) so a `rerender` with
// the same tree doesn't reset TanStack Query state mid-test.
function buildTree(qc: QueryClient, routerState?: Record<string, unknown>) {
  return (
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
    </QueryClientProvider>
  );
}

function renderPost(routerState?: Record<string, unknown>) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(buildTree(qc, routerState));
  return {
    ...view,
    // Streaming tests drive `wsState` directly (outside React) and need to
    // force a re-render with the SAME tree to pick up the new mock return
    // value - `rerenderSame` hides the qc/routerState bookkeeping.
    rerenderSame: () => view.rerender(buildTree(qc, routerState)),
  };
}


// Applies to every describe block in this file so the streaming-preview
// suites below don't leak `wsState` into unrelated tests.
beforeEach(() => {
  wsState.phase = null;
  wsState.chars = null;
  wsState.text = null;
  wsState.errorMessage = null;
});

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
    expect(screen.getByText("MiniMax-M3")).toBeInTheDocument();

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
    expect(await screen.findByText("gpt-5-mini")).toBeInTheDocument();
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

describe("MeetingPost — streaming enhance preview", () => {
  beforeEach(() => {
    state.active = null;
    patchMock.mockClear();
  });

  it("renders the raw preview read-only on the Enhanced tab while streaming", async () => {
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Standup",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "- x",
      enriched_md: "",
      transcript_json: "[]",
    });

    const view = renderPost();
    await waitFor(() => {
      expect(screen.getByTestId("meeting-post-route")).toBeInTheDocument();
    });

    wsState.phase = "streaming";
    wsState.chars = 42;
    wsState.text = "# Draft heading\n\nSome streamed content";
    view.rerenderSame();

    await waitFor(() => {
      expect(screen.getByTestId("yogurt-editor")).toHaveAttribute(
        "data-streaming",
        "",
      );
    });
    expect(
      await screen.findByText(/Some streamed content/),
    ).toBeInTheDocument();

    const prosemirror = screen
      .getByTestId("yogurt-editor")
      .querySelector(".ProseMirror");
    expect(prosemirror).toHaveAttribute("contenteditable", "false");
  });

  it("drops the preview once done + the enhance response land, keeping Enhanced read-only", async () => {
    // A real server persists `enriched_md` BEFORE emitting the `done` WS
    // frame (BL-5: persistence failure emits `error` instead), so a GET
    // that lands after `done` always sees the new value too — this mock
    // mutates its own row on the enhance POST to match that, instead of
    // always replaying the original fixture.
    let row: Record<string, unknown> = {
      id: MEETING_ID,
      title: "Standup",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "- x",
      enriched_md: "",
      transcript_json: "[]",
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
        const u = String(url);
        if (u.includes(`/api/meetings/${MEETING_ID}`)) {
          if (init?.method === "POST") {
            row = { ...row, enriched_md: "## regenerated", llm_model: "gpt-5-mini" };
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

    const view = renderPost();
    await waitFor(() => {
      expect(screen.getByTestId("meeting-post-route")).toBeInTheDocument();
    });

    wsState.phase = "streaming";
    wsState.text = "# Draft";
    view.rerenderSame();
    await waitFor(() => {
      expect(screen.getByTestId("yogurt-editor")).toHaveAttribute(
        "data-streaming",
        "",
      );
    });

    // The enhance POST resolving is what actually replaces the document -
    // the Re-enhance button drives the same POST path a real enhance
    // completion would. Wait for the header model pill (set from the same
    // response's `llm_model`) rather than editor text, since the editor
    // still shows the raw preview until the WS also reports `done` below.
    fireEvent.click(screen.getByTestId("re-enhance-button"));
    expect(await screen.findByText("gpt-5-mini")).toBeInTheDocument();

    wsState.phase = "done";
    wsState.text = null;
    view.rerenderSame();

    await waitFor(() => {
      expect(screen.getByTestId("yogurt-editor")).not.toHaveAttribute(
        "data-streaming",
      );
    });
    expect(await screen.findByText("regenerated")).toBeInTheDocument();
    // The Enhanced document is read-only even after the preview settles;
    // only My notes takes typing.
    const prosemirror = () =>
      screen.getByTestId("yogurt-editor").querySelector(".ProseMirror");
    expect(prosemirror()).toHaveAttribute("contenteditable", "false");
    fireEvent.click(screen.getByRole("tab", { name: "My notes" }));
    await waitFor(() => {
      expect(prosemirror()).toHaveAttribute("contenteditable", "true");
    });
  });

  it("holds the last streamed preview through the done-to-response gap, so there is no flash to the old/blank document", async () => {
    // The server persists `enriched_md` and emits `done` (clearing ws.text)
    // BEFORE the enhance POST's HTTP response actually completes — a
    // deferred Response lets this test hold the POST open to reproduce
    // that gap deterministically.
    let resolvePost: (res: Response) => void = () => {};
    const postPromise = new Promise<Response>((resolve) => {
      resolvePost = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
        const u = String(url);
        if (u.includes(`/api/meetings/${MEETING_ID}`)) {
          if (init?.method === "POST") return postPromise;
          return new Response(
            JSON.stringify({
              id: MEETING_ID,
              title: "Standup",
              started_at: 1000,
              ended_at: 2000,
              notes_md: "- x",
              enriched_md: "",
              transcript_json: "[]",
            }),
            { status: 200 },
          );
        }
        return new Response("{}", { status: 200 });
      }),
    );

    const view = renderPost();
    await waitFor(() => {
      expect(screen.getByTestId("meeting-post-route")).toBeInTheDocument();
    });

    wsState.phase = "streaming";
    wsState.text = "# Draft heading\n\nStreamed body";
    view.rerenderSame();
    await waitFor(() => {
      expect(screen.getByTestId("yogurt-editor")).toHaveAttribute(
        "data-streaming",
        "",
      );
    });

    fireEvent.click(screen.getByTestId("re-enhance-button"));

    // Server reports `done` (clearing ws.text) while the POST is still
    // pending.
    wsState.phase = "done";
    wsState.text = null;
    view.rerenderSame();

    // No flash: the held preview keeps showing, wrapper stays streaming.
    expect(screen.getByTestId("yogurt-editor")).toHaveAttribute(
      "data-streaming",
      "",
    );
    expect(screen.getByText(/Streamed body/)).toBeInTheDocument();

    // Now the POST resolves with the final document.
    await act(async () => {
      resolvePost(
        new Response(
          JSON.stringify({
            enriched_md: "## regenerated final",
            notes_file: "/tmp/notes.md",
            too_short: false,
            llm_model: "gpt-5-mini",
          }),
          { status: 200 },
        ),
      );
    });

    await waitFor(() => {
      expect(screen.getByTestId("yogurt-editor")).not.toHaveAttribute(
        "data-streaming",
      );
    });
    expect(await screen.findByText("regenerated final")).toBeInTheDocument();
  });
});

describe("MeetingPost — auto-enhance on arrival", () => {
  beforeEach(() => {
    state.active = null;
    patchMock.mockClear();
  });

  it("fires exactly one enhance POST from location.state.autoEnhance with the given notes_md and title", async () => {
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Weekly sync",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "",
      enriched_md: null,
      transcript_json: "[]",
    });

    renderPost({
      autoEnhance: { notes_md: "- raw notes from meeting", title: "Weekly sync" },
    });

    await waitFor(() => {
      const post = vi
        .mocked(fetch)
        .mock.calls.find(([, init]) => init?.method === "POST");
      expect(post).toBeDefined();
      expect(JSON.parse(String(post?.[1]?.body))).toMatchObject({
        notes_md: "- raw notes from meeting",
        title: "Weekly sync",
        transcript_json: "[]",
      });
    });

    // Give any duplicate (StrictMode-style double-effect) POST a chance to
    // fire, and the resolved response's state updates a chance to flush,
    // before asserting there's exactly one.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    const postCalls = vi
      .mocked(fetch)
      .mock.calls.filter(([, init]) => init?.method === "POST");
    expect(postCalls).toHaveLength(1);
  });

  it("renders the too-short screen when the autoEnhance POST reports too_short, then returns to the library", async () => {
    // Reproduces the docs/TODO.md "too short" case: the enhance POST
    // (fired from `location.state.autoEnhance`) reports `too_short` (empty
    // notes + trivial transcript, see `enhance.rs`'s
    // `TOO_SHORT_TRANSCRIPT_WORDS` gate). The route must show the brief
    // state instead of the editor, then auto-return to the library.
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
        const u = String(url);
        if (u.includes(`/api/meetings/${MEETING_ID}`) && init?.method === "POST") {
          return new Response(
            JSON.stringify({ enriched_md: "", notes_file: "", too_short: true }),
            { status: 200 },
          );
        }
        if (u.includes(`/api/meetings/${MEETING_ID}`)) {
          return new Response(
            JSON.stringify({
              id: MEETING_ID,
              title: "Accidental tap",
              started_at: 1000,
              ended_at: 1500,
              notes_md: "",
              enriched_md: null,
              transcript_json: "[]",
            }),
            { status: 200 },
          );
        }
        return new Response("{}", { status: 200 });
      }),
    );

    renderPost({ autoEnhance: { notes_md: "", title: "Accidental tap" } });

    await waitFor(() => {
      expect(screen.getByTestId("meeting-too-short")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("meeting-post-route")).toBeNull();

    await waitFor(
      () => {
        expect(screen.getByTestId("library-view")).toBeInTheDocument();
      },
      { timeout: 3000 },
    );
  });
});
