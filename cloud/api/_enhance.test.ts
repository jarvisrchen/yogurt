import { handleEnhance, NOT_SET_UP, parseRequest } from "./_enhance";
import type { Env } from "./_chat";

const ENV: Env = {
  SUPABASE_URL: "http://sb.test",
  SUPABASE_PUBLISHABLE_KEY: "sb_publishable_test",
  YOGURT_LLM_BASE_URL: "http://llm.test/v1/",
  YOGURT_LLM_MODEL: "MiniMax-M3",
  YOGURT_LLM_API_KEY: "sk-test-secret",
};
const MESSAGES = [{ role: "user", content: "Enhance these notes." }];
const ASK = { token: "browser-token", messages: MESSAGES };

const sse = (...events: string[]) =>
  new Response(new ReadableStream({ start(c) { for (const e of events) c.enqueue(new TextEncoder().encode(e)); c.close(); } }), { headers: { "content-type": "text/event-stream" } });
const delta = (content: string, finish: string | null = null) => `data: ${JSON.stringify({ choices: [{ delta: { content }, finish_reason: finish }] })}\n\n`;
const ownerRead = () => new Response("[]", { status: 200 });

/** A fetch that answers the Supabase check with `read` and the LLM with `llm`, recording every call. */
function fakeFetch(read: Response, llm: () => Response = () => sse(delta("ok", "stop"))) {
  const calls: { url: string; init: RequestInit }[] = [];
  const f = (async (url: string, init: RequestInit) => {
    calls.push({ url, init });
    return url.includes("/rest/v1/") ? read : llm();
  }) as unknown as typeof fetch;
  return { f, calls };
}
const post = (body: unknown, auth = "Bearer jwt") =>
  new Request("http://cloud.test/api/enhance", { method: "POST", headers: { authorization: auth }, body: typeof body === "string" ? body : JSON.stringify(body) });
const frames = async (res: Response) => (await res.text()).trim().split("\n").map((l) => JSON.parse(l));

test("streams the raw reply, think tags and all, then names the model", async () => {
  const { f, calls } = fakeFetch(ownerRead(), () => sse(delta("<think>hm</think>template: general"), delta("\n\n## Decisions"), delta("", "stop"), "data: [DONE]\n\n"));
  const res = await handleEnhance(post(ASK), ENV, f);
  expect(res.headers.get("content-type")).toBe("application/x-ndjson");
  expect(await frames(res)).toEqual([{ delta: "<think>hm</think>template: general" }, { delta: "\n\n## Decisions" }, { done: true, model: "MiniMax-M3" }]);

  const [check, llm] = calls;
  expect(check.url).toBe("http://sb.test/rest/v1/rpc/yogurt_device_info");
  expect(check.init.headers).toMatchObject({ apikey: "sb_publishable_test", authorization: "Bearer jwt" });
  expect(JSON.parse(check.init.body as string)).toEqual({ token: "browser-token" });

  expect(llm.url).toBe("http://llm.test/v1/chat/completions");
  expect(llm.init.headers).toMatchObject({ authorization: "Bearer sk-test-secret" });
  expect(JSON.parse(llm.init.body as string)).toEqual({ model: "MiniMax-M3", messages: MESSAGES, stream: true, reasoning_split: true, thinking: { type: "disabled" } });
});

test("a stream that ends without a finish reason keeps what arrived, as the Mac does", async () => {
  const { f } = fakeFetch(ownerRead(), () => sse(delta("## Notes")));
  expect(await frames(await handleEnhance(post(ASK), { ...ENV, YOGURT_LLM_MODEL: "deepseek-v4-flash" }, f))).toEqual([{ delta: "## Notes" }, { done: true, model: "deepseek-v4-flash" }]);
});

test("a rejected browser token passes PostgREST's status and code through, and the LLM is never called", async () => {
  const { f, calls } = fakeFetch(new Response(JSON.stringify({ code: "28000", message: "token rejected" }), { status: 403 }));
  const res = await handleEnhance(post(ASK), ENV, f);
  expect(res.status).toBe(403);
  expect(await res.json()).toEqual({ code: "28000", message: "token rejected" });
  expect(calls).toHaveLength(1);
});

test("no session is a 401 with no calls", async () => {
  const { f, calls } = fakeFetch(ownerRead());
  expect((await handleEnhance(post(ASK, ""), ENV, f)).status).toBe(401);
  expect(calls).toHaveLength(0);
});

test("an unconfigured LLM is a 503 after the owner check", async () => {
  const { f, calls } = fakeFetch(ownerRead());
  const res = await handleEnhance(post(ASK), { ...ENV, YOGURT_LLM_MODEL: " " }, f);
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ message: NOT_SET_UP });
  expect(calls).toHaveLength(1);
});

