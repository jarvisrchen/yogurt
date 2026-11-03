import workletUrl from "./pcm-worklet.ts?worker&url";
import { AudioQueue, type Chunk } from "./audioQueue";
import { finalEndMs, flush, fold, utterance, type Heard } from "./deepgram";
import { BYTES_PER_MS } from "./pcm";
import { Emitter, type Recorder, type RecorderState, type StartOptions } from "./recorder";

export interface SttGrant {
  token: string;
  url: string;
}

const MAX_BACKOFF_MS = 10_000;
// Deepgram sends the last results after CloseStream, then closes. This bounds a socket that never does.
const CLOSE_WAIT_MS = 5_000;

/** Records in the page: the mic through an AudioWorklet to a Deepgram socket, with a fresh token and a replay of the audio Deepgram had not finalized after every drop. */
export class BrowserRecorder extends Emitter implements Recorder {
  private stream: MediaStream | null = null;
  private ctx: AudioContext | null = null;
  private ws: WebSocket | null = null;
  private open = false;
  private queue = new AudioQueue();
  private st = utterance();
  /** Where this socket's audio starts, in ms since the recording started. Deepgram's times count from it. */
  private socketStart: number | null = null;
  private offset = 0;
  private sent = 0;
  private lines = 0;
  private attempt = 0;
  private timer = 0;
  private stopped = false;
  private closed: (() => void) | null = null;
  private wake: WakeLockSentinel | null = null;

  constructor(private readonly grant: () => Promise<SttGrant>) {
    super();
  }

  async start({ startedAtMs }: StartOptions) {
    this.stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    this.ctx = new AudioContext();
    // Autoplay rules can start a context made after an await suspended, even inside the tap that asked for it.
    await this.ctx.resume().catch(() => undefined);
    await this.ctx.audioWorklet.addModule(workletUrl);
    // With no outputs the node is a sink, which the graph always runs.
    const node = new AudioWorkletNode(this.ctx, "yogurt-pcm", { numberOfInputs: 1, numberOfOutputs: 0 });
    node.port.onmessage = (e: MessageEvent<ArrayBuffer>) => this.audio(e.data);
    this.ctx.createMediaStreamSource(this.stream).connect(node);
    this.offset = Date.now() - startedAtMs;
    document.addEventListener("visibilitychange", this.keepAwake);
    void this.keepAwake();
    void this.connect();
  }

  async stop() {
    this.stopped = true;
    window.clearTimeout(this.timer);
    document.removeEventListener("visibilitychange", this.keepAwake);
    void this.wake?.release().catch(() => undefined);
    this.stream?.getTracks().forEach((t) => t.stop());
    await this.ctx?.close().catch(() => undefined);
    const ws = this.ws;
    const clean =
      !!ws &&
      this.open &&
      (await new Promise<boolean>((done) => {
        this.closed = () => done(true);
        ws.send(JSON.stringify({ type: "CloseStream" }));
        window.setTimeout(() => done(false), CLOSE_WAIT_MS);
      }));
    if (!clean) {
      this.heard(flush(this.st));
      const held = this.queue.pending();
      const dropped = this.queue.takeGap();
      const last = held.at(-1);
      if (last || dropped) {
        this.emit({ type: "gap", from_ms: dropped?.from_ms ?? held[0].ts_ms, to_ms: last ? last.ts_ms + last.data.byteLength / BYTES_PER_MS : dropped!.to_ms });
      }
    }
    ws?.close();
    this.ws = null;
    this.state("stopped");
  }

  // The lock lapses whenever the page is hidden, so it is asked for again on every return.
  private keepAwake = async () => {
    if (this.stopped || document.visibilityState !== "visible") return;
    try {
      this.wake = await navigator.wakeLock?.request("screen");
    } catch {
      // Refused, as on low battery. The page still records while it stays in front.
    }
  };

  private audio(data: ArrayBuffer) {
    const chunk: Chunk = { ts_ms: this.offset + this.sent / BYTES_PER_MS, data };
    this.sent += data.byteLength;
    this.queue.push(chunk);
    if (this.open && this.ws) this.send(this.ws, chunk);
  }

  private send(ws: WebSocket, c: Chunk) {
    this.socketStart ??= c.ts_ms;
    ws.send(c.data);
  }

  private state(s: RecorderState) {
    this.emit({ type: "state", state: s });
  }

  private heard(h: Heard | null) {
    if (!h) return;
    const ts_ms = Math.round((this.socketStart ?? 0) + h.ts_ms);
    if (h.final) {
      this.emit({ type: "line", index: this.lines++, line: { channel: "them", text: h.text, ts_ms } });
      this.emit({ type: "interim", text: "" });
    } else this.emit({ type: "interim", text: h.text });
  }

  private async connect() {
    if (this.stopped) return;
    this.state(this.attempt ? "reconnecting" : "connecting");
    let grant: SttGrant;
    try {
      grant = await this.grant();
    } catch (e) {
      this.emit({ type: "error", message: e instanceof Error ? e.message : String(e) });
      return this.retry();
    }
    if (this.stopped) return;
    // Deepgram reads a granted token from the bearer subprotocol, as its own browser SDK sends it.
    const ws = new WebSocket(grant.url, ["bearer", grant.token]);
    ws.binaryType = "arraybuffer";
    this.ws = ws;
    ws.onopen = () => {
      if (this.ws !== ws) return;
      this.attempt = 0;
      this.st = utterance();
      this.socketStart = null;
      const gap = this.queue.takeGap();
      if (gap) this.emit({ type: "gap", ...gap });
      for (const c of this.queue.pending()) this.send(ws, c);
      this.open = true;
      this.state("live");
    };
    ws.onmessage = (m) => {
      if (this.ws !== ws || typeof m.data !== "string") return;
      const end = finalEndMs(m.data);
      if (end !== null && this.socketStart !== null) this.queue.confirm(this.socketStart + end);
      this.heard(fold(this.st, m.data));
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.heard(flush(this.st));
      this.emit({ type: "interim", text: "" });
      this.open = false;
      if (this.closed) return this.closed();
      this.ws = null;
      this.retry();
    };
  }

  private retry() {
    if (this.stopped) return;
    this.state("reconnecting");
    const wait = Math.min(1000 * 2 ** this.attempt++, MAX_BACKOFF_MS);
    this.timer = window.setTimeout(() => void this.connect(), wait);
  }
}
