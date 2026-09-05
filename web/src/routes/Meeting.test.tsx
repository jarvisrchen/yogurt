/**
 * Meeting.tsx — hero in-meeting flow coverage.
 *
 * Covers three of the biggest behavior changes:
 *   1. "+ New meeting" auto-starts recording on mount (task NOTES-01) —
 *      no more create-then-hunt-for-Start.
 *   2. End meeting stops recording BEFORE enhancing, sends the always-
 *      empty `transcript_json: "[]"` (server prefers its own stored
 *      transcript), omits started/ended timestamps, and navigates to the
 *      post-meeting view (task NOTES-07).
 *   3. The mic picker and STT engine chip stay visible once a meeting is
 *      stopped (but not ended) — only End meeting should clear the header
 *      down to just the buttons. The picker's own read/write wiring
 *      (hot-swap vs `settings.audio_input_device` patch) is covered by
 *      `MicDevicePicker.test.tsx`; here we only assert Meeting.tsx keeps it
 *      mounted and passes it the right `recording` flag.
 *
 * TranscriptDock / AskExperience are stubbed out — they own their own WS
 * connections and are covered by their own test files; pulling them in
 * here would just add unrelated network/WS noise to a route-logic test.
 * MicDevicePicker is stubbed with a prop-recording marker (not `() => null`)
 * so this file can assert on its mount state and `recording` prop without
 * pulling in its real `audioApi`/`settingsApi` network calls.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import { MemoryRouter, Routes, Route, useLocation } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../components/TranscriptDock", () => ({
  TranscriptDock: () => null,
}));
vi.mock("../components/AskExperience", () => ({
  AskExperience: () => null,
}));
vi.mock("../components/MicDevicePicker", () => ({
  MicDevicePicker: ({ recording }: { meetingId: string; recording: boolean }) => (
    <div data-testid="mic-picker" data-recording={String(recording)} />
  ),
}));
// AUD-6: stubbed the same way as MicDevicePicker above — this file only
// needs to assert Meeting.tsx keeps it mounted and passes it the right
// `recording` flag, not its own `audioApi.setMicMuted` wiring (covered by
// MicMuteToggle.test.tsx).
vi.mock("../components/MicMuteToggle", () => ({
  MicMuteToggle: ({ meetingId, recording }: { meetingId: string; recording: boolean }) => (
    <div
      data-testid="mic-mute-toggle"
      data-meeting-id={meetingId}
      data-recording={String(recording)}
    />
  ),
}));
vi.mock("../components/EchoDevicePicker", () => ({
  EchoDevicePicker: ({ recording }: { meetingId: string; recording: boolean }) => (
    <div data-testid="echo-picker" data-recording={String(recording)} />
  ),
}));
vi.mock("../components/MicEchoToggle", () => ({
  MicEchoToggle: ({ meetingId, recording }: { meetingId: string; recording: boolean }) => (
    <div
      data-testid="mic-echo-toggle"
      data-meeting-id={meetingId}
      data-recording={String(recording)}
    />
  ),
}));
vi.mock("../lib/session", () => ({
  ensureSessionToken: () => Promise.resolve("test-token"),
}));
vi.mock("../lib/ws", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/ws")>();
  return {
    ...actual,
    useSttError: () => ({ message: null, dismiss: () => {} }),
  };
});

const state = vi.hoisted(() => ({
  meetingRow: undefined as unknown,
  // Backs the mocked `useSettings` below — the live header reads the
  // active provider's model for the "will enhance with" LLM pill.
  providers: [{ id: "p1", is_active: true, model: "gpt-5-mini" }] as Array<{
    id: string;
    is_active: boolean;
    model: string;
  }>,
  activeRecording: null as {
    id: string;
    title: string;
    started_at: number;
    stt?: "cloud" | "local";
  } | null,
}));

vi.mock("../lib/api/meetings", () => ({
  meetingKey: (id: string) => ["meetings", id],
  activeRecordingKey: ["meetings", "active"],
  detectedMeetingKey: ["meetings", "detected"],
  meetingsApi: {
    patch: vi.fn().mockResolvedValue({}),
  },
  useMeeting: () => ({ data: state.meetingRow, isError: false }),
  // InlineTitle (real, unmocked — it's the reused library rename flow)
  // needs this the moment `meetingId` is known and it mounts.
  useUpdateMeetingTitle: () => ({ mutate: vi.fn() }),
  useActiveRecording: () => ({
    data: state.activeRecording,
    isLoading: false,
    error: null,
  }),
  // MeetingLabels (mounted in the header) needs this hook to exist.
  useSetMeetingLabels: () => ({ mutate: vi.fn() }),
}));

vi.mock("../lib/api/settings", () => ({
  useSettings: () => ({ data: { providers: state.providers } }),
}));

import { Meeting } from "./Meeting";
import { activeRecordingKey, detectedMeetingKey } from "../lib/api/meetings";

// Task 2 (End meeting navigates immediately with `state: { autoEnhance }`
// instead of awaiting a POST first): a plain probe div can't assert on the
// state passed to `navigate`, so stamp it into a data attribute via
// `useLocation` for the "End meeting flow" tests below to read.
function PostViewProbe() {
  const location = useLocation();
  return (
    <div data-testid="post-view" data-state={JSON.stringify(location.state)} />
  );
}

function renderAt(
  initialPath: string,
  routerState?: Record<string, unknown>,
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return {
    qc,
    ...render(
    <QueryClientProvider client={qc}>
      <MemoryRouter
        initialEntries={[{ pathname: initialPath, state: routerState }]}
      >
        <Routes>
          <Route path="/meeting/new" element={<Meeting />} />
          <Route path="/meeting/:id" element={<Meeting />} />
          <Route path="/meeting/:id/post" element={<PostViewProbe />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
    ),
  };
}

describe("Meeting — auto-start on '+ New meeting'", () => {
  beforeEach(() => {
    state.meetingRow = undefined;
    state.activeRecording = null;
    vi.clearAllMocks();
  });

  // The real "+ New meeting" flow (Sidebar.tsx / Library.tsx's ⌘N handler)
  // already POSTs /api/meetings itself via `useCreateMeeting`, then
  // navigates straight to `/meeting/:id` with a REAL id and
  // `state: { autoStart: true }` — it never hits the `/meeting/new`
  // bootstrap route (that route has no `:id` segment at all, so
  // `routeId` can never literally equal `"new"` when reached through the
  // real UI). This is the path that actually matters in production.
  it("POSTs /start automatically on mount when navigated here with autoStart state", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/meetings/meeting-1/start") {
        return new Response(JSON.stringify({ status: "started" }), {
          status: 200,
        });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    const { qc } = renderAt("/meeting/meeting-1", { autoStart: true });
    // Seed so invalidation is observable.
    qc.setQueryData(detectedMeetingKey, null);
    qc.setQueryData(activeRecordingKey, null);

    await waitFor(() => {
      expect(
        fetchMock.mock.calls.some(
          ([u]) => String(u) === "/api/meetings/meeting-1/start",
        ),
      ).toBe(true);
    });

    // Recording started — the toolbar swaps "Start recording" for "Stop
    // recording" without the user clicking anything.
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByRole("button", { name: /^start recording$/i }),
    ).toBeNull();

    expect(qc.getQueryState(detectedMeetingKey)?.isInvalidated).toBe(true);
    expect(qc.getQueryState(activeRecordingKey)?.isInvalidated).toBe(true);

    vi.unstubAllGlobals();
  });

  it("does NOT auto-start when the meeting is visited without autoStart state (e.g. a resumed in-progress meeting)", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      throw new Error(`unexpected fetch: ${String(input)}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderAt("/meeting/meeting-1");

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument();
    });
    expect(
      fetchMock.mock.calls.some((c) => String(c[0]).endsWith("/start")),
    ).toBe(false);

    vi.unstubAllGlobals();
  });

  it("shows the backend's error verbatim plus an Open Settings link when auto-start fails", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/meetings/meeting-2/start") {
        return new Response(
          JSON.stringify({ error: "no Deepgram API key configured" }),
          { status: 400 },
        );
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderAt("/meeting/meeting-2", { autoStart: true });

    await waitFor(() => {
      expect(
        screen.getByText("no Deepgram API key configured"),
      ).toBeInTheDocument();
    });
    expect(screen.getByRole("link", { name: /open settings/i })).toHaveAttribute(
      "href",
      "/settings",
    );

    vi.unstubAllGlobals();
  });
});

describe("Meeting — End meeting flow", () => {
  beforeEach(() => {
    state.meetingRow = {
      id: "meeting-3",
      title: "Weekly sync",
      started_at: 1000,
      ended_at: null,
      notes_md: "",
      enriched_md: null,
      transcript_json: "[]",
      starred: false,
      created_at: "",
      updated_at: "",
    };
    state.activeRecording = null;
    vi.clearAllMocks();
  });

  it("stops recording, then navigates to /post with autoEnhance state instead of awaiting enhance itself", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/meetings/meeting-3/start") {
        return new Response(JSON.stringify({ status: "started" }), { status: 200 });
      }
      if (url === "/api/meetings/meeting-3/stop") {
        return new Response(JSON.stringify({ status: "stopped" }), { status: 200 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderAt("/meeting/meeting-3");

    // Recording must be active for "stop before enhance" to be meaningful.
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument();
    });
    act(() => {
      screen.getByRole("button", { name: /^start recording$/i }).click();
    });
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument();
    });

    await act(async () => {
      screen.getByTestId("end-meeting").click();
    });

    await waitFor(() => {
      expect(screen.getByTestId("post-view")).toBeInTheDocument();
    });

    // stop happened (so the server flushed its transcript) before navigating.
    expect(
      fetchMock.mock.calls.some(([u]) => String(u) === "/api/meetings/meeting-3/stop"),
    ).toBe(true);
    // Task 2: Meeting.tsx no longer calls /enhance itself — it hands the raw
    // notes to MeetingPost via router state so the post view can fire the
    // POST and stream the preview immediately.
    expect(
      fetchMock.mock.calls.some(([u]) => String(u).includes("/enhance")),
    ).toBe(false);

    const navState = JSON.parse(
      screen.getByTestId("post-view").getAttribute("data-state") ?? "null",
    );
    expect(navState).toMatchObject({
      autoEnhance: { notes_md: "", title: "Weekly sync" },
    });

    vi.unstubAllGlobals();
  });

  // Regression: the first "End meeting" click used to be a dead click while
  // still recording. `MeetingPost` bounces back to this route whenever the
  // cached active-recording value still names this meeting, and that query
  // is on a 5 s `refetchInterval` — so navigating to /post immediately
  // after /stop landed on a stale cache and got redirected right back. The
  // user had to click End a second time (or Stop first) to get through.
  // Stopping must therefore clear the cache itself rather than wait for the
  // poll. Asserting the cache write is what makes this fail if the line
  // goes away; the mocked `useActiveRecording` reads module state, not the
  // client, so the bounce itself can't be exercised from this file.
  it("clears the cached active recording on stop, so End meeting doesn't bounce off a stale poll", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/meetings/meeting-3/start") {
        return new Response(JSON.stringify({ status: "started" }), { status: 200 });
      }
      if (url === "/api/meetings/meeting-3/stop") {
        return new Response(JSON.stringify({ status: "stopped" }), { status: 200 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    const { qc } = renderAt("/meeting/meeting-3");

    // Seed the cache the way the 5 s poll would have while recording.
    qc.setQueryData(["meetings", "active"], { id: "meeting-3", title: "Weekly sync", started_at: 1000 });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument();
    });
    act(() => {
      screen.getByRole("button", { name: /^start recording$/i }).click();
    });
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument();
    });

    await act(async () => {
      screen.getByTestId("end-meeting").click();
    });

    await waitFor(() => {
      expect(screen.getByTestId("post-view")).toBeInTheDocument();
    });
    expect(qc.getQueryData(["meetings", "active"])).toBeNull();

    vi.unstubAllGlobals();
  });

  it("sends hydrated prior notes_md via autoEnhance state, not an empty string, when End is clicked before any new keystroke", async () => {
    // Regression coverage for task NOTES-02's hydration bug: YogurtEditor's
    // `enrichedMarkdown` content swap intentionally does NOT fire onChange,
    // so without explicitly syncing `notesMd` state on hydration, a user
    // who reopens an in-progress meeting (prior notes already on the
    // server) and immediately hits End meeting would silently send "" and
    // clobber their real notes.
    state.meetingRow = {
      ...(state.meetingRow as Record<string, unknown>),
      notes_md: "existing notes from before refresh",
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/meetings/meeting-3/stop") {
          return new Response(JSON.stringify({ status: "stopped" }), { status: 200 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderAt("/meeting/meeting-3");
    await waitFor(() => {
      expect(screen.getByTestId("end-meeting")).toBeInTheDocument();
    });

    await act(async () => {
      screen.getByTestId("end-meeting").click();
    });

    await waitFor(() => {
      expect(screen.getByTestId("post-view")).toBeInTheDocument();
    });
    const navState = JSON.parse(
      screen.getByTestId("post-view").getAttribute("data-state") ?? "null",
    );
    expect(navState).toMatchObject({
      autoEnhance: { notes_md: "existing notes from before refresh" },
    });

    vi.unstubAllGlobals();
  });
});

describe("Meeting — recovers recording state on return", () => {
  beforeEach(() => {
    state.meetingRow = undefined;
    state.activeRecording = null;
    vi.clearAllMocks();
  });

  it("shows Stop recording (not Start) when GET /api/meetings/active reports this meeting live, without POSTing /start again", async () => {
    state.activeRecording = {
      id: "meeting-live",
      title: "Weekly sync",
      started_at: Date.now(),
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      throw new Error(`unexpected fetch: ${String(input)}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderAt("/meeting/meeting-live");

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByRole("button", { name: /^start recording$/i }),
    ).toBeNull();
    expect(
      fetchMock.mock.calls.some((c) => String(c[0]).endsWith("/start")),
    ).toBe(false);
    // The stt_error banner must stay silent — recovery is a pure UI resync,
    // not something that fabricates a transcription error.
    expect(screen.queryByTestId("stt-error-banner")).toBeNull();

    vi.unstubAllGlobals();
  });

  it("renders a truthful engine chip next to the mic picker when the live recording reports stt", async () => {
    // The chip must reflect the server's `stt_engine` field (stamped once
    // `Registry::start` resolves), not the Settings page — settings only
    // apply at the *next* start, so this is the one honest source.
    state.activeRecording = {
      id: "meeting-live",
      title: "Weekly sync",
      started_at: Date.now(),
      stt: "local",
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        throw new Error(`unexpected fetch: ${String(input)}`);
      }),
    );

    renderAt("/meeting/meeting-live");

    await waitFor(() => {
      expect(screen.getByText("Local STT")).toBeInTheDocument();
    });
    expect(screen.queryByText("Cloud STT")).toBeNull();

    vi.unstubAllGlobals();
  });

  it("shows the active provider's model as a pending LLM pill on a fresh meeting", async () => {
    // A brand-new meeting has no `llm_model` stamp yet (enhance.rs writes
    // it only after a successful enhance), but the header should still say
    // which model *will* fuse the notes — parity with the STT pill.
    state.activeRecording = {
      id: "meeting-live",
      title: "Weekly sync",
      started_at: Date.now(),
      stt: "local",
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        throw new Error(`unexpected fetch: ${String(input)}`);
      }),
    );

    renderAt("/meeting/meeting-live");

    const pill = await screen.findByTestId("llm-pill");
    expect(pill).toHaveTextContent("gpt-5-mini");
    expect(pill).toHaveAttribute("title", "Will enhance with gpt-5-mini");

    vi.unstubAllGlobals();
  });

  it("omits the engine chip when the active recording hasn't reported stt yet", async () => {
    state.activeRecording = {
      id: "meeting-live",
      title: "Weekly sync",
      started_at: Date.now(),
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        throw new Error(`unexpected fetch: ${String(input)}`);
      }),
    );

    renderAt("/meeting/meeting-live");

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument();
    });
    expect(screen.queryByText("Local STT")).toBeNull();
    expect(screen.queryByText("Cloud STT")).toBeNull();

    vi.unstubAllGlobals();
  });

  it("shows Start recording when nothing is actively recording", async () => {
    state.activeRecording = null;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      throw new Error(`unexpected fetch: ${String(input)}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderAt("/meeting/meeting-idle");

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument();
    });
    expect(
      fetchMock.mock.calls.some((c) => String(c[0]).endsWith("/start")),
    ).toBe(false);

    vi.unstubAllGlobals();
  });
});

describe("Meeting — header stays put once stopped (meeting still open)", () => {
  // "Stopped" here means Stop recording was clicked but End meeting was
  // not — the meeting is still open on this route. Only End meeting (which
  // navigates away) should clear the mic picker / engine chip; Stop must
  // not.
  beforeEach(() => {
    state.meetingRow = {
      id: "meeting-4",
      title: "Weekly sync",
      started_at: 1000,
      ended_at: null,
      notes_md: "",
      enriched_md: null,
      transcript_json: "[]",
      starred: false,
      stt_engine: "cloud · nova-3",
      llm_model: null,
      created_at: "",
      updated_at: "",
    };
    state.activeRecording = {
      id: "meeting-4",
      title: "Weekly sync",
      started_at: Date.now(),
    };
    vi.clearAllMocks();
  });

  it("keeps the mic picker mounted after Stop recording, flipping its recording prop to false", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/meetings/meeting-4/stop") {
          return new Response(JSON.stringify({ status: "stopped" }), { status: 200 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderAt("/meeting/meeting-4");

    await waitFor(() => {
      expect(screen.getByTestId("mic-picker")).toHaveAttribute(
        "data-recording",
        "true",
      );
    });
    // AUD-6: always mounted (disabled, not gone, while not recording) — see
    // the `data-recording` flip below.
    expect(screen.getByTestId("mic-mute-toggle")).toHaveAttribute(
      "data-recording",
      "true",
    );

    await act(async () => {
      screen.getByRole("button", { name: /stop recording/i }).click();
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument();
    });
    // Gone from the DOM would mean the old `recording &&` gate regressed.
    expect(screen.getByTestId("mic-picker")).toHaveAttribute(
      "data-recording",
      "false",
    );
    expect(screen.getByTestId("mic-mute-toggle")).toHaveAttribute(
      "data-recording",
      "false",
    );

    vi.unstubAllGlobals();
  });

  it("shows the mic picker with recording=false for a meeting opened already-stopped", async () => {
    state.activeRecording = null;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        throw new Error(`unexpected fetch: ${String(input)}`);
      }),
    );

    renderAt("/meeting/meeting-4");

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument();
    });
    expect(screen.getByTestId("mic-picker")).toHaveAttribute(
      "data-recording",
      "false",
    );
    // AUD-6: still mounted (disabled) while stopped, not unmounted — a
    // core action should stay findable, not disappear.
    expect(screen.getByTestId("mic-mute-toggle")).toHaveAttribute(
      "data-recording",
      "false",
    );

    vi.unstubAllGlobals();
  });

  it("keeps the STT engine chip visible after Stop recording when the meeting has a stamped stt_engine", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/meetings/meeting-4/stop") {
          return new Response(JSON.stringify({ status: "stopped" }), { status: 200 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );

    renderAt("/meeting/meeting-4");

    await waitFor(() => {
      expect(screen.getByText("Cloud · nova-3")).toBeInTheDocument();
    });

    await act(async () => {
      screen.getByRole("button", { name: /stop recording/i }).click();
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument();
    });
    // The chip is meeting metadata (stamped at start), not a live status —
    // it must not vanish just because recording flipped to false.
    expect(screen.getByText("Cloud · nova-3")).toBeInTheDocument();

    vi.unstubAllGlobals();
  });
});
