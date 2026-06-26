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
 * `enhance_progress` WS event (Phase 4 — CONTEXT D-23 / D-24).
 *
 * Emitted by `crates/yogurt-server/src/enhance.rs` on `Meeting.events_tx`
 * and multiplexed onto the per-meeting WebSocket by `ws.rs`. The shape is
 * flat (no `payload` wrapper) because that channel forwards arbitrary
 * `serde_json::Value`s with a top-level `type` key.
 *
 * Lifecycle: `sending` → `streaming` (one or more, with `chars`) → `done`.
 * Phase 4 emits `chars` once at completion; Phase 5 will stream per-chunk.
 */
export interface EnhanceProgressEvent {
  type: "enhance_progress";
  phase: "sending" | "streaming" | "done";
  chars?: number;
}

/**
 * Union of all WS messages the per-meeting socket can deliver.
 *
 *   - `transcript` frames are wrapped: `{type:"transcript", payload:…}`.
 *   - `enhance_progress` frames are flat: `{type:"enhance_progress", …}`.
 *
 * `enhance_progress` is the Phase 4 surface; Phase 6 will add `chat_chunk`.
 */
export type WsMessage =
  | { type: "transcript"; payload: TranscriptEvent }
  | EnhanceProgressEvent;

/**
 * Server envelope: kept as an alias for backwards compatibility with the
 * Phase 3 transcript dock code that imported `ServerFrame` directly.
 */
type ServerFrame = WsMessage;

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

/**
 * Connection lifecycle state surfaced to consumers.
 *
 *   - `idle`         — no `meetingId` yet, hook is dormant.
 *   - `connecting`   — initial WebSocket opening, or backoff between
 *                      reconnect attempts.
 *   - `connected`    — WS is open and receiving frames.
 *   - `reconnecting` — WS dropped; backoff timer is running before the
 *                      next attempt (WR-01).
 *   - `failed`       — exhausted MAX_RECONNECT_ATTEMPTS with no success.
 */
export type ConnectionStatus =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "failed";

export interface UseTranscriptWsResult {
  events: TranscriptEvent[];
  /**
   * Kept for backwards compatibility with the TranscriptDock dot
   * indicator. Equivalent to `connectionStatus === "connected"`.
   */
  connected: boolean;
  /**
   * Granular connection lifecycle. WR-01: the dock can render
   * "reconnecting…" or "offline" distinct from "connected".
   */
  connectionStatus: ConnectionStatus;
}

/**
 * Maximum reconnect attempts after the first drop. The 03-03 plan
 * specifies "3 attempts with exponential backoff" so we honor that
 * verbatim. Backoff schedule: 500ms, 1000ms, 2000ms.
 */
const MAX_RECONNECT_ATTEMPTS = 3;

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
  token: string | null,
): UseTranscriptWsResult {
  const [events, setEvents] = useState<TranscriptEvent[]>([]);
  const [connectionStatus, setConnectionStatus] =
    useState<ConnectionStatus>("idle");

  useEffect(() => {
    // WR-06 + BL-01: the per-meeting WS handler now requires the session
    // token (Origin alone is insufficient). Wait until both meetingId AND
    // token are available before attempting to connect — otherwise the
    // first connect would 403 and burn a reconnect attempt.
    if (!meetingId || !token) {
      setEvents([]);
      setConnectionStatus("idle");
      return;
    }

    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    // Encode the token as a URL param (Phase 0's URL-param auth contract).
    // The server's redact_token_in_uri masks this in logs.
    const url = `${proto}//${window.location.host}/ws/meetings/${meetingId}?token=${encodeURIComponent(token)}`;

    // WR-01: reconnect state lives in refs so the lifecycle is owned by the
    // effect's cleanup, not by React reconciliation. The `cancelled` flag
    // is set in cleanup so any in-flight backoff timer or pending WS knows
    // to stand down (we DO NOT want a stale reconnect to fire after the
    // user navigates away or changes meetingId).
    let attempt = 0;
    let cancelled = false;
    let currentWs: WebSocket | null = null;
    let backoffTimer: ReturnType<typeof setTimeout> | null = null;

    const cleanupCurrentWs = () => {
      if (currentWs) {
        currentWs.onopen = null;
        currentWs.onclose = null;
        currentWs.onerror = null;
        currentWs.onmessage = null;
        if (
          currentWs.readyState === WebSocket.CONNECTING ||
          currentWs.readyState === WebSocket.OPEN
        ) {
          currentWs.close();
        }
        currentWs = null;
      }
    };

    const connect = () => {
      if (cancelled) return;
      setConnectionStatus(attempt === 0 ? "connecting" : "reconnecting");

      const ws = new WebSocket(url);
      currentWs = ws;

      ws.onopen = () => {
        if (cancelled) return;
        attempt = 0; // success — reset for any future drop
        setConnectionStatus("connected");
      };

      ws.onmessage = (event) => {
        if (cancelled) return;
        try {
          const frame = JSON.parse(event.data as string) as ServerFrame;
          if (frame.type === "transcript") {
            setEvents((prev) => mergeEvent(prev, frame.payload));
          }
          // `enhance_progress` is also forwarded on this socket (Phase 4
          // CONTEXT D-23) — `useEnhanceProgress` handles it via its own
          // subscription; this hook intentionally ignores it so the
          // transcript dock is unaffected.
        } catch {
          // Ignore non-JSON / malformed frames; future phases may add typed
          // error frames that we'll log here.
        }
      };

      const scheduleReconnect = () => {
        if (cancelled) return;
        // Detach listeners on the dead socket so its eventual onclose
        // doesn't re-enter this branch after we've already scheduled.
        cleanupCurrentWs();
        if (attempt >= MAX_RECONNECT_ATTEMPTS) {
          setConnectionStatus("failed");
          return;
        }
        // Backoff: 500ms, 1000ms, 2000ms (1<<attempt * 500).
        const delay = 500 * (1 << attempt);
        attempt += 1;
        setConnectionStatus("reconnecting");
        backoffTimer = setTimeout(() => {
          backoffTimer = null;
          connect();
        }, delay);
      };

      ws.onclose = scheduleReconnect;
      ws.onerror = scheduleReconnect;
    };

    connect();

    return () => {
      cancelled = true;
      if (backoffTimer) {
        clearTimeout(backoffTimer);
        backoffTimer = null;
      }
      cleanupCurrentWs();
      setConnectionStatus("idle");
    };
    // BL-4: `token` is read inside the effect (it's interpolated into the WS
    // URL above) but was missing from the dep array. When `meetingId` arrived
    // BEFORE `token` resolved (the common bootstrap order in MeetingPost),
    // the effect short-circuited at the `!token` guard and never re-ran when
    // `token` flipped from null to a real value — dock stayed offline
    // forever. useEnhanceProgress (below) had this right with [meetingId,
    // token]; this brings the two hooks into agreement.
  }, [meetingId, token]);

  return {
    events,
    connected: connectionStatus === "connected",
    connectionStatus,
  };
}

