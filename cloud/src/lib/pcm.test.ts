import { Downsampler } from "./pcm";

const tone = (rate: number, seconds: number) => Float32Array.from({ length: rate * seconds }, (_, i) => 0.5 * Math.sin((2 * Math.PI * 440 * i) / rate));

test.each([48_000, 44_100, 16_000])("%i Hz becomes 16 kHz, whatever the block size", (rate) => {
  const input = tone(rate, 1);
  const whole = new Downsampler(rate).push(input);
  expect(whole.length).toBe(16_000);
  const ds = new Downsampler(rate);
  const blocks: number[] = [];
  for (let i = 0; i < input.length; i += 128) blocks.push(...ds.push(input.subarray(i, i + 128)));
  expect(blocks).toEqual([...whole]);
});

test("a 440 Hz tone keeps its amplitude and its zero crossings", () => {
  const out = new Downsampler(48_000).push(tone(48_000, 1));
  const peak = Math.max(...out);
  expect(peak).toBeGreaterThan(0.48 * 0x7fff);
  expect(peak).toBeLessThanOrEqual(0.5 * 0x7fff);
  let crossings = 0;
  for (let i = 1; i < out.length; i++) if (out[i - 1] < 0 !== out[i] < 0) crossings++;
  expect(Math.abs(crossings - 880)).toBeLessThanOrEqual(2);
});

test("full scale clamps to the int16 range", () => {
  const out = new Downsampler(16_000).push(Float32Array.from([2, -2, 1, -1, 0]));
  expect([...out]).toEqual([32767, -32768, 32767, -32768, 0]);
});

test("a mic below 16 kHz is refused", () => expect(() => new Downsampler(8_000)).toThrow(/8000 Hz/));
