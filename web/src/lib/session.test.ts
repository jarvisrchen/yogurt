import { afterEach, describe, expect, it, vi } from "vitest";

const reload = vi.fn();

function serve(tokens: string[], body = "forbidden: bad token") {
  const fetch = vi.fn(async (url: string) =>
    url === "/api/session-token"
      ? new Response(JSON.stringify({ token: tokens.shift() }))
      : new Response(body, { status: 403 }),
  );
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

describe("bearerFetch", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
    reload.mockReset();
  });

  async function load() {
    vi.stubGlobal("location", { reload });
    return import("./session");
  }

  it("reloads once the server hands out a different token than the rejected one", async () => {
    const fetch = serve(["old", "new"]);
    const { bearerFetch } = await load();
    await bearerFetch("/api/meetings/active");
    await Promise.all([bearerFetch("/api/meetings/active"), bearerFetch("/api/labels")]);
    expect(reload).toHaveBeenCalledOnce();
    expect(fetch.mock.calls.filter(([url]) => url === "/api/session-token")).toHaveLength(2);
  });

  it("leaves a 403 that is not about the token alone", async () => {
    const fetch = serve(["old", "new"], "permission_denied");
    const { bearerFetch } = await load();
    await bearerFetch("/api/audio/devices");
    expect(reload).not.toHaveBeenCalled();
    expect(fetch.mock.calls.filter(([url]) => url === "/api/session-token")).toHaveLength(1);
  });

  it("does not reload when the server still hands out the same token", async () => {
    serve(["same", "same"]);
    const { bearerFetch } = await load();
    await bearerFetch("/api/meetings/active");
    expect(reload).not.toHaveBeenCalled();
  });
});