/**
 * Hook result for `useEnhanceProgress` — exposes the most recent
 * `enhance_progress` event from the per-meeting WebSocket.
 *
 *   - `phase`  is `"sending"` / `"streaming"` / `"done"` once any event
 *     has been seen, else `null`.
 *   - `chars`  is the running character count from the most recent event,
 *     or `null` if absent.
 *   - `enhancing` is a convenience derived from `phase` —`true` while
 *     `sending` or `streaming`, `false` once `done` (or before any event).
 */
export interface UseEnhanceProgressResult {
  phase: EnhanceProgressEvent["phase"] | null;
  chars: number | null;
  enhancing: boolean;
}

/**
 * Subscribe to `enhance_progress` events on the per-meeting WebSocket
 * (CONTEXT D-23 / D-24).
 *
 * Opens its own connection (independent of `useTranscriptWs`) so a route
 * that only cares about enhance progress (post-meeting view) doesn't have
 * to also mount the transcript list. Both hooks can coexist on the same
 * page — the server fans out to one broadcast subscriber per WS, so two
 * sockets means two subscribers, no message is lost.
 *
 * If `meetingId` or `token` is null, the hook is dormant. Reconnect on
 * drop follows the same backoff schedule as `useTranscriptWs`
 * (500/1000/2000ms, 3 attempts then give up).
 */
export function useEnhanceProgress(
  meetingId: string | null,
  token: string | null,
): UseEnhanceProgressResult {
  const [phase, setPhase] =
    useState<EnhanceProgressEvent["phase"] | null>(null);
  const [chars, setChars] = useState<number | null>(null);

  useEffect(() => {
    if (!meetingId || !token) {
      setPhase(null);
      setChars(null);
      return;
    }

    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/ws/meetings/${meetingId}?token=${encodeURIComponent(token)}`;

    let attempt = 0;
    let cancelled = false;
    let currentWs: WebSocket | null = null;
    let backoffTimer: ReturnType<typeof setTimeout> | null = null;

    const cleanupCurrentWs = () => {
      if (currentWs) {
        currentWs.onopen = null;
        currentWs.onclose = null;
        currentWs.onerror = null;
        currentWs.onmessage = null;
        if (
          currentWs.readyState === WebSocket.CONNECTING ||
          currentWs.readyState === WebSocket.OPEN
        ) {
          currentWs.close();
        }
        currentWs = null;
      }
    };

    const connect = () => {
      if (cancelled) return;
      const ws = new WebSocket(url);
      currentWs = ws;

      ws.onmessage = (event) => {
        if (cancelled) return;
        try {
          const frame = JSON.parse(event.data as string) as WsMessage;
          if (frame.type === "enhance_progress") {
            setPhase(frame.phase);
            if (typeof frame.chars === "number") {
              setChars(frame.chars);
            }
          }
        } catch {
          // Ignore non-JSON / malformed frames.
        }
      };

      const scheduleReconnect = () => {
        if (cancelled) return;
        cleanupCurrentWs();
        if (attempt >= MAX_RECONNECT_ATTEMPTS) return;
        const delay = 500 * (1 << attempt);
        attempt += 1;
        backoffTimer = setTimeout(() => {
          backoffTimer = null;
          connect();
        }, delay);
      };

      ws.onopen = () => {
        if (cancelled) return;
        attempt = 0;
      };
      ws.onclose = scheduleReconnect;
      ws.onerror = scheduleReconnect;
    };

    connect();

    return () => {
      cancelled = true;
      if (backoffTimer) {
        clearTimeout(backoffTimer);
        backoffTimer = null;
      }
      cleanupCurrentWs();
    };
  }, [meetingId, token]);

  const enhancing = phase === "sending" || phase === "streaming";

  return { phase, chars, enhancing };
}
