import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { sttKeys } from "../lib/api/stt";
import { ensureSessionToken } from "../lib/session";

export interface DownloadState {
  bytesDownloaded: number;
  totalBytes: number;
  bytesPerSec: number;
  etaSeconds: number | null;
  complete: boolean;
  error: string | null;
}

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
          qc.invalidateQueries({ queryKey: sttKeys.models });
        } else {
          setState((prev) => ({
            ...(prev ?? INITIAL),
            error: ev.error,
          }));
        }
      };

      // onerror/onclose: keep the last state so the user can retry from the pill.
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
