import { afterEach, expect, test, vi } from "vitest";
import type { Api } from "./api";
import { streamChat } from "./useChat";

const api = { sb: { auth: { getSession: async () => ({ data: { session: { access_token: "jwt" } } }) } } } as unknown as Api;
const scope = { device_id: "d", meeting_id: "m" };
const turns = [{ role: "user" as const, content: "What was just said?" }];

afterEach(() => vi.unstubAllGlobals());

test("the push runs to the end before the question is sent", async () => {
  const order: string[] = [];
  vi.stubGlobal("fetch", async () => {
    order.push("ask");
    return new Response('{"delta":"Line 3."}\n');
  });
  const before = async () => {
    await new Promise((r) => setTimeout(r, 20));
    order.push("push");
  };
  const deltas: string[] = [];
  await streamChat(api, "t", scope, turns, () => undefined, (d) => deltas.push(d), new AbortController().signal, before);
  expect(order).toEqual(["push", "ask"]);
  expect(deltas).toEqual(["Line 3."]);
});

test("a failed push stops the question and surfaces its error", async () => {
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  const before = () => Promise.reject(new Error("Could not save the meeting before asking. You're offline."));
  await expect(streamChat(api, "t", scope, turns, () => undefined, () => undefined, new AbortController().signal, before)).rejects.toThrow("Could not save the meeting before asking");
  expect(fetch).not.toHaveBeenCalled();
});
