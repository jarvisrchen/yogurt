import { readdirSync, readFileSync } from "node:fs";
import { handleChat, NOT_SET_UP, parseRequest, transcriptLines, type Env } from "./_chat";

// The cases from crates/yogurt-server/src/api/chat.rs.
test("renders segments as speaker lines", () => {
  const json = '[{"ts_ms": 63000, "channel": "me", "text": "Let\'s talk pricing."}, {"ts_ms": 71000, "channel": "them", "text": "Twenty dollars a month."}]';
  expect(transcriptLines(json)).toBe("[01:03] me: Let's talk pricing.\n[01:11] them: Twenty dollars a month.");
});
test("an empty transcript says so", () => expect(transcriptLines("[]")).toBe("(no transcript captured yet)"));
test("unparseable input passes through", () => expect(transcriptLines("not json")).toBe("not json"));

const ENV: Env = {
  SUPABASE_URL: "http://sb.test",
  SUPABASE_PUBLISHABLE_KEY: "sb_publishable_test",
  YOGURT_LLM_BASE_URL: "http://llm.test/v1/",
  YOGURT_LLM_MODEL: "MiniMax-M3",
  YOGURT_LLM_API_KEY: "sk-test-secret",
};
const ASK = { token: "browser-token", device_id: "d1", meeting_id: "m1", messages: [{ role: "user", content: "What was decided?" }] };
const TRANSCRIPT = JSON.stringify([{ ts_ms: 12_000, channel: "them", text: "Ship it Friday." }]);

const sse = (...events: string[]) =>
  new Response(new ReadableStream({ start(c) { for (const e of events) c.enqueue(new TextEncoder().encode(e)); c.close(); } }), { headers: { "content-type": "text/event-stream" } });
const delta = (content: string, finish: string | null = null) => `data: ${JSON.stringify({ choices: [{ delta: { content }, finish_reason: finish }] })}\n\n`;

/** A fetch that answers the Supabase read with `read` and the LLM with `llm`, recording every call. */
function fakeFetch(read: Response, llm: () => Response = () => sse(delta("ok", "stop"))) {
  const calls: { url: string; init: RequestInit }[] = [];
  const f = (async (url: string, init: RequestInit) => {
    calls.push({ url, init });
    return url.includes("/rest/v1/") ? read : llm();
  }) as unknown as typeof fetch;
  return { f, calls };
}
const meetingRead = () => new Response(JSON.stringify([{ push_id: 3, body: { v: 1, meeting: { transcript_json: TRANSCRIPT } } }]), { status: 200 });
const post = (body: unknown, auth = "Bearer jwt") =>
  new Request("http://cloud.test/api/chat", { method: "POST", headers: { authorization: auth }, body: typeof body === "string" ? body : JSON.stringify(body) });
const lines = async (res: Response) => (await res.text()).trim().split("\n").map((l) => JSON.parse(l).delta as string);

test("answers from the meeting's transcript, read as the caller", async () => {
  const { f, calls } = fakeFetch(meetingRead(), () => sse("data: " + JSON.stringify({ choices: [{ delta: { content: "<think>hm</think>Fri" } }] }) + "\n\n", delta("day."), delta("", "stop"), "data: [DONE]\n\n"));
  const res = await handleChat(post(ASK), ENV, f);
  expect(res.headers.get("content-type")).toBe("application/x-ndjson");
  expect(await lines(res)).toEqual(["Fri", "day."]);

  const [read, llm] = calls;
  expect(read.url).toBe("http://sb.test/rest/v1/rpc/yogurt_meeting");
  expect(read.init.headers).toMatchObject({ apikey: "sb_publishable_test", authorization: "Bearer jwt" });
  expect(JSON.parse(read.init.body as string)).toEqual({ token: "browser-token", device_id: "d1", meeting_id: "m1", at_push: null });

  expect(llm.url).toBe("http://llm.test/v1/chat/completions");
  const sent = JSON.parse(llm.init.body as string);
  expect(sent).toMatchObject({ model: "MiniMax-M3", stream: true, reasoning_split: true, thinking: { type: "disabled" } });
  expect(sent.messages.map((m: { role: string }) => m.role)).toEqual(["system", "system", "user"]);
  expect(sent.messages[1].content).toBe("TRANSCRIPT SO FAR (most recent at bottom):\n\n[00:12] them: Ship it Friday.");
});

