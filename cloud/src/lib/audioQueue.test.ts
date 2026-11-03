import { AudioQueue, QUEUE_MS } from "./audioQueue";

const chunk = (i: number) => ({ ts_ms: i * 100, data: new Int16Array(1600).fill(i).buffer });

test("holds chunks in order until they are confirmed", () => {
  const q = new AudioQueue();
  for (let i = 0; i < 50; i++) q.push(chunk(i));
  const held = q.pending();
  expect(held.map((c) => c.ts_ms)).toEqual(Array.from({ length: 50 }, (_, i) => i * 100));
  expect(new Int16Array(held[7].data)[0]).toBe(7);
  expect(q.takeGap()).toBeNull();
  expect(q.pending()).toHaveLength(50);
});

test("confirm drops the finalized audio and cuts a chunk that straddles it", () => {
  const q = new AudioQueue();
  for (let i = 0; i < 5; i++) q.push(chunk(i));
  q.confirm(250);
  const held = q.pending();
  expect(held.map((c) => c.ts_ms)).toEqual([250, 300, 400]);
  expect(held[0].data.byteLength).toBe(50 * 32);
  expect(new Int16Array(held[0].data)[0]).toBe(2);
  q.confirm(10_000);
  expect(q.pending()).toEqual([]);
});

test("holds five minutes and drops the oldest past it, as one gap", () => {
  const q = new AudioQueue();
  const cap = QUEUE_MS / 100;
  for (let i = 0; i < cap + 25; i++) q.push(chunk(i));
  expect(q.length).toBe(cap);
  const held = q.pending();
  expect(held[0].ts_ms).toBe(2_500);
  expect(held.at(-1)!.ts_ms).toBe((cap + 24) * 100);
  expect(q.takeGap()).toEqual({ from_ms: 0, to_ms: 2_500 });
  expect(q.takeGap()).toBeNull();
});

test("a small cap drops one chunk at a time, and a confirm frees room", () => {
  const q = new AudioQueue(300);
  for (let i = 0; i < 5; i++) q.push(chunk(i));
  expect(q.pending().map((c) => c.ts_ms)).toEqual([200, 300, 400]);
  expect(q.takeGap()).toEqual({ from_ms: 0, to_ms: 200 });
  q.confirm(400);
  q.push(chunk(5));
  q.push(chunk(6));
  expect(q.pending().map((c) => c.ts_ms)).toEqual([400, 500, 600]);
  expect(q.takeGap()).toBeNull();
});
