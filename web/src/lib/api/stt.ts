/**
 * Typed fetch wrappers + TanStack-Query hooks for the whisper.cpp model
 * REST surface (Phase 8 Plan 08-03).
 *
 * Mirrors `crates/yogurt-server/src/api/stt_models.rs`:
 *
 * | Method | Path                                  | Returns                            |
 * |--------|---------------------------------------|-------------------------------------|
 * | GET    | `/api/stt/models`                     | `ModelView[]`                       |
 * | POST   | `/api/stt/models/:name/download`      | 202 Accepted (no body)              |
 * | DELETE | `/api/stt/models/:name`               | 200 `{ freed_bytes }` (idempotent - 0 if already gone); 404 unknown name; 409 plain-text if it's the active local model, or if the only copy is Homebrew-installed |
 *
 * The download endpoint is fire-and-forget — progress + terminal state
 * arrive over the app-wide `/ws` WebSocket as
 * `stt_model_download_progress` / `_complete` / `_error` events. See
 * `web/src/hooks/useModelDownloadProgress.ts`.
 *
 * All three routes mount behind `require_session_token` (Phase 0 WR-06),
 * so every call attaches the bootstrap token via `bearerFetch`.  The
 * original comment claimed "no auth header sent" — that was wrong and
 * caused the SPA to 403 every poll until the helper was wired in.
 */

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { bearerFetch } from "../session";

// ─── Types ──────────────────────────────────────────────────────────────────

/** Whisper-rs / whisper.cpp model identifiers shipped in
 *  `yogurt_stt::models::REGISTRY`. The Rust handler returns one entry
 *  per `ModelSpec` in REGISTRY order. */
export type ModelName =
  | "tiny.en"
  | "small.en"
  | "medium.en"
  | "large-v3-turbo"
  | "large-v3";

/** Wire-shape mirror of
 *  `crates/yogurt-server/src/api/stt_models.rs::ModelView`. */
export interface ModelView {
  name: ModelName;
  size_mb: number;
  downloaded: boolean;
  intel_supported: boolean;
  /** The verified copy lives in a Homebrew prefix, not yogurt's own
   *  download dir (AUD-4). DELETE refuses these - the picker shows a
   *  "brew" chip where the trash icon would be. */
  managed_by_homebrew: boolean;
}

// ─── HTTP helper ────────────────────────────────────────────────────────────

async function http<T>(input: string, init?: RequestInit): Promise<T> {
  const res = await bearerFetch(input, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  // 202 Accepted (download start) and 204 No Content (delete) have no body.
  if (res.status === 202 || res.status === 204) {
    return undefined as unknown as T;
  }
  return res.json() as Promise<T>;
}

// ─── REST client ────────────────────────────────────────────────────────────

export async function fetchModels(): Promise<ModelView[]> {
  return http<ModelView[]>("/api/stt/models");
}

async function startDownload(name: ModelName | string): Promise<void> {
  return http<void>(
    `/api/stt/models/${encodeURIComponent(name)}/download`,
    { method: "POST" },
  );
}

async function deleteModel(
  name: ModelName | string,
): Promise<{ freed_bytes: number }> {
  return http<{ freed_bytes: number }>(
    `/api/stt/models/${encodeURIComponent(name)}`,
    { method: "DELETE" },
  );
}

/** Format a byte count exactly the way the pill size chip in `ModelPicker`
 *  formats `size_mb`, so "freed 487 MB" matches the "487 MB" chip that
 *  was just deleted: `size_mb` is decimal (bytes / 1e6), whole MB below
 *  1000, else one-decimal GB via / 1024. */
export function formatBytes(n: number): string {
  const mb = n / 1_000_000;
  return mb < 1000 ? `${Math.floor(mb)} MB` : `${(mb / 1024).toFixed(1)} GB`;
}

// ─── Query keys ─────────────────────────────────────────────────────────────

/** Shared cache key — `useModels`, the WS-progress hook, and the download
 *  mutation all read/invalidate it. Exported so the WS hook can call
 *  `qc.invalidateQueries({ queryKey: sttKeys.models })` from outside. */
export const sttKeys = {
  models: ["stt", "models"] as const,
};

// ─── React-Query hooks ──────────────────────────────────────────────────────

/** Cached `GET /api/stt/models`. The list is small (4 entries) and the
 *  "downloaded" boolean is the only field that mutates — the WS hook
 *  invalidates on `stt_model_download_complete`, so cached `staleTime`
 *  can be generous. */
export function useModels(): UseQueryResult<ModelView[], Error> {
  return useQuery({
    queryKey: sttKeys.models,
    queryFn: fetchModels,
    staleTime: 30_000,
  });
}

/** `POST /api/stt/models/:name/download` — fire-and-forget. The mutation
 *  resolves on the 202; actual progress comes over the WebSocket. We
 *  still invalidate `sttKeys.models` on success so any in-flight refetch
 *  picks up the new file once the WS hook fires `_complete`. */
export function useDownloadModel(): UseMutationResult<
  void,
  Error,
  ModelName | string
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: startDownload,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: sttKeys.models });
    },
  });
}

/** `DELETE /api/stt/models/:name` — idempotent (404 on the file is
 *  treated as already-deleted server-side). Resolves with `freed_bytes`
 *  so the caller can show "freed N GB"; rejects with the server's 409
 *  message when the model is the active local one. */
export function useDeleteModel(): UseMutationResult<
  { freed_bytes: number },
  Error,
  ModelName | string
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: deleteModel,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: sttKeys.models });
    },
  });
}
