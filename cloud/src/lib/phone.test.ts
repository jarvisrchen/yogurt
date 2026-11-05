import { bundle, mergeLabels, readDevice, readMac, saveDevice, saveMac, ulid, type PhoneMeeting } from "./phone";

test("a ULID is the time then the randomness, in Crockford base32, and sorts by time", () => {
  const zeros = (n: number) => new Uint8Array(n);
  expect(ulid(0, zeros)).toBe("00000000000000000000000000");
  expect(ulid(1_469_918_176_385, zeros)).toBe("01ARYZ6S410000000000000000");
  expect(ulid(0, (n) => new Uint8Array(n).fill(255))).toBe("0000000000ZZZZZZZZZZZZZZZZ");
  const a = ulid(1_762_000_000_000);
  const b = ulid(1_762_000_000_001);
  expect(a).toMatch(/^[0-9A-HJKMNP-TV-Z]{26}$/);
  expect(a < b).toBe(true);
});

const m: PhoneMeeting = {
  id: "01JBZQ3T6R8Y0000000000000",
  title: "  ",
  started_at: 1_762_000_000_000,
  ended_at: null,
  notes_md: "- pro tier",
  lines: [{ channel: "them", text: "Ship it Friday.", ts_ms: 12_000 }],
  created_at: "2025-11-01T12:26:40.000Z",
};

test("the bundle has the Mac's exact fields, an untitled default, and the phone's engine", () => {
  const b = bundle(m);
  expect(Object.keys(b).sort()).toEqual(["chat", "meeting", "v"]);
  expect(Object.keys(b.meeting).sort()).toEqual(["created_at", "ended_at", "enriched_md", "for_device", "id", "labels", "llm_model", "notes_md", "starred", "started_at", "stt_engine", "template", "title", "transcript_json"]);
  expect(b).toMatchObject({ v: 1, chat: [], meeting: { title: "Untitled meeting", ended_at: null, stt_engine: "cloud · nova-3 · phone", starred: false, labels: [] } });
  expect(b.meeting.transcript_json).toBe('[{"channel":"them","text":"Ship it Friday.","ts_ms":12000}]');
});

test("an ended meeting keeps its title and end", () => {
  expect(bundle({ ...m, title: "Q4 pricing sync", ended_at: m.started_at + 60_000 }).meeting).toMatchObject({ title: "Q4 pricing sync", ended_at: 1_762_000_060_000 });
});

test("picked labels ride in the bundle by name, the shape the Mac reads when it takes the meeting", () => {
  const labels = [{ name: "Build", color: "matcha" }, { name: "Site visit", color: "" }];
  expect(bundle({ ...m, labels }).meeting.labels).toEqual([
    { id: "build", name: "Build", color: "matcha" },
    { id: "site visit", name: "Site visit", color: "" },
  ]);
});

test("the Macs' labels merge to one per name as the Mac matches it, sorted, keeping the first color", () => {
  expect(mergeLabels([{ name: "build", color: "blue" }, { name: " Build ", color: "matcha" }, { name: "Admin", color: "straw" }, { name: " ", color: "blue" }])).toEqual([
    { name: "Admin", color: "straw" },
    { name: "build", color: "blue" },
  ]);
  // The Mac folds only A to Z, so these stay two labels there and here.
  expect(mergeLabels([{ name: "Über", color: "blue" }, { name: "über", color: "lilac" }])).toHaveLength(2);
});

test("a recording goes to the Mac picked last while it is still offered, or else the one seen last", () => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
  const macs = [{ id: "mbp", seen_at: "2026-11-04T10:00:00Z" }, { id: "mini", seen_at: "2026-11-04T11:00:00Z" }];
  expect(readMac(macs)).toBe("mini");
  saveMac("mbp");
  expect(readMac(macs)).toBe("mbp");
  expect(readMac([macs[1]])).toBe("mini");
  expect(readMac([])).toBeNull();
  expect(bundle({ ...m, for_device: "mini" }).meeting.for_device).toBe("mini");
  expect(bundle(m).meeting.for_device).toBeNull();
  vi.unstubAllGlobals();
});

test("the device id is made once and kept", () => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
  const first = readDevice();
  expect(first).toMatchObject({ name: "Phone", registered: false });
  saveDevice({ ...first, name: "Pixel", registered: true });
  expect(readDevice()).toEqual({ id: first.id, name: "Pixel", registered: true });
  vi.unstubAllGlobals();
});
