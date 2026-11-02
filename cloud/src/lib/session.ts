import { useSyncExternalStore } from "react";
import { TokenRejected, type Api } from "./api";
import type { Gap } from "./audioQueue";
import { BrowserRecorder } from "./browserRecorder";
import { NativeRecorder } from "./nativeRecorder";
import { bundle, clearUnfinished, loadUnfinished, readDevice, saveDevice, saveUnfinished, ulid, type PhoneMeeting, type Unfinished } from "./phone";
import type { Recorder, RecorderEvent, RecorderState } from "./recorder";

/** A killed tab loses at most this much of the Cloud's copy. */
export const PUSH_EVERY_MS = 30_000;

export interface Snapshot {
  /** The meeting this page is recording, or ending. */
  meeting: PhoneMeeting | null;
  state: RecorderState;
  interim: string;
  gaps: Gap[];
  /** When the socket dropped, so the notice can say how much audio is held. */
  downSince: number | null;
  error: string | null;
  saveError: string | null;
  ending: boolean;
  /** Whether the Android app records, which keeps going with the page in the background. */
  native: boolean;
  /** A meeting a closed page left unfinished, to resume or end. */
  unfinished: Unfinished | null;
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * The recording outlives the screens, so moving between the library and the record screen never stops it.
 * The meeting is written to IndexedDB on every change and pushed whole every 30 seconds and at End, through the RPCs a laptop pushes with.
 */
export class RecordingSession {
  private snap: Snapshot = { meeting: null, state: "stopped", interim: "", gaps: [], downSince: null, error: null, saveError: null, ending: false, native: false, unfinished: null };
  private subs = new Set<() => void>();
  private recorder: Recorder | null = null;
  private deviceId = "";
  private timer = 0;
  private pushes: Promise<unknown> = Promise.resolve();
  private writes: Promise<unknown> = Promise.resolve();
  private lastPushed = "";
  private unsubscribeAuth = () => {};

  constructor(
    private readonly api: Api,
    private readonly browserToken: () => string | null,
    private readonly onRejected: () => void,
  ) {
    loadUnfinished().then(
      (u) => u && !this.snap.meeting && this.set({ unfinished: u }),
      () => undefined,
    );
  }

  subscribe = (fn: () => void) => {
    this.subs.add(fn);
    return () => void this.subs.delete(fn);
  };
  get = () => this.snap;

  private set(p: Partial<Snapshot>) {
    this.snap = { ...this.snap, ...p };
    for (const fn of this.subs) fn();
  }

  /** Makes sure the Cloud has this browser as a phone device, then records a new meeting. */
  async start(title: string, name: string) {
    const device = readDevice();
    const named = device.registered ? device.name : name.trim() || "Phone";
    // Every time, since an insert that finds the device is a no-op, and a device purged from the Cloud comes back.
    try {
      await this.api.registerPhone(device.id, named);
    } catch (e) {
      this.set({ error: `Could not add this phone to your Cloud. ${message(e)}` });
      return;
    }
    saveDevice({ id: device.id, name: named, registered: true });
    const now = Date.now();
    await this.run(device.id, { id: ulid(now), title, started_at: now, ended_at: null, notes_md: "", lines: [], created_at: new Date(now).toISOString() }, true);
  }

  async resume() {
    const u = this.snap.unfinished;
    if (u) await this.run(u.device_id, { ...u.meeting, ended_at: null }, false);
  }

  private async run(deviceId: string, meeting: PhoneMeeting, fresh: boolean) {
    this.deviceId = deviceId;
    this.lastPushed = "";
    const native = !!window.YogurtAndroid;
    this.set({ meeting, unfinished: null, state: "connecting", interim: "", gaps: [], downSince: null, error: null, saveError: null, native });
    this.write();
    const rec: Recorder = window.YogurtAndroid
      ? new NativeRecorder(window.YogurtAndroid, async () => ({ jwt: await this.api.jwt(), browserToken: this.browserToken() ?? "" }), new URL("/api/stt-token", location.href).href)
      : new BrowserRecorder(() => this.grant());
    rec.on(this.heard);
    this.recorder = rec;
    try {
      await rec.start({ meetingId: meeting.id, startedAtMs: meeting.started_at, have: meeting.lines.length });
    } catch (e) {
      this.recorder = null;
      await rec.stop().catch(() => undefined);
      const err = e instanceof DOMException && e.name === "NotAllowedError" ? "The microphone is blocked. Allow it for this site in the browser's settings, then try again." : message(e);
      // A meeting that never recorded anything is dropped, so it does not come back as one to resume.
      if (fresh) await (this.writes = this.writes.then(clearUnfinished)).catch(() => undefined);
      this.set({ meeting: null, state: "stopped", error: err, unfinished: fresh ? null : { device_id: deviceId, meeting, saved_at: Date.now() } });
      return;
    }
    if (rec instanceof NativeRecorder) {
      const { data } = this.api.sb.auth.onAuthStateChange((_e, s) => s && rec.setAuth({ jwt: s.access_token, browserToken: this.browserToken() ?? "" }));
      this.unsubscribeAuth = () => data.subscription.unsubscribe();
    }
    this.timer = window.setInterval(() => void this.pushQuietly(), PUSH_EVERY_MS);
    // At once, so the meeting is in the library from the start.
    void this.pushQuietly();
  }

