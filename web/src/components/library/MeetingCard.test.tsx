import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MeetingCard } from "./MeetingCard";
import type { MeetingSummary } from "../../lib/api/meetings";

const START = new Date("2026-06-25T14:45:00").getTime();

function meeting(over: Partial<MeetingSummary> = {}): MeetingSummary {
  return {
    id: "m1",
    title: "Standup",
    started_at: START,
    ended_at: null,
    starred: false,
    stt_engine: null,
    llm_model: null,
    has_enriched: false,
    labels: [],
    ...over,
  };
}

function renderCard(m: MeetingSummary, activeId?: string | null) {
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

describe("MeetingCard — engine pill", () => {
  it("shows the LLM pill once the meeting has been enhanced", () => {
    renderCard(meeting({ id: "llm-1", llm_model: "gpt-5-mini" }));
    expect(screen.getByTestId("llm-pill")).toHaveTextContent(/^gpt-5-mini/);
  });

  it("shows no engine pill for a null stt_engine (pre-column meetings)", () => {
    renderCard(meeting({ id: "old-1", stt_engine: null }));
    expect(screen.queryByText(/Local|Cloud/)).toBeNull();
  });

  it("shows the model alone with the provider on hover", () => {
    renderCard(meeting({ id: "local-1", stt_engine: "local · small.en" }));
    expect(screen.getByTestId("engine-pill")).toHaveTextContent(/^small\.en/);
    expect(within(screen.getByTestId("engine-pill")).getByRole("tooltip")).toHaveTextContent(
      "Local small.en",
    );
  });

  it("keeps Cloud in the hover for a cloud stamp", () => {
    renderCard(meeting({ id: "cloud-1", stt_engine: "cloud · nova-3" }));
    expect(screen.getByTestId("engine-pill")).toHaveTextContent(/^nova-3/);
    expect(within(screen.getByTestId("engine-pill")).getByRole("tooltip")).toHaveTextContent(
      "Cloud nova-3",
    );
  });
});
