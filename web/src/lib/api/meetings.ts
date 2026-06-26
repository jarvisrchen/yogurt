/**
 * Typed fetch wrappers + TanStack-Query hooks for the Phase 7 Library
 * REST surface (`/api/meetings*` from `crates/yogurt-server/src/api/meetings.rs`).
 *
 * Phase 7 / Plan 07-01.
 */

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { ensureSessionToken } from "../session";

// ─── Types (mirror yogurt_db::Meeting) ─────────────────────────────────────

/**
 * Wire shape of one meeting row, matching `yogurt_db::Meeting` after serde
 * serialization. The server stamps `created_at` / `updated_at` as ISO 8601
 * UTC strings; everything else is a primitive.
 */
export interface Meeting {
  id: string;
  title: string;
  /** Unix milliseconds — recording-start clock. */
  started_at: number;
  /** Unix milliseconds; null while the meeting is live. */
  ended_at: number | null;
  notes_md: string;
  enriched_md: string | null;
  transcript_json: string;
  starred: boolean;
  /** ISO 8601 UTC string. */
  created_at: string;
  /** ISO 8601 UTC string. */
  updated_at: string;
}

/** Patch shape — see Rust `MeetingPatch` for the tri-state semantics. */
export interface MeetingPatch {
  title?: string;
  started_at?: number;
  notes_md?: string;
  transcript_json?: string;
  /** `null` clears, `undefined` leaves alone (TanStack will skip the key). */
  enriched_md?: string | null;
  ended_at?: number | null;
  starred?: boolean;
}

// ─── Cache keys ────────────────────────────────────────────────────────────

export const meetingsKey = ["meetings"] as const;
export const meetingKey = (id: string) => ["meetings", id] as const;

// ─── HTTP helper ────────────────────────────────────────────────────────────

/**
 * Thin `fetch` wrapper that attaches the bootstrap session token and
 * throws on non-2xx, returns `undefined` for 204, otherwise parses JSON.
 *
 * The token is fetched lazily via `ensureSessionToken()` (Phase 0 helper)
 * and cached for the lifetime of the page.
 */
async function json<T>(input: string, init?: RequestInit): Promise<T> {
  const token = await ensureSessionToken();
  const headers = new Headers(init?.headers ?? {});
  headers.set("Authorization", `Bearer ${token}`);
  if (init?.body != null && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  const res = await fetch(input, { ...init, headers });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  if (res.status === 204) return undefined as unknown as T;
  return (await res.json()) as T;
}

// ─── Raw API (exported for non-hook callers + tests) ───────────────────────

export const meetingsApi = {
  list: () => json<Meeting[]>("/api/meetings"),
  get: (id: string) => json<Meeting>(`/api/meetings/${id}`),
  create: (body: { title?: string; started_at_unix_ms?: number } = {}) =>
    json<Meeting>("/api/meetings", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  patch: (id: string, patch: MeetingPatch) =>
    json<Meeting>(`/api/meetings/${id}`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    }),
  delete: (id: string) =>
    json<void>(`/api/meetings/${id}`, { method: "DELETE" }),
};

// ─── React-Query hooks ─────────────────────────────────────────────────────

/**
 * `GET /api/meetings`. 5s staleTime — the Library re-fetches lazily as
 * the user creates / deletes meetings via the other hooks (which
 * invalidate this key).
 */
export function useMeetings(): UseQueryResult<Meeting[], Error> {
  return useQuery({
    queryKey: meetingsKey,
    queryFn: meetingsApi.list,
    staleTime: 5_000,
  });
}

/** `GET /api/meetings/:id`. Useful for direct-link hydration. */
export function useMeeting(id: string | undefined): UseQueryResult<Meeting, Error> {
  return useQuery({
    queryKey: id ? meetingKey(id) : ["meetings", "__missing"],
    queryFn: () => meetingsApi.get(id!),
    enabled: !!id,
    staleTime: 5_000,
  });
}

/** `POST /api/meetings`. Invalidates the list on success. */
export function useCreateMeeting(): UseMutationResult<
  Meeting,
  Error,
  string | undefined
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (title?: string) =>
      meetingsApi.create(title ? { title } : {}),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: meetingsKey });
    },
  });
}

/** `DELETE /api/meetings/:id`. Invalidates list + the specific row. */
export function useDeleteMeeting(): UseMutationResult<void, Error, string> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => meetingsApi.delete(id),
    onSuccess: (_void, id) => {
      qc.invalidateQueries({ queryKey: meetingsKey });
      qc.removeQueries({ queryKey: meetingKey(id) });
    },
  });
}
