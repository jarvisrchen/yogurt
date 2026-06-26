/**
 * `useModelDownloadProgress` — subscribe to whisper.cpp model download
 * progress over the app-wide `/ws` WebSocket (Phase 8 Plan 08-03).
 *
 * The hook opens a dedicated WebSocket on mount (when `model` is
 * non-null), filters frames by `ev.model === model`, and surfaces the
 * latest state as React state. On `stt_model_download_complete` it also
 * invalidates the cached model list (`sttKeys.models`) so the
 * `downloaded: true` flip is visible in the picker the moment the
 * dialog auto-closes.
 *
 * The socket closes on unmount or when `model` flips to `null`.
 *
 * ## Wire shape (matches `crates/yogurt-server/src/ws.rs::WsEvent`)
 *
 * ```json
 * { "type": "stt_model_download_progress",
 *   "model": "small.en",
 *   "bytes_downloaded": 1234567,
 *   "total_bytes": 487654321,
 *   "bytes_per_sec": 4500000,
 *   "eta_seconds": 105 }
 *
 * { "type": "stt_model_download_complete", "model": "small.en" }
 * { "type": "stt_model_download_error",
 *   "model": "small.en", "error": "hash mismatch" }
 * ```
 *
 * Auth: `/ws` requires the session token as a `?token=` query param
 * (Phase 0 D-20 — same gate as `/ws/meetings/:id`). The hook fetches
 * the token via `ensureSessionToken()` before opening the socket, so
 * the URL is fully formed by the time `new WebSocket(...)` is called.
 */
import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { sttKeys } from "../lib/api/stt";
import { ensureSessionToken } from "../lib/session";

/** Latest download state surfaced to the dialog component.  All numeric
 *  fields default to 0 until the first `_progress` frame arrives. */
export interface DownloadState {
  bytesDownloaded: number;
  totalBytes: number;
  bytesPerSec: number;
  etaSeconds: number | null;
  complete: boolean;
  error: string | null;
}

/** Discriminated union of every frame this hook cares about.  Frames
 *  with other `type` discriminators (e.g. `chat_chunk`) are ignored. */
type ServerEvent =
  | {
      type: "stt_model_download_progress";
      model: string;
      bytes_downloaded: number;
      total_bytes: number;
      bytes_per_sec: number;
      eta_seconds: number | null;
    }
  | { type: "stt_model_download_complete"; model: string }
  | { type: "stt_model_download_error"; model: string; error: string };

const INITIAL: DownloadState = {
  bytesDownloaded: 0,
  totalBytes: 0,
  bytesPerSec: 0,
  etaSeconds: null,
  complete: false,
  error: null,
};

/**
 * Open a `/ws` socket and surface the latest download state for `model`.
 *
 * Returns `null` when `model` is `null` (the dialog is closed); otherwise
 * a `DownloadState` that starts at zero and ticks up as frames arrive.
 */
export function useModelDownloadProgress(
  model: string | null,
): DownloadState | null {
  const [state, setState] = useState<DownloadState | null>(null);
  const qc = useQueryClient();

  useEffect(() => {
    if (!model) {
      setState(null);
      return;
    }

    setState(INITIAL);
    let cancelled = false;
    let ws: WebSocket | null = null;

    (async () => {
      let token: string;
      try {
        token = await ensureSessionToken();
      } catch (e) {
        if (cancelled) return;
        setState({
          ...INITIAL,
          error: `session-token bootstrap failed: ${String(e)}`,
        });
        return;
      }
      if (cancelled) return;

      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      const url = `${proto}//${window.location.host}/ws?token=${encodeURIComponent(token)}`;
      ws = new WebSocket(url);

      ws.onmessage = (event) => {
        if (cancelled) return;
        let frame: unknown;
        try {
          frame = JSON.parse(event.data as string);
        } catch {
          return;
        }
        if (!frame || typeof frame !== "object") return;
        const ev = frame as ServerEvent;
        // Only care about our three model events for THIS model.
        if (
          ev.type !== "stt_model_download_progress" &&
          ev.type !== "stt_model_download_complete" &&
          ev.type !== "stt_model_download_error"
        ) {
          return;
        }
        if (ev.model !== model) return;

        if (ev.type === "stt_model_download_progress") {
          setState({
            bytesDownloaded: ev.bytes_downloaded,
            totalBytes: ev.total_bytes,
            bytesPerSec: ev.bytes_per_sec,
            etaSeconds: ev.eta_seconds,
            complete: false,
            error: null,
          });
        } else if (ev.type === "stt_model_download_complete") {
          setState((prev) => ({
            ...(prev ?? INITIAL),
            complete: true,
            error: null,
          }));
          // Re-fetch the model list so `downloaded: true` is visible
          // in the picker the moment the dialog closes.
          qc.invalidateQueries({ queryKey: sttKeys.models });
        } else {
          setState((prev) => ({
            ...(prev ?? INITIAL),
            error: ev.error,
          }));
        }
      };

      // ws.onerror / ws.onclose: the dialog stays open with whatever
      // state we last had; the user can re-trigger via the pill.
    })();

    return () => {
      cancelled = true;
      if (ws) {
        ws.onopen = null;
        ws.onmessage = null;
        ws.onerror = null;
        ws.onclose = null;
        if (
          ws.readyState === WebSocket.CONNECTING ||
          ws.readyState === WebSocket.OPEN
        ) {
          ws.close();
        }
        ws = null;
      }
    };
  }, [model, qc]);

  return state;
}
