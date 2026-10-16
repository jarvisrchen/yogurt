/**
 * TranscriptDock and AskExperience own WS connections covered by their own tests, so they are stubbed.
 * MicDevicePicker is stubbed with a prop-recording marker to assert its mount state and `recording` prop.
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
// Stubbed like MicDevicePicker above; MicMuteToggle.test.tsx covers its wiring.
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
vi.mock("../components/EchoTestButton", () => ({
  EchoTestButton: () => <div data-testid="echo-test-button" />,
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
  bearerFetch: (input: string, init?: RequestInit) =>
    fetch(input, {
      ...init,
      headers: { ...(init?.headers as object), Authorization: "Bearer test-token" },
    }),
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
  // Backs the mocked `useSettings`: the live header shows the active provider's model.
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
    auto_stop_at?: number | null;
  } | null,
  // Mirrors `general.audio_echo_feature`, off by default.
  audioEchoFeature: false,
}));

const keepRecordingSpy = vi.hoisted(() => vi.fn());

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
  useKeepRecording: () => ({ mutate: keepRecordingSpy, isPending: false }),
  // MeetingLabels (mounted in the header) needs this hook to exist.
  useSetMeetingLabels: () => ({ mutate: vi.fn() }),
}));

vi.mock("../lib/api/settings", () => ({
  useSettings: () => ({
    data: {
      providers: state.providers,
      general: { audio_echo_feature: state.audioEchoFeature },
    },
  }),
}));

import { Meeting } from "./Meeting";
import { activeRecordingKey, detectedMeetingKey } from "../lib/api/meetings";

// A probe div can't assert on the state passed to `navigate`, so stamp it into a data attribute.
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
  const tree = () => (
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
    </QueryClientProvider>
  );
  const utils = render(tree());
  return { qc, ...utils, refresh: () => utils.rerender(tree()) };
}

describe("Meeting — auto-start on '+ New meeting'", () => {
  beforeEach(() => {
    state.meetingRow = undefined;
    state.activeRecording = null;
    vi.clearAllMocks();
  });

  // The real flow POSTs via `useCreateMeeting` and navigates to `/meeting/:id` with a
  // real id and `autoStart`. It never reaches the `/meeting/new` bootstrap route.
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

    // Stop ran (so the server flushed its transcript) before navigating.
    expect(
      fetchMock.mock.calls.some(([u]) => String(u) === "/api/meetings/meeting-3/stop"),
    ).toBe(true);
    // Notes go to MeetingPost via router state, which fires the POST itself.
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

  // MeetingPost bounces back here while the cached active-recording value still names
  // this meeting (5 s refetchInterval), so Stop must clear the cache itself. The mocked
  // `useActiveRecording` reads module state, so only the cache write is asserted.
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
    // The `enrichedMarkdown` swap doesn't fire onChange, so hydration must sync `notesMd`
    // or End meeting would send "" and clobber real notes.
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
    // The chip reflects the server's `stt_engine`, stamped at start. Settings only apply at the next start.
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
    // A new meeting has no `llm_model` stamp yet; the header shows the model that will fuse the notes.
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
  // Stopped: Stop clicked but not End meeting. Only End meeting should clear the picker and chip.
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
    // Always mounted (disabled while not recording); see the `data-recording` flip below.
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
    // Still mounted (disabled) while stopped: a core action should stay findable.
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

describe("Meeting - echo column gated behind AUD-13 feature toggle", () => {
  beforeEach(() => {
    state.meetingRow = undefined;
    state.activeRecording = null;
    state.audioEchoFeature = false;
    vi.clearAllMocks();
  });

  it("omits the Echo to column when the mic-echo feature is off (default)", async () => {
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
      expect(screen.getByTestId("mic-picker")).toBeInTheDocument();
    });
    expect(screen.queryByText("Echo to")).toBeNull();
    expect(screen.queryByTestId("echo-picker")).toBeNull();

    vi.unstubAllGlobals();
  });

  it("shows the Echo to column when the mic-echo feature is on", async () => {
    state.audioEchoFeature = true;
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
      expect(screen.getByText("Echo to")).toBeInTheDocument();
    });
    expect(screen.getByTestId("echo-picker")).toBeInTheDocument();

    vi.unstubAllGlobals();
  });
});

describe("Meeting - server-side auto-stop", () => {
  const row = {
    id: "meeting-live",
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
  const live = {
    id: "meeting-live",
    title: "Weekly sync",
    started_at: 1000,
  };

  beforeEach(() => {
    state.meetingRow = row;
    state.activeRecording = null;
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ status: "stopped" }))),
    );
  });
  afterEach(() => vi.unstubAllGlobals());

  it("navigates to the post page with autoEnhance when the server ends the recording", async () => {
    state.activeRecording = live;
    const view = renderAt("/meeting/meeting-live");
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument(),
    );

    state.activeRecording = null;
    view.refresh();

    await waitFor(() =>
      expect(screen.getByTestId("post-view")).toBeInTheDocument(),
    );
    const navState = JSON.parse(
      screen.getByTestId("post-view").getAttribute("data-state") ?? "null",
    );
    expect(navState).toMatchObject({ autoEnhance: { title: "Weekly sync" } });
  });

  it("does not navigate when a manual Stop is followed by Start", async () => {
    state.activeRecording = live;
    const view = renderAt("/meeting/meeting-live");
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument(),
    );

    await act(async () => {
      screen.getByRole("button", { name: /stop recording/i }).click();
    });
    state.activeRecording = null;
    view.refresh();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /^start recording$/i }),
      ).toBeInTheDocument(),
    );

    await act(async () => {
      screen.getByRole("button", { name: /^start recording$/i }).click();
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument(),
    );
    view.refresh();
    expect(screen.queryByTestId("post-view")).toBeNull();
  });

  it("does not treat a poll that predates the recording as a server stop", async () => {
    state.activeRecording = null;
    const view = renderAt("/meeting/meeting-live", { autoStart: true });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument(),
    );
    view.refresh();
    expect(screen.queryByTestId("post-view")).toBeNull();
  });

  it("shows the silence warning with a countdown and Keep recording", async () => {
    state.activeRecording = { ...live, auto_stop_at: Date.now() + 42_000 };
    renderAt("/meeting/meeting-live");

    await waitFor(() =>
      expect(screen.getByText(/No audio for a few minutes/)).toBeInTheDocument(),
    );
    expect(screen.getByText(/0:4[12]/)).toBeInTheDocument();
    act(() => {
      screen.getByRole("button", { name: "Keep recording" }).click();
    });
    expect(keepRecordingSpy).toHaveBeenCalledWith("meeting-live");
  });

  it("shows no warning while auto_stop_at is null", async () => {
    state.activeRecording = { ...live, auto_stop_at: null };
    renderAt("/meeting/meeting-live");
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /stop recording/i }),
      ).toBeInTheDocument(),
    );
    expect(screen.queryByText(/No audio for a few minutes/)).toBeNull();
  });
});
