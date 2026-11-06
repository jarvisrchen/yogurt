import type { Gap } from "./audioQueue";
import type { Line } from "./phone";

export type RecorderState = "connecting" | "live" | "reconnecting" | "stopped";

/** What a recorder tells the record screen. The same shapes as the Android bridge's `yogurt-native` events. */
export type RecorderEvent =
  | { type: "line"; index: number; line: Line }
  | { type: "interim"; text: string }
  | { type: "state"; state: RecorderState }
  | ({ type: "gap" } & Gap)
  | { type: "error"; message: string }
  // A model on the phone has no words until a pause, so it says when it hears speech, and how much it has yet to transcribe.
  | { type: "speaking"; speaking: boolean }
  | { type: "backlog"; ms: number };

export interface StartOptions {
  meetingId: string;
  startedAtMs: number;
  /** Final lines the page already holds, which a recorder already running for this meeting skips. */
  have: number;
  /** The model that transcribes on the phone, or null for Deepgram. */
  model?: string | null;
}

/** The mic, the Deepgram socket, and the audio waiting for it. `BrowserRecorder` runs them in the page, `NativeRecorder` in the Android app. */
export interface Recorder {
  start(o: StartOptions): Promise<void>;
  stop(): Promise<void>;
  on(fn: (e: RecorderEvent) => void): () => void;
}

export class Emitter {
  private fns = new Set<(e: RecorderEvent) => void>();
  on(fn: (e: RecorderEvent) => void) {
    this.fns.add(fn);
    return () => void this.fns.delete(fn);
  }
  protected emit(e: RecorderEvent) {
    for (const fn of this.fns) fn(e);
  }
}
