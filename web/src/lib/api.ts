import { bearerFetch } from "./session";

export interface HealthResponse {
  status: string;
  service: string;
}

export type ChatRole = "user" | "assistant";

export interface ChatMessage {
  id: string;
  meeting_id: string;
  role: ChatRole;
  content: string;
  /** Unix ms. */
  created_at: number;
  /** Cross-meeting chat only: the meetings the answer may cite as `[n]`, numbered from 1. */
  sources?: ChatSource[];
}

export interface ChatSource {
  id: string;
  title: string;
  /** Unix ms. */
  started_at: number;
}

/** `POST /api/chat`: one cross-meeting turn. Calls `onSources` once, then `onDelta` per streamed chunk. */
export async function streamLibraryChat(
  messages: { role: ChatRole; content: string }[],
  onSources: (sources: ChatSource[]) => void,
  onDelta: (delta: string) => void,
): Promise<void> {
  const res = await bearerFetch("/api/chat", {
    method: "POST",
    body: JSON.stringify({ messages }),
  });
  if (!res.ok || !res.body) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `chat failed: ${res.status} ${res.statusText}`);
  }
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += value;
    const lines = buf.split("\n");
    buf = lines.pop()!;
    for (const line of lines) {
      if (!line) continue;
      const frame = JSON.parse(line) as { sources?: ChatSource[]; delta?: string };
      if (frame.sources) onSources(frame.sources);
      if (frame.delta) onDelta(frame.delta);
    }
  }
}

export async function postChatMessage(
  meetingId: string,
  content: string,
  token: string,
): Promise<{ message_id: string }> {
  const res = await bearerFetch(
    `/api/meetings/${meetingId}/chat?token=${encodeURIComponent(token)}`,
    { method: "POST", body: JSON.stringify({ content }) },
  );
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `chat send failed: ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as { message_id: string };
}

export async function fetchChatHistory(
  meetingId: string,
  token: string,
): Promise<ChatMessage[]> {
  const res = await bearerFetch(
    `/api/meetings/${meetingId}/chat?token=${encodeURIComponent(token)}`,
  );
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `chat history failed: ${res.status}`);
  }
  const body = (await res.json()) as { messages: ChatMessage[] };
  return body.messages;
}

export async function fetchHealth(): Promise<HealthResponse> {
  const res = await fetch("/api/health");
  if (!res.ok) throw new Error(`health check failed: ${res.status}`);
  return res.json() as Promise<HealthResponse>;
}

export interface EnhanceRequestBody {
  /** Preserved verbatim across merge. */
  notes_md: string;
  /** JSON-serialized segments `[{ts_ms, channel, text}]`. An empty array is fine. */
  transcript_json: string;
  /** Drives the on-disk markdown filename slug. */
  title?: string;
  started_at_unix_ms?: number;
  ended_at_unix_ms?: number;
  /** Note format id (`GET /api/templates`). Omit to let the model pick; the response's `template` says which. */
  template?: string;
}

export interface EnhanceResponse {
  /** Wire-format markdown with `<span data-ai-grey>` and `<span data-transcript-link>` spans inline. */
  enriched_md: string;
  notes_file: string;
  /** Model that produced `enriched_md`; absent/null when `too_short`. */
  llm_model?: string | null;
  /** Note format used (requested or auto-detected); absent/null when `too_short`. */
  template?: string | null;
  /** No notes and a trivial transcript: enhancing was skipped and `enriched_md` / `notes_file` are empty. */
  too_short?: boolean;
}

// Last-resort guard against a dead socket. The server owns liveness (60s stream-open and per-chunk idle
// timeouts surface as `enhance_progress` error frames), and a healthy stream can run for minutes.
const ENHANCE_FETCH_TIMEOUT_MS = 15 * 60_000;

// The hard timeout keeps a wedged connection from leaving Re-enhance stuck in `busy`.
export async function postEnhance(
  meetingId: string,
  body: EnhanceRequestBody,
  signal?: AbortSignal,
): Promise<EnhanceResponse> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), ENHANCE_FETCH_TIMEOUT_MS);
  const externalAbort = () => ctrl.abort();
  if (signal) {
    if (signal.aborted) ctrl.abort();
    else signal.addEventListener("abort", externalAbort);
  }
  try {
    const res = await bearerFetch(`/api/meetings/${meetingId}/enhance`, {
      method: "POST",
      body: JSON.stringify(body),
      signal: ctrl.signal,
    });
    if (!res.ok) {
      const text = await res.text().catch(() => "");
      throw new Error(
        text || `enhance failed: ${res.status} ${res.statusText}`,
      );
    }
    return (await res.json()) as EnhanceResponse;
  } catch (e) {
    if (ctrl.signal.aborted && !signal?.aborted) {
      throw new Error(
        `enhance timed out after ${ENHANCE_FETCH_TIMEOUT_MS / 1000}s — try Re-enhance`,
      );
    }
    throw e;
  } finally {
    clearTimeout(timer);
    if (signal) signal.removeEventListener("abort", externalAbort);
  }
}
