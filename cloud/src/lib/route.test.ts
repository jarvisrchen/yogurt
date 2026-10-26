import { afterEach, describe, expect, it, vi } from "vitest";
import { browserLabel, formatWait, lockoutMinutes, screenFor, type Status } from "./route";
import { TOKEN_KEY, clearToken, readToken, saveToken } from "./token";

const owner: Status = { owner_email_set: true, is_owner: true, passcode_set: true, passcode_length: 6, locked_until: null };
const base = { signedIn: true, status: owner, token: "t", view: "library" as const };

describe("screenFor", () => {
  it("sends a visitor without a session to sign in", () => {
    expect(screenFor({ ...base, signedIn: false, status: null, token: null })).toBe("signin");
  });
  it("waits for yogurt_status after sign-in", () => {
    expect(screenFor({ ...base, status: null })).toBe("loading");
  });
  it("refuses a login that is not the owner", () => {
    expect(screenFor({ ...base, status: { ...owner, is_owner: false, passcode_set: false } })).toBe("not-owner");
  });
  it("asks the owner to choose a PIN first", () => {
    expect(screenFor({ ...base, status: { ...owner, passcode_set: false }, token: null })).toBe("choose-pin");
  });
  it("unlocks when there is no token, and shows the view when there is one", () => {
    expect(screenFor({ ...base, token: null })).toBe("unlock");
    expect(screenFor(base)).toBe("library");
    expect(screenFor({ ...base, view: "settings" })).toBe("settings");
  });
});

describe("lockout wording", () => {
  const now = Date.parse("2026-10-24T12:00:00Z");
  it("rounds to whole minutes, never below 1 while locked", () => {
    expect(lockoutMinutes("2026-10-24T12:14:40Z", now)).toBe(15);
    expect(lockoutMinutes("2026-10-24T12:00:05Z", now)).toBe(1);
    expect(lockoutMinutes("2026-10-24T11:59:00Z", now)).toBe(0);
    expect(lockoutMinutes(null, now)).toBe(0);
  });
  it("names the wait", () => {
    expect(formatWait(1)).toBe("1 minute");
    expect(formatWait(30)).toBe("30 minutes");
    expect(formatWait(60)).toBe("1 hour");
    expect(formatWait(240)).toBe("4 hours");
  });
});

describe("browserLabel", () => {
  it("names browser and system", () => {
    expect(browserLabel("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Version/17.0 Mobile/15E148 Safari/604.1")).toBe("Safari on iPhone");
    expect(browserLabel("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120 Safari/537.36")).toBe("Chrome on Mac");
  });
});

describe("token storage", () => {
  const store = (impl: Partial<Storage>) => vi.stubGlobal("localStorage", impl);
  afterEach(() => vi.unstubAllGlobals());

  it("round-trips under one key and clears", () => {
    const m = new Map<string, string>();
    store({ getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v), removeItem: (k) => void m.delete(k) });
    saveToken({ token: "abc", remember: true });
    expect([...m.keys()]).toEqual([TOKEN_KEY]);
    expect(readToken()).toEqual({ token: "abc", remember: true });
    clearToken();
    expect(readToken()).toBeNull();
  });
  it("ignores a corrupt value", () => {
    store({ getItem: () => "{not json" });
    expect(readToken()).toBeNull();
  });
  it("survives a blocked localStorage", () => {
    const blocked = () => {
      throw new Error("blocked");
    };
    store({ getItem: blocked, setItem: blocked, removeItem: blocked });
    expect(readToken()).toBeNull();
    expect(() => saveToken({ token: "x", remember: false })).not.toThrow();
    expect(() => clearToken()).not.toThrow();
  });
});
