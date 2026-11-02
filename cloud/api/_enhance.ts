// POST /api/enhance: streams the model's reply to an enhance prompt the page rendered with yogurt-wasm, so a phone meeting gets the Mac's enhance.
// The caller's session and browser token must pass a Supabase read first, so only the owner's unlocked browser reaches the LLM.
// The reply streams back unchanged as NDJSON `{"delta":"..."}` lines, because the page cleans and merges the whole reply in WASM as the Mac does.
// It ends with `{"done":true,"model":"..."}`, or `{"error":"..."}` when the model fails partway, so a reply cut short is never taken for a whole one.
import { type Env, type Message, deltaText, json, openCompletion, scrubber, sseData } from "./_chat.js";

// Under Vercel's 4.5 MB request cap. An hour of transcript is a few hundred KB.
const MAX_BODY = 4 * 1024 * 1024;
// Under the function's 300 s limit, so a stalled provider still ends in an error the page shows.
const LLM_TIMEOUT_MS = 280_000;
export const NOT_SET_UP = "Enhance isn't set up on this Cloud.";
const UNREACHABLE = "Enhance couldn't reach your Cloud. Try again.";

const bytes = (s: string) => new TextEncoder().encode(s).length;

/** The request, or why it is refused. */
export function parseRequest(raw: string): { token: string; messages: Message[] } | string {
  if (bytes(raw) > MAX_BODY) return "The meeting is too long to enhance.";
  let b: { token?: unknown; messages?: unknown };
  try {
    b = JSON.parse(raw);
  } catch {
    return "The request is not JSON.";
  }
  if (typeof b?.token !== "string" || !b.token) return "The request has no browser token.";
  const m = b.messages;
  if (!Array.isArray(m) || !m.length || !m.every((t) => (t?.role === "system" || t?.role === "user") && typeof t.content === "string" && t.content.trim())) {
    return "The request needs the rendered prompt.";
  }
  return { token: b.token, messages: m.map(({ role, content }) => ({ role, content })) };
}

export async function handleEnhance(req: Request, rawEnv: Env, fetchImpl: typeof fetch = fetch, timeoutMs = LLM_TIMEOUT_MS): Promise<Response> {
  // A value pasted with a stray space or newline would otherwise break a URL, or slip past the key scrub.
  const env = Object.fromEntries(Object.entries(rawEnv).map(([k, v]) => [k, v?.trim()])) as Env;
  try {
    return await respond(req, env, fetchImpl, timeoutMs);
  } catch {
    return json(502, { message: UNREACHABLE });
  }
}

async function respond(req: Request, env: Env, fetchImpl: typeof fetch, timeoutMs: number): Promise<Response> {
  if (req.method !== "POST") return json(405, { message: "Use POST." });
  const jwt = /^Bearer (.+)$/.exec(req.headers.get("authorization") ?? "")?.[1];
  if (!jwt) return json(401, { message: "Sign in again." });
  const body = parseRequest(await req.text());
  if (typeof body === "string") return json(400, { message: body });
  if (!env.SUPABASE_URL || !env.SUPABASE_PUBLISHABLE_KEY) return json(500, { message: "This Cloud has no Supabase settings." });
  // Stop reading and generating, on the owner's key, once nobody is waiting.
  const gone = new AbortController();
  if (req.signal?.aborted) gone.abort();
  req.signal?.addEventListener("abort", () => gone.abort());

  // Any read that needs the owner and the browser token does. PostgREST's status and code pass through, so the page treats a rejected token as it does on every read.
  const check = await fetchImpl(`${env.SUPABASE_URL}/rest/v1/rpc/yogurt_device_info`, {
    method: "POST",
    headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, authorization: `Bearer ${jwt}`, "content-type": "application/json" },
    body: JSON.stringify({ token: body.token }),
    signal: gone.signal,
  });
  if (!check.ok) {
    const e = await check.json().catch(() => ({}));
    return json(check.status, { code: e.code, message: e.message ?? `Your Cloud answered ${check.status}.` });
  }

  const { YOGURT_LLM_BASE_URL: base, YOGURT_LLM_MODEL: model, YOGURT_LLM_API_KEY: key } = env;
  if (!base || !model || !key) return json(503, { message: NOT_SET_UP });
  const scrub = scrubber(key);
  const timeout = AbortSignal.timeout(timeoutMs);
  const failure = (e: unknown) =>
    timeout.aborted ? `${model} took longer than ${Math.round(timeoutMs / 1000)} seconds.` : scrub(e instanceof Error ? e.message : String(e));
  let res: Response;
  try {
    res = await openCompletion(body.messages, { base, model, key }, fetchImpl, AbortSignal.any([gone.signal, timeout]));
  } catch (e) {
    return json(502, { message: failure(e) });
  }
  if (!res.ok || !res.body) return json(502, { message: `LLM stream open failed: ${res.status} - ${scrub(await res.text()).slice(0, 500)}` });

  const frames = relay(res.body, model, failure)[Symbol.asyncIterator]();
  const encode = (line: object) => new TextEncoder().encode(JSON.stringify(line) + "\n");
  return new Response(
    new ReadableStream({
      async pull(c) {
        const { value, done } = await frames.next();
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

async function* relay(body: ReadableStream<Uint8Array>, model: string, failure: (e: unknown) => string): AsyncGenerator<object> {
  try {
    for await (const data of sseData(body)) {
      if (data.trim() === "[DONE]") break;
      const chunk = JSON.parse(data);
      if (chunk.error) {
        yield { error: failure(new Error(String(chunk.error.message ?? JSON.stringify(chunk.error)))) };
        return;
      }
      const choice = chunk.choices?.[0];
      const delta = deltaText(choice?.delta?.content);
      if (delta) yield { delta };
      if (choice?.finish_reason) break;
    }
  } catch (e) {
    yield { error: failure(e) };
    return;
  }
  // As on the Mac, a stream that ends without a finish reason keeps what arrived.
  yield { done: true, model };
}
