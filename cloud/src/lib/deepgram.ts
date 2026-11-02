/** One Deepgram socket's utterance in progress. A port of `UtteranceState` in crates/yogurt-stt/src/deepgram.rs. */
export interface Utterance {
  /** Text of the window-finalized parts of the utterance. */
  buf: string;
  /** Where the utterance's first word starts, in ms of this socket's audio. */
  startMs: number | null;
  /** The last line emitted, since Deepgram can resend the closing frame. */
  lastFinal: string | null;
}

export const utterance = (): Utterance => ({ buf: "", startMs: null, lastFinal: null });

/** Text and start in ms of this socket's audio. `final` is a finished line; otherwise it is the words so far, greyed until they finish. */
export interface Heard {
  text: string;
  ts_ms: number;
  final: boolean;
}

const join = (a: string, b: string) => (a && b ? `${a} ${b}` : a || b);

/**
 * Folds one frame into `st`, as `fold_deepgram_event` does: an interim shows the buffered text plus the live window, a window-final joins the buffer,
 * and `speech_final` closes the utterance into one line. Null for frames that change nothing.
 */
export function fold(st: Utterance, frame: string): Heard | null {
  let v: { type?: string; channel?: { alternatives?: { transcript?: string }[] }; is_final?: boolean; speech_final?: boolean; start?: number };
  try {
    v = JSON.parse(frame);
  } catch {
    return null;
  }
  const said = v.channel?.alternatives?.[0]?.transcript;
  if (v.type !== "Results" || typeof said !== "string") return null;
  const text = said.trim();
  const windowFinal = v.is_final === true;
  const speechFinal = v.speech_final ?? windowFinal;
  const now = Math.round((v.start ?? 0) * 1000);
  if (!text && !speechFinal) return null;
  if (st.startMs === null && text) st.startMs = now;
  const ts_ms = st.startMs ?? now;
  if (speechFinal) {
    const full = join(st.buf, text);
    st.buf = "";
    st.startMs = null;
    if (!full || full === st.lastFinal) return null;
    st.lastFinal = full;
    return { text: full, ts_ms, final: true };
  }
  if (windowFinal) {
    st.buf = join(st.buf, text);
    return { text: st.buf, ts_ms, final: false };
  }
  return { text: join(st.buf, text), ts_ms, final: false };
}

/** The window-finalized words of an utterance its socket closed on, as a line, or null. Deepgram has finished those words, so they are kept. */
export function flush(st: Utterance): Heard | null {
  const text = st.buf;
  const ts_ms = st.startMs ?? 0;
  st.buf = "";
  st.startMs = null;
  if (!text || text === st.lastFinal) return null;
  st.lastFinal = text;
  return { text, ts_ms, final: true };
}
