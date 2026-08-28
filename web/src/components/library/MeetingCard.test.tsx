/**
 * MeetingCard — render coverage for the live-recording-aware `Link` target
 * (see `formatMeta` pure-function tests in `MeetingCard.test.ts`).
 *
 * A meeting still recording server-side must route to the LIVE capture
 * surface (`/meeting/:id`, controls + live transcript) instead of the
 * frozen post view (`/meeting/:id/post`) — and shows a pulsing-dot
 * "Recording" indicator in place of its time/duration meta line.
 */
import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MeetingCard } from "./MeetingCard";
import type { Meeting } from "../../lib/api/meetings";

const START = new Date("2026-06-25T14:45:00").getTime();

function meeting(over: Partial<Meeting> = {}): Meeting {
  return {
    id: "m1",
    title: "Standup",
    started_at: START,
    ended_at: null,
    notes_md: "",
    enriched_md: null,
    transcript_json: "[]",
    starred: false,
    stt_engine: null,
    created_at: new Date(START).toISOString(),
    updated_at: new Date(START).toISOString(),
    ...over,
  };
}

function renderCard(m: Meeting, activeId?: string | null) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <MeetingCard meeting={m} activeId={activeId} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => cleanup());

describe("MeetingCard — live-recording routing", () => {
  it("links to the live view and shows the recording indicator when it's the active recording", () => {
    const m = meeting({ id: "live-1" });
    renderCard(m, "live-1");

    expect(screen.getByRole("link")).toHaveAttribute("href", "/meeting/live-1");
    expect(screen.getByText("Recording")).toBeInTheDocument();
  });

  it("links to /post and shows normal meta when it's not the active recording", () => {
    const m = meeting({ id: "past-1" });
    renderCard(m, "live-1");

    expect(screen.getByRole("link")).toHaveAttribute("href", "/meeting/past-1/post");
    expect(screen.queryByText("Recording")).toBeNull();
  });

  it("links to /post when nothing is recording (activeId null/undefined)", () => {
    const m = meeting({ id: "past-2" });
    renderCard(m, null);

    expect(screen.getByRole("link")).toHaveAttribute("href", "/meeting/past-2/post");
    expect(screen.queryByText("Recording")).toBeNull();
  });
});

describe("MeetingCard — engine chip", () => {
  it("shows Local for a null stt_engine (pre-column meetings)", () => {
    renderCard(meeting({ id: "old-1", stt_engine: null }));
    expect(screen.getByText("Local")).toBeInTheDocument();
  });

  it("shows Local for a local · <model> stamp", () => {
    renderCard(meeting({ id: "local-1", stt_engine: "local · small.en" }));
    expect(screen.getByText("Local")).toBeInTheDocument();
  });

  it("shows Cloud for a cloud · <model> stamp", () => {
    renderCard(meeting({ id: "cloud-1", stt_engine: "cloud · nova-3" }));
    expect(screen.getByText("Cloud")).toBeInTheDocument();
  });
});
