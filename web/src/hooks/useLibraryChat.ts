import { useCallback, useRef, useState } from "react";
import { type ChatMessage, streamLibraryChat } from "../lib/api";
import type { UseChatResult } from "./useChat";

/** Cross-meeting chat. The conversation lives only in this hook; the server persists nothing. */
export function useLibraryChat(): UseChatResult {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streamingId, setStreamingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // `send` reads the latest conversation without re-creating itself on every chunk.
  const messagesRef = useRef(messages);
  messagesRef.current = messages;

  const send = useCallback(async (content: string) => {
    const trimmed = content.trim();
    if (!trimmed) return;
    const now = Date.now();
    const question: ChatMessage = {
      id: `u-${now}`,
      meeting_id: "",
      role: "user",
      content: trimmed,
      created_at: now,
    };
    const answerId = `a-${now}`;
    const history = [...messagesRef.current, question];
    setMessages([
      ...history,
      { id: answerId, meeting_id: "", role: "assistant", content: "", created_at: now },
    ]);
    setStreamingId(answerId);
    setError(null);
    const patch = (f: (m: ChatMessage) => ChatMessage) =>
      setMessages((prev) => prev.map((m) => (m.id === answerId ? f(m) : m)));
    try {
      await streamLibraryChat(
        history.map(({ role, content }) => ({ role, content })),
        (sources) => patch((m) => ({ ...m, sources })),
        (delta) => patch((m) => ({ ...m, content: m.content + delta })),
      );
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to send message";
      setError(msg);
      patch((m) => ({ ...m, content: m.content + `\n\n[${msg}]` }));
    } finally {
      setStreamingId(null);
    }
  }, []);

  return { messages, send, streamingId, error };
}
