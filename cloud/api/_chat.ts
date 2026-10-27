// POST /api/chat: one turn of chat about one meeting.
// The meeting is read from Supabase with the caller's own session and browser token, so Postgres decides what the caller may read, and a locked browser never reaches the LLM.
// The answer streams back as NDJSON `{"delta":"..."}` lines, and a failure after that point arrives as a final delta, as the desktop's chat does.
import { CHAT_SYSTEM } from "./_prompts.js";
import { ThinkStripper } from "./_think.js";

export interface Env {
  SUPABASE_URL?: string;
  SUPABASE_PUBLISHABLE_KEY?: string;
  YOGURT_LLM_BASE_URL?: string;
  YOGURT_LLM_MODEL?: string;
  YOGURT_LLM_API_KEY?: string;
}

export type Turn = { role: "user" | "assistant"; content: string };
type Message = { role: "system" | Turn["role"]; content: string };

export interface ChatRequest {
  token: string;
  device_id: string;
  meeting_id: string;
  messages: Turn[];
}

const MAX_BODY = 64 * 1024;
const MAX_QUESTION = 4 * 1024;
export const NOT_SET_UP = "Chat isn't set up on this Cloud.";

const bytes = (s: string) => new TextEncoder().encode(s).length;
const json = (status: number, body: object) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
const text = (v: unknown): v is string => typeof v === "string" && v !== "";

/** The request, or why it is refused. */
export function parseRequest(raw: string): ChatRequest | string {
  if (bytes(raw) > MAX_BODY) return "The conversation is too long.";
  let b: Partial<ChatRequest>;
  try {
    b = JSON.parse(raw);
  } catch {
    return "The request is not JSON.";
  }
  if (!text(b?.token) || !text(b.device_id) || !text(b.meeting_id)) return "The request names no meeting.";
  const turns = b.messages;
  if (!Array.isArray(turns) || !turns.every((t) => (t?.role === "user" || t?.role === "assistant") && typeof t.content === "string")) {
    return "Each message needs a user or assistant role and text.";
  }
  const last = turns.at(-1);
  if (last?.role !== "user" || !last.content.trim()) return "The conversation must end with a question.";
  if (bytes(last.content) > MAX_QUESTION) return "The question is too long.";
  return { token: b.token, device_id: b.device_id, meeting_id: b.meeting_id, messages: turns.map(({ role, content }) => ({ role, content })) };
}

/** A port of `transcript_lines` in crates/yogurt-server/src/api/chat.rs. */
export function transcriptLines(transcriptJson: string): string {
  let segs: unknown;
  try {
    segs = JSON.parse(transcriptJson);
  } catch {
    return transcriptJson;
  }
  const isSegs = (v: unknown): v is { ts_ms: number; channel: string; text: string }[] =>
    Array.isArray(v) && v.every((s) => typeof s?.ts_ms === "number" && typeof s.channel === "string" && typeof s.text === "string");
  if (!isSegs(segs)) return transcriptJson;
  if (!segs.length) return "(no transcript captured yet)";
  const pad = (n: number) => String(n).padStart(2, "0");
  return segs
    .map((s) => {
      const total = Math.floor(s.ts_ms / 1000);
      const speaker = s.channel === "me" || s.channel === "mic" ? "me" : "them";
      return `[${pad(Math.floor(total / 60))}:${pad(total % 60)}] ${speaker}: ${s.text}`;
    })
    .join("\n");
}

// Under the function's 60 s limit, so a stalled provider still ends in an error the user sees, as the desktop's 60 s request timeout does.
const LLM_TIMEOUT_MS = 50_000;
const UNREACHABLE = "Chat couldn't reach your Cloud. Try again.";

export async function handleChat(req: Request, rawEnv: Env, fetchImpl: typeof fetch = fetch): Promise<Response> {
  // A value pasted with a stray space or newline would otherwise break a URL, or slip past the key scrub.
  const env = Object.fromEntries(Object.entries(rawEnv).map(([k, v]) => [k, v?.trim()])) as Env;
  try {
    return await respond(req, env, fetchImpl);
  } catch {
    return json(502, { message: UNREACHABLE });
  }
}

