import { useEffect, useRef, useState } from "react";

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
 * Wire shape of one row in the persisted `meetings.transcript_json` column
 * (`crates/yogurt-server/src/meetings.rs::segment_json` — mic is "me",
 * system is "them", distinct from the LIVE `Channel` naming above).
 */
export interface StoredTranscriptSegment {
  ts_ms: number;
  channel: "me" | "them";
  text: string;
}

/**
 * MeetingPost.tsx task 9 — map a persisted transcript row onto the same
 * `TranscriptEvent` shape the live dock renders, so `TranscriptLine` and
 * the `data-transcript-ts-sec` deep-link lookup need zero changes to
 * support static (post-meeting) playback. Stored rows are always final.
 */
export function storedSegmentToEvent(
  seg: StoredTranscriptSegment,
): TranscriptEvent {
  return {
    ts_ms: seg.ts_ms,
    channel: seg.channel === "me" ? "mic" : "system",
    text: seg.text,
    is_final: true,
  };
}

/**
 * `enhance_progress` WS event (Phase 4 — CONTEXT D-23 / D-24).
 *
 * Emitted by `crates/yogurt-server/src/enhance.rs` on `Meeting.events_tx`
 * and multiplexed onto the per-meeting WebSocket by `ws.rs`. The shape is
 * flat (no `payload` wrapper) because that channel forwards arbitrary
 * `serde_json::Value`s with a top-level `type` key.
 *
 * Lifecycle: `sending` → `streaming` (one or more, with `chars`) → `done`,
 * OR `sending` → `error` (terminal). BL-5: `error` is emitted on LLM
 * timeout, LLM HTTP failure, merge_notes failure, or persistence failure.
 * The browser banner transitions to a recoverable error state with a
 * Retry affordance; Re-enhance is re-enabled.
 *
 * Phase 4 emitted `chars` once at completion; Phase 5 streams per-chunk and
 * adds `text` - the FULL accumulated raw markdown so far (a snapshot, not a
 * delta), repeated on every `streaming` frame so each one fully replaces the
 * previous.
 */
