import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";
import { TranscriptDock } from "./TranscriptDock";
import type { StoredTranscriptSegment, TranscriptEvent } from "../lib/ws";

/**
 * Mock WebSocket shared by every dock test. Same shape as ws.test.ts —
 * intentionally duplicated rather than factored out so each test file
 * stays self-contained.
 */
class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  static lastInstance: MockWebSocket | null = null;

  url: string;
  readyState: number = MockWebSocket.CONNECTING;
  onopen: ((this: WebSocket, ev: Event) => void) | null = null;
  onclose: ((this: WebSocket, ev: CloseEvent) => void) | null = null;
  onmessage: ((this: WebSocket, ev: MessageEvent) => void) | null = null;
  onerror: ((this: WebSocket, ev: Event) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.lastInstance = this;
    queueMicrotask(() => {
      this.readyState = MockWebSocket.OPEN;
      this.onopen?.call(this as unknown as WebSocket, new Event("open"));
    });
  }

  emit(payload: { type: string; payload: TranscriptEvent }): void {
    this.onmessage?.call(
      this as unknown as WebSocket,
      new MessageEvent("message", { data: JSON.stringify(payload) }),
    );
  }

  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.call(
      this as unknown as WebSocket,
      new CloseEvent("close"),
    );
  }
}

function frame(ev: TranscriptEvent) {
  return { type: "transcript" as const, payload: ev };
}

