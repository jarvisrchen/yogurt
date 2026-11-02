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
}

export const STT_ENGINE = "cloud · nova-3 · phone";
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

/** The document a push sends: the Mac's `bundle_value` shape, field for field, for a meeting with no labels, chat, or enhance yet. */
export function bundle(m: PhoneMeeting) {
  return {
    v: 1 as const,
    meeting: {
      id: m.id,
      title: m.title.trim() || UNTITLED,
      started_at: m.started_at,
      ended_at: m.ended_at,
      notes_md: m.notes_md,
      enriched_md: null,
      transcript_json: JSON.stringify(m.lines.map(({ channel, text, ts_ms }) => ({ channel, text, ts_ms }))),
      starred: false,
      stt_engine: STT_ENGINE,
      llm_model: null,
      template: null,
      created_at: m.created_at,
      labels: [],
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
