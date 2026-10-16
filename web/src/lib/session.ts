// The handout endpoint is Origin-gated so third-party tabs cannot read the token.
// The promise is cached so components mounting in parallel share one request.

let cached: Promise<string> | null = null;

interface SessionTokenResponse {
  token: string;
}

export function ensureSessionToken(): Promise<string> {
  if (cached) return cached;
  cached = (async () => {
    const res = await fetch("/api/session-token", {
      credentials: "same-origin",
    });
    if (!res.ok) {
      cached = null; // allow a retry on next call
      throw new Error(
        `GET /api/session-token returned ${res.status} ${res.statusText}`,
      );
    }
    const body = (await res.json()) as SessionTokenResponse;
    if (typeof body?.token !== "string" || body.token.length === 0) {
      cached = null;
      throw new Error("session-token response missing `token` field");
    }
    return body.token;
  })();
  return cached;
}

/** Use for every `/api/*` call except the `/api/session-token` bootstrap. */
export async function bearerFetch(
  input: string,
  init?: RequestInit,
): Promise<Response> {
  const token = await ensureSessionToken();
  const headers = new Headers(init?.headers ?? {});
  headers.set("Authorization", `Bearer ${token}`);
  if (init?.body != null && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  return fetch(input, { ...init, headers });
}

export async function bearerJson<T>(
  input: string,
  init?: RequestInit,
): Promise<T> {
  const res = await bearerFetch(input, init);
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  if (res.status === 204) return undefined as unknown as T;
  return (await res.json()) as T;
}
