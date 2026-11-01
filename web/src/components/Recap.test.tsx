import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";
import { RecapCard } from "./Recap";
import { TranscriptDock } from "./TranscriptDock";
import type { TranscriptEvent } from "../lib/ws";

const BULLETS = [
  { ts_ms: 42_000, text: "Goal: settle the Pro price" },
  { ts_ms: 355_000, text: "Dana proposes $18/month" },
];
const state = (over: Record<string, unknown> = {}) => ({
  bullets: BULLETS,
  status: "idle",
  error: null,
  updated_at: Date.now(),
  paused: false,
  ...over,
});

let recapBody: Record<string, unknown>;
let sockets: FakeSocket[] = [];

// Like the server, every meeting socket opens with a `live_recap` frame of the current state.
class FakeSocket {
  static readonly OPEN = 1;
  readyState = 0;
  onopen: ((ev: Event) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  constructor(public url: string) {
    sockets.push(this);
    queueMicrotask(() => {
      this.readyState = 1;
      this.onopen?.(new Event("open"));
      this.onmessage?.(new MessageEvent("message", { data: JSON.stringify({ type: "live_recap", ...recapBody }) }));
    });
  }
  close() {
    this.readyState = 3;
  }
}

function emitRecap(body: Record<string, unknown>) {
  act(() => {
    for (const ws of sockets) {
      ws.onmessage?.(new MessageEvent("message", { data: JSON.stringify({ type: "live_recap", ...body }) }));
    }
  });
}

describe("RecapCard", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    sockets = [];
    recapBody = state();
    vi.stubGlobal("WebSocket", FakeSocket);
    Object.defineProperty(window, "location", {
      configurable: true,
      writable: true,
      value: { protocol: "http:", host: "localhost:5173" },
    });
    Element.prototype.scrollIntoView = vi.fn();
    fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (url.endsWith("/api/session-token")) return Response.json({ token: "t" });
      if (url.endsWith("/recap") && init?.method === "PATCH") {
        const { paused } = JSON.parse(init.body as string) as { paused: boolean };
        return Response.json(state({ paused }));
      }
      return Response.json({}, { status: 202 });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reserves the list's height before the first update and keeps it as bullets arrive", async () => {
    recapBody = state({ bullets: [], updated_at: null });
    render(<RecapCard meetingId="m" token="t" mode="recording" />);
    const list = screen.getByTestId("recap-list");
    expect(list.style.height).toBe("148px");
    expect(await screen.findByText(/starts after about 3 minutes/)).toBeInTheDocument();

    emitRecap(state());
    expect(screen.getByText("Dana proposes $18/month")).toBeInTheDocument();
    expect(screen.getByTestId("recap-list").style.height).toBe("148px");
    expect(screen.getByText("Updated just now")).toBeInTheDocument();
  });

  it("stays hidden when stopped or final with no bullets", async () => {
    recapBody = state({ bullets: [] });
    const { rerender } = render(<RecapCard meetingId="m" token="t" mode="stopped" />);
    await act(async () => {});
    expect(screen.queryByTestId("recap-card")).toBeNull();
    rerender(<RecapCard meetingId="m" token="t" mode="final" />);
    expect(screen.queryByTestId("recap-card")).toBeNull();
  });

  it("keeps a paused recap after Stop so Resume stays reachable", async () => {
    recapBody = state({ bullets: [], paused: true });
    render(<RecapCard meetingId="m" token="t" mode="stopped" />);
    expect(await screen.findByRole("button", { name: "Resume recap" })).toBeInTheDocument();
  });

  it("counts bullets that arrive while collapsed", async () => {
    render(<RecapCard meetingId="m" token="t" mode="recording" />);
    await screen.findByText("Dana proposes $18/month");
    act(() => screen.getByRole("button", { name: "Collapse recap" }).click());
    expect(screen.queryByTestId("recap-list")).toBeNull();

    emitRecap(state({ bullets: [...BULLETS, { ts_ms: 500_000, text: "New" }] }));
    expect(screen.getByText("1 new")).toBeInTheDocument();
    expect(screen.getByTestId("recap-unseen-dot")).toBeInTheDocument();

    act(() => screen.getByRole("button", { name: "Expand recap" }).click());
    expect(screen.queryByText("1 new")).toBeNull();
    expect(screen.getByText("New")).toBeInTheDocument();
  });

  it("pauses and resumes from the header", async () => {
    render(<RecapCard meetingId="m" token="t" mode="recording" />);
    await screen.findByText("Dana proposes $18/month");
    act(() => screen.getByRole("button", { name: "Pause recap" }).click());
    await screen.findByRole("button", { name: "Resume recap" });
    expect(screen.getByText(/Paused for this meeting/)).toBeInTheDocument();
    act(() => screen.getByRole("button", { name: "Resume recap" }).click());
    await screen.findByRole("button", { name: "Pause recap" });
  });

  it("opens the dock on Transcript at a bullet's moment", async () => {
    const history: TranscriptEvent[] = [
      { ts_ms: 40_000, channel: "system", text: "early line", is_final: true },
      { ts_ms: 356_000, channel: "system", text: "eighteen a month", is_final: true },
    ];
    render(
      <>
        <RecapCard meetingId="m" token="t" mode="recording" />
        <TranscriptDock meetingId="m" token="t" history={history} />
      </>,
    );
    expect(screen.queryByTestId("transcript-dock-panel")).toBeNull();
    const bullet = await screen.findByRole("button", { name: /Dana proposes/ });
    act(() => bullet.click());

    const transcript = await screen.findByTestId("transcript-list");
    const line = transcript.querySelector('[data-transcript-ts-sec="356"]');
    await waitFor(() => expect((line as HTMLElement).scrollIntoView).toHaveBeenCalled());
    // The dock in this mode is Transcript only.
    expect(screen.queryByRole("button", { name: /show recap/i })).toBeNull();
  });

  it("reads final in its header and folds away once the summary clears it", async () => {
    render(<RecapCard meetingId="m" token="t" mode="final" />);
    await screen.findByText("Dana proposes $18/month");
    expect(screen.getByText("Final, the summary replaces it when ready")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Pause recap" })).toBeNull();

    emitRecap(state({ bullets: [] }));
    // The last bullets stay while it folds.
    expect(screen.getByTestId("recap-card")).toHaveClass("grid-rows-[0fr]");
    expect(screen.getByText("Dana proposes $18/month")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByTestId("recap-card")).toBeNull());
  });
});
