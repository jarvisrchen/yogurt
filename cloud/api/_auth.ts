// The caller checks every API function shares. Each reads Supabase with the caller's own session, so Postgres decides what the caller may do.

export interface SupabaseEnv {
  SUPABASE_URL?: string;
  SUPABASE_PUBLISHABLE_KEY?: string;
}

/** A value pasted with a stray space or newline would otherwise break a URL, or slip past a key scrub. */
export const trimmed = <E extends object>(raw: E): E => Object.fromEntries(Object.entries(raw).map(([k, v]) => [k, v?.trim()])) as E;

export const json = (status: number, body: object, headers: Record<string, string> = {}) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", ...headers } });

/** The session token from `Authorization: Bearer`. */
export const bearer = (req: Request) => /^Bearer (.+)$/.exec(req.headers.get("authorization") ?? "")?.[1];

export interface Caller {
  /** A Supabase RPC as the caller. A refusal comes back as the response to send, with PostgREST's status and code, so the webapp treats a rejected token as it does on every read. */
  rpc<T>(fn: string, args: object): Promise<T | Response>;
  /** Null when the caller is the owner with an unlocked browser `token`, and otherwise the refusal to send. */
  authorize(token: string): Promise<Response | null>;
}

/** The caller with session `jwt`, or the response when this Cloud has no Supabase settings. `headers` go on every response it makes. */
export function caller(env: SupabaseEnv, jwt: string, fetchImpl: typeof fetch, { signal, headers }: { signal?: AbortSignal; headers?: Record<string, string> } = {}): Caller | Response {
  const { SUPABASE_URL: url, SUPABASE_PUBLISHABLE_KEY: key } = env;
  if (!url || !key) return json(500, { message: "This Cloud has no Supabase settings." }, headers);
  const rpc = async <T>(fn: string, args: object): Promise<T | Response> => {
    const read = await fetchImpl(`${url}/rest/v1/rpc/${fn}`, {
      method: "POST",
      headers: { apikey: key, authorization: `Bearer ${jwt}`, "content-type": "application/json" },
      body: JSON.stringify(args),
      signal,
    });
    if (read.ok) return read.json();
    const e = await read.json().catch(() => ({}));
    return json(read.status, { code: e.code, message: e.message ?? `Your Cloud answered ${read.status}.` }, headers);
  };
  // Any read that needs the owner and an unlocked browser does.
  const authorize = async (token: string) => {
    const r = await rpc("yogurt_device_info", { token });
    return r instanceof Response ? r : null;
  };
  return { rpc, authorize };
}
