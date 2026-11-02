// POST /api/stt-token: a short-lived Deepgram token for the recorder, and the listen URL to open with it.
// The caller must be the signed-in owner with an unlocked browser token, which Postgres checks, so a locked browser never gets one.
// The Deepgram key stays here: only the granted token, valid for 30 seconds, reaches the browser.

export interface Env {
  SUPABASE_URL?: string;
  SUPABASE_PUBLISHABLE_KEY?: string;
  YOGURT_DEEPGRAM_API_KEY?: string;
  /** Deepgram's address, `https://api.deepgram.com` unless set. The e2e specs point it at a fake. */
  YOGURT_DEEPGRAM_URL?: string;
}

export const NOT_SET_UP = "Recording isn't set up on this Cloud.";
const UNREACHABLE = "Couldn't reach your Cloud. Try again.";

// The Mac's stream settings (crates/yogurt-stt/src/deepgram.rs): 16 kHz mono linear16, interim results, and a line per second of silence.
const LISTEN = "model=nova-3&encoding=linear16&sample_rate=16000&channels=1&interim_results=true&punctuate=true&smart_format=true&endpointing=1000";

const json = (status: number, body: object) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });

export async function handleSttToken(req: Request, rawEnv: Env, fetchImpl: typeof fetch = fetch): Promise<Response> {
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
  let token: unknown;
  try {
    ({ token } = JSON.parse(await req.text()));
  } catch {
    return json(400, { message: "The request is not JSON." });
  }
  if (typeof token !== "string" || !token) return json(400, { message: "The request has no browser token." });
  if (!env.SUPABASE_URL || !env.SUPABASE_PUBLISHABLE_KEY) return json(500, { message: "This Cloud has no Supabase settings." });

  // Any read that needs the owner and an unlocked browser does. PostgREST's status and code pass through, so the webapp treats a rejected token as on every read.
  const check = await fetchImpl(`${env.SUPABASE_URL}/rest/v1/rpc/yogurt_device_info`, {
    method: "POST",
    headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, authorization: `Bearer ${jwt}`, "content-type": "application/json" },
    body: JSON.stringify({ token }),
  });
  if (!check.ok) {
    const e = await check.json().catch(() => ({}));
    return json(check.status, { code: e.code, message: e.message ?? `Your Cloud answered ${check.status}.` });
  }

  const key = env.YOGURT_DEEPGRAM_API_KEY;
  if (!key) return json(503, { message: NOT_SET_UP });
  const base = (env.YOGURT_DEEPGRAM_URL || "https://api.deepgram.com").replace(/\/+$/, "");
  const grant = await fetchImpl(`${base}/v1/auth/grant`, { method: "POST", headers: { authorization: `Token ${key}` } });
  // Deepgram's error text never reaches the browser, since it can quote the request.
  if (!grant.ok) return json(502, { message: grant.status === 401 || grant.status === 403 ? "Deepgram refused this Cloud's key. It needs the Member role or higher." : `Deepgram answered ${grant.status}.` });
  const { access_token } = await grant.json();
  if (typeof access_token !== "string" || !access_token) return json(502, { message: "Deepgram returned no token." });
  return json(200, { token: access_token, url: `${base.replace(/^http/, "ws")}/v1/listen?${LISTEN}` });
}
