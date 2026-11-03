import { Adopted, type Api } from "./api";
import type { Unfinished } from "./phone";
import { RecordingSession } from "./session";

const store = vi.hoisted(() => ({ kept: null as Unfinished | null }));
vi.mock("./phone", async (real) => ({
  ...(await real<typeof import("./phone")>()),
  loadUnfinished: async () => store.kept,
  saveUnfinished: async (u: Unfinished) => void (store.kept = u),
  clearUnfinished: async () => void (store.kept = null),
}));

const left: Unfinished = {
  device_id: "phone-1",
  meeting: { id: "01JPHONE", title: "Pricing sync", started_at: 1_000, ended_at: 9_000, notes_md: "", lines: [{ channel: "them", text: "hello", ts_ms: 0 }], created_at: "1970-01-01T00:00:01Z" },
  saved_at: 9_000,
};

test("ending a meeting a Mac already adopted drops the page's copy and enhances nothing", async () => {
  store.kept = left;
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  vi.stubGlobal("window", {});
  const api = { pushMeeting: vi.fn().mockRejectedValue(new Adopted("A Mac took this meeting.")) } as unknown as Api;
  const s = new RecordingSession(api, () => "token", () => undefined);
  await vi.waitFor(() => expect(s.get().unfinished).not.toBeNull());

  expect(await s.endUnfinished()).toBe("phone-1/01JPHONE");
  expect(s.get().unfinished).toBeNull();
  expect(s.get().error).toBeNull();
  expect(store.kept).toBeNull();
  expect(api.pushMeeting).toHaveBeenCalledTimes(1);
  expect(fetch).not.toHaveBeenCalled();
  vi.unstubAllGlobals();
});
