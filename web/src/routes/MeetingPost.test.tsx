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
  bearerFetch: (input: string, init?: RequestInit) =>
    fetch(input, {
      ...init,
      headers: { ...(init?.headers as object), Authorization: "Bearer test-token" },
    }),
}));
// Controllable mock for the streaming-preview tests; the default is no streaming activity.
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
  useSetArchived: () => ({ mutate: vi.fn(), isPending: false }),
}));

import { MeetingPost } from "./MeetingPost";

const MEETING_ID = "019f10a8-861c-7c61-b10f-cdf424b02b4d";

const TEMPLATES = [
  { id: "general", name: "General", when: "nothing else fits" },
  { id: "standup", name: "Standup", when: "a daily standup" },
  { id: "interview", name: "Interview", when: "a job interview" },
];

function mockMeetingFetch(row: Record<string, unknown>) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const u = String(url);
      if (u.includes("/api/templates")) {
        return new Response(JSON.stringify(TEMPLATES), { status: 200 });
      }
      if (u.includes(`/api/meetings/${MEETING_ID}`)) {
        if (init?.method === "POST") {
          // Echo the requested format back the way the server does when
          // one is forced; "standup" stands in for auto-detection.
          const template =
            (JSON.parse(String(init.body)) as { template?: string })
              .template ?? "standup";
          return new Response(
            JSON.stringify({
              enriched_md: "## regenerated",
              notes_file: "/tmp/notes.md",
              too_short: false,
              llm_model: "gpt-5-mini",
              template,
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

  describe("imported from Granola", () => {
    const base = {
      id: MEETING_ID,
      title: "Synthetic sync",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "",
      enriched_md: "## Granola summary",
      transcript_json: "[]",
    };
    const imported = {
      ...base,
      source: "granola",
      source_json: {
        owner: { name: "Alex Example", email: "a@example.com", is_user: false },
        web_url: "https://notes.granola.ai/d/example",
      },
    };
    const posts = () =>
      vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method === "POST");

    it("shows the provenance line only for an imported meeting", async () => {
      mockMeetingFetch(imported);
      const first = renderPost();
      expect(await screen.findByTestId("imported-from")).toHaveTextContent(
        "Captured by Alex Example",
      );
      first.unmount();

      mockMeetingFetch(base);
      renderPost();
      await screen.findByText("Granola summary");
      expect(screen.queryByTestId("imported-from")).toBeNull();
    });

    it("asks before enhancing an imported meeting and Cancel does not enhance", async () => {
      mockMeetingFetch(imported);
      renderPost();
      await screen.findByTestId("imported-from");

      fireEvent.click(screen.getByTestId("re-enhance-button"));
      expect(screen.getByTestId("re-enhance-confirm")).toHaveTextContent(
        "This replaces the notes imported from Granola.",
      );
      expect(posts()).toHaveLength(0);

      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      expect(screen.queryByTestId("re-enhance-confirm")).toBeNull();
      expect(posts()).toHaveLength(0);

      fireEvent.click(screen.getByTestId("re-enhance-button"));
      fireEvent.click(screen.getByRole("button", { name: "Replace" }));
      await waitFor(() => expect(posts()).toHaveLength(1));
    });

    it("enhances a native meeting with no confirmation", async () => {
      mockMeetingFetch(base);
      renderPost();
      await screen.findByText("Granola summary");
      fireEvent.click(screen.getByTestId("re-enhance-button"));
      expect(screen.queryByTestId("re-enhance-confirm")).toBeNull();
      await waitFor(() => expect(posts()).toHaveLength(1));
    });
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
    expect(screen.getByTestId("llm-pill")).toHaveTextContent(/^MiniMax-M3/);

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
    expect(await screen.findByTestId("llm-pill")).toHaveTextContent(/^gpt-5-mini/);
  });

  it("re-enhances with the picked note format and shows the one that was used", async () => {
    mockMeetingFetch({
      id: MEETING_ID,
      title: "Daily",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "- x",
      enriched_md: "## Enhanced summary",
      transcript_json: "[]",
      template: "general",
    });

    renderPost();
    await screen.findByText("Enhanced summary");
    const picker = (await screen.findByTestId("template-picker")) as HTMLSelectElement;
    await waitFor(() => expect(picker.value).toBe("general"));
    expect(screen.getByRole("option", { name: "Auto" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Interview" })).toBeInTheDocument();

    fireEvent.change(picker, { target: { value: "auto" } });
    fireEvent.click(screen.getByTestId("re-enhance-button"));
    await waitFor(() => {
      const post = vi
        .mocked(fetch)
        .mock.calls.find(([, init]) => init?.method === "POST");
      expect(post).toBeDefined();
      expect(JSON.parse(String(post?.[1]?.body))).not.toHaveProperty("template");
    });
    await waitFor(() => expect(picker.value).toBe("standup"));

    fireEvent.change(picker, { target: { value: "interview" } });
    fireEvent.click(screen.getByTestId("re-enhance-button"));
    await waitFor(() => {
      const posts = vi
        .mocked(fetch)
        .mock.calls.filter(([, init]) => init?.method === "POST");
      expect(posts).toHaveLength(2);
      expect(JSON.parse(String(posts[1]?.[1]?.body))).toMatchObject({
        template: "interview",
      });
    });
    expect(picker.value).toBe("interview");
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
    // A real server persists `enriched_md` before emitting `done`, so a GET after `done`
    // sees the new value. This mock mutates its row on the enhance POST to match.
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

    // Wait for the header model pill (from the POST response's `llm_model`), not editor text,
    // since the editor shows the raw preview until the WS also reports `done`.
    fireEvent.click(screen.getByTestId("re-enhance-button"));
    expect(await screen.findByTestId("llm-pill")).toHaveTextContent(/^gpt-5-mini/);

    wsState.phase = "done";
    wsState.text = null;
    view.rerenderSame();

    await waitFor(() => {
      expect(screen.getByTestId("yogurt-editor")).not.toHaveAttribute(
        "data-streaming",
      );
    });
    expect(await screen.findByText("regenerated")).toBeInTheDocument();
    const prosemirror = () =>
      screen.getByTestId("yogurt-editor").querySelector(".ProseMirror");
    expect(prosemirror()).toHaveAttribute("contenteditable", "false");
    fireEvent.click(screen.getByRole("tab", { name: "My notes" }));
    await waitFor(() => {
      expect(prosemirror()).toHaveAttribute("contenteditable", "true");
    });
  });

  it("holds the last streamed preview through the done-to-response gap, so there is no flash to the old/blank document", async () => {
    // The server emits `done` (clearing ws.text) before the POST response completes. A
    // deferred Response holds the POST open to reproduce that gap deterministically.
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

    wsState.phase = "done";
    wsState.text = null;
    view.rerenderSame();

    expect(screen.getByTestId("yogurt-editor")).toHaveAttribute(
      "data-streaming",
      "",
    );
    expect(screen.getByText(/Streamed body/)).toBeInTheDocument();

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

    // Give a duplicate (StrictMode-style) POST and state flushes time to land before asserting exactly one.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    const postCalls = vi
      .mocked(fetch)
      .mock.calls.filter(([, init]) => init?.method === "POST");
    expect(postCalls).toHaveLength(1);
  });

  it("renders the too-short screen when the autoEnhance POST reports too_short, then returns to the library", async () => {
    // `too_short` from the autoEnhance POST shows the brief state instead of the editor,
    // then returns to the library.
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
