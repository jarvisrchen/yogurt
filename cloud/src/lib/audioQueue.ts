import { BYTES_PER_MS } from "./pcm";

/** 100 ms of 16 kHz linear16, stamped with its start in ms since the recording started. */
export interface Chunk {
  ts_ms: number;
  data: ArrayBuffer;
}

export interface Gap {
  from_ms: number;
  to_ms: number;
}

/** About 9.6 MB of audio. */
export const QUEUE_MS = 5 * 60_000;

const ms = (c: Chunk) => c.data.byteLength / BYTES_PER_MS;

/**
 * Audio waiting for a socket. Chunks are self-contained PCM, so a new socket takes them as they are, in order.
 * Past the cap the oldest chunks drop, and `drain` reports the span they covered as one gap.
 */
export class AudioQueue {
  private chunks: Chunk[] = [];
  private held = 0;
  private gap: Gap | null = null;

  constructor(private readonly capMs = QUEUE_MS) {}

  get length() {
    return this.chunks.length;
  }

  push(c: Chunk) {
    this.chunks.push(c);
    this.held += ms(c);
    while (this.held > this.capMs) {
      const old = this.chunks.shift()!;
      this.held -= ms(old);
      this.gap = { from_ms: this.gap?.from_ms ?? old.ts_ms, to_ms: old.ts_ms + ms(old) };
    }
  }

  /** Every queued chunk, oldest first, and the gap dropped before them, if any. Leaves the queue empty. */
  drain(): { chunks: Chunk[]; gap: Gap | null } {
    const out = { chunks: this.chunks, gap: this.gap };
    this.chunks = [];
    this.held = 0;
    this.gap = null;
    return out;
  }
}