  private async grant() {
    const token = this.browserToken();
    if (!token) throw new Error("Unlock this browser to keep transcribing.");
    try {
      return await this.api.sttGrant(token);
    } catch (e) {
      if (e instanceof TokenRejected) this.onRejected();
      throw e;
    }
  }

  private heard = (e: RecorderEvent) => {
    const m = this.snap.meeting;
    if (!m) return;
    switch (e.type) {
      case "line":
        this.set({ meeting: { ...m, lines: [...m.lines, e.line] } });
        return this.write();
      case "interim":
        return this.set({ interim: e.text });
      case "state":
        return this.set({ state: e.state, downSince: e.state === "reconnecting" ? (this.snap.downSince ?? Date.now()) : null, ...(e.state === "live" && { error: null }) });
      case "gap":
        return this.set({ gaps: [...this.snap.gaps, { from_ms: e.from_ms, to_ms: e.to_ms }] });
      case "error":
        return this.set({ error: e.message });
    }
  };

  setTitle(title: string) {
    this.edit({ title });
  }
  setNotes(notes_md: string) {
    this.edit({ notes_md });
  }
  private edit(p: Partial<PhoneMeeting>) {
    const m = this.snap.meeting;
    if (!m) return;
    this.set({ meeting: { ...m, ...p } });
    this.write();
  }

  /** Writes in order, so an older copy never lands over a newer one. */
  private write() {
    const m = this.snap.meeting;
    if (!m) return;
    const u: Unfinished = { device_id: this.deviceId, meeting: m, saved_at: Date.now() };
    this.writes = this.writes.then(() => saveUnfinished(u)).catch(() => undefined);
  }

  /** One push at a time, of the meeting as it is when its turn comes. A push of an unchanged bundle is skipped, so the history does not fill with copies. */
  private push(deviceId: string, m: () => PhoneMeeting): Promise<void> {
    const run = async () => {
      const body = bundle(m());
      const sig = JSON.stringify(body);
      if (sig === this.lastPushed) return;
      await this.api.pushMeeting(deviceId, body.meeting.id, body);
      this.lastPushed = sig;
    };
    const p = this.pushes.then(run);
    this.pushes = p.catch(() => undefined);
    return p;
  }

  private async pushQuietly() {
    const m = this.snap.meeting;
    if (!m) return;
    try {
      await this.push(this.deviceId, () => this.snap.meeting ?? m);
      this.set({ saveError: null });
    } catch (e) {
      this.set({ saveError: message(e) });
    }
  }

  /** Stops recording, then saves the ended meeting. Resolves to its library key, or null when the save failed and End can be tried again. */
  async end(): Promise<string | null> {
    const m = this.snap.meeting;
    if (!m || this.snap.ending) return null;
    this.set({ ending: true });
    window.clearInterval(this.timer);
    this.unsubscribeAuth();
    const rec = this.recorder;
    this.recorder = null;
    await rec?.stop().catch(() => undefined);
    const ended = { ...this.snap.meeting!, ended_at: m.ended_at ?? Date.now() };
    this.set({ meeting: ended, interim: "", state: "stopped" });
    this.write();
    try {
      await this.push(this.deviceId, () => ended);
      await (this.writes = this.writes.then(clearUnfinished));
    } catch (e) {
      this.set({ ending: false, saveError: message(e) });
      return null;
    }
    this.set({ meeting: null, ending: false, saveError: null, error: null, gaps: [] });
    return `${this.deviceId}/${ended.id}`;
  }

  /** Saves a meeting a closed page left, as ended when the page last wrote it. */
  async endUnfinished(): Promise<boolean> {
    const u = this.snap.unfinished;
    if (!u) return false;
    const ended = { ...u.meeting, ended_at: u.meeting.ended_at ?? u.saved_at };
    this.lastPushed = "";
    this.set({ ending: true, error: null });
    try {
      await this.push(u.device_id, () => ended);
      await (this.writes = this.writes.then(clearUnfinished));
    } catch (e) {
      this.set({ ending: false, error: `Could not save it. ${message(e)}` });
      return false;
    }
    this.set({ ending: false, unfinished: null });
    return true;
  }
}

export const useSession = (s: RecordingSession) => useSyncExternalStore(s.subscribe, s.get);
