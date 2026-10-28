import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { bearerFetch, bearerJson } from "../session";

export type PaletteColor = "blue" | "matcha" | "straw" | "lilac" | "honey" | "slate";

/** A palette key or a custom `#rrggbb` hex string. `string & {}` keeps palette autocomplete. */
export type LabelColor = PaletteColor | (string & {});

export interface Label {
  id: string;
  name: string;
  color: LabelColor;
}

/** Provenance of an imported meeting. `owner.is_user` is set by `GET /api/meetings/{id}` only. */
export interface MeetingSourceJson {
  owner?: { name?: string | null; email?: string | null; is_user?: boolean };
  web_url?: string;
  updated_at?: string;
  imported_at?: string;
}

export interface Meeting {
  id: string;
  title: string;
  /** Unix ms. */
  started_at: number;
  /** Unix ms; null while the meeting is live. */
  ended_at: number | null;
  notes_md: string;
  enriched_md: string | null;
  transcript_json: string;
  starred: boolean;
  /** E.g. "local · small.en". `null` for meetings recorded before this field existed or when the stamp failed. */
  stt_engine: string | null;
  /** LLM that produced `enriched_md` (e.g. "MiniMax-Text-01"); `null` until enhanced. */
  llm_model: string | null;
  /** Note format that shaped `enriched_md` (e.g. "standup"); `null` until enhanced. */
  template: string | null;
  /** E.g. "granola". Absent for a meeting recorded in yogurt. */
  source?: string;
  source_id?: string;
  source_json?: MeetingSourceJson;
  /** ISO 8601 UTC string. Absent while the meeting is not archived. */
  archived_at?: string;
  /** ISO 8601 UTC string. */
  created_at: string;
  /** ISO 8601 UTC string. */
  updated_at: string;
  labels: Label[];
}

/** One Library row: no note or transcript bodies. `GET /api/meetings/{id}` returns the full `Meeting`. */
export interface MeetingSummary {
  id: string;
  title: string;
  /** Unix ms. */
  started_at: number;
  /** Unix ms; null while the meeting is live. */
  ended_at: number | null;
  starred: boolean;
  stt_engine: string | null;
  llm_model: string | null;
  /** `enriched_md` is set. */
  has_enriched: boolean;
  labels: Label[];
  source?: string;
  /** Who captured an imported meeting: the owner's name, or their email. */
  source_owner?: string;
}

/** `total` counts every match before paging. */
export interface MeetingsPage {
  meetings: MeetingSummary[];
  total: number;
}

export interface MeetingsParams {
  /** 1-based. */
  page: number;
  size: number;
  labelId: string | null;
  starred: boolean;
  /** Only archived meetings, which every other view leaves out. */
  archived?: boolean;
  /** Trimmed. Empty lists chronologically instead of searching. */
  q: string;
}

/** See Rust `MeetingPatch` for the tri-state semantics. */
export interface MeetingPatch {
  title?: string;
  started_at?: number;
  notes_md?: string;
  transcript_json?: string;
  /** `null` clears, `undefined` leaves alone (TanStack will skip the key). */
  enriched_md?: string | null;
  ended_at?: number | null;
  starred?: boolean;
  archived?: boolean;
  /** Plain optional semantics (not tri-state). */
  stt_engine?: string;
  label_ids?: string[];
}

export const meetingsKey = ["meetings"] as const;
export const meetingKey = (id: string) => ["meetings", id] as const;

export const json = bearerJson;

export const meetingsApi = {
  list: ({ page, size, labelId, starred, archived, q }: MeetingsParams) => {
    const qs = new URLSearchParams({
      limit: String(size),
      offset: String((page - 1) * size),
    });
    if (q) qs.set("q", q);
    if (labelId) qs.set("label_id", labelId);
    if (starred) qs.set("starred", "true");
    if (archived) qs.set("archived", "true");
    return json<MeetingsPage>(`/api/meetings${q ? "/search" : ""}?${qs}`);
  },
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
  /** Archives every meeting that started before `before` (unix ms) except the one recording, or with `preview` only counts them. */
  archiveBefore: (before: number, preview: boolean) =>
    json<{ count: number }>("/api/meetings/archive", {
      method: "POST",
      body: JSON.stringify({ before, preview }),
    }),
  delete: (id: string, deleteFile: boolean) =>
    json<void>(`/api/meetings/${id}?delete_file=${deleteFile}`, {
      method: "DELETE",
    }),
};

