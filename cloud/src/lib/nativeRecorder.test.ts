import { NativeRecorder, type Bridge } from "./nativeRecorder";
import type { RecorderEvent } from "./recorder";

const line = (i: number) => ({ channel: "them" as const, text: `line ${i}`, ts_ms: i * 1000 });

/** A stand-in for the app: it holds the lines of one meeting, `kept`, numbered from `base`, and records every call. */
function fakeApp(status: object = { recording: false, meetingId: null }, meetingId = "M1", base = 0) {
  const kept = [line(base), line(base + 1), line(base + 2)];
  const calls: string[] = [];
  const app = { meetingId, base };
  const bridge: Bridge = {
    start: (json) => (calls.push(`start ${json}`), '{"ok":true}'),
    setAuth: (json) => void calls.push(`setAuth ${json}`),
    stop: () => void calls.push("stop"),
    status: () => JSON.stringify(status),
    lines: (id, from) => {
      if (typeof id !== "string" || typeof from !== "number") throw new Error("lines takes a meeting id and an index");
      calls.push(`lines ${id} ${from}`);
      return JSON.stringify(id === app.meetingId ? kept.slice(Math.max(0, from - app.base)) : []);
    },
    appVersion: () => "0.1.0",
  };
  const win = new EventTarget() as unknown as Window;
  const doc = Object.assign(new EventTarget(), { visibilityState: "visible" });
  Object.assign(win, { document: doc });
  const post = (detail: unknown) => win.dispatchEvent(new CustomEvent("yogurt-native", { detail }));
  /** A line event as the app sends it, for its meeting unless told otherwise. */
  const heard = (index: number, id = app.meetingId) => post({ type: "line", meetingId: id, index, line: line(index) });
  return { app, kept, calls, bridge, win, doc, post, heard };
}

async function started(app: ReturnType<typeof fakeApp>, have = 0, meetingId = "M1") {
  const rec = new NativeRecorder(app.bridge, async () => ({ jwt: "j", browserToken: "b" }), "https://cloud.test/api/stt-token", app.win);
  const events: RecorderEvent[] = [];
  rec.on((e) => events.push(e));
  await rec.start({ meetingId, startedAtMs: 1000, have });
  return { rec, events, texts: () => events.filter((e) => e.type === "line").map((e) => (e.type === "line" ? `${e.index}:${e.line.text}` : "")) };
}

test("start hands the app the meeting, the token address, and the session, then takes the lines it already has", async () => {
  const app = fakeApp();
  const { texts } = await started(app);
  expect(JSON.parse(app.calls[0].slice(6))).toEqual({ meetingId: "M1", startedAtMs: 1000, have: 0, tokenUrl: "https://cloud.test/api/stt-token", auth: { jwt: "j", browserToken: "b" } });
  expect(texts()).toEqual(["0:line 0", "1:line 1", "2:line 2"]);
});

test("a line event in order is taken, a repeat is dropped, and a skipped one fetches the missed lines", async () => {
  const app = fakeApp();
  const { texts } = await started(app);
  app.kept.push(line(3), line(4), line(5));
  app.heard(3);
  app.heard(3);
  app.heard(1);
  app.post(JSON.stringify({ type: "line", meetingId: "M1", index: 5, line: line(5) }));
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
  expect(app.calls.at(-1)).toBe("lines M1 3");
});

test("a recorder already running for this meeting is joined, not restarted, and shows its state", async () => {
  const app = fakeApp({ recording: true, meetingId: "M1", state: "live" });
  const { texts, events } = await started(app, 2);
  expect(app.calls.some((c) => c.startsWith("start"))).toBe(false);
  expect(texts()).toEqual(["2:line 2"]);
  expect(events[0]).toEqual({ type: "state", state: "live" });
});

test("a refused start throws the app's error", async () => {
  const app = fakeApp();
  app.bridge.start = () => '{"ok":false,"error":"no mic permission"}';
  await expect(started(app)).rejects.toThrow("no mic permission");
});

test("stop waits for the app's stopped, takes any last lines, and stops listening", async () => {
  const app = fakeApp();
  const { rec, events, texts } = await started(app);
  app.bridge.stop = () => {
    app.calls.push("stop");
    // The app answers at once and reports the last lines and stopped later, as its stop flushes Deepgram.
    setTimeout(() => {
      app.kept.push(line(3), line(4));
      app.heard(3);
      app.post({ type: "state", state: "stopped" });
    }, 10);
  };
  let done = false;
  const stopping = rec.stop().then(() => (done = true));
  await new Promise((r) => setTimeout(r, 5));
  expect(done).toBe(false);
  await stopping;
  expect(app.calls).toContain("stop");
  expect(texts().slice(-2)).toEqual(["3:line 3", "4:line 4"]);
  expect(events.at(-1)).toEqual({ type: "state", state: "stopped" });
  const before = events.length;
  app.post({ type: "interim", text: "late" });
  expect(events).toHaveLength(before);
});

test("a resumed meeting carries on the numbering, so a reload mid-recording joins at the page's count and drops nothing", async () => {
  // The page holds 3 lines from before the app was killed, and the app starts the meeting again numbering from there.
  const app = fakeApp({ recording: false, meetingId: null }, "M1", 3);
  app.kept.length = 0;
  const first = await started(app, 3);
  expect(JSON.parse(app.calls[0].slice(6))).toMatchObject({ meetingId: "M1", have: 3 });
  app.kept.push(line(3));
  app.heard(3);
  expect(first.texts()).toEqual(["3:line 3"]);
  // The page reloads with 4 lines and finds the app still recording the meeting.
  const again = fakeApp({ recording: true, meetingId: "M1", state: "live" }, "M1", 3);
  again.kept.length = 0;
  again.kept.push(line(3), line(4));
  const second = await started(again, 4);
  again.kept.push(line(5));
  again.heard(5);
  expect(second.texts()).toEqual(["4:line 4", "5:line 5"]);
});

test("a start that waits for the mic takes none of the previous meeting's lines, and ignores its late events", async () => {
  // The app still holds M0's lines, because the permission dialog comes before it switches to M1.
  const app = fakeApp({ recording: false, meetingId: "M0" }, "M0");
  const { texts } = await started(app, 0, "M1");
  expect(texts()).toEqual([]);
  app.heard(3);
  app.app.meetingId = "M1";
  app.kept.length = 0;
  app.kept.push(line(0));
  app.heard(0);
  app.heard(4, "M0");
  expect(texts()).toEqual(["0:line 0"]);
});

test("the app's request for a fresh session is answered through setAuth, and never reaches the record screen", async () => {
  const app = fakeApp();
  let n = 0;
  const rec = new NativeRecorder(app.bridge, async () => ({ jwt: `j${++n}`, browserToken: "b" }), "https://cloud.test/api/stt-token", app.win);
  const events: RecorderEvent[] = [];
  rec.on((e) => events.push(e));
  await rec.start({ meetingId: "M1", startedAtMs: 1000, have: 3 });
  const before = events.length;
  app.post({ type: "auth" });
  await new Promise((r) => setTimeout(r, 0));
  expect(app.calls.at(-1)).toBe('setAuth {"jwt":"j2","browserToken":"b"}');
  expect(events).toHaveLength(before);
});

test("a page joining a running recorder hands it its own session", async () => {
  const app = fakeApp({ recording: true, meetingId: "M1", state: "live" });
  await started(app, 3);
  await new Promise((r) => setTimeout(r, 0));
  expect(app.calls).toContain('setAuth {"jwt":"j","browserToken":"b"}');
});
