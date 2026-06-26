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

  it("attempts up to 3 reconnects with exponential backoff before failing (WR-01)", async () => {
    // Always-fail mock: skips the parent's onopen path entirely so the
    // close fires as the first lifecycle event (mirroring a server that
    // refuses every connection — TCP RST or 5xx during upgrade).
    const constructed: { url: string; onclose: ((ev: CloseEvent) => void) | null }[] = [];
    class FailingWebSocket {
      static readonly CONNECTING = 0;
      static readonly OPEN = 1;
      static readonly CLOSING = 2;
      static readonly CLOSED = 3;

      url: string;
      readyState: number = FailingWebSocket.CONNECTING;
      onopen: ((this: WebSocket, ev: Event) => void) | null = null;
      onclose: ((this: WebSocket, ev: CloseEvent) => void) | null = null;
      onmessage: ((this: WebSocket, ev: MessageEvent) => void) | null = null;
      onerror: ((this: WebSocket, ev: Event) => void) | null = null;

      constructor(url: string) {
        this.url = url;
        const entry = { url, onclose: null as ((ev: CloseEvent) => void) | null };
        constructed.push(entry);
        queueMicrotask(() => {
          this.readyState = FailingWebSocket.CLOSED;
          entry.onclose = this.onclose as ((ev: CloseEvent) => void) | null;
          this.onclose?.call(
            this as unknown as WebSocket,
            new CloseEvent("close"),
          );
        });
      }

      close(): void {
        this.readyState = FailingWebSocket.CLOSED;
      }
    }
    vi.stubGlobal("WebSocket", FailingWebSocket);

    const { result } = renderHook(() => useTranscriptWs("meeting-flaky"));

    // Initial connect attempt happens synchronously inside useEffect; the
    // close fires on the next microtask. After 500ms + 1000ms + 2000ms of
    // real time, we should have seen four total WebSocket constructions
    // (1 initial + 3 reconnect attempts).
    // Wait for the failure terminal state. By the time the 4th WS is
    // constructed and immediately closes, `attempt` is 3 and the next
    // `scheduleReconnect` flips status to "failed". `waitFor` keeps
    // polling so any pending microtasks finish first.
    await waitFor(
      () => {
        expect(constructed.length).toBe(4);
        expect(result.current.connectionStatus).toBe("failed");
      },
      { timeout: 8000 },
    );
  }, 15_000);

  it("resets the reconnect counter after a successful open", async () => {
    // First connection fails immediately; subsequent ones stay open.
    let n = 0;
    const constructed: MockWebSocket[] = [];
    class FlakyThenStableMockWebSocket extends MockWebSocket {
      constructor(url: string) {
        super(url);
        constructed.push(this);
        n += 1;
        if (n === 1) {
          queueMicrotask(() => {
            this.readyState = MockWebSocket.CLOSED;
            this.onclose?.call(
              this as unknown as WebSocket,
              new CloseEvent("close"),
            );
          });
        }
      }
    }
    vi.stubGlobal("WebSocket", FlakyThenStableMockWebSocket);

    const { result } = renderHook(() => useTranscriptWs("meeting-stable"));

    // After the initial-fail + 500ms reconnect, a second WS opens and stays.
    await waitFor(
      () => {
        expect(result.current.connectionStatus).toBe("connected");
      },
      { timeout: 2000 },
    );
    expect(constructed.length).toBe(2);
  });
});

// mergeEvent is exercised through the hook in the merge test above; this is
// the spec for the partial-replacement logic ("mergeEvent" appears verbatim).
