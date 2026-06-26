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

// ─── Phase 7 Plan 07-02 — FTS5 search ──────────────────────────────────────

/** Cache key for `useMeetingsSearch`. Includes the trimmed query so each
 *  distinct search has its own cache slot (and clearing the input falls
 *  through to the chronological `useMeetings` cache). */
export const meetingsSearchKey = (q: string) =>
  ["meetings", "search", q] as const;

/**
 * `GET /api/meetings/search?q=…`. Empty / whitespace-only `q` disables the
 * query — the Library route then renders the `useMeetings()` chronological
 * feed instead. 5s staleTime matches `useMeetings`.
 *
 * The hook uses the same `json<T>()` wrapper as the other meeting hooks
 * so the bootstrap session token is attached automatically.
 */
export function useMeetingsSearch(
  q: string,
): UseQueryResult<Meeting[], Error> {
  const trimmed = q.trim();
  return useQuery({
    queryKey: meetingsSearchKey(trimmed),
    queryFn: () =>
      json<Meeting[]>(`/api/meetings/search?q=${encodeURIComponent(trimmed)}`),
    staleTime: 5_000,
    enabled: trimmed.length > 0,
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

// ─── Phase 7 Plan 07-03 — inline-edit title + Copy markdown + Reveal ───────

/**
 * `PATCH /api/meetings/:id` specialized for inline title rename.
 *
 * Empty / whitespace-only input is normalized to "Untitled meeting" on the
 * client so the optimistic cache write reflects the same fallback the
 * server applies (LIB-08). Invalidates the list and primes the individual
 * cache so a follow-up `useMeeting(id)` doesn't refetch.
 */
export function useUpdateMeetingTitle(): UseMutationResult<
  Meeting,
  Error,
  { id: string; title: string }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, title }) => {
      const t = title.trim().length === 0 ? "Untitled meeting" : title.trim();
      return meetingsApi.patch(id, { title: t });
    },
    onSuccess: (m) => {
      qc.invalidateQueries({ queryKey: meetingsKey });
      qc.setQueryData(meetingKey(m.id), m);
    },
  });
}

/**
 * `GET /api/meetings/:id/markdown` → `navigator.clipboard.writeText`.
 *
 * Returns the on-disk Phase-4 MarkdownExporter file contents (YAML
 * front-matter + body). Uses the bearer-token-attached `fetch` path
 * directly rather than `json<T>()` because the response is `text/markdown`,
 * not JSON.
 *
 * Throws on non-2xx so callers can surface a toast. Clipboard write may
 * throw on browsers without `navigator.clipboard` (HTTP/non-localhost) —
 * yogurt runs on localhost so the secure-context requirement is satisfied.
 */
export async function copyMeetingMarkdown(id: string): Promise<void> {
  const token = await ensureSessionToken();
  const res = await fetch(`/api/meetings/${id}/markdown`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  const md = await res.text();
  await navigator.clipboard.writeText(md);
}

/**
 * `POST /api/meetings/:id/reveal` — server shells out to `open -R` on
 * macOS to reveal the on-disk markdown file in Finder. Returns 204; no
 * body to parse.
 */
export async function revealMeetingInFinder(id: string): Promise<void> {
  await json<void>(`/api/meetings/${id}/reveal`, { method: "POST" });
}