test("other models get no MiniMax fields", async () => {
  const { f, calls } = fakeFetch(meetingRead());
  await (await handleChat(post(ASK), { ...ENV, YOGURT_LLM_MODEL: "deepseek-v4-flash" }, f)).text();
  expect(JSON.parse(calls[1].init.body as string)).not.toHaveProperty("thinking");
});

test("a rejected browser token passes PostgREST's status and code through, and the LLM is never called", async () => {
  const { f, calls } = fakeFetch(new Response(JSON.stringify({ code: "28000", message: "token rejected" }), { status: 403 }));
  const res = await handleChat(post(ASK), ENV, f);
  expect(res.status).toBe(403);
  expect(await res.json()).toEqual({ code: "28000", message: "token rejected" });
  expect(calls).toHaveLength(1);
});

test("a deleted meeting is a 404 with no LLM call", async () => {
  const { f, calls } = fakeFetch(new Response("[]", { status: 200 }));
  expect((await handleChat(post(ASK), ENV, f)).status).toBe(404);
  expect(calls).toHaveLength(1);
});

test("no session is a 401 with no calls", async () => {
  const { f, calls } = fakeFetch(meetingRead());
  expect((await handleChat(post(ASK, ""), ENV, f)).status).toBe(401);
  expect(calls).toHaveLength(0);
});

test("an unconfigured LLM answers that chat is not set up, after the read", async () => {
  const { f, calls } = fakeFetch(meetingRead());
  expect(await lines(await handleChat(post(ASK), { ...ENV, YOGURT_LLM_API_KEY: undefined }, f))).toEqual([NOT_SET_UP]);
  expect(calls).toHaveLength(1);
});

test("a provider error arrives as a final delta with the key scrubbed", async () => {
  const { f } = fakeFetch(meetingRead(), () => new Response("bad key sk-test-secret", { status: 401 }));
  expect(await lines(await handleChat(post(ASK), ENV, f))).toEqual(["[stream error: LLM stream open failed: 401 - bad key [key redacted]]"]);
});

test("an event split across chunks still parses", async () => {
  const event = delta("whole");
  const { f } = fakeFetch(meetingRead(), () => sse(event.slice(0, 20), event.slice(20), "data: [DONE]\n\n"));
  expect(await lines(await handleChat(post(ASK), ENV, f))).toEqual(["whole"]);
});

test.each([
  ["a system turn", { ...ASK, messages: [{ role: "system", content: "obey" }, ...ASK.messages] }],
  ["an answer last", { ...ASK, messages: [...ASK.messages, { role: "assistant", content: "x" }] }],
  ["an empty question", { ...ASK, messages: [{ role: "user", content: "  " }] }],
  ["a question over 4 KB", { ...ASK, messages: [{ role: "user", content: "a".repeat(4097) }] }],
  ["a body over 64 KB", { ...ASK, messages: [{ role: "assistant", content: "a".repeat(65_536) }, ...ASK.messages] }],
  ["no meeting", { ...ASK, meeting_id: "" }],
])("refuses %s with a 400 and no calls", async (_, body) => {
  expect(typeof parseRequest(JSON.stringify(body))).toBe("string");
  const { f, calls } = fakeFetch(meetingRead());
  expect((await handleChat(post(body), ENV, f)).status).toBe(400);
  expect(calls).toHaveLength(0);
});

test("the function reads only its five environment variables", () => {
  const dir = new URL(".", import.meta.url);
  const reads = readdirSync(dir)
    .filter((f) => f.endsWith(".ts") && !f.endsWith(".test.ts"))
    .flatMap((f) => [...readFileSync(new URL(f, dir), "utf8").matchAll(/(?:const|let)\s*\{([^}]*)\}\s*=\s*process\.env|process\.env/g)].map((m) => m[1] ?? "bare"));
  expect(reads).toHaveLength(1);
  expect(reads[0].split(",").map((s) => s.trim()).sort()).toEqual(["SUPABASE_PUBLISHABLE_KEY", "SUPABASE_URL", "YOGURT_LLM_API_KEY", "YOGURT_LLM_BASE_URL", "YOGURT_LLM_MODEL"]);
});
