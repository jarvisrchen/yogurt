// The in-flight model download lives in this module, not in a component, so leaving Settings
// does not drop it. One app-wide `/ws` subscription in the Shell feeds it.
// Only one download is tracked: frames for another model are ignored while one is shown,
// and adopted when none is, so a download started elsewhere (another tab, `yogurt ctl`) still shows.
import { useEffect, useSyncExternalStore } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  cancelDownload,
  fetchModels,
  startDownload,
  sttKeys,
} from "../lib/api/stt";
import { ensureSessionToken } from "../lib/session";
import { importApi } from "../lib/api/granolaImport";
import { applyImportEvent, applyImportStatus, type ImportEvent } from "./useGranolaImport";

export interface DownloadState {
  bytesDownloaded: number;
  totalBytes: number;
  bytesPerSec: number;
  etaSeconds: number | null;
  complete: boolean;
  error: string | null;
}

export interface ModelDownload {
  model: string;
  progress: DownloadState;
  dialogOpen: boolean;
}

export type DownloadEvent =
  | {
      type: "stt_model_download_progress";
      model: string;
      bytes_downloaded: number;
      total_bytes: number;
      bytes_per_sec: number;
      eta_seconds: number | null;
    }
  | { type: "stt_model_download_complete"; model: string }
  | { type: "stt_model_download_error"; model: string; error: string }
  | { type: "stt_model_download_cancelled"; model: string };

const INITIAL: DownloadState = {
  bytesDownloaded: 0,
  totalBytes: 0,
  bytesPerSec: 0,
  etaSeconds: null,
  complete: false,
  error: null,
};

/** How long the pill shows "ready" before it goes away. */
export const READY_MS = 3000;
const RECONNECT_MS = 2000;

let current: ModelDownload | null = null;
const listeners = new Set<() => void>();

function set(next: ModelDownload | null) {
  current = next;
  listeners.forEach((l) => l());
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useModelDownload(): ModelDownload | null {
  return useSyncExternalStore(subscribe, () => current);
}

function withProgress(patch: Partial<DownloadState>) {
  if (current) set({ ...current, progress: { ...current.progress, ...patch } });
}

export async function beginDownload(model: string): Promise<void> {
  set({ model, progress: INITIAL, dialogOpen: true });
  try {
    await startDownload(model);
  } catch (e) {
    // 409: the server already runs this download, so its frames will fill in the progress.
    const msg = e instanceof Error ? e.message : String(e);
    if (!msg.startsWith("409") && current?.model === model) {
      withProgress({ error: msg });
    }
  }
}

export function showDownloadDialog() {
  if (current) set({ ...current, dialogOpen: true });
}

/** Closing on an error dismisses the failed download. A running one keeps going in the background. */
export function hideDownloadDialog() {
  if (!current) return;
  set(current.progress.error ? null : { ...current, dialogOpen: false });
}

export async function cancelActiveDownload(): Promise<void> {
  if (!current) return;
  const { model } = current;
  set(null);
  try {
    await cancelDownload(model);
  } catch {
    // 409: it finished or failed first, and that frame arrives on its own.
  }
}

export function dismissDownload() {
  set(null);
}

export function applyDownloadEvent(ev: DownloadEvent, onModelsChanged: () => void) {
  if (ev.type !== "stt_model_download_progress") onModelsChanged();
  if (current && current.model !== ev.model) return;
  if (ev.type === "stt_model_download_cancelled") {
    set(null);
    return;
  }
  if (!current) set({ model: ev.model, progress: INITIAL, dialogOpen: false });

  if (ev.type === "stt_model_download_progress") {
    withProgress({
      bytesDownloaded: ev.bytes_downloaded,
      totalBytes: ev.total_bytes,
      bytesPerSec: ev.bytes_per_sec,
      etaSeconds: ev.eta_seconds,
      error: null,
    });
  } else if (ev.type === "stt_model_download_complete") {
    withProgress({ complete: true, error: null });
    window.setTimeout(() => {
      if (current?.model === ev.model && current.progress.complete) set(null);
    }, READY_MS);
  } else {
    withProgress({ error: ev.error });
  }
}

/** A frame can be missed while the socket is down, and a server restart kills a download without one. Ask the server instead. */
async function reconcile() {
  if (!current || current.progress.complete || current.progress.error) return;
  const { model } = current;
  const m = (await fetchModels()).find((x) => x.name === model);
  if (!m || m.downloading || current?.model !== model) return;
  if (m.downloaded) withProgress({ complete: true });
  else withProgress({ error: "The download stopped before it finished." });
}

/** Mounted once, in the Shell. It also carries the Granola import frames, which share the one socket. */
export function useModelDownloadEvents() {
  const qc = useQueryClient();

  useEffect(() => {
    let cancelled = false;
    let ws: WebSocket | null = null;
    let retry: number | undefined;
    const onModelsChanged = () =>
      qc.invalidateQueries({ queryKey: sttKeys.models });

    const connect = async (isReconnect: boolean) => {
      let token: string;
      try {
        token = await ensureSessionToken();
      } catch {
        if (!cancelled) retry = window.setTimeout(() => connect(true), RECONNECT_MS);
        return;
      }
      if (cancelled) return;

      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      ws = new WebSocket(
        `${proto}//${window.location.host}/ws?token=${encodeURIComponent(token)}`,
      );
      ws.onopen = () => {
        importApi.status().then(applyImportStatus, () => {});
        if (!isReconnect) return;
        onModelsChanged();
        reconcile().catch(() => {});
      };
      ws.onmessage = (event) => {
        let frame: unknown;
        try {
          frame = JSON.parse(event.data as string);
        } catch {
          return;
        }
        const type = String((frame as { type?: unknown } | null)?.type);
        if (type.startsWith("stt_model_download_")) {
          applyDownloadEvent(frame as DownloadEvent, onModelsChanged);
        } else if (type.startsWith("import_")) {
          applyImportEvent(frame as ImportEvent);
        }
      };
      ws.onclose = () => {
        if (!cancelled) retry = window.setTimeout(() => connect(true), RECONNECT_MS);
      };
    };
    connect(false);

    return () => {
      cancelled = true;
      window.clearTimeout(retry);
      if (ws) {
        ws.onopen = null;
        ws.onmessage = null;
        ws.onclose = null;
        if (
          ws.readyState === WebSocket.CONNECTING ||
          ws.readyState === WebSocket.OPEN
        ) {
          ws.close();
        }
      }
    };
  }, [qc]);
}
