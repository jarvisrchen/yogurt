// Thin HTTP wrappers around the yogurt-server REST surface.
//
// Phase 0 shipped `fetchHealth()`. Phase 4 adds `postEnhance(...)` — the
// browser-side trigger for `POST /api/meetings/{id}/enhance` (CONTEXT D-22).
//
// ## postEnhance signature
//
// The server-side `EnhanceRequest` (crates/yogurt-server/src/enhance.rs:40)
// is `{ notes_md, transcript_json, title?, started_at_unix_ms?,
// ended_at_unix_ms? }` — the body carries what the in-memory `Meeting`
// struct does NOT yet store (per 04-03's documented deviation #5). The
// endpoint is mounted behind `require_session_token` (WR-06), so every
// request MUST send `Authorization: Bearer <token>`.

export interface HealthResponse {
  status: string;
  service: string;
}

// ─── Phase 6 (Plan 06-02): chat REST surface ───────────────────────────────

/**
 * Wire shape for one persisted chat row, matches `yogurt_db::chat::ChatMessage`
 * from `crates/yogurt-db/src/chat.rs`. `created_at` is unix milliseconds.
 */
export type ChatRole = "user" | "assistant";

export interface ChatMessage {
  id: string;
  meeting_id: string;
  role: ChatRole;
  content: string;
  created_at: number;
}

/**
 * POST /api/meetings/{meetingId}/chat — insert a user turn + spawn the
 * LLM stream. Returns the assistant message_id (ULID) which subsequent
 * `chat_chunk` WS events carry. Throws on non-2xx.
 */
export async function postChatMessage(
  meetingId: string,
  content: string,
  token: string,
): Promise<{ message_id: string }> {
  const res = await fetch(
    `/api/meetings/${meetingId}/chat?token=${encodeURIComponent(token)}`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ content }),
    },
  );
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `chat send failed: ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as { message_id: string };
}

/**
 * GET /api/meetings/{meetingId}/chat — hydrate chat history in
 * chronological order. Used on meeting view remount.
 */
export async function fetchChatHistory(
  meetingId: string,
  token: string,
): Promise<ChatMessage[]> {
  const res = await fetch(
    `/api/meetings/${meetingId}/chat?token=${encodeURIComponent(token)}`,
    { headers: { Authorization: `Bearer ${token}` } },
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
  /** User's raw markdown notes — preserved verbatim across merge. */
  notes_md: string;
  /**
   * JSON-serialized transcript segments: `[{ts_ms, channel, text}, …]`.
   * Empty array is fine — the LLM still gets the notes.
   */
  transcript_json: string;
  /** Optional meeting title — drives the on-disk markdown filename slug. */
  title?: string;
  /** Optional meeting start time (unix ms). */
  started_at_unix_ms?: number;
  /** Optional meeting end time (unix ms). */
  ended_at_unix_ms?: number;
}

export interface EnhanceResponse {
  /**
   * Wire-format enriched markdown with `<span data-ai-grey…>` and
   * `<span data-transcript-link…>` spans embedded inline. YogurtEditor
   * round-trips this back into ProseMirror via `setContent(html, false)`.
   */
  enriched_md: string;
  /** Path on disk where the per-meeting markdown file was written. */
  notes_file: string;
  /** Model that produced `enriched_md`; absent/null when `too_short`. */
  llm_model?: string | null;
  /**
   * True when the meeting had no notes and a trivial transcript, so the
   * server skipped enhancing entirely (`enriched_md` / `notes_file` are
   * both empty). The caller shows a "Meeting too short" state instead of
   * a near-empty editor.
   */
  too_short?: boolean;
}

/**
 * Browser-side hard ceiling on a single enhance HTTP round-trip.
 *
 * The server owns liveness: a 60s stream-open timeout and a 60s per-chunk
 * idle timeout (BL-5), both surfaced as `enhance_progress { phase: "error" }`
 * frames well before this ceiling matters. A healthy long stream can
 * legitimately run for minutes (a slow local model summarizing a long
 * meeting), so this is only a last-resort guard against a dead socket that
 * never got so much as a TCP close - not a cap on stream duration.
 */
const ENHANCE_FETCH_TIMEOUT_MS = 15 * 60_000;

/**
 * POST /api/meetings/{meetingId}/enhance.
 *
 * Throws on non-2xx OR on client-side timeout. The thrown Error's message
 * is the server's plaintext body — useful in DevTools without making the
 * caller drill into a typed error shape.
 *
 * BL-5: hard timeout via AbortController so a wedged TCP connection or a
 * never-resolving fetch (rare but documented in fetch-polyfill bugs) can't
 * leave the Re-enhance button permanently in `busy=true`. The button's
 * `finally` will fire, clear `busy`, and the user can retry.
 *
 * @param meetingId The meeting's UUID (returned by POST /api/meetings).
 * @param body      The enhance payload (see `EnhanceRequestBody`).
 * @param token     The session token (from `ensureSessionToken()`).
 * @param signal    Optional external AbortSignal — used by future cancel UI.
 */
export async function postEnhance(
  meetingId: string,
  body: EnhanceRequestBody,
  token: string,
  signal?: AbortSignal,
): Promise<EnhanceResponse> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), ENHANCE_FETCH_TIMEOUT_MS);
  // Forward external aborts.
  const externalAbort = () => ctrl.abort();
  if (signal) {
    if (signal.aborted) ctrl.abort();
    else signal.addEventListener("abort", externalAbort);
  }
  try {
    const res = await fetch(`/api/meetings/${meetingId}/enhance`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
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
