/** Deepgram's stream format and the Mac's frame contract: 16 kHz mono linear16. */
export const RATE = 16_000;
/** Bytes of linear16 per millisecond at 16 kHz. */
export const BYTES_PER_MS = (RATE * 2) / 1000;

/**
 * Turns mic blocks at the context's rate into 16 kHz linear16. Each output sample is the mean of the input samples in its span, which also low-passes the speech band before decimating.
 * The span boundaries carry across blocks, so a stream of 128-frame blocks gives the same samples as one long block.
 */
export class Downsampler {
  private readonly ratio: number;
  private seen = 0;
  private next: number;
  private sum = 0;
  private n = 0;

  constructor(inRate: number) {
    if (inRate < RATE) throw new Error(`The mic runs at ${inRate} Hz, below ${RATE} Hz.`);
    this.ratio = inRate / RATE;
    this.next = this.ratio;
  }

  push(block: Float32Array): Int16Array {
    const out = new Int16Array(Math.ceil((block.length + this.n) / this.ratio) + 1);
    let k = 0;
    for (const x of block) {
      this.sum += x;
      this.n++;
      if (++this.seen >= this.next) {
        const v = Math.max(-1, Math.min(1, this.sum / this.n));
        out[k++] = v < 0 ? v * 0x8000 : v * 0x7fff;
        this.sum = 0;
        this.n = 0;
        this.next += this.ratio;
      }
    }
    return out.subarray(0, k);
  }
}