/** The key starts with `meetingsKey`, so invalidating that refetches every cached page. `keepPreviousData` holds the old page on screen while the next loads. */
export function useMeetings(
  params: MeetingsParams,
  enabled = true,
): UseQueryResult<MeetingsPage, Error> {
  return useQuery({
    queryKey: ["meetings", params],
    queryFn: () => meetingsApi.list(params),
    staleTime: 5_000,
    placeholderData: keepPreviousData,
    enabled,
  });
}

export function useMeeting(id: string | undefined): UseQueryResult<Meeting, Error> {
  return useQuery({
    queryKey: id ? meetingKey(id) : ["meetings", "__missing"],
    queryFn: () => meetingsApi.get(id!),
    enabled: !!id,
    staleTime: 5_000,
  });
}

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
      qc.invalidateQueries({ queryKey: ["labels"] });
      qc.removeQueries({ queryKey: meetingKey(id) });
    },
  });
}

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

/** Label counts change too. */
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
      // Literal rather than imported from ./labels to avoid a circular import. TanStack matches keys structurally.
      qc.invalidateQueries({ queryKey: ["labels"] });
      qc.setQueryData(meetingKey(m.id), m);
    },
  });
}

// No bulk server route: both hooks fan out over the per-meeting endpoints with `Promise.allSettled` and invalidate
// once regardless of partial failure. A rejected `Promise.all` would abort the batch and leave meetings that
// succeeded server-side stale in the cache.
export function useBulkDeleteMeetings(): UseMutationResult<
  void,
  Error,
  { ids: string[]; deleteFile: boolean }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ ids, deleteFile }) => {
      const results = await Promise.allSettled(
        ids.map((id) => meetingsApi.delete(id, deleteFile)),
      );
      const failed = results.filter((r) => r.status === "rejected");
      if (failed.length > 0) {
        throw new Error(
          `Failed to delete ${failed.length} of ${ids.length} meetings`,
        );
      }
    },
    // onSettled (not onSuccess): a partial failure still needs the
    // successfully-deleted meetings reflected in the list.
    onSettled: (_data, _error, { ids }) => {
      qc.invalidateQueries({ queryKey: meetingsKey });
      qc.invalidateQueries({ queryKey: ["labels"] });
      for (const id of ids) qc.removeQueries({ queryKey: meetingKey(id) });
    },
  });
}

/** Each meeting's next label set is computed from its own labels, so it is safe over meetings that already have or lack the label. */
export function useBulkSetLabel(): UseMutationResult<
  void,
  Error,
  { meetings: Pick<Meeting, "id" | "labels">[]; labelId: string; mode: "add" | "remove" }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ meetings, labelId, mode }) => {
      const results = await Promise.allSettled(
        meetings.map((m) => {
          const ids = new Set(m.labels.map((l) => l.id));
          if (mode === "add") ids.add(labelId);
          else ids.delete(labelId);
          return meetingsApi.patch(m.id, { label_ids: [...ids] });
        }),
      );
      const failed = results.filter((r) => r.status === "rejected");
      if (failed.length > 0) {
        throw new Error(
          `Failed to update labels on ${failed.length} of ${meetings.length} meetings`,
        );
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: meetingsKey });
      qc.invalidateQueries({ queryKey: ["labels"] });
    },
    onError: (err) => console.error("bulk label update failed", err),
  });
}

/** Fans out one PATCH per meeting like the bulk hooks above. The server refuses the meeting being recorded with a 409. */
export function useSetArchived(): UseMutationResult<
  void,
  Error,
  { ids: string[]; archived: boolean }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ ids, archived }) => {
      const results = await Promise.allSettled(
        ids.map((id) => meetingsApi.patch(id, { archived })),
      );
      for (const r of results) {
        if (r.status === "fulfilled") qc.setQueryData(meetingKey(r.value.id), r.value);
      }
      const failed = results.filter((r) => r.status === "rejected");
      if (failed.length > 0) {
        throw new Error(
          `Failed to ${archived ? "archive" : "unarchive"} ${failed.length} of ${ids.length} meetings`,
        );
      }
    },
    onSettled: () => qc.invalidateQueries({ queryKey: meetingsKey }),
    onError: (err) => console.error("archive failed", err),
  });
}

