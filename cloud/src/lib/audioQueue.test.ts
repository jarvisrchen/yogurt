import { AudioQueue, QUEUE_MS } from "./audioQueue";

const chunk = (i: number) => ({ ts_ms: i * 100, data: new Int16Array(1600).fill(i).buffer });

test("drains in order and empties", () => {
  const q = new AudioQueue();
  for (let i = 0; i < 50; i++) q.push(chunk(i));
  const { chunks, gap } = q.drain();
  expect(chunks.map((c) => c.ts_ms)).toEqual(Array.from({ length: 50 }, (_, i) => i * 100));
  expect(new Int16Array(chunks[7].data)[0]).toBe(7);
  expect(gap).toBeNull();
  expect(q.drain().chunks).toEqual([]);
});

test("holds five minutes and drops the oldest past it, as one gap", () => {
  const q = new AudioQueue();
  const cap = QUEUE_MS / 100;
  for (let i = 0; i < cap + 25; i++) q.push(chunk(i));
  expect(q.length).toBe(cap);
  const { chunks, gap } = q.drain();
  expect(chunks[0].ts_ms).toBe(2_500);
  expect(chunks.at(-1)!.ts_ms).toBe((cap + 24) * 100);
  expect(gap).toEqual({ from_ms: 0, to_ms: 2_500 });
  expect(q.drain().gap).toBeNull();
});

test("a small cap drops one chunk at a time", () => {
  const q = new AudioQueue(300);
  for (let i = 0; i < 5; i++) q.push(chunk(i));
  const { chunks, gap } = q.drain();
  expect(chunks.map((c) => c.ts_ms)).toEqual([200, 300, 400]);
  expect(gap).toEqual({ from_ms: 0, to_ms: 200 });
});
