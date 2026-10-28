import { useCallback, useEffect, useRef, useState } from "react";
import type { ChatMessage, ChatSource } from "@web/lib/api";
import { OFFLINE, TokenRejected, type Api } from "./api";

type Turn = Pick<ChatMessage, "role" | "content">;

/** What a chat asks about: one meeting, or every meeting, with dates in the asker's time zone. */
export type ChatScope = { device_id: string; meeting_id: string } | { tz: string };

/** Under the function's 64 KB body cap, with room for the ids and token. */
const MAX_BODY = 60 * 1024;

/**
 * One turn of `POST /api/chat` (`cloud/api/_chat.ts`), calling `onSources` with the meetings a cross-meeting answer may cite, then `onDelta` per streamed chunk.
 * A source's id is `<device_id>/<meeting_id>`, the key the library opens a meeting by.
 */
export async function streamChat(api: Api, token: string, scope: ChatScope, turns: Turn[], onSources: (sources: ChatSource[]) => void, onDelta: (delta: string) => void, signal: AbortSignal) {
  // The oldest exchanges go first once the conversation outgrows the cap, so a long chat keeps working.
  let body = "";
  for (let from = 0; from < turns.length; from += 2) {
    body = JSON.stringify({ token, ...scope, messages: turns.slice(from) });
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
      if (!line) continue;
      const frame = JSON.parse(line) as { sources?: { device_id: string; meeting_id: string; title: string; started_at: number }[]; delta?: string };
      if (frame.sources) onSources(frame.sources.map(({ device_id, meeting_id, title, started_at }) => ({ id: `${device_id}/${meeting_id}`, title, started_at })));
      if (frame.delta) onDelta(frame.delta);
    }
  }
}

export interface CloudChat {
  messages: ChatMessage[];
  streamingId: string | null;
  send: (content: string) => void;
  open: boolean;
  setOpen: (open: boolean) => void;
}

/**
 * Chat about `scope`, or null when there is none, as for a meeting from an earlier push.
 * The conversation lives only here, like the desktop's cross-meeting chat, and starts over when the scope changes.
 * It sits above the phone and desktop layouts, so crossing the breakpoint keeps it.
 */
export function useChat(api: Api, token: string, scope: ChatScope | null, onRejected: () => void): CloudChat | null {
  const key = scope ? JSON.stringify(scope) : "";
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
  // Leaving the scope stops its answer, which also stops the model on the owner's key.
  useEffect(() => () => inFlight.current?.abort(), [key]);

  const send = useCallback(
    async (content: string) => {
      const trimmed = content.trim();
      if (!trimmed || !scope || inFlight.current) return;
      const meetingId = "meeting_id" in scope ? scope.meeting_id : "";
      const now = Date.now();
      const question: ChatMessage = { id: `u-${now}`, meeting_id: meetingId, role: "user", content: trimmed, created_at: now };
      const answerId = `a-${now}`;
      const history = [...messagesRef.current, question];
      setMessages([...history, { id: answerId, meeting_id: meetingId, role: "assistant", content: "", created_at: now }]);
      setStreamingId(answerId);
      const patch = (f: (m: ChatMessage) => ChatMessage) => setMessages((prev) => prev.map((m) => (m.id === answerId ? f(m) : m)));
      const ctl = (inFlight.current = new AbortController());
      try {
        await streamChat(
          api,
          token,
          scope,
          history.map(({ role, content }) => ({ role, content })),
          (sources) => patch((m) => ({ ...m, sources })),
          (delta) => patch((m) => ({ ...m, content: m.content + delta })),
          ctl.signal,
        );
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
    // `scope` is a fresh object each render, so depend on its key.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [api, token, key, onRejected],
  );

  return scope ? { messages, streamingId, send: (q) => void send(q), open, setOpen } : null;
}