/** How many meetings `useArchiveBefore` would archive. Keyed under `meetingsKey` so any meeting change refetches it. */
export function useArchivePreview(
  before: number | null,
): UseQueryResult<{ count: number }, Error> {
  return useQuery({
    queryKey: ["meetings", "archive-preview", before],
    queryFn: () => meetingsApi.archiveBefore(before!, true),
    enabled: before != null,
    placeholderData: keepPreviousData,
  });
}

export function useArchiveBefore(): UseMutationResult<{ count: number }, Error, number> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (before: number) => meetingsApi.archiveBefore(before, false),
    onSettled: () => qc.invalidateQueries({ queryKey: meetingsKey }),
  });
}

/** Blank input becomes "Untitled meeting" so the optimistic cache write matches the server's fallback. */
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

/** Uses `bearerFetch` directly because the response is `text/markdown`. The clipboard write needs a secure context, which localhost satisfies. */
export async function copyMeetingMarkdown(id: string): Promise<void> {
  const res = await bearerFetch(`/api/meetings/${id}/markdown`);
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  const md = await res.text();
  await navigator.clipboard.writeText(md);
}

/** The server shells out to `open -R` on macOS to reveal the file in Finder. */
export async function revealMeetingInFinder(id: string): Promise<void> {
  await json<void>(`/api/meetings/${id}/reveal`, { method: "POST" });
}

export interface ActiveRecording {
  id: string;
  title: string;
  /** Unix ms. */
  started_at: number;
  /** The engine `select_stt` resolved to. Settings only apply at the next start, so this can differ from the current Settings value mid-recording. */
  stt?: "cloud" | "local";
  /** `Channel::System` keeps recording while the mic is paused. */
  mic_muted: boolean;
  echo_enabled: boolean;
  /** Unix ms at which the server stops this recording for silence. Non-null
   *  only during the warning window. */
  auto_stop_at: number | null;
}

export const activeRecordingKey = ["meetings", "active"] as const;

/** Powers `<RecordingPill>`. Recording continues server-side regardless of the browser, so this is the way back to it. */
export function useActiveRecording(): UseQueryResult<ActiveRecording | null, Error> {
  return useQuery({
    queryKey: activeRecordingKey,
    queryFn: () => json<ActiveRecording | null>("/api/meetings/active"),
    refetchInterval: 5_000,
    // A call keeps this tab in the background, and the silence warning and
    // server-side stop must still reach the page.
    refetchIntervalInBackground: true,
    refetchOnWindowFocus: true,
  });
}

/** `POST /api/meetings/:id/keep-recording`: dismiss the silence warning. */
export function useKeepRecording(): UseMutationResult<void, Error, string> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      await json<{ status: string }>(`/api/meetings/${id}/keep-recording`, {
        method: "POST",
      });
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: activeRecordingKey }),
  });
}

export interface DetectedMeeting {
  /** `CGWindowID` of the matched window. Identity for "same call". */
  window_id: number;
  /** App label for the prompt, e.g. `"Zoom"` / `"Google Meet"`. */
  app: string;
  /** The matched window title, e.g. `"Meet - abc-defg-hij"`. */
  title: string;
}

export const detectedMeetingKey = ["meetings", "detected"] as const;

/** The server already returns `null` while recording and while the current window is dismissed, so the component needs no "should this show" logic. */
export function useDetectedMeeting(): UseQueryResult<DetectedMeeting | null, Error> {
  return useQuery({
    queryKey: detectedMeetingKey,
    queryFn: () => json<DetectedMeeting | null>("/api/meetings/detected"),
    refetchInterval: 5_000,
    refetchOnWindowFocus: true,
  });
}

/** `POST /api/meetings/detected/dismiss`: silence the prompt for the call
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
