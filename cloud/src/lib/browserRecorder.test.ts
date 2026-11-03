import { BrowserRecorder } from "./browserRecorder";
import type { RecorderEvent } from "./recorder";

class FakeSocket {
  static all: FakeSocket[] = [];
  binaryType = "";
  audio: ArrayBuffer[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((m: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  constructor() {
    FakeSocket.all.push(this);
  }
  send(d: unknown) {
    if (d instanceof ArrayBuffer) this.audio.push(d);
  }
  close() {}
  final(text: string, start: number, duration: number) {
    const frame = { type: "Results", channel: { alternatives: [{ transcript: text }] }, start, duration, is_final: true, speech_final: true };
    this.onmessage!({ data: JSON.stringify(frame) });
  }
}

/** 100 ms of audio whose every sample is `i`, so a socket's first sample says which chunk it got first. */
const chunk = (i: number) => new Int16Array(1600).fill(i).buffer;

async function recording() {
  FakeSocket.all = [];
  vi.useFakeTimers();
  vi.stubGlobal("window", globalThis);
  vi.stubGlobal("document", { removeEventListener() {} });
  vi.stubGlobal("WebSocket", FakeSocket);
  const rec = new BrowserRecorder(async () => ({ token: "t", url: "wss://dg" }));
  const events: RecorderEvent[] = [];
  rec.on((e) => events.push(e));
  const r = rec as unknown as { audio(d: ArrayBuffer): void; connect(): Promise<void> };
  let n = 0;
  const speak = (ms: number) => {
    for (let end = n + ms / 100; n < end; n++) r.audio(chunk(n));
  };
  await r.connect();
  const socket = async () => {
    await vi.runOnlyPendingTimersAsync();
    const ws = FakeSocket.all.at(-1)!;
    ws.onopen!();
    return ws;
  };
  return { rec, events, speak, socket, first: FakeSocket.all[0] };
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const lines = (events: RecorderEvent[]) => events.flatMap((e) => (e.type === "line" ? [[e.line.ts_ms, e.line.text]] : []));
const gaps = (events: RecorderEvent[]) => events.filter((e) => e.type === "gap");

test("a dropped socket replays the audio Deepgram had not finalized, so nothing is lost or heard twice", async () => {
  const { events, speak, socket, first } = await recording();
  first.onopen!();
  speak(3_000);
  first.final("one", 0, 2);
  speak(1_000);
  first.onclose!();
  speak(500);

  const second = await socket();
  expect(new Int16Array(second.audio[0])[0]).toBe(20);
  expect(second.audio).toHaveLength(25);
  second.final("two", 0, 2);
  speak(500);
  second.final("three", 2, 1);
  expect(lines(events)).toEqual([
    [0, "one"],
    [2_000, "two"],
    [4_000, "three"],
  ]);
  expect(gaps(events)).toEqual([]);
});

test("a dropped socket keeps the words Deepgram finalized in the window, and replays only the audio after them", async () => {
  const { events, speak, socket, first } = await recording();
  first.onopen!();
  speak(2_000);
  first.onmessage!({ data: JSON.stringify({ type: "Results", channel: { alternatives: [{ transcript: "kept words" }] }, start: 0, duration: 1.5, is_final: true, speech_final: false }) });
  first.onclose!();
  expect(lines(events)).toEqual([[0, "kept words"]]);
  const second = await socket();
  expect(new Int16Array(second.audio[0])[0]).toBe(15);
  expect(second.audio).toHaveLength(5);
});

test("stopping while the socket is down marks the audio it held as a gap", async () => {
  const { rec, events, speak, first } = await recording();
  first.onopen!();
  speak(3_000);
  first.final("one", 0, 2);
  first.onclose!();
  speak(500);
  await rec.stop();
  expect(gaps(events)).toEqual([{ type: "gap", from_ms: 2_000, to_ms: 3_500 }]);
  expect(events.at(-1)).toEqual({ type: "state", state: "stopped" });
});
