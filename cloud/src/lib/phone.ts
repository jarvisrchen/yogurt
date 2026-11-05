/** One transcript line as the Mac stores it. A phone has one mic, so every line is on the them channel. */
export interface Line {
  channel: "them";
  text: string;
  ts_ms: number;
}

/** The meeting being recorded on this phone. */
export interface PhoneMeeting {
  id: string;
  title: string;
  /** Unix ms. */
  started_at: number;
  ended_at: number | null;
  notes_md: string;
  lines: Line[];
  /** RFC 3339, as the Mac's chrono timestamp serializes. */
  created_at: string;
  /** Absent on a meeting saved before labels. */
  labels?: PhoneLabel[];
  /** The Mac that takes the meeting, or any Mac when null or absent. */
  for_device?: string | null;
}

/** A label by name, since the Mac that takes the meeting finds its own label with that name or makes one. */
export interface PhoneLabel {
  name: string;
  color: string;
}

/** The Mac refuses a longer label name, and then could not take the meeting. */
export const LABEL_MAX = 40;

/** A label's name as the Mac matches it: trimmed, with only A to Z folded, as SQLite's NOCASE does. */
export const labelKey = (name: string) => name.trim().replace(/[A-Z]/g, (c) => c.toLowerCase());

/** The Macs' labels as one list: one per name as the Mac matches it, sorted by name. */
export function mergeLabels(labels: PhoneLabel[]): PhoneLabel[] {
  const byName = new Map<string, PhoneLabel>();
  for (const l of labels) {
    const key = labelKey(l.name);
    if (key && !byName.has(key)) byName.set(key, { name: l.name.trim(), color: l.color });
  }
  return [...byName.values()].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
}

export const STT_ENGINE = "cloud · nova-3 · phone";

/** Transcribed from a phone's one mic, so every line is on the them channel. A Mac's adopted copy is too, so whether the webapp may write a meeting comes from its device's kind instead. */
export const oneMic = (m: { stt_engine?: string | null }) => !!m.stt_engine?.endsWith("· phone");
export const UNTITLED = "Untitled meeting";

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** A ULID like the Mac's meeting ids: 48 bits of ms time, then 80 random bits, in Crockford base32. */
export function ulid(now = Date.now(), random: (n: number) => Uint8Array = (n) => crypto.getRandomValues(new Uint8Array(n))): string {
  let time = "";
  for (let t = now, i = 0; i < 10; i++, t = Math.floor(t / 32)) time = CROCKFORD[t % 32] + time;
  const bytes = random(10);
  let bits = 0;
  let acc = 0;
  let rand = "";
  for (const b of bytes) {
    acc = (acc << 8) | b;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      rand += CROCKFORD[(acc >> bits) & 31];
    }
  }
  return time + rand;
}

/** What an enhance changes in a meeting, under the Mac's field names. */
export interface Enhanced {
  enriched_md: string;
  llm_model: string;
  template: string;
}

/** The document a push sends: the Mac's `bundle_value` shape, field for field, for a meeting with no chat. A label's id is its matched name, the one key the phone has. */
export function bundle(m: PhoneMeeting, enhanced: Enhanced | null = null) {
  return {
    v: 1 as const,
    meeting: {
      id: m.id,
      title: m.title.trim() || UNTITLED,
      started_at: m.started_at,
      ended_at: m.ended_at,
      notes_md: m.notes_md,
      enriched_md: enhanced?.enriched_md ?? null,
      transcript_json: JSON.stringify(m.lines.map(({ channel, text, ts_ms }) => ({ channel, text, ts_ms }))),
      starred: false,
      stt_engine: STT_ENGINE,
      llm_model: enhanced?.llm_model ?? null,
      template: enhanced?.template ?? null,
      created_at: m.created_at,
      labels: (m.labels ?? []).map(({ name, color }) => ({ id: labelKey(name), name, color })),
      for_device: m.for_device ?? null,
    },
    chat: [],
  };
}

/** This browser as a device: an id it made, the name the user gave it, and whether the Cloud has it yet. */
export interface PhoneDevice {
  id: string;
  name: string;
  registered: boolean;
}

const DEVICE_KEY = "yogurt-cloud-phone";
const MAC_KEY = "yogurt-cloud-phone-mac";

/** The Mac this phone last saved a recording to, if it is still one of `macs`, or else the one seen last. */
export function readMac(macs: { id: string; seen_at: string }[]): string | null {
  let last: string | null = null;
  try {
    last = localStorage.getItem(MAC_KEY);
  } catch {
    // Storage blocked: the first Mac.
  }
  const latest = [...macs].sort((a, b) => Date.parse(b.seen_at) - Date.parse(a.seen_at))[0];
  return macs.find((m) => m.id === last)?.id ?? latest?.id ?? null;
}

export function saveMac(id: string): void {
  try {
    localStorage.setItem(MAC_KEY, id);
  } catch {
    // Storage blocked: the next recording starts on the first Mac.
  }
}

export function readDevice(): PhoneDevice {
  try {
    const v = JSON.parse(localStorage.getItem(DEVICE_KEY) ?? "null");
    if (typeof v?.id === "string" && v.id) return { id: v.id, name: typeof v.name === "string" && v.name.trim() ? v.name : "Phone", registered: v.registered === true };
  } catch {
    // Unreadable storage starts a new device.
  }
  return { id: crypto.randomUUID(), name: "Phone", registered: false };
}

export function saveDevice(d: PhoneDevice): void {
  try {
    localStorage.setItem(DEVICE_KEY, JSON.stringify(d));
  } catch {
    // Storage blocked: the next recording registers again under a new id.
  }
}

/** The meeting in progress, kept so a killed tab can resume it. `saved_at` is when the page last wrote it, so roughly when it stopped. */
export interface Unfinished {
  device_id: string;
  meeting: PhoneMeeting;
  saved_at: number;
}

const DB = "yogurt-cloud";
const STORE = "recording";
const KEY = "current";

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function run<T>(mode: IDBTransactionMode, op: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await open();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const req = op(tx.objectStore(STORE));
      tx.oncomplete = () => resolve(req.result);
      tx.onerror = tx.onabort = () => reject(tx.error);
    });
  } finally {
    db.close();
  }
}

export const loadUnfinished = () => run<Unfinished | undefined>("readonly", (s) => s.get(KEY)).then((v) => v ?? null);
export const saveUnfinished = (u: Unfinished) => run("readwrite", (s) => s.put(u, KEY)).then(() => undefined);
export const clearUnfinished = () => run("readwrite", (s) => s.delete(KEY)).then(() => undefined);
