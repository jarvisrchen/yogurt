import { handleSttToken, NOT_SET_UP, type Env } from "./_stt-token";

const ENV: Env = { SUPABASE_URL: "http://sb.test", SUPABASE_PUBLISHABLE_KEY: "sb_publishable_test", YOGURT_DEEPGRAM_API_KEY: "dg-secret-key" };

/** A fetch that answers the Supabase check with `check` and Deepgram with `grant`, recording every call. */
function fakeFetch(check: () => Response, grant: () => Response = () => new Response(JSON.stringify({ access_token: "granted.jwt", expires_in: 30 }))) {
  const calls: { url: string; init: RequestInit }[] = [];
  const f = (async (url: string, init: RequestInit) => {
    calls.push({ url, init });
    return url.includes("/rest/v1/") ? check() : grant();
  }) as unknown as typeof fetch;
  return { f, calls };
}
const owner = () => new Response("[]", { status: 200 });
const post = (body: unknown, auth = "Bearer jwt") =>
  new Request("http://cloud.test/api/stt-token", { method: "POST", headers: { authorization: auth }, body: typeof body === "string" ? body : JSON.stringify(body) });

test("checks the owner and browser token, then returns a granted token and the Mac's listen settings", async () => {
  const { f, calls } = fakeFetch(owner);
  const res = await handleSttToken(post({ token: "browser-token" }), ENV, f);
  expect(res.status).toBe(200);
  const body = await res.json();
  expect(body.token).toBe("granted.jwt");
  const url = new URL(body.url);
  expect(url.origin + url.pathname).toBe("wss://api.deepgram.com/v1/listen");
  expect(Object.fromEntries(url.searchParams)).toEqual({
    model: "nova-3",
    encoding: "linear16",
    sample_rate: "16000",
    channels: "1",
    interim_results: "true",
    punctuate: "true",
    smart_format: "true",
    endpointing: "1000",
  });

  const [check, grant] = calls;
  expect(check.url).toBe("http://sb.test/rest/v1/rpc/yogurt_device_info");
  expect(check.init.headers).toMatchObject({ apikey: "sb_publishable_test", authorization: "Bearer jwt" });
  expect(JSON.parse(check.init.body as string)).toEqual({ token: "browser-token" });
  expect(grant.url).toBe("https://api.deepgram.com/v1/auth/grant");
  expect(grant.init).toMatchObject({ method: "POST", headers: { authorization: "Token dg-secret-key" } });
});

test("the key never appears in a response, even when Deepgram quotes it back", async () => {
  for (const grant of [() => new Response("bad key dg-secret-key", { status: 403 }), () => new Response("dg-secret-key", { status: 500 })]) {
    const { f } = fakeFetch(owner, grant);
    const res = await handleSttToken(post({ token: "t" }), ENV, f);
    expect(res.status).toBe(502);
    expect(await res.text()).not.toContain("dg-secret");
  }
});

test("a refused key says it needs the Member role", async () => {
  const { f } = fakeFetch(owner, () => new Response("{}", { status: 403 }));
  expect((await (await handleSttToken(post({ token: "t" }), ENV, f)).json()).message).toMatch(/Member role/);
});

test("YOGURT_DEEPGRAM_URL moves both the grant and the socket, with a plain ws for http", async () => {
  const { f, calls } = fakeFetch(owner);
  const body = await (await handleSttToken(post({ token: "t" }), { ...ENV, YOGURT_DEEPGRAM_URL: " http://127.0.0.1:5193/ " }, f)).json();
  expect(calls[1].url).toBe("http://127.0.0.1:5193/v1/auth/grant");
  expect(body.url).toMatch(/^ws:\/\/127\.0\.0\.1:5193\/v1\/listen\?model=nova-3&/);
});

test("a rejected browser token passes PostgREST's status and code through, and Deepgram is never called", async () => {
  const { f, calls } = fakeFetch(() => new Response(JSON.stringify({ code: "28000", message: "invalid or expired browser token" }), { status: 403 }));
  const res = await handleSttToken(post({ token: "t" }), ENV, f);
  expect(res.status).toBe(403);
  expect(await res.json()).toEqual({ code: "28000", message: "invalid or expired browser token" });
  expect(calls).toHaveLength(1);
});

test("no key is a 503 that says recording is not set up, after the owner check", async () => {
  const { f, calls } = fakeFetch(owner);
  const res = await handleSttToken(post({ token: "t" }), { ...ENV, YOGURT_DEEPGRAM_API_KEY: " " }, f);
  expect(res.status).toBe(503);
  expect((await res.json()).message).toBe(NOT_SET_UP);
  expect(calls).toHaveLength(1);
});

test.each([
  ["no session", post({ token: "t" }, ""), 401],
  ["no browser token", post({}), 400],
  ["a body that is not JSON", post("nope"), 400],
  ["a GET", new Request("http://cloud.test/api/stt-token", { headers: { authorization: "Bearer jwt" } }), 405],
])("refuses %s with no calls", async (_, req, status) => {
  const { f, calls } = fakeFetch(owner);
  expect((await handleSttToken(req, ENV, f)).status).toBe(status);
  expect(calls).toHaveLength(0);
});

test("an unreachable Cloud is a 502 rather than a crash", async () => {
  const f = (async () => {
    throw new TypeError("fetch failed");
  }) as unknown as typeof fetch;
  expect((await handleSttToken(post({ token: "t" }), ENV, f)).status).toBe(502);
});
