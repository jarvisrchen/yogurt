import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { routes } from "./router";

// The Library route (now mounted at "/") fetches /api/meetings on render.
// Stub the meetings api so the test doesn't hit a real backend.
vi.mock("./lib/api/meetings", async () => {
  const actual =
    await vi.importActual<typeof import("./lib/api/meetings")>(
      "./lib/api/meetings",
    );
  return {
    ...actual,
    useMeetings: () => ({ data: [], isLoading: false, error: null }),
    useCreateMeeting: () => ({
      mutateAsync: vi.fn(),
      isPending: false,
    }),
  };
});

// The Sidebar fetches /api/settings via useQuery. Stub the typed client.
vi.mock("./lib/api/settings", () => ({
  settingsApi: {
    get: vi.fn().mockResolvedValue({
      general: {
        port: 7878,
        open_browser_on_start: true,
        audio_input_device: "",
      },
      providers: [],
      presets: [],
    }),
  },
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
    // The Library hero greeting always says "Good {timeOfDay}, you" —
    // pick a stable substring.
    expect(
      await screen.findByRole("heading", { name: /Good \w+, you/i }),
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
});
