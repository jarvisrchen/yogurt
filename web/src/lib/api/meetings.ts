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

/** Palette keys understood by `LabelChip` — mirrors Rust `labels::COLORS`. */
export type LabelColor = "blue" | "matcha" | "straw" | "lilac" | "honey" | "slate";

/** Wire shape of one label row, matching `yogurt_db::Label`. */
export interface Label {
  id: string;
  name: string;
  color: LabelColor;
}

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
  /**
   * Which STT engine transcribed this meeting, e.g. "local · small.en" or
   * "cloud · nova-3". `null` for meetings recorded before this field
   * existed, or if the best-effort stamp on start failed.
   */
  stt_engine: string | null;
  /** LLM that produced `enriched_md` (e.g. "MiniMax-Text-01"); `null` until enhanced. */
  llm_model: string | null;
  /** Note format that shaped `enriched_md` (e.g. "standup"); `null` until enhanced. */
  template: string | null;
  /** ISO 8601 UTC string. */
  created_at: string;
  /** ISO 8601 UTC string. */
  updated_at: string;
  /** Labels attached to this meeting, sorted by name. */
  labels: Label[];
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
  /** Plain optional semantics (not tri-state) — mirrors Rust `notes_md`. */
  stt_engine?: string;
  /** Replace this meeting's label set with exactly these ids. */
  label_ids?: string[];
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
export async function json<T>(input: string, init?: RequestInit): Promise<T> {
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
  delete: (id: string, deleteFile: boolean) =>
    json<void>(`/api/meetings/${id}?delete_file=${deleteFile}`, {
      method: "DELETE",
    }),
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
export function useDeleteMeeting(): UseMutationResult<
  void,
  Error,
  { id: string; deleteFile: boolean }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, deleteFile }) => meetingsApi.delete(id, deleteFile),
    onSuccess: (_void, { id }) => {
      qc.invalidateQueries({ queryKey: meetingsKey });
      qc.removeQueries({ queryKey: meetingKey(id) });
    },
  });
}

/**
 * `PATCH /api/meetings/:id { starred }` — star / unstar a meeting from
 * the Library card hover actions. Invalidates the list and primes the
 * individual cache so a follow-up `useMeeting(id)` doesn't refetch.
 */
export function useToggleStarred(): UseMutationResult<
  Meeting,
  Error,
  { id: string; starred: boolean }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, starred }) => meetingsApi.patch(id, { starred }),
    onSuccess: (m) => {
      qc.invalidateQueries({ queryKey: meetingsKey });
      qc.setQueryData(meetingKey(m.id), m);
    },
  });
}

/**
 * `PATCH /api/meetings/:id { label_ids }` — replace a meeting's label set.
 * Invalidates the meetings list (covers search keys via shared prefix) AND
 * the labels list (label counts change), and primes the individual
 * meeting cache with the returned row.
 */
export function useSetMeetingLabels(): UseMutationResult<
  Meeting,
  Error,
  { id: string; label_ids: string[] }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, label_ids }) => meetingsApi.patch(id, { label_ids }),
    onSuccess: (m) => {
      qc.invalidateQueries({ queryKey: meetingsKey });
      // Literal (not imported from ./labels) to avoid a meetings.ts <->
      // labels.ts circular import — TanStack matches query keys
      // structurally, so this still invalidates `labelsKey` there.
      qc.invalidateQueries({ queryKey: ["labels"] });
      qc.setQueryData(meetingKey(m.id), m);
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

// ─── "Return to recording" floating pill ───────────────────────────────────

/** Wire shape of `GET /api/meetings/active`'s non-null body. */
export interface ActiveRecording {
  id: string;
  title: string;
  /** Unix milliseconds — recording-start clock. */
  started_at: number;
  /**
   * Which STT engine `select_stt` actually resolved to for this recording.
   * Omitted (not `null`) when the server hasn't recorded one yet. Settings
   * only apply at the *next* start, so this can differ from the current
   * Settings page value mid-recording — this field is the truthful one.
   */
  stt?: "cloud" | "local";
  /** AUD-6: whether the mic is currently paused. `Channel::System` keeps
   *  recording regardless — this only reflects the mic. */
  mic_muted: boolean;
  /** AUD-11: whether the mic is currently being echoed to an output device. */
  echo_enabled: boolean;
}

export const activeRecordingKey = ["meetings", "active"] as const;

/**
 * `GET /api/meetings/active` — the single currently-recording meeting, if
 * any. Powers `<RecordingPill>` so the user always has a way back to a live
 * recording after navigating elsewhere (recording continues server-side
 * regardless of what the browser is showing). Polls every 5s and on window
 * focus so the pill appears promptly.
 */
export function useActiveRecording(): UseQueryResult<ActiveRecording | null, Error> {
  return useQuery({
    queryKey: activeRecordingKey,
    queryFn: () => json<ActiveRecording | null>("/api/meetings/active"),
    refetchInterval: 5_000,
    refetchOnWindowFocus: true,
  });
}

// ─── "Meeting detected" prompt (MTG-11) ────────────────────────────────────

/** Wire shape of `GET /api/meetings/detected`'s non-null body — mirrors
 *  `yogurt_audio::detect::DetectedMeeting`. */
export interface DetectedMeeting {
  /** `CGWindowID` of the matched window. Identity for "same call". */
  window_id: number;
  /** App label for the prompt, e.g. `"Zoom"` / `"Google Meet"`. */
  app: string;
  /** The matched window title, e.g. `"Meet - abc-defg-hij"`. */
  title: string;
}

export const detectedMeetingKey = ["meetings", "detected"] as const;

/**
 * `GET /api/meetings/detected` — the meeting-app window the server's
 * detection watcher last saw, or `null` when there is nothing to offer.
 *
 * The server already returns `null` while a recording is running and while
 * the current window is dismissed, so the component has no "should this
 * show" logic of its own. Same 5s cadence as `useActiveRecording` — it is
 * bounded by the watcher's own poll interval anyway.
 */
export function useDetectedMeeting(): UseQueryResult<DetectedMeeting | null, Error> {
  return useQuery({
    queryKey: detectedMeetingKey,
    queryFn: () => json<DetectedMeeting | null>("/api/meetings/detected"),
    refetchInterval: 5_000,
    refetchOnWindowFocus: true,
  });
}

/** `POST /api/meetings/detected/dismiss` — silence the prompt for the call
 *  currently on screen. The next different meeting window prompts again. */
export function useDismissDetectedMeeting(): UseMutationResult<void, Error, void> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      await json<{ status: string }>("/api/meetings/detected/dismiss", {
        method: "POST",
      });
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: detectedMeetingKey }),
  });
}
