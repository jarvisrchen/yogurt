import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { MeetingsParams, MeetingSummary } from "../lib/api/meetings";
import { Library } from "./Library";

const TOTAL = 60;
const all: MeetingSummary[] = Array.from({ length: TOTAL }, (_, i) => ({
  id: `m${i + 1}`,
  title: `Meeting ${i + 1}`,
  started_at: 1_700_000_000_000 - i * 3_600_000,
  ended_at: null,
  starred: false,
  stt_engine: null,
  llm_model: null,
  has_enriched: false,
  labels: [],
}));

const requested: MeetingsParams[] = [];
const useMeetings = vi.fn((p: MeetingsParams, enabled = true) => {
  if (enabled) requested.push(p);
  const start = (p.page - 1) * p.size;
  return {
    data: enabled ? { meetings: all.slice(start, start + p.size), total: TOTAL } : undefined,
    isLoading: false,
    error: null,
  };
});

vi.mock("../lib/api/meetings", async () => ({
  ...(await vi.importActual<typeof import("../lib/api/meetings")>("../lib/api/meetings")),
  useMeetings: (p: MeetingsParams, enabled?: boolean) => useMeetings(p, enabled),
  useActiveRecording: () => ({ data: null }),
  useCreateMeeting: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));
vi.mock("../lib/api/labels", () => ({
  useLabels: () => ({ data: [], isLoading: false }),
}));
vi.mock("../components/library/Sidebar", () => ({ Sidebar: () => null }));
vi.mock("../components/AskExperience", () => ({ LibraryAskExperience: () => null }));
vi.mock("../components/library/MeetingCard", () => ({
  MeetingCard: ({
    meeting,
    onToggleSelect,
  }: {
    meeting: MeetingSummary;
    onToggleSelect?: (id: string, shift: boolean) => void;
  }) => (
    <button onClick={(e) => onToggleSelect?.(meeting.id, e.shiftKey)}>{meeting.title}</button>
  ),
}));
vi.mock("../components/library/SelectionActionBar", () => ({
  SelectionActionBar: ({
    meetings,
    onSelectAll,
  }: {
    meetings: { id: string }[];
    onSelectAll: () => void;
  }) => (
    <div>
      <span data-testid="selected">{meetings.length}</span>
      <button onClick={onSelectAll}>Select all</button>
    </div>
  ),
}));
vi.mock("../hooks/useScreenRecordingStatus", () => ({
  useScreenRecordingStatus: () => ({ granted: true, isLoading: false }),
}));
vi.mock("../hooks/useMicrophoneStatus", () => ({
  useMicrophoneStatus: () => ({ granted: true, isLoading: false }),
}));
vi.mock("../lib/api/cloud", () => ({ useCloudStatus: () => ({ data: undefined }) }));

function Where() {
  const l = useLocation();
  return <output data-testid="where">{l.pathname + l.search}</output>;
}

function renderLibrary(url = "/") {
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[url]}>
        <Where />
        <Routes>
          <Route path="/" element={<Library />} />
          <Route path="/starred" element={<Library starredOnly />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const where = () => screen.getByTestId("where").textContent;
// The greeting asks for the unfiltered total with `size: 1`.
const lastRequest = () => requested.filter((r) => r.size !== 1).at(-1)!;

beforeEach(() => {
  requested.length = 0;
  localStorage.clear();
  window.scrollTo = vi.fn();
});
afterEach(() => cleanup());

describe("Library paging", () => {
  it("renders one page, the pager range, and the unfiltered total in the greeting", () => {
    renderLibrary();
    expect(screen.getAllByRole("button", { name: /^Meeting \d+$/ })).toHaveLength(25);
    expect(screen.getByTestId("pager-range")).toHaveTextContent("1–25 of 60");
    expect(screen.getByText(/60 meetings/)).toBeInTheDocument();
  });

  it("moves between pages through the URL", () => {
    renderLibrary();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(where()).toBe("/?page=2");
    expect(screen.getByTestId("pager-range")).toHaveTextContent("26–50 of 60");
    expect(screen.getByRole("button", { name: "Meeting 26" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Previous" }));
    expect(where()).toBe("/");
  });

  it("reads page and size from the URL", () => {
    renderLibrary("/?page=3&size=10");
    expect(screen.getByTestId("pager-range")).toHaveTextContent("21–30 of 60");
    expect(lastRequest()).toMatchObject({ page: 3, size: 10 });
  });

  it("remembers the page size and returns to page 1 when it changes", () => {
    renderLibrary("/?page=2");
    fireEvent.change(screen.getByRole("combobox", { name: "Meetings per page" }), {
      target: { value: "10" },
    });
    expect(where()).toBe("/?size=10");
    expect(localStorage.getItem("yogurt.library.pageSize")).toBe("10");
    expect(screen.getAllByRole("button", { name: /^Meeting \d+$/ })).toHaveLength(10);
  });

  it("uses the remembered size when the URL has none", () => {
    localStorage.setItem("yogurt.library.pageSize", "50");
    renderLibrary();
    expect(lastRequest().size).toBe(50);
  });

  it("returns to page 1 when the search text changes", async () => {
    renderLibrary("/?page=2");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "budget" } });
    await waitFor(() => expect(where()).toBe("/"));
    expect(lastRequest()).toMatchObject({ page: 1, q: "budget" });
  });

  it("asks for the starred filter in the query and starts at page 1 on that route", () => {
    renderLibrary("/starred");
    expect(lastRequest()).toMatchObject({ starred: true, page: 1 });
    expect(screen.getByText(/60 meetings/)).toBeInTheDocument();
  });

  it("selects only the visible page and clears the selection on a page change", () => {
    renderLibrary();
    fireEvent.click(screen.getByRole("button", { name: "Meeting 1" }));
    fireEvent.click(screen.getByRole("button", { name: "Select all" }));
    expect(screen.getByTestId("selected")).toHaveTextContent("25");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.queryByTestId("selected")).not.toBeInTheDocument();
  });

  it("steps back to the last page when the URL points past the end", async () => {
    renderLibrary("/?page=9");
    await waitFor(() => expect(where()).toBe("/?page=3"));
  });
});
