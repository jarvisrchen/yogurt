import { describe, expect, it } from "vitest";
import { authError, OFFLINE, offline, SIGNUP_CLOSED } from "./api";

describe("offline", () => {
  it("catches a postgrest-js and an auth-js network failure", () => {
    expect(offline({ status: 0 })).toBe(true);
    expect(offline({ name: "AuthRetryableFetchError", status: 0 })).toBe(true);
  });
  it("keeps a real error", () => {
    expect(offline({ name: "AuthApiError", status: 400 })).toBe(false);
  });
});

describe("authError", () => {
  it("explains a closed project", () => {
    const e = authError({ status: 422, name: "AuthApiError", code: "signup_disabled" });
    expect((e as Error).message).toBe(SIGNUP_CLOSED);
  });
  it("maps a network failure to the offline message", () => {
    expect((authError({ status: 0, name: "AuthRetryableFetchError" }) as Error).message).toBe(OFFLINE);
  });
  it("passes any other error through", () => {
    const e = { status: 400, name: "AuthApiError", code: "weak_password" };
    expect(authError(e)).toBe(e);
  });
});
