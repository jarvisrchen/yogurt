import { flush, fold, utterance } from "./deepgram";

const frame = (transcript: string, o: { is_final?: boolean; speech_final?: boolean; start?: number } = {}) =>
  JSON.stringify({ type: "Results", channel: { alternatives: [{ transcript, confidence: 0.9 }] }, start: 0, ...o });

test("interims, window-finals, and speech_final fold into one line that starts at the first word", () => {
  const st = utterance();
  expect(fold(st, frame("hello", { start: 1.42 }))).toEqual({ text: "hello", ts_ms: 1420, final: false });
  expect(fold(st, frame("hello world", { start: 1.42, is_final: true, speech_final: false }))).toEqual({ text: "hello world", ts_ms: 1420, final: false });
  expect(fold(st, frame("how", { start: 7.5 }))).toEqual({ text: "hello world how", ts_ms: 1420, final: false });
  expect(fold(st, frame("how are you", { start: 7.5, is_final: true, speech_final: true }))).toEqual({ text: "hello world how are you", ts_ms: 1420, final: true });
  expect(fold(st, frame("next", { start: 9 }))).toEqual({ text: "next", ts_ms: 9000, final: false });
});

test("a resent closing frame and an empty speech_final emit nothing", () => {
  const st = utterance();
  expect(fold(st, frame("done", { is_final: true, speech_final: true }))?.final).toBe(true);
  expect(fold(st, frame("done", { is_final: true, speech_final: true }))).toBeNull();
  expect(fold(st, frame("", { is_final: true, speech_final: true }))).toBeNull();
  expect(fold(st, frame(""))).toBeNull();
});

test("is_final stands in for a missing speech_final", () => {
  expect(fold(utterance(), frame("ok", { is_final: true }))?.final).toBe(true);
});

test("other frames and bad JSON are ignored", () => {
  const st = utterance();
  expect(fold(st, JSON.stringify({ type: "Metadata", request_id: "x" }))).toBeNull();
  expect(fold(st, "not json")).toBeNull();
});

test("flush keeps the finished words of an utterance whose socket closed", () => {
  const st = utterance();
  fold(st, frame("half a", { start: 3, is_final: true, speech_final: false }));
  fold(st, frame("thought", { start: 4 }));
  expect(flush(st)).toEqual({ text: "half a", ts_ms: 3000, final: true });
  expect(flush(st)).toBeNull();
});
