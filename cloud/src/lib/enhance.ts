import { type Api, type Bundle, OFFLINE, TokenRejected } from "./api";
import { isPhoneMeeting, type Enhanced } from "./phone";

type Meeting = Bundle["meeting"] & { template?: string | null };

// About 12 updates a second, as the Mac's preview, so a long reply does not re-render on every chunk.
const PREVIEW_MS = 80;

// The 1.3 MB module loads on the first enhance, not with the page.
let wasm: Promise<typeof import("../wasm/yogurt_wasm.js")> | null = null;
const loadWasm = () =>
  (wasm ??= import("../wasm/yogurt_wasm.js").then(async (w) => {
    await w.default();
    return w;
  }));

/**
 * Enhances a phone meeting exactly as the Mac's `/enhance` does: the prompt and the merge run in yogurt-wasm, and `/api/enhance` streams the model's reply.
 * `onText` gets the document so far as it streams. `template` is a format id, and null lets the model pick, as the Mac's Auto does.
 * It defaults to the format the meeting's last enhance used, as the Mac's picker does for a Re-enhance.
 * Resolves null for an accidental start and stop, which the Mac does not enhance.
 */
export async function enhanceMeeting(
  api: Api,
  token: string,
  meeting: Meeting,
  { template = meeting.template ?? null, onText, signal }: { template?: string | null; onText?: (text: string) => void; signal?: AbortSignal } = {},
): Promise<Enhanced | null> {
  if (!isPhoneMeeting(meeting)) throw new Error("Only a meeting recorded on a phone can be enhanced here.");
  const w = await loadWasm();
  if (w.too_short(meeting.notes_md, meeting.transcript_json)) return null;
  const messages = JSON.parse(w.render_enhance(meeting.notes_md, meeting.transcript_json, template));
  // getSession refreshes an expired session, as every other read does through supabase-js.
  const { data } = await api.sb.auth.getSession();
  let res: Response;
  try {
    res = await fetch("/api/enhance", {
      method: "POST",
      headers: { authorization: `Bearer ${data.session?.access_token ?? ""}`, "content-type": "application/json" },
      body: JSON.stringify({ token, messages }),
      signal,
    });
  } catch (e) {
    if (signal?.aborted) throw e;
    throw new Error(OFFLINE);
  }
  if (!res.ok || !res.body) {
    const e = await res.json().catch(() => ({}));
    // Only the database's own rejection. A 403 from anything in front of the function must not forget a valid browser.
    if (e.code === "28000") throw new TokenRejected(e.message);
    throw new Error(e.message ?? `Enhance answered ${res.status}.`);
  }
  let reply = "";
  let model: string | null = null;
  // At most every PREVIEW_MS, and always the latest text, so a pause in the stream never leaves the preview behind.
  let shown = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const show = () => {
    timer = undefined;
    shown = Date.now();
    onText?.(w.preview(reply));
  };
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = "";
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buf += value;
      const lines = buf.split("\n");
      buf = lines.pop()!;
      for (const line of lines) {
        if (!line) continue;
        const frame = JSON.parse(line) as { delta?: string; done?: boolean; model?: string; error?: string };
        if (frame.error) throw new Error(frame.error);
        if (frame.delta) {
          reply += frame.delta;
          timer ??= setTimeout(show, Math.max(0, shown + PREVIEW_MS - Date.now()));
        }
        if (frame.done) model = frame.model ?? "";
      }
    }
  } finally {
    clearTimeout(timer);
  }
  // No done frame means the function was cut off, and a partial reply must never be stored as the notes.
  if (model === null) throw new Error("Enhance stopped before the model finished. Try again.");
  const out = w.finish_enhance(meeting.notes_md, meeting.transcript_json, reply, template);
  try {
    return { enriched_md: out.enriched_md, llm_model: model, template: out.template };
  } finally {
    out.free();
  }
}
