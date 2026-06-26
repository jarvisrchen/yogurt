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
