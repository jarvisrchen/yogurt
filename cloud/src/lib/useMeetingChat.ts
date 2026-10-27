import { useCallback, useRef, useState } from "react";
import type { ChatMessage } from "@web/lib/api";
import { OFFLINE, TokenRejected, type Api } from "./api";

type Turn = Pick<ChatMessage, "role" | "content">;

/** One turn of `POST /api/chat` (`cloud/api/_chat.ts`), calling `onDelta` per streamed chunk. */
export async function streamMeetingChat(api: Api, token: string, device: string, meeting: string, messages: Turn[], onDelta: (delta: string) => void) {
  // getSession refreshes an expired session, as every other read does through supabase-js.
  const { data } = await api.sb.auth.getSession();
  let res: Response;
  try {
    res = await fetch("/api/chat", {
      method: "POST",
      headers: { authorization: `Bearer ${data.session?.access_token ?? ""}`, "content-type": "application/json" },
      body: JSON.stringify({ token, device_id: device, meeting_id: meeting, messages }),
    });
  } catch {
    throw new Error(OFFLINE);
  }
  if (!res.ok || !res.body) {
    const e = await res.json().catch(() => ({}));
    if (e.code === "28000" || res.status === 403) throw new TokenRejected(e.message);
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

/** Chat about one meeting. The conversation lives only in this hook, like the desktop's cross-meeting chat. */
export function useMeetingChat(api: Api, token: string, device: string, meeting: string, onRejected: () => void) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streamingId, setStreamingId] = useState<string | null>(null);
  // `send` reads the latest conversation without re-creating itself on every chunk.
  const messagesRef = useRef(messages);
  messagesRef.current = messages;

  const send = useCallback(
    async (content: string) => {
      const trimmed = content.trim();
      if (!trimmed) return;
      const now = Date.now();
      const question: ChatMessage = { id: `u-${now}`, meeting_id: meeting, role: "user", content: trimmed, created_at: now };
      const answerId = `a-${now}`;
      const history = [...messagesRef.current, question];
      setMessages([...history, { id: answerId, meeting_id: meeting, role: "assistant", content: "", created_at: now }]);
      setStreamingId(answerId);
      const patch = (f: (m: ChatMessage) => ChatMessage) => setMessages((prev) => prev.map((m) => (m.id === answerId ? f(m) : m)));
      try {
        await streamMeetingChat(api, token, device, meeting, history.map(({ role, content }) => ({ role, content })), (delta) => patch((m) => ({ ...m, content: m.content + delta })));
      } catch (e) {
        if (e instanceof TokenRejected) return onRejected();
        const msg = e instanceof Error ? e.message : String(e);
        patch((m) => ({ ...m, content: `${m.content}${m.content ? "\n\n" : ""}[${msg}]` }));
      } finally {
        setStreamingId(null);
      }
    },
    [api, token, device, meeting, onRejected],
  );

  return { messages, send, streamingId };
}
