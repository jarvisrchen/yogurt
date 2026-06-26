import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { App } from "./App";

/**
 * Mock WebSocket so the Meeting view's TranscriptDock (mounted after click)
 * doesn't try to dial a real socket during the test run.
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

  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.call(
      this as unknown as WebSocket,
      new CloseEvent("close"),
    );
  }
}

function renderApp() {
  return render(
    <MemoryRouter>
      <App />
    </MemoryRouter>,
  );
}

describe("App", () => {
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

  it("renders the library stub by default", () => {
    renderApp();
    expect(
      screen.getByRole("heading", { name: /yogurt/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Open a new meeting/i }),
    ).toBeInTheDocument();
  });

  it("switches to the meeting view on click", async () => {
    renderApp();
    const btn = screen.getByRole("button", { name: /Open a new meeting/i });
    act(() => {
      btn.click();
    });
    expect(
      await screen.findByRole("heading", { name: /New meeting/i }),
    ).toBeInTheDocument();
    // Flush any WS onopen microtasks so React state settles before unmount.
    await waitFor(() => expect(MockWebSocket.lastInstance).toBeNull());
  });
});