export interface EnhanceProgressEvent {
  type: "enhance_progress";
  phase: "sending" | "streaming" | "done" | "error";
  chars?: number;
  /** Full accumulated raw markdown snapshot, present on `streaming` frames. */
  text?: string;
  /** Human-readable message accompanying `phase: "error"`. */
  message?: string;
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
  /**
   * Live-dock-loses-history-on-remount fix: persisted transcript segments
   * (from `GET /api/meetings/:id`, mapped via `storedSegmentToEvent`) to
   * seed `events` with ONCE per `meetingId`, before any live WS events
   * arrive. Callers pass this from `meetingRow.transcript_json` so
   * navigating away from a live meeting and back repopulates the dock
   * instead of showing an empty list that only fills with new lines.
   */
  seedHistory?: TranscriptEvent[],
): UseTranscriptWsResult {
  const [events, setEvents] = useState<TranscriptEvent[]>([]);
  const [connectionStatus, setConnectionStatus] =
    useState<ConnectionStatus>("idle");

  // Guards the seed below to fire once per meetingId — `seedHistory` itself
  // may change reference (query refetch) without new data, and re-seeding
  // would duplicate the history entries already merged in.
  const seededMeetingIdRef = useRef<string | null>(null);
  // (channel, text) signatures of the seeded finals — checked in the WS
  // onmessage handler below to drop a live FINAL that duplicates one of
  // these (Deepgram/the server can redeliver the same final between the
  // history fetch and the WS subscribe completing).
  const seededFinalsRef = useRef<Set<string>>(new Set());
  // Which meetingId the current `events` list belongs to, so the reset
  // below fires on a meeting switch and nothing else (MTG-1).
  const eventsMeetingIdRef = useRef<string | null>(null);

  useEffect(() => {
    // A previous meeting's lines must not bleed into the next one, which
    // is a `meetingId` concern only. MTG-1: this reset used to live in the
    // connect effect below, behind the `!token` guard - and `token` is
    // fetched after mount, so on every client-side nav it fired one tick
    // AFTER this effect had already seeded from persisted history and
    // latched `seededMeetingIdRef`, wiping the seed for good. Keep the
    // reset here, ahead of the seed, in the one effect that owns `events`.
    if (eventsMeetingIdRef.current !== meetingId) {
      eventsMeetingIdRef.current = meetingId;
      seededMeetingIdRef.current = null;
      seededFinalsRef.current = new Set();
      setEvents([]);
    }
    if (!meetingId) return;
    if (seededMeetingIdRef.current === meetingId) return;
    if (!seedHistory || seedHistory.length === 0) return;
    seededMeetingIdRef.current = meetingId;
    seededFinalsRef.current = new Set(
      seedHistory
        .filter((e) => e.is_final)
        .map((e) => `${e.channel} ${e.text}`),
    );
    setEvents((prev) => [...seedHistory, ...prev]);
  }, [meetingId, seedHistory]);

  useEffect(() => {
    // WR-06 + BL-01: the per-meeting WS handler now requires the session
    // token (Origin alone is insufficient). Wait until both meetingId AND
    // token are available before attempting to connect — otherwise the
    // first connect would 403 and burn a reconnect attempt. MTG-1: this
    // branch must not touch `events` - clearing on a missing token wiped
    // the history seeded above during the token-fetch window.
    if (!meetingId || !token) {
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
            const ev = frame.payload;
            // Dedupe at the seam: a live FINAL whose (channel, text)
            // matches one already seeded from persisted history is the
            // same utterance landing twice (fetch raced the WS subscribe,
            // or the server redelivered it) — drop it. Partials are never
            // in the seeded set (seed history is always-final) so they're
            // unaffected.
            if (
              ev.is_final &&
              seededFinalsRef.current.has(`${ev.channel} ${ev.text}`)
            ) {
              return;
            }
            setEvents((prev) => mergeEvent(prev, ev));
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
 *   - `text`   is the full accumulated raw markdown snapshot from the most
 *     recent `streaming` frame, or `null` on `sending` / `done` / `error`
 *     (and before any event) - consumers render it as a live preview and
 *     must clear it once it goes back to `null`.
 *   - `enhancing` is a convenience derived from `phase` —`true` while
 *     `sending` or `streaming`, `false` once `done` (or before any event).
 */
export interface UseEnhanceProgressResult {
  phase: EnhanceProgressEvent["phase"] | null;
  chars: number | null;
  text: string | null;
  enhancing: boolean;
  /** BL-5: last error message from a `phase: "error"` event, or null. */
  errorMessage: string | null;
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
  const [text, setText] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!meetingId || !token) {
      setPhase(null);
      setChars(null);
      setText(null);
      setErrorMessage(null);
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
            // Phase 5: `text` carries the full accumulated raw markdown
            // snapshot on `streaming` frames - reset it on every other
            // phase so a stale preview never lingers once streaming ends.
            if (frame.phase === "streaming" && typeof frame.text === "string") {
              setText(frame.text);
            } else {
              setText(null);
            }
            // BL-5: surface the server-supplied error message so the
            // EnhancingBanner can render a recoverable error pill with
            // human-readable copy. Clear the message on a successful
            // re-attempt (any non-error phase).
            if (frame.phase === "error") {
              setErrorMessage(frame.message ?? "Enhance failed");
            } else {
              setErrorMessage(null);
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

  return { phase, chars, text, enhancing, errorMessage };
}

/** `stt_error` WS frame — see `crates/yogurt-server/src/meetings.rs::send_stt_error`. */
interface SttErrorEvent {
  type: "stt_error";
  message: string;
}

export interface UseSttErrorResult {
  /** Most recent `stt_error` message, or null once dismissed / not yet fired. */
  message: string | null;
  /** Clear the current message (e.g. a dismissible banner's close button). */
  dismiss: () => void;
}

/**
 * Meeting.tsx task 6 — subscribe to `stt_error` frames on the per-meeting
 * WS. Fired when the STT engine dies mid-recording (bad Deepgram key,
 * local model load failure); recording keeps running (audio capture is
 * independent of STT) so the user needs a visible, dismissible signal
 * that transcription itself has stopped.
 *
 * Mirrors `useEnhanceProgress`'s connect/backoff lifecycle — a third
 * independent subscription on the same per-meeting socket, alongside the
 * transcript dock's and the chat panel's. The server fans out one
 * broadcast subscriber per WS connection, so this is safe.
 */
export function useSttError(
  meetingId: string | null,
  token: string | null,
): UseSttErrorResult {
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!meetingId || !token) {
      setMessage(null);
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
          const frame = JSON.parse(event.data as string) as { type?: string };
          if (frame.type === "stt_error") {
            setMessage((frame as SttErrorEvent).message);
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

  return { message, dismiss: () => setMessage(null) };
}

/** `audio_level` WS frame — see `crates/yogurt-server/src/meetings.rs::maybe_emit_audio_level`. */
interface AudioLevelEvent {
  type: "audio_level";
  channel: Channel;
  level: number;
}

export interface AudioLevels {
  mic: number;
  system: number;
}

/** Zero out a channel's level if no `audio_level` event refreshes it within this window. */
const AUDIO_LEVEL_DECAY_MS = 600;

/**
 * Subscribe to `audio_level` frames on the per-meeting WS (real amplitude
 * wave, replacing the old heartbeat-only equalizer glyph). Mirrors
 * `useSttError`'s connect/backoff lifecycle — a fourth independent
 * subscription on the same per-meeting socket.
 *
 * Each channel decays to 0 if no fresh event arrives within
 * `AUDIO_LEVEL_DECAY_MS`, so the wave visibly settles once audio stops
 * instead of freezing at its last value.
 */
export function useAudioLevels(
  meetingId: string | null,
  token: string | null,
): AudioLevels {
  const [levels, setLevels] = useState<AudioLevels>({ mic: 0, system: 0 });

  useEffect(() => {
    if (!meetingId || !token) {
      setLevels({ mic: 0, system: 0 });
      return;
    }

    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/ws/meetings/${meetingId}?token=${encodeURIComponent(token)}`;

    let attempt = 0;
    let cancelled = false;
    let currentWs: WebSocket | null = null;
    let backoffTimer: ReturnType<typeof setTimeout> | null = null;
    const decayTimers: Record<Channel, ReturnType<typeof setTimeout> | null> = {
      mic: null,
      system: null,
    };

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

    const scheduleDecay = (channel: Channel) => {
      if (decayTimers[channel]) clearTimeout(decayTimers[channel]!);
      decayTimers[channel] = setTimeout(() => {
        setLevels((prev) => ({ ...prev, [channel]: 0 }));
      }, AUDIO_LEVEL_DECAY_MS);
    };

    const connect = () => {
      if (cancelled) return;
      const ws = new WebSocket(url);
      currentWs = ws;

      ws.onmessage = (event) => {
        if (cancelled) return;
        try {
          const frame = JSON.parse(event.data as string) as { type?: string };
          if (frame.type === "audio_level") {
            const ev = frame as AudioLevelEvent;
            setLevels((prev) => ({ ...prev, [ev.channel]: ev.level }));
            scheduleDecay(ev.channel);
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
      if (decayTimers.mic) clearTimeout(decayTimers.mic);
      if (decayTimers.system) clearTimeout(decayTimers.system);
      cleanupCurrentWs();
      setLevels({ mic: 0, system: 0 });
    };
  }, [meetingId, token]);

  return levels;
}
