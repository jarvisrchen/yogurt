/**
 * Session token bootstrap (WR-06).
 *
 * The SPA fetches `GET /api/session-token` once on startup to learn the
 * per-process token Phase 0 wrote to `~/.yogurt/session-token`. Every
 * subsequent `/api/meetings*` REST call and per-meeting WebSocket carries
 * it (either as a `?token=` query param for WS or an `X-Yogurt-Token`
 * header for REST).
 *
 * The handout endpoint itself is Origin-gated (only `http://localhost:{port}`
 * pages can call it). Third-party tabs and image-preload exploits can't
 * forge an Origin header from a browser context, so they can't reach the
 * token even if they reach the URL.
 *
 * Module-level cache: the fetch happens once per page load. A second call
 * returns the cached promise so a flurry of components mounting in parallel
 * doesn't fan out to N network requests.
 */

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

/**
 * Test-only: drop the cached token so the next `ensureSessionToken()` call
 * re-fetches. Not used in production code.
 */
export function _resetSessionTokenCache(): void {
  cached = null;
}

/**
 * Shared `fetch` wrapper that attaches `Authorization: Bearer <token>` after
 * waiting on `ensureSessionToken()`. Use this for every `/api/*` call except
 * the bootstrap `/api/session-token` itself (which acquires the token).
 *
 * Why this lives here, not per-file: three separate API clients shipped
 * with bare `fetch()` and silently 403'd, gating the SPA on a bootstrap
 * token that never reached subsequent calls. A single shared helper makes
 * "token attached" the path of least resistance.
 *
 * Returns the raw `Response` so callers can decide how to parse. For
 * JSON-only callers, see `bearerJson`.
 */
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

/**
 * `bearerFetch` + throw-on-non-2xx + `.json()` (with 204 → `undefined`).
 * Matches the shape `meetings.ts::json` shipped — extracted here so every
 * API client adopts the same pattern.
 */
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
