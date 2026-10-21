// The download endpoints are fire-and-forget: progress and terminal state arrive over `/ws`
// (see `useModelDownload`).
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { bearerFetch } from "../session";

export type ModelName =
  | "tiny.en"
  | "small.en"
  | "medium.en"
  | "large-v3-turbo"
  | "large-v3";

export interface ModelView {
  name: ModelName;
  size_mb: number;
  downloaded: boolean;
  downloading: boolean;
  intel_supported: boolean;
  /** The verified copy lives in a Homebrew prefix. DELETE refuses these, so the picker shows a "brew" chip. */
  managed_by_homebrew: boolean;
}

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

export async function fetchModels(): Promise<ModelView[]> {
  return http<ModelView[]>("/api/stt/models");
}

export async function startDownload(name: ModelName | string): Promise<void> {
  return http<void>(
    `/api/stt/models/${encodeURIComponent(name)}/download`,
    { method: "POST" },
  );
}

/** Rejects with a 409 when the model is not downloading. */
export async function cancelDownload(name: ModelName | string): Promise<void> {
  return http<void>(
    `/api/stt/models/${encodeURIComponent(name)}/download`,
    { method: "DELETE" },
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

/** Matches the `ModelPicker` size chip: `size_mb` is decimal MB, whole MB below 1000, else one-decimal GB. */
export function formatBytes(n: number): string {
  const mb = n / 1_000_000;
  return mb < 1000 ? `${Math.floor(mb)} MB` : `${(mb / 1024).toFixed(1)} GB`;
}

/** Exported so the WS hook can invalidate it from outside. */
export const sttKeys = {
  models: ["stt", "models"] as const,
};

/** The WS hook invalidates on every terminal download frame, so a generous staleTime is safe. */
export function useModels(): UseQueryResult<ModelView[], Error> {
  return useQuery({
    queryKey: sttKeys.models,
    queryFn: fetchModels,
    staleTime: 30_000,
  });
}

/** Idempotent. Rejects with the server's 409 message when the model is the active local one. */
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