async function respond(req: Request, env: Env, fetchImpl: typeof fetch): Promise<Response> {
  if (req.method !== "POST") return json(405, { message: "Use POST." });
  const jwt = /^Bearer (.+)$/.exec(req.headers.get("authorization") ?? "")?.[1];
  if (!jwt) return json(401, { message: "Sign in again." });
  const body = parseRequest(await req.text());
  if (typeof body === "string") return json(400, { message: body });
  if (!env.SUPABASE_URL || !env.SUPABASE_PUBLISHABLE_KEY) return json(500, { message: "This Cloud has no Supabase settings." });

  const read = await fetchImpl(`${env.SUPABASE_URL}/rest/v1/rpc/yogurt_meeting`, {
    method: "POST",
    headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, authorization: `Bearer ${jwt}`, "content-type": "application/json" },
    body: JSON.stringify({ token: body.token, device_id: body.device_id, meeting_id: body.meeting_id, at_push: null }),
  });
  if (!read.ok) {
    // PostgREST's own status and code pass through, so the webapp treats a rejected token here as it does on every other read.
    const e = await read.json().catch(() => ({}));
    return json(read.status, { code: e.code, message: e.message ?? `Your Cloud answered ${read.status}.` });
  }
  const rows: { body: { meeting: { transcript_json: string } } | null }[] = await read.json();
  const meeting = rows[0]?.body?.meeting;
  if (!meeting) return json(404, { message: "This meeting is no longer in your Cloud." });

  const messages: Message[] = [
    { role: "system", content: CHAT_SYSTEM },
    { role: "system", content: `TRANSCRIPT SO FAR (most recent at bottom):\n\n${transcriptLines(meeting.transcript_json)}` },
    ...body.messages,
  ];
  // Stop generating, on the owner's key, once nobody is reading.
  const gone = new AbortController();
  req.signal?.addEventListener("abort", () => gone.abort());
  const deltas = answer(messages, env, fetchImpl, gone.signal)[Symbol.asyncIterator]();
  const encode = (delta: string) => new TextEncoder().encode(JSON.stringify({ delta }) + "\n");
  return new Response(
    new ReadableStream({
      async pull(c) {
        const { value, done } = await deltas.next();
        if (done) c.close();
        else c.enqueue(encode(value));
      },
      cancel() {
        gone.abort();
      },
    }),
    { headers: { "content-type": "application/x-ndjson" } },
  );
}

/** The visible text of a streamed delta. Some providers send content as a list of parts, of which only the text ones show. */
function deltaText(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content.map((p) => (p?.type === "text" && typeof p.text === "string" ? p.text : "")).join("");
}

/** The model's visible answer, chunk by chunk, with any failure as a last bracketed chunk. */
async function* answer(messages: Message[], env: Env, fetchImpl: typeof fetch, gone: AbortSignal): AsyncGenerator<string> {
  const { YOGURT_LLM_BASE_URL: base, YOGURT_LLM_MODEL: model, YOGURT_LLM_API_KEY: key } = env;
  if (!base || !model || !key) {
    yield NOT_SET_UP;
    return;
  }
  // As in yogurt-llm: MiniMax spends minutes on hidden reasoning unless thinking is disabled, and splits any it does into another field.
  const minimax = model.toLowerCase().startsWith("minimax-");
  // Providers quote the key back in error bodies, and a fetch error can quote a base URL that carries credentials.
  const scrub = (s: string) => (key.length < 4 ? s : s.replaceAll(key, "[key redacted]")).replace(/\/\/[^/\s@]+@/g, "//[credentials redacted]@");
  const timeout = AbortSignal.timeout(LLM_TIMEOUT_MS);
  try {
    const res = await fetchImpl(`${base.replace(/\/+$/, "")}/chat/completions`, {
      method: "POST",
      headers: { authorization: `Bearer ${key}`, "content-type": "application/json" },
      body: JSON.stringify({ model, messages, stream: true, ...(minimax && { reasoning_split: true, thinking: { type: "disabled" } }) }),
      signal: AbortSignal.any([gone, timeout]),
    });
    if (!res.ok || !res.body) {
      yield `[stream error: LLM stream open failed: ${res.status} - ${scrub(await res.text()).slice(0, 500)}]`;
      return;
    }
    const strip = new ThinkStripper();
    for await (const data of sseData(res.body)) {
      if (data.trim() === "[DONE]") break;
      const chunk = JSON.parse(data);
      if (chunk.error) {
        yield `\n\n[stream error: ${scrub(String(chunk.error.message ?? JSON.stringify(chunk.error)))}]`;
        return;
      }
      const choice = chunk.choices?.[0];
      const visible = strip.push(deltaText(choice?.delta?.content));
      if (visible) yield visible;
      if (choice?.finish_reason) break;
    }
    const rest = strip.flush();
    if (rest) yield rest;
  } catch (e) {
    if (gone.aborted) return;
    yield timeout.aborted
      ? `\n\n[stream error: the LLM took longer than ${LLM_TIMEOUT_MS / 1000} seconds]`
      : `\n\n[stream error: ${scrub(e instanceof Error ? e.message : String(e))}]`;
  }
}

/** The `data` of each server-sent event, multi-line data joined with newlines. */
export async function* sseData(body: ReadableStream<Uint8Array>): AsyncGenerator<string> {
  const decoder = new TextDecoder();
  let buf = "";
  let data: string[] = [];
  const reader = body.getReader();
  try {
    for (;;) {
      const { value, done } = await reader.read();
      buf += done ? decoder.decode() : decoder.decode(value, { stream: true });
      const lines = buf.split(/\r?\n/);
      buf = done ? "" : lines.pop()!;
      for (const line of lines) {
        if (line === "") {
          if (data.length) yield data.join("\n");
          data = [];
        } else if (line.startsWith("data:")) {
          data.push(line.slice(line.startsWith("data: ") ? 6 : 5));
        }
      }
      if (done) break;
    }
    if (data.length) yield data.join("\n");
  } finally {
    // Closes the provider's connection when the relay stops early, at [DONE] or finish_reason.
    await reader.cancel().catch(() => undefined);
  }
}
