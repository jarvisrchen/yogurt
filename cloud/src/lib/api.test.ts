import { describe, expect, it } from "vitest";
import { offline } from "./api";

describe("offline", () => {
  it("catches a postgrest-js and an auth-js network failure", () => {
    expect(offline({ status: 0 })).toBe(true);
    expect(offline({ name: "AuthRetryableFetchError", status: 0 })).toBe(true);
  });
  it("keeps a real error", () => {
    expect(offline({ name: "AuthApiError", status: 400 })).toBe(false);
  });
});
