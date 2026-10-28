import { useState } from "react";
import { Link } from "react-router";
import { AskPill } from "./AskPill";
import { ChatSources } from "./ChatSources";
import { type ChatCopy, ChatWindow, LIBRARY_CHAT_COPY } from "./ChatWindow";
import { type UseChatResult, useChat } from "../hooks/useChat";
import { useLibraryChat } from "../hooks/useLibraryChat";

interface Props {
  meetingId: string | null;
  token: string | null;
}

/** `meetingId`/`token` are null during bootstrap. The pill still renders so it never waits on data, and `send()` is a no-op. */
export function AskExperience({ meetingId, token }: Props) {
  return <AskSurface chat={useChat(meetingId, token)} />;
}

export function LibraryAskExperience() {
  return <AskSurface chat={useLibraryChat()} copy={LIBRARY_CHAT_COPY} sources />;
}

function AskSurface({ chat, copy, sources = false }: { chat: UseChatResult; copy?: ChatCopy; sources?: boolean }) {
  const [open, setOpen] = useState(false);
  if (open) {
    return (
      <ChatWindow
        messages={chat.messages}
        streamingId={chat.streamingId}
        onSend={(content) => {
          void chat.send(content);
        }}
        onCollapse={() => setOpen(false)}
        copy={copy}
        renderExtra={
          sources
            ? (m) => (
                <ChatSources
                  message={m}
                  link={(id, className, children) => (
                    <Link to={`/meeting/${id}/post`} className={className}>
                      {children}
                    </Link>
                  )}
                />
              )
            : undefined
        }
      />
    );
  }
  return <AskPill onExpand={() => setOpen(true)} label={copy?.placeholder} />;
}
