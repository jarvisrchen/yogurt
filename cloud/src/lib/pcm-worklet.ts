// Runs in the AudioWorkletGlobalScope, where these are globals the DOM types do not declare.
import { Downsampler, RATE } from "./pcm";

declare const sampleRate: number;
declare function registerProcessor(name: string, ctor: unknown): void;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}

// 100 ms per message, so the main thread wakes ten times a second.
const CHUNK = RATE / 10;

class PcmProcessor extends AudioWorkletProcessor {
  private ds = new Downsampler(sampleRate);
  private buf = new Int16Array(CHUNK);
  private n = 0;

  process(inputs: Float32Array[][]): boolean {
    const channel = inputs[0]?.[0];
    if (!channel) return true;
    for (const s of this.ds.push(channel)) {
      this.buf[this.n++] = s;
      if (this.n === CHUNK) {
        this.port.postMessage(this.buf.buffer, [this.buf.buffer]);
        this.buf = new Int16Array(CHUNK);
        this.n = 0;
      }
    }
    return true;
  }
}

registerProcessor("yogurt-pcm", PcmProcessor);
