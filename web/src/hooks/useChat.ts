import { useCallback, useEffect, useRef, useState } from "react";
import {
  type ChatMessage,
  fetchChatHistory,
  postChatMessage,
} from "../lib/api";

export interface UseChatResult {
  messages: ChatMessage[];
  send: (content: string) => Promise<void>;
  streamingId: string | null;
  error: string | null;
}

interface ChatChunkFrame {
  type: "chat_chunk";
  message_id: string;
  delta: string;
  done: boolean;
}

const MAX_RECONNECT_ATTEMPTS = 3;

export function useChat(
  meetingId: string | null,
  token: string | null,
): UseChatResult {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streamingId, setStreamingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Ref because the WS onmessage closure is captured once and would see a stale streamingId.
  const streamingIdRef = useRef<string | null>(null);

  useEffect(() => {
    streamingIdRef.current = streamingId;
  }, [streamingId]);

  useEffect(() => {
    if (!meetingId || !token) {
      setMessages([]);
      return;
    }
    let cancelled = false;
    fetchChatHistory(meetingId, token).then(
      (rows) => {
        if (!cancelled) setMessages(rows);
      },
      (e: unknown) => {
        if (!cancelled) {
          setError(
            e instanceof Error ? e.message : "Failed to load chat history",
          );
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [meetingId, token]);

  useEffect(() => {
    if (!meetingId || !token) return;
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/ws/meetings/${meetingId}?token=${encodeURIComponent(token)}`;

    let attempt = 0;
    let cancelled = false;
    let currentWs: WebSocket | null = null;
    let backoffTimer: ReturnType<typeof setTimeout> | null = null;

    const cleanupCurrentWs = () => {
      if (currentWs) {
        currentWs.onopen = null;
        currentWs.onclose = null;
        currentWs.onerror = null;
        currentWs.onmessage = null;
        if (
          currentWs.readyState === WebSocket.CONNECTING ||
          currentWs.readyState === WebSocket.OPEN
        ) {
          currentWs.close();
        }
        currentWs = null;
      }
    };

    const connect = () => {
      if (cancelled) return;
      const ws = new WebSocket(url);
      currentWs = ws;

      ws.onopen = () => {
        if (cancelled) return;
        attempt = 0;
      };

      ws.onmessage = (event) => {
        if (cancelled) return;
        let frame: unknown;
        try {
          frame = JSON.parse(event.data as string);
        } catch {
          return;
        }
        const f = frame as { type?: string };
        if (f.type !== "chat_chunk") return;
        const ev = frame as ChatChunkFrame;
        setMessages((prev) => {
          const idx = prev.findIndex((m) => m.id === ev.message_id);
          if (idx >= 0) {
            const copy = prev.slice();
            copy[idx] = {
              ...copy[idx],
              content: copy[idx].content + ev.delta,
            };
            return copy;
          }
          // Defensive: `send` normally pre-creates the placeholder.
          return [
            ...prev,
            {
              id: ev.message_id,
              meeting_id: meetingId,
              role: "assistant",
              content: ev.delta,
              created_at: Date.now(),
            },
          ];
        });
        if (ev.done && streamingIdRef.current === ev.message_id) {
          setStreamingId(null);
        }
      };

      const scheduleReconnect = () => {
        if (cancelled) return;
        cleanupCurrentWs();
        if (attempt >= MAX_RECONNECT_ATTEMPTS) return;
        const delay = 500 * (1 << attempt);
        attempt += 1;
        backoffTimer = setTimeout(() => {
          backoffTimer = null;
          connect();
        }, delay);
      };

      ws.onclose = scheduleReconnect;
      ws.onerror = scheduleReconnect;
    };

    connect();

    return () => {
      cancelled = true;
      if (backoffTimer) {
        clearTimeout(backoffTimer);
        backoffTimer = null;
      }
      cleanupCurrentWs();
    };
  }, [meetingId, token]);

  const send = useCallback(
    async (content: string) => {
      if (!meetingId || !token) return;
      const trimmed = content.trim();
      if (!trimmed) return;

      const optimisticId = `tmp-${Date.now()}`;
      const userBubble: ChatMessage = {
        id: optimisticId,
        meeting_id: meetingId,
        role: "user",
        content: trimmed,
        created_at: Date.now(),
      };
      setMessages((prev) => [...prev, userBubble]);
      setError(null);

      try {
        const { message_id } = await postChatMessage(meetingId, trimmed, token);
        // Pre-create the assistant bubble so streaming chunks have a target.
        setMessages((prev) => [
          ...prev,
          {
            id: message_id,
            meeting_id: meetingId,
            role: "assistant",
            content: "",
            created_at: Date.now(),
          },
        ]);
        setStreamingId(message_id);
      } catch (e) {
        setMessages((prev) => prev.filter((m) => m.id !== optimisticId));
        setError(e instanceof Error ? e.message : "Failed to send message");
      }
    },
    [meetingId, token],
  );

  return { messages, send, streamingId, error };
}
