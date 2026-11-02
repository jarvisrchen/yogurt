// POST /api/stt-token: a short-lived Deepgram token for the recorder, and the listen URL to open with it.
// The caller must be the signed-in owner with an unlocked browser token, which Postgres checks, so a locked browser never gets one.
// The Deepgram key stays here: only the granted token, valid for 30 seconds, reaches the browser.
import { type SupabaseEnv, bearer, caller, json as reply, trimmed } from "./_auth.js";

export interface Env extends SupabaseEnv {
  YOGURT_DEEPGRAM_API_KEY?: string;
  /** Deepgram's address, `https://api.deepgram.com` unless set. The e2e specs point it at a fake. */
  YOGURT_DEEPGRAM_URL?: string;
}

export const NOT_SET_UP = "Recording isn't set up on this Cloud.";
const UNREACHABLE = "Couldn't reach your Cloud. Try again.";

// The Mac's stream settings (crates/yogurt-stt/src/deepgram.rs): 16 kHz mono linear16, interim results, and a line per second of silence.
const LISTEN = "model=nova-3&encoding=linear16&sample_rate=16000&channels=1&interim_results=true&punctuate=true&smart_format=true&endpointing=1000";

const NO_STORE = { "cache-control": "no-store" };
const json = (status: number, body: object) => reply(status, body, NO_STORE);

export async function handleSttToken(req: Request, rawEnv: Env, fetchImpl: typeof fetch = fetch): Promise<Response> {
  const env = trimmed(rawEnv);
  try {
    return await respond(req, env, fetchImpl);
  } catch {
    return json(502, { message: UNREACHABLE });
  }
}

async function respond(req: Request, env: Env, fetchImpl: typeof fetch): Promise<Response> {
  if (req.method !== "POST") return json(405, { message: "Use POST." });
  const jwt = bearer(req);
  if (!jwt) return json(401, { message: "Sign in again." });
  let token: unknown;
  try {
    ({ token } = JSON.parse(await req.text()));
  } catch {
    return json(400, { message: "The request is not JSON." });
  }
  if (typeof token !== "string" || !token) return json(400, { message: "The request has no browser token." });
  const who = caller(env, jwt, fetchImpl, { headers: NO_STORE });
  if (who instanceof Response) return who;
  const refused = await who.authorize(token);
  if (refused) return refused;

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
