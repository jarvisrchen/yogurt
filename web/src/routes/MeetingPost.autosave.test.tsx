/**
 * MeetingPost.tsx — enriched-autosave data-loss regression.
 *
 * Reproduces the observed live bug: a post-view mount that merely LOADS a
 * meeting (user never touches the editor) must NEVER fire an enriched_md
 * PATCH — not on the debounce tick and not on unmount. Before the
 * userEdited gate, a stale mount flushed `enriched_md: ""` on unmount and
 * destroyed a real enhanced document.
 *
 * Also covers the empty-summary state: raw notes remain available in their
 * own document rather than being presented as generated output.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
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

const patchMock = vi.hoisted(() => vi.fn().mockResolvedValue({}));
vi.mock("../lib/api/meetings", () => ({
  meetingsApi: { patch: patchMock },
  useMeeting: () => ({ data: undefined, isError: false }),
  useUpdateMeetingTitle: () => ({ mutate: vi.fn() }),
  // This suite is about the autosave data-loss guard, not live-recording
  // redirect (see MeetingPost.test.tsx) — always report nothing recording.
  useActiveRecording: () => ({ data: null, isLoading: false, error: null }),
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
          <Route path="/" element={<div>library</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("MeetingPost enriched autosave data-loss guard", () => {
  beforeEach(() => {
    patchMock.mockClear();
  });

  it("never PATCHes when the user did not edit - not on debounce, not on unmount", async () => {
    mockMeetingFetch({
      id: MEETING_ID,
      title: "guarded",
      started_at: 1000,
      ended_at: 2000,
      notes_md: "- pricing",
      enriched_md: "## Real\n\n- content",
      transcript_json: "[]",
    });
    const view = renderPost();

    // Let hydration fully settle (fetch + state + editor swap).
    await waitFor(() => {
      expect(screen.getByText(/content/)).toBeInTheDocument();
    });

    // Sit past the 1s debounce window.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 1300));
    });
    // Unmount (the path that destroyed data before the fix).
    view.unmount();

    const enrichedPatches = patchMock.mock.calls.filter(
      ([, body]) => body && "enriched_md" in (body as object),
    );
    expect(enrichedPatches).toEqual([]);
  });

  it("shows My notes when the generated summary is empty", async () => {
    mockMeetingFetch({
      id: MEETING_ID,
      title: "fallback",
      started_at: 1000,
      ended_at: null,
      notes_md: "- my raw notes survive",
      enriched_md: "",
      transcript_json: "[]",
    });
    renderPost();

    await waitFor(() => {
      expect(
        screen.getByRole("tab", { name: "My notes" }),
      ).toHaveAttribute("aria-selected", "true");
      expect(screen.getByText(/my raw notes survive/)).toBeInTheDocument();
    });
  });
});
