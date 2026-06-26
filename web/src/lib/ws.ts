import { useEffect, useState } from "react";

/**
 * Channel discriminator on the wire — matches `yogurt-stt::Channel`
 * (serialized lowercase per Phase 3 D-03).
 */
export type Channel = "mic" | "system";

/**
 * Transcript wire frame — matches PRD §10 + 03-02 server payload verbatim.
 * snake_case fields, lowercase channel.
 */
export interface TranscriptEvent {
  ts_ms: number;
  channel: Channel;
  text: string;
  is_final: boolean;
}

/**
 * Server envelope: every WS frame is `{type, payload}` so future phases
 * (Phase 4 notes_edit, Phase 6 chat_send) can multiplex over the same socket.
 */
interface ServerFrame {
  type: "transcript";
  payload: TranscriptEvent;
}

/**
 * Merge a single incoming transcript event into the running list.
 *
 * Semantics (per PLAN Task 1 + test contract):
 *   - A partial REPLACES the trailing partial on the same channel if one
 *     exists; otherwise it appends.
 *   - A final REPLACES the trailing partial on the same channel if one
 *     exists (it's the same speech segment, now confirmed); otherwise it
 *     appends as a new locked line.
 *
 * Net effect: stable list of finals interleaved with at most one trailing
 * partial per channel. The dock renders that trailing partial at opacity
 * 0.7 with the blink-cursor (D-18); the moment Deepgram returns the final
 * for that segment, the dimmer line becomes the locked final in-place.
 */
export function mergeEvent(
  prev: TranscriptEvent[],
  next: TranscriptEvent,
): TranscriptEvent[] {
  // Search backwards for the trailing event on the same channel.
  for (let i = prev.length - 1; i >= 0; i--) {
    if (prev[i].channel === next.channel) {
      if (!prev[i].is_final) {
        // Trailing event is a partial — replace in place regardless of
        // whether `next` is partial or final.
        const copy = prev.slice();
        copy[i] = next;
        return copy;
      }
      // Trailing on this channel is already finalized — fall through.
      break;
    }
  }
  return [...prev, next];
}

export interface UseTranscriptWsResult {
  events: TranscriptEvent[];
  connected: boolean;
}

/**
 * Open a WebSocket against the meetings fan-out endpoint and accumulate
 * transcript events.
 *
 *   - `meetingId === null` → no connection; useful while the user hasn't
 *     created a meeting yet.
 *   - Reopens whenever `meetingId` changes.
 *   - Cleans up the socket on unmount.
 *
 * Wire format (S→C): `{"type":"transcript","payload":{ts_ms,channel,text,is_final}}`
 * — matches yogurt-server::ws::handle_meeting_socket.
 */
export function useTranscriptWs(
  meetingId: string | null,
): UseTranscriptWsResult {
  const [events, setEvents] = useState<TranscriptEvent[]>([]);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    if (!meetingId) {
      setEvents([]);
      setConnected(false);
      return;
    }

    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/ws/meetings/${meetingId}`;
    const ws = new WebSocket(url);

    ws.onopen = () => setConnected(true);
    ws.onclose = () => setConnected(false);
    ws.onerror = () => setConnected(false);
    ws.onmessage = (event) => {
      try {
        const frame = JSON.parse(event.data as string) as ServerFrame;
        if (frame.type === "transcript") {
          setEvents((prev) => mergeEvent(prev, frame.payload));
        }
      } catch {
        // Ignore non-JSON / malformed frames; future phases may add typed
        // error frames that we'll log here.
      }
    };

    return () => {
      ws.onopen = null;
      ws.onclose = null;
      ws.onerror = null;
      ws.onmessage = null;
      // readyState may be CONNECTING (0) or OPEN (1) at this point; either way
      // calling close() is safe and triggers a clean teardown.
      if (
        ws.readyState === WebSocket.CONNECTING ||
        ws.readyState === WebSocket.OPEN
      ) {
        ws.close();
      }
      setConnected(false);
    };
  }, [meetingId]);

  return { events, connected };
}
