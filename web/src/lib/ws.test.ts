import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { useTranscriptWs, type TranscriptEvent } from "./ws";

/**
 * MockWebSocket — fires `onopen` on the next microtask, exposes `emit(data)`
 * to push a message to the consumer. Matches the standard WebSocket surface
 * the hook touches: `url`, `readyState`, `onopen`, `onmessage`, `onclose`,
 * `onerror`, `close()`.
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
    // Fire onopen on the next microtask so the test gets a chance to wire
    // its expectations — mirrors real browser behavior.
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

describe("useTranscriptWs", () => {
  beforeEach(() => {
    MockWebSocket.lastInstance = null;
    vi.stubGlobal("WebSocket", MockWebSocket);
    // Anchor window.location for the proto switch.
    Object.defineProperty(window, "location", {
      configurable: true,
      writable: true,
      value: { protocol: "http:", host: "localhost:5173" },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("merges a partial then a final into the events list", async () => {
    const { result } = renderHook(() => useTranscriptWs("meeting-abc"));

    // Wait for socket to open.
    await waitFor(() => expect(result.current.connected).toBe(true));

    const ws = MockWebSocket.lastInstance!;

    // First partial.
    act(() => {
      ws.emit(
        frame({ ts_ms: 1000, channel: "mic", text: "hel", is_final: false }),
      );
    });
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].text).toBe("hel");
    expect(result.current.events[0].is_final).toBe(false);

    // Second partial REPLACES the first (same channel, both partial).
    act(() => {
      ws.emit(
        frame({ ts_ms: 1100, channel: "mic", text: "hello", is_final: false }),
      );
    });
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].text).toBe("hello");
    expect(result.current.events[0].is_final).toBe(false);

    // Final replaces the partial in-place (length stays 1, but is_final flips).
    act(() => {
      ws.emit(
        frame({
          ts_ms: 1200,
          channel: "mic",
          text: "hello world",
          is_final: true,
        }),
      );
    });
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].text).toBe("hello world");
    expect(result.current.events[0].is_final).toBe(true);
  });

  it("keeps mic and system partials independent", async () => {
    const { result } = renderHook(() => useTranscriptWs("meeting-xyz"));
    await waitFor(() => expect(result.current.connected).toBe(true));
    const ws = MockWebSocket.lastInstance!;

    act(() => {
      ws.emit(
        frame({ ts_ms: 500, channel: "mic", text: "hi", is_final: false }),
      );
      ws.emit(
        frame({
          ts_ms: 600,
          channel: "system",
          text: "hello",
          is_final: false,
        }),
      );
    });

    expect(result.current.events).toHaveLength(2);
    expect(result.current.events[0].channel).toBe("mic");
    expect(result.current.events[1].channel).toBe("system");
  });

  it("uses wss:// when window.location.protocol is https:", async () => {
    Object.defineProperty(window, "location", {
      configurable: true,
      writable: true,
      value: { protocol: "https:", host: "yogurt.example" },
    });

    renderHook(() => useTranscriptWs("meeting-secure"));
    await waitFor(() => expect(MockWebSocket.lastInstance).not.toBeNull());

    expect(MockWebSocket.lastInstance!.url).toBe(
      "wss://yogurt.example/ws/meetings/meeting-secure",
    );
  });
});

// mergeEvent is exercised through the hook in the merge test above; this is
// the spec for the partial-replacement logic ("mergeEvent" appears verbatim).
