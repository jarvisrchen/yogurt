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
 * Audio Deepgram has not finalized yet: everything waiting for a socket, plus what was sent but not yet covered by a final result.
 * Holding the sent part means a dropped socket loses nothing, since the next one replays from the last finalized window.
 * Chunks are self-contained PCM, so a new socket takes them as they are, in order.
 * Past the cap the oldest chunks drop, and `takeGap` reports the span they covered as one gap.
 * A port of `AudioQueue` in the Android app.
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

  /** Drops the audio before `untilMs`, cutting a chunk that straddles it. */
  confirm(untilMs: number) {
    while (this.chunks.length) {
      const head = this.chunks[0];
      if (head.ts_ms + ms(head) <= untilMs) {
        this.chunks.shift();
        this.held -= ms(head);
        continue;
      }
      if (head.ts_ms < untilMs) {
        // Whole samples only, so the cut chunk is still linear16.
        const cut = Math.floor((untilMs - head.ts_ms) * BYTES_PER_MS / 2) * 2;
        this.chunks[0] = { ts_ms: head.ts_ms + cut / BYTES_PER_MS, data: head.data.slice(cut) };
        this.held -= cut / BYTES_PER_MS;
      }
      return;
    }
  }

  /** Every held chunk, oldest first, for a new socket to replay. */
  pending(): Chunk[] {
    return [...this.chunks];
  }

  takeGap(): Gap | null {
    const g = this.gap;
    this.gap = null;
    return g;
  }
}
