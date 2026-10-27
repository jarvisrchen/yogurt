import { useState } from "react";
import { AskPill } from "./AskPill";
import { ChatSources } from "./ChatSources";
import { type ChatCopy, ChatWindow } from "./ChatWindow";
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

const LIBRARY_COPY: ChatCopy = {
  title: "Ask across meetings",
  placeholder: "Ask across meetings…",
  hint: "Ask about anything from your past meetings.",
  suggestions: ["What did I decide last week?", "What are my open action items?"],
};

export function LibraryAskExperience() {
  return <AskSurface chat={useLibraryChat()} copy={LIBRARY_COPY} sources />;
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
        renderExtra={sources ? (m) => <ChatSources message={m} /> : undefined}
      />
    );
  }
  return <AskPill onExpand={() => setOpen(true)} label={copy?.placeholder} />;
}
