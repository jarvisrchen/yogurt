import { useCallback, useEffect, useRef, useState } from "react";
import type { ChatMessage } from "@web/lib/api";
import { OFFLINE, TokenRejected, type Api } from "./api";

type Turn = Pick<ChatMessage, "role" | "content">;

/** Under the function's 64 KB body cap, with room for the ids and token. */
const MAX_BODY = 60 * 1024;

/** One turn of `POST /api/chat` (`cloud/api/_chat.ts`), calling `onDelta` per streamed chunk. */
export async function streamMeetingChat(api: Api, token: string, device: string, meeting: string, turns: Turn[], onDelta: (delta: string) => void, signal: AbortSignal) {
  // The oldest exchanges go first once the conversation outgrows the cap, so a long chat keeps working.
  let body = "";
  for (let from = 0; from < turns.length; from += 2) {
    body = JSON.stringify({ token, device_id: device, meeting_id: meeting, messages: turns.slice(from) });
    if (new TextEncoder().encode(body).length <= MAX_BODY) break;
  }
  // getSession refreshes an expired session, as every other read does through supabase-js.
  const { data } = await api.sb.auth.getSession();
  let res: Response;
  try {
    res = await fetch("/api/chat", {
      method: "POST",
      headers: { authorization: `Bearer ${data.session?.access_token ?? ""}`, "content-type": "application/json" },
      body,
      signal,
    });
  } catch (e) {
    if (signal.aborted) throw e;
    throw new Error(OFFLINE);
  }
  if (!res.ok || !res.body) {
    const e = await res.json().catch(() => ({}));
    // Only the database's own rejection. A 403 from anything in front of the function must not forget a valid browser.
    if (e.code === "28000") throw new TokenRejected(e.message);
    throw new Error(e.message ?? `Chat answered ${res.status}.`);
  }
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += value;
    const lines = buf.split("\n");
    buf = lines.pop()!;
    for (const line of lines) {
      const delta = line && (JSON.parse(line) as { delta?: string }).delta;
      if (delta) onDelta(delta);
    }
  }
}

export interface MeetingChat {
  messages: ChatMessage[];
  streamingId: string | null;
  send: (content: string) => void;
  open: boolean;
  setOpen: (open: boolean) => void;
}

/**
 * Chat about the open meeting, or null when there is none or it comes from an earlier push.
 * The conversation lives only here, like the desktop's cross-meeting chat, and starts over for each meeting.
 * It sits above the phone and desktop layouts, so crossing the breakpoint keeps it.
 */
export function useMeetingChat(api: Api, token: string, target: { device: string; meeting: string } | null, onRejected: () => void): MeetingChat | null {
  const key = target ? `${target.device}/${target.meeting}` : "";
  const [owner, setOwner] = useState(key);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streamingId, setStreamingId] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  if (owner !== key) {
    setOwner(key);
    setMessages([]);
    setStreamingId(null);
    setOpen(false);
  }
  // `send` reads the latest conversation without re-creating itself on every chunk.
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const inFlight = useRef<AbortController | null>(null);
  // Leaving the meeting stops its answer, which also stops the model on the owner's key.
  useEffect(() => () => inFlight.current?.abort(), [key]);

  const send = useCallback(
    async (content: string) => {
      const trimmed = content.trim();
      if (!trimmed || !target || inFlight.current) return;
      const now = Date.now();
      const question: ChatMessage = { id: `u-${now}`, meeting_id: target.meeting, role: "user", content: trimmed, created_at: now };
      const answerId = `a-${now}`;
      const history = [...messagesRef.current, question];
      setMessages([...history, { id: answerId, meeting_id: target.meeting, role: "assistant", content: "", created_at: now }]);
      setStreamingId(answerId);
      const patch = (f: (m: ChatMessage) => ChatMessage) => setMessages((prev) => prev.map((m) => (m.id === answerId ? f(m) : m)));
      const ctl = (inFlight.current = new AbortController());
      try {
        await streamMeetingChat(api, token, target.device, target.meeting, history.map(({ role, content }) => ({ role, content })), (delta) => patch((m) => ({ ...m, content: m.content + delta })), ctl.signal);
      } catch (e) {
        if (ctl.signal.aborted) return;
        if (e instanceof TokenRejected) return onRejected();
        const msg = e instanceof Error ? e.message : String(e);
        patch((m) => ({ ...m, content: `${m.content}${m.content ? "\n\n" : ""}[${msg}]` }));
      } finally {
        if (inFlight.current === ctl) {
          inFlight.current = null;
          setStreamingId(null);
        }
      }
    },
    // `target` is a fresh object each render, so depend on its key.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [api, token, key, onRejected],
  );

  return target ? { messages, streamingId, send: (q) => void send(q), open, setOpen } : null;
}
