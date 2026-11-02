import type { Line } from "./phone";
import { Emitter, type Recorder, type RecorderEvent, type StartOptions } from "./recorder";

/** The Android app's `@JavascriptInterface`. Every value crossing it is a JSON string. */
export interface Bridge {
  start(json: string): string;
  setAuth(json: string): void;
  stop(): void;
  status(): string;
  lines(from: number): string;
  appVersion(): string;
}

export interface Auth {
  jwt: string;
  browserToken: string;
}

declare global {
  interface Window {
    YogurtAndroid?: Bridge;
  }
}

/**
 * The page side of the bridge: the app owns the mic and the socket and keeps every final line, numbered from 0, until its next start.
 * Lines arrive as events while the page is in front. A page that comes back asks for the ones it missed, so it never misses or repeats one.
 */
// The app's stop waits up to 5 seconds for Deepgram's last lines. This bounds an app that never reports stopped.
const STOP_WAIT_MS = 10_000;

export class NativeRecorder extends Emitter implements Recorder {
  private seen = 0;
  private listening = false;
  private onStopped: (() => void) | null = null;

  constructor(
    private readonly bridge: Bridge,
    private readonly auth: () => Promise<Auth>,
    private readonly tokenUrl: string,
    private readonly win: Window = window,
  ) {
    super();
  }

  async start({ meetingId, startedAtMs, have }: StartOptions) {
    const status = JSON.parse(this.bridge.status());
    if (status.recording && status.meetingId === meetingId) {
      this.seen = have;
      // The state events went out before this page listened.
      this.emit({ type: "state", state: status.state });
    } else {
      const r = JSON.parse(this.bridge.start(JSON.stringify({ meetingId, startedAtMs, tokenUrl: this.tokenUrl, auth: await this.auth() })));
      if (!r.ok) throw new Error(r.error || "The recorder did not start.");
      this.seen = 0;
    }
    this.win.addEventListener("yogurt-native", this.onEvent);
    this.win.document.addEventListener("visibilitychange", this.onVisible);
    this.listening = true;
    this.catchUp();
  }

  /** The app's stop returns at once and reports `stopped` after the last lines, which this waits for before taking any it missed. */
  async stop() {
    if (!this.listening) return this.bridge.stop();
    let timer: ReturnType<typeof setTimeout> | undefined;
    await new Promise<void>((resolve) => {
      this.onStopped = resolve;
      timer = setTimeout(resolve, STOP_WAIT_MS);
      this.bridge.stop();
    });
    clearTimeout(timer);
    this.onStopped = null;
    this.listening = false;
    this.catchUp();
    this.win.removeEventListener("yogurt-native", this.onEvent);
    this.win.document.removeEventListener("visibilitychange", this.onVisible);
    this.emit({ type: "state", state: "stopped" });
  }

  setAuth(a: Auth) {
    this.bridge.setAuth(JSON.stringify(a));
  }

  private onEvent = (e: Event) => {
    const raw = (e as CustomEvent).detail;
    const d: RecorderEvent = typeof raw === "string" ? JSON.parse(raw) : raw;
    if (d.type === "state" && d.state === "stopped") this.onStopped?.();
    if (d.type !== "line") return this.emit(d);
    if (d.index === this.seen) this.line(d.line);
    else if (d.index > this.seen) this.catchUp();
  };

  private onVisible = () => {
    if (this.win.document.visibilityState === "visible") this.catchUp();
  };

  private catchUp() {
    const lines: Line[] = JSON.parse(this.bridge.lines(this.seen));
    for (const l of lines) this.line(l);
  }

  private line(line: Line) {
    this.emit({ type: "line", index: this.seen++, line });
  }
}
