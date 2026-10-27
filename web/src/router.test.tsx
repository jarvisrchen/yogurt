import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { routes } from "./router";

vi.mock("./lib/api/meetings", async () => {
  const actual =
    await vi.importActual<typeof import("./lib/api/meetings")>(
      "./lib/api/meetings",
    );
  return {
    ...actual,
    useMeetings: () => ({
      data: { meetings: [], total: 0 },
      isLoading: false,
      error: null,
    }),
    useCreateMeeting: () => ({
      mutateAsync: vi.fn(),
      isPending: false,
    }),
    // RecordingPill (mounted globally by Shell) polls this on every route —
    // stub it to "nothing recording" so these route-smoke tests don't hit
    // the network or render an unrelated floating pill.
    useActiveRecording: () => ({ data: null, isLoading: false, error: null }),
  };
});

// Stub settings so the Shell's `useFirstRunRedirect` sees a set-up user.
vi.mock("./lib/api/settings", () => {
  const fixture = {
    general: {
      port: 7878,
      open_browser_on_start: true,
      audio_input_device: "",
        audio_echo_feature: false,
        audio_echo_output_device: "",
        audio_echo_enabled: false,
        audio_echo_buffer: 512,
      first_run_completed: true,
    },
    providers: [
      {
        id: "01HXXXXXXXXXXXXXXXXXXXXXXX",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        model: "gpt-4o-mini",
        is_active: true,
        created_at: 0,
        api_key_masked: "••••ABCD",
      },
    ],
    presets: [],
  };
  return {
    settingsApi: { get: vi.fn().mockResolvedValue(fixture) },
    settingsKey: ["settings"],
    useSettings: () => ({
      data: fixture,
      isLoading: false,
      error: null,
    }),
    useDataPaths: () => undefined,
    useSetFirstRunCompleted: () => ({
      mutateAsync: vi.fn().mockResolvedValue(fixture.general),
      isPending: false,
    }),
  };
});

// `granted=true` so the Shell never redirects.
vi.mock("./hooks/useScreenRecordingStatus", () => ({
  useScreenRecordingStatus: () => ({
    granted: true,
    status: "granted",
    isLoading: false,
    error: null,
  }),
}));

function makeQc(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
}

describe("router", () => {
  it("renders the Library at /", async () => {
    const qc = makeQc();
    const router = createMemoryRouter(routes, { initialEntries: ["/"] });
    render(
      <QueryClientProvider client={qc}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    // The Library hero greeting always says "Good {timeOfDay}" —
    // pick a stable substring.
    expect(
      await screen.findByRole("heading", {
        name: /Good (morning|afternoon|evening)/i,
      }),
    ).toBeInTheDocument();
  });

  it("renders the StyleGuide at /style-guide", async () => {
    const qc = makeQc();
    const router = createMemoryRouter(routes, {
      initialEntries: ["/style-guide"],
    });
    render(
      <QueryClientProvider client={qc}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    expect(
      await screen.findByRole("heading", { name: /style guide/i }),
    ).toBeInTheDocument();
  });

  it("renders the Welcome route at /welcome", async () => {
    const qc = makeQc();
    const router = createMemoryRouter(routes, {
      initialEntries: ["/welcome"],
    });
    render(
      <QueryClientProvider client={qc}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    expect(
      await screen.findByRole("heading", { name: /welcome to yogurt/i }),
    ).toBeInTheDocument();
  });
});
