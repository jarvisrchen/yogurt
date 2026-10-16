import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";
import { TranscriptDock } from "./TranscriptDock";
import type { StoredTranscriptSegment, TranscriptEvent } from "../lib/ws";

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  static lastInstance: MockWebSocket | null = null;
  // Live mode opens two WS connections (transcript + useAudioLevels). The transcript
  // socket is `instances[0]` because its effect runs first.
  static instances: MockWebSocket[] = [];

  url: string;
  readyState: number = MockWebSocket.CONNECTING;
  onopen: ((this: WebSocket, ev: Event) => void) | null = null;
  onclose: ((this: WebSocket, ev: CloseEvent) => void) | null = null;
  onmessage: ((this: WebSocket, ev: MessageEvent) => void) | null = null;
  onerror: ((this: WebSocket, ev: Event) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.lastInstance = this;
    MockWebSocket.instances.push(this);
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
    MockWebSocket.instances = [];
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

    await waitFor(() => expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2));
    const ws = MockWebSocket.instances[0];

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

    const dataRows = document.querySelectorAll("[data-channel]");
    expect(dataRows.length).toBe(2);

    const firstLabel = dataRows[0].querySelector("span") as HTMLElement;
    const secondLabel = dataRows[1].querySelector("span") as HTMLElement;

    expect(firstLabel.textContent).toBe("Me");
    expect(secondLabel.textContent).toBe("Them");

    // Tokens, not hex, so the labels follow dark mode.
    expect(firstLabel.style.color).toBe("var(--color-ink)");
    expect(secondLabel.style.color).toBe("var(--color-grey)");
  });

  it("keeps the header icon-free and shows the shortened 'Transcript' label on the collapsed tab in live mode", async () => {
    render(<TranscriptDock meetingId="meeting-wave" token="test-token" />);
    const btn = screen.getByRole("button", { name: /show live transcript/i });
    expect(btn.textContent).toContain("Transcript");
    expect(btn.textContent).not.toContain("Live transcript");

    act(() => {
      btn.click();
    });
    const panel = screen.getByTestId("transcript-dock-panel");
    expect(panel.textContent).toContain("Live transcript");
    const waveIcons = panel.querySelectorAll('[role="presentation"]');
    expect(waveIcons.length).toBe(0); // wave lives ONLY on the collapsed tab

    await waitFor(() => expect(MockWebSocket.lastInstance).not.toBeNull());
    await act(async () => {
      await Promise.resolve();
    });
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

  it("renders the collapsed tab with the same shortened label and no wave icon in static mode", () => {
    render(<TranscriptDock meetingId={null} token={null} segments={[]} />);
    const btn = screen.getByRole("button", { name: /show transcript/i });
    expect(btn.textContent).toContain("Transcript");
    expect(btn.querySelectorAll('[role="presentation"]').length).toBe(0);
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

    // Dispatched on click, before React re-renders the dock as open.
    act(() => {
      window.dispatchEvent(
        new CustomEvent("yogurt:transcript:scrollTo", { detail: { ts: 61 } }),
      );
    });
    expect(scrollIntoViewMock).not.toHaveBeenCalled();

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
