import { afterEach, expect, test } from "vitest";
import { bundle, type PhoneMeeting } from "./phone";
import { engineFor, localChoice, localModel, readStt, saveStt } from "./stt";

const store = new Map<string, string>();
Object.assign(globalThis, {
  localStorage: { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) },
  window: globalThis,
});

afterEach(() => {
  store.clear();
  delete (globalThis as { YogurtAndroid?: unknown }).YogurtAndroid;
});

test("a phone starts on Cloud with Parakeet picked for Local, as the Mac keeps its model while Cloud is in use", () => {
  expect(readStt()).toEqual({ engine: "cloud", model: "parakeet-v3" });
  saveStt({ engine: "local", model: "small.en" });
  expect(readStt()).toEqual({ engine: "local", model: "small.en" });
});

test("only the Android app transcribes on the phone, so a browser records with Deepgram whatever was saved", () => {
  saveStt({ engine: "local", model: "small.en" });
  expect(localChoice()).toBeNull();
  (globalThis as { YogurtAndroid?: unknown }).YogurtAndroid = {};
  expect(localChoice()).toBe("small.en");
});

test("the meeting's engine names the model, and a meeting from before keeps Deepgram's", () => {
  expect(engineFor("parakeet-v3")).toBe("local · parakeet-v3 · phone");
  expect(localModel(engineFor("parakeet-v3"))).toBe("parakeet-v3");
  expect(localModel(engineFor(null))).toBeNull();
  const m: PhoneMeeting = { id: "M", title: "", started_at: 1, ended_at: null, notes_md: "", lines: [], created_at: "" };
  expect(bundle(m).meeting.stt_engine).toBe("cloud · nova-3 · phone");
  expect(bundle({ ...m, stt_engine: engineFor("tiny.en") }).meeting.stt_engine).toBe("local · tiny.en · phone");
});
