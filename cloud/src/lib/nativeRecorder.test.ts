import { NativeRecorder, type Bridge } from "./nativeRecorder";
import type { RecorderEvent } from "./recorder";

const line = (i: number) => ({ channel: "them" as const, text: `line ${i}`, ts_ms: i * 1000 });

/** A stand-in for the app: it holds `kept` final lines and records every call. */
function fakeApp(status = { recording: false, meetingId: null as string | null }) {
  const kept = [line(0), line(1), line(2)];
  const calls: string[] = [];
  const bridge: Bridge = {
    start: (json) => (calls.push(`start ${json}`), '{"ok":true}'),
    setAuth: (json) => void calls.push(`setAuth ${json}`),
    stop: () => void calls.push("stop"),
    status: () => JSON.stringify(status),
    lines: (from) => (calls.push(`lines ${from}`), JSON.stringify(kept.slice(from))),
    appVersion: () => "0.1.0",
  };
  const win = new EventTarget() as unknown as Window;
  const doc = Object.assign(new EventTarget(), { visibilityState: "visible" });
  Object.assign(win, { document: doc });
  const post = (detail: unknown) => win.dispatchEvent(new CustomEvent("yogurt-native", { detail }));
  return { kept, calls, bridge, win, doc, post };
}

async function started(app: ReturnType<typeof fakeApp>, have = 0) {
  const rec = new NativeRecorder(app.bridge, async () => ({ jwt: "j", browserToken: "b" }), "https://cloud.test/api/stt-token", app.win);
  const events: RecorderEvent[] = [];
  rec.on((e) => events.push(e));
  await rec.start({ meetingId: "M1", startedAtMs: 1000, have });
  return { rec, events, texts: () => events.filter((e) => e.type === "line").map((e) => (e.type === "line" ? `${e.index}:${e.line.text}` : "")) };
}

test("start hands the app the meeting, the token address, and the session, then takes the lines it already has", async () => {
  const app = fakeApp();
  const { texts } = await started(app);
  expect(JSON.parse(app.calls[0].slice(6))).toEqual({ meetingId: "M1", startedAtMs: 1000, tokenUrl: "https://cloud.test/api/stt-token", auth: { jwt: "j", browserToken: "b" } });
  expect(texts()).toEqual(["0:line 0", "1:line 1", "2:line 2"]);
});

test("a line event in order is taken, a repeat is dropped, and a skipped one fetches the missed lines", async () => {
  const app = fakeApp();
  const { texts } = await started(app);
  app.kept.push(line(3), line(4), line(5));
  app.post({ type: "line", index: 3, line: line(3) });
  app.post({ type: "line", index: 3, line: line(3) });
  app.post({ type: "line", index: 1, line: line(1) });
  app.post(JSON.stringify({ type: "line", index: 5, line: line(5) }));
  expect(texts()).toEqual(["0:line 0", "1:line 1", "2:line 2", "3:line 3", "4:line 4", "5:line 5"]);
});

test("other events pass through as they are", async () => {
  const app = fakeApp();
  const { events } = await started(app);
  const sent = [
    { type: "interim", text: "what about" },
    { type: "state", state: "reconnecting" },
    { type: "gap", from_ms: 1000, to_ms: 2000 },
    { type: "error", message: "mic busy" },
  ];
  sent.forEach(app.post);
  expect(events.filter((e) => e.type !== "line")).toEqual(sent);
});

test("coming back to the page asks for the lines missed while hidden", async () => {
  const app = fakeApp();
  const { texts } = await started(app);
  app.kept.push(line(3));
  app.doc.dispatchEvent(new Event("visibilitychange"));
  expect(texts().at(-1)).toBe("3:line 3");
  expect(app.calls.at(-1)).toBe("lines 3");
});

test("a recorder already running for this meeting is joined, not restarted", async () => {
  const app = fakeApp({ recording: true, meetingId: "M1" });
  const { texts } = await started(app, 2);
  expect(app.calls.some((c) => c.startsWith("start"))).toBe(false);
  expect(texts()).toEqual(["2:line 2"]);
});

test("a refused start throws the app's error", async () => {
  const app = fakeApp();
  app.bridge.start = () => '{"ok":false,"error":"no mic permission"}';
  await expect(started(app)).rejects.toThrow("no mic permission");
});

test("stop stops the app, takes any last lines, and stops listening", async () => {
  const app = fakeApp();
  const { rec, events, texts } = await started(app);
  app.kept.push(line(3));
  await rec.stop();
  expect(app.calls).toContain("stop");
  expect(texts().at(-1)).toBe("3:line 3");
  expect(events.at(-1)).toEqual({ type: "state", state: "stopped" });
  const before = events.length;
  app.post({ type: "interim", text: "late" });
  expect(events).toHaveLength(before);
});