describe("TranscriptDock", () => {
  beforeEach(() => {
    MockWebSocket.lastInstance = null;
    vi.stubGlobal("WebSocket", MockWebSocket);
    Object.defineProperty(window, "location", {
      configurable: true,
      writable: true,
      value: { protocol: "http:", host: "localhost:5173" },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders the collapsed tab by default (panel hidden)", async () => {
    render(<TranscriptDock meetingId="meeting-1" token="test-token" />);
    expect(
      screen.getByRole("button", { name: /show live transcript/i }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("transcript-dock-panel")).toBeNull();
    // Let the WS onopen microtask flush so React state settles before teardown.
    await waitFor(() => expect(MockWebSocket.lastInstance).not.toBeNull());
    await act(async () => {
      await Promise.resolve();
    });
  });

  it("expands the panel on click and applies dock-open animation class", async () => {
    render(<TranscriptDock meetingId="meeting-2" token="test-token" />);
    const btn = screen.getByRole("button", { name: /show live transcript/i });

    act(() => {
      btn.click();
    });

    const panel = screen.getByTestId("transcript-dock-panel");
    expect(panel).toBeInTheDocument();
    expect(panel.className).toMatch(/dock-open/);
    // After opening the button label flips to "Hide live transcript".
    expect(
      screen.getByRole("button", { name: /hide live transcript/i }),
    ).toBeInTheDocument();
    await waitFor(() => expect(MockWebSocket.lastInstance).not.toBeNull());
    await act(async () => {
      await Promise.resolve();
    });
  });

  it("renders Me/Them labels with the right colors when events arrive", async () => {
    render(<TranscriptDock meetingId="meeting-3" token="test-token" />);
    act(() => {
      screen.getByRole("button", { name: /show live transcript/i }).click();
    });

    // Wait for the WS to open.
    await waitFor(() => expect(MockWebSocket.lastInstance).not.toBeNull());
    const ws = MockWebSocket.lastInstance!;

    act(() => {
      ws.emit(
        frame({ ts_ms: 1000, channel: "mic", text: "hi there", is_final: true }),
      );
      ws.emit(
        frame({
          ts_ms: 2000,
          channel: "system",
          text: "hello back",
          is_final: true,
        }),
      );
    });

    const rows = await screen.findAllByText(/hi there|hello back/);
    expect(rows.length).toBeGreaterThanOrEqual(2);

    // Inspect the data-channel rows.
    const dataRows = document.querySelectorAll("[data-channel]");
    expect(dataRows.length).toBe(2);

    const firstLabel = dataRows[0].querySelector("span") as HTMLElement;
    const secondLabel = dataRows[1].querySelector("span") as HTMLElement;

    expect(firstLabel.textContent).toBe("Me");
    expect(secondLabel.textContent).toBe("Them");

    // Browser normalizes hex colors to rgb() — assert the computed inline
    // style (jsdom resolves `style.color` to rgb form).
    expect(firstLabel.style.color).toBe("rgb(33, 29, 24)");
    expect(secondLabel.style.color).toBe("rgb(168, 159, 144)");
  });

  it("re-collapses when the tab is clicked again, animating out first (NOTES-14)", async () => {
    render(<TranscriptDock meetingId="meeting-4" token="test-token" />);
    const btn = screen.getByRole("button", { name: /show live transcript/i });

    act(() => {
      btn.click();
    });
    const panel = screen.getByTestId("transcript-dock-panel");
    expect(panel).toBeInTheDocument();

    act(() => {
      screen
        .getByRole("button", { name: /hide live transcript/i })
        .click();
    });
    // Symmetric close: the panel stays mounted mid-animation with the
    // dock-closed (slideOutRight) class instead of vanishing instantly.
    expect(screen.getByTestId("transcript-dock-panel").className).toMatch(
      /dock-closed/,
    );
    act(() => {
      panel.dispatchEvent(new Event("animationend", { bubbles: true }));
    });
    expect(screen.queryByTestId("transcript-dock-panel")).toBeNull();

    await waitFor(() => expect(MockWebSocket.lastInstance).not.toBeNull());
    await act(async () => {
      await Promise.resolve();
    });
  });
});

describe("TranscriptDock — static mode (MeetingPost task 9)", () => {
  it("renders 'Transcript' label with no connection chip", () => {
    render(
      <TranscriptDock meetingId={null} token={null} segments={[]} forceOpen />,
    );
    expect(
      screen.getByRole("button", { name: /hide transcript/i }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/connected|offline/)).toBeNull();
  });

  it("shows the no-transcript empty state when segments is empty", () => {
    render(
      <TranscriptDock meetingId={null} token={null} segments={[]} forceOpen />,
    );
    expect(
      screen.getByText("No transcript was captured for this meeting."),
    ).toBeInTheDocument();
  });

  it("renders Me/Them lines mapped from me/them stored channels", () => {
    render(
      <TranscriptDock
        meetingId={null}
        token={null}
        forceOpen
        segments={[
          { ts_ms: 1000, channel: "me", text: "hi there" },
          { ts_ms: 2000, channel: "them", text: "hello back" },
        ]}
      />,
    );
    const dataRows = document.querySelectorAll("[data-channel]");
    expect(dataRows.length).toBe(2);
    expect(dataRows[0].getAttribute("data-channel")).toBe("mic");
    expect(dataRows[1].getAttribute("data-channel")).toBe("system");
    expect(screen.getByText("hi there")).toBeInTheDocument();
    expect(screen.getByText("hello back")).toBeInTheDocument();
  });

  it("retries a pending scrollTo once the dock opens after the event fired while closed", async () => {
    // jsdom doesn't implement scrollIntoView; stub it so the call doesn't
    // log a "not implemented" error and so we can assert it fired.
    const scrollIntoViewMock = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoViewMock;

    const segments: StoredTranscriptSegment[] = [
      { ts_ms: 5000, channel: "me", text: "first point" },
      { ts_ms: 60000, channel: "them", text: "second point" },
    ];

    const { rerender } = render(
      <TranscriptDock
        meetingId={null}
        token={null}
        forceOpen={false}
        segments={segments}
      />,
    );

    // MeetingPost dispatches this the instant the user clicks a deep-link,
    // BEFORE React has re-rendered the dock as open — so no matching DOM
    // nodes exist yet.
    act(() => {
      window.dispatchEvent(
        new CustomEvent("yogurt:transcript:scrollTo", { detail: { ts: 61 } }),
      );
    });
    expect(scrollIntoViewMock).not.toHaveBeenCalled();

    // Now the dock actually opens (MeetingPost's `transcriptOpen` state
    // flips true right after the dispatch above).
    rerender(
      <TranscriptDock
        meetingId={null}
        token={null}
        forceOpen={true}
        segments={segments}
      />,
    );

    await waitFor(() => expect(scrollIntoViewMock).toHaveBeenCalledTimes(1));

    const highlighted = document.querySelector(".transcript-highlight");
    expect(highlighted?.textContent).toContain("second point");
  });
});
