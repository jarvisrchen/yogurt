import { type Api, type Bundle, OFFLINE, TokenRejected } from "./api";

type Meeting = Bundle["meeting"] & { stt_engine?: string | null; template?: string | null };

/** What an enhance changes in a meeting, under the Mac's field names. */
export interface Enhanced {
  enriched_md: string;
  llm_model: string;
  template: string;
}

// The 1.3 MB module loads on the first enhance, not with the page.
let wasm: Promise<typeof import("../wasm/yogurt_wasm.js")> | null = null;
const loadWasm = () =>
  (wasm ??= import("../wasm/yogurt_wasm.js").then(async (w) => {
    await w.default();
    return w;
  }));

/** Recorded on a phone. The webapp writes only these, never a Mac's meetings. */
export const isPhoneMeeting = (m: Meeting) => !!m.stt_engine?.endsWith("· phone");

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
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = "";
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
        onText?.(w.preview(reply));
      }
      if (frame.done) model = frame.model ?? "";
    }
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

/** Pushes `bundle` with `fields` as a new version under its own device, through the RPCs a Mac's push uses. */
export async function pushEnhanced(api: Api, deviceId: string, bundle: Bundle, fields: Enhanced): Promise<void> {
  if (!isPhoneMeeting(bundle.meeting)) throw new Error("Only a meeting recorded on a phone can be enhanced here.");
  // A push needs the owner's session but no browser token, as a laptop's does, so a 403 here is never a rejected token.
  const fail = (error: { message: string }, status: number) => new Error(status === 0 ? OFFLINE : error.message);
  const start = await api.sb.rpc("yogurt_start_push", { device_id: deviceId });
  if (start.error) throw fail(start.error, start.status);
  const body = { ...bundle, meeting: { ...bundle.meeting, ...fields } };
  const insert = await api.sb.from("yogurt_versions").insert({ meeting_id: bundle.meeting.id, push_id: start.data, device_id: deviceId, body });
  if (insert.error) throw fail(insert.error, insert.status);
  // Search catches up after a push, as it does after a laptop's. A failure here only delays it to the next push.
  await api.sb.rpc("yogurt_search_catch_up", { max_ms: 2000 });
}