test("a provider that refuses to open is a 502 with the key scrubbed", async () => {
  const { f } = fakeFetch(ownerRead(), () => new Response(`${"x".repeat(490)} bad key sk-test-secret`, { status: 401 }));
  const res = await handleEnhance(post(ASK), ENV, f);
  expect(res.status).toBe(502);
  const { message } = await res.json();
  expect(message).toMatch(/^LLM stream open failed: 401 - /);
  expect(message).not.toContain("sk-test");
});

test("a mid-stream error ends with an error frame and no done frame", async () => {
  const { f } = fakeFetch(ownerRead(), () => sse(delta("Partial "), `data: ${JSON.stringify({ error: { message: "overloaded for sk-test-secret" } })}\n\n`));
  expect(await frames(await handleEnhance(post(ASK), ENV, f))).toEqual([{ delta: "Partial " }, { error: "overloaded for [key redacted]" }]);
});

test("credentials in a base URL never reach the page", async () => {
  const f = (async (url: string) => {
    if (url.includes("/rest/v1/")) return ownerRead();
    throw new TypeError(`Request cannot be constructed from a URL that includes credentials: ${url}`);
  }) as unknown as typeof fetch;
  const res = await handleEnhance(post(ASK), { ...ENV, YOGURT_LLM_BASE_URL: "https://user:s3cret@llm.test/v1" }, f);
  expect(res.status).toBe(502);
  expect(JSON.stringify(await res.json())).not.toContain("s3cret");
});

test("an unreachable Cloud is a 502 rather than a crash", async () => {
  const f = (async () => {
    throw new TypeError("fetch failed");
  }) as unknown as typeof fetch;
  const res = await handleEnhance(post(ASK), ENV, f);
  expect(res.status).toBe(502);
  expect(await res.json()).toEqual({ message: "Enhance couldn't reach your Cloud. Try again." });
});

test("cancelling the stream cancels the LLM call", async () => {
  let llmSignal: AbortSignal | undefined;
  const f = (async (url: string, init: RequestInit) => {
    if (url.includes("/rest/v1/")) return ownerRead();
    llmSignal = init.signal ?? undefined;
    return sse(delta("first "));
  }) as unknown as typeof fetch;
  const reader = (await handleEnhance(post(ASK), ENV, f)).body!.getReader();
  await reader.read();
  await reader.cancel();
  expect(llmSignal?.aborted).toBe(true);
});

test("a client that leaves during the owner check never reaches the LLM", async () => {
  const left = new AbortController();
  const calls: string[] = [];
  const f = (async (url: string, init: RequestInit) => {
    calls.push(url);
    left.abort();
    if (init.signal?.aborted) throw new DOMException("aborted", "AbortError");
    return ownerRead();
  }) as unknown as typeof fetch;
  await handleEnhance(new Request("http://cloud.test/api/enhance", { method: "POST", headers: { authorization: "Bearer jwt" }, body: JSON.stringify(ASK), signal: left.signal }), ENV, f);
  expect(calls).toEqual(["http://sb.test/rest/v1/rpc/yogurt_device_info"]);
});

test("a stalled provider ends in a timeout naming the model", async () => {
  const f = (async (url: string, init: RequestInit) => {
    if (url.includes("/rest/v1/")) return ownerRead();
    return new Promise((_, reject) => init.signal!.addEventListener("abort", () => reject(init.signal!.reason)));
  }) as unknown as typeof fetch;
  const res = await handleEnhance(post(ASK), ENV, f, 2_000);
  expect(res.status).toBe(502);
  expect(await res.json()).toEqual({ message: "MiniMax-M3 took longer than 2 seconds." });
});

test.each([
  ["no messages", { token: "t", messages: [] }],
  ["an assistant turn", { token: "t", messages: [{ role: "assistant", content: "x" }] }],
  ["an empty prompt", { token: "t", messages: [{ role: "user", content: " " }] }],
  ["no browser token", { messages: MESSAGES }],
  ["a body over 4 MB", { token: "t", messages: [{ role: "user", content: "a".repeat(4 * 1024 * 1024) }] }],
  ["not JSON", "{"],
])("refuses %s with a 400 and no calls", async (_, body) => {
  expect(typeof parseRequest(typeof body === "string" ? body : JSON.stringify(body))).toBe("string");
  const { f, calls } = fakeFetch(ownerRead());
  expect((await handleEnhance(post(body), ENV, f)).status).toBe(400);
  expect(calls).toHaveLength(0);
});
