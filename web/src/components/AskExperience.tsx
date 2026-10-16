import { useState } from "react";
import { AskPill } from "./AskPill";
import { ChatWindow } from "./ChatWindow";
import { useChat } from "../hooks/useChat";

interface Props {
  meetingId: string | null;
  token: string | null;
}

/** `meetingId`/`token` are null during bootstrap. The pill still renders so it never waits on data, and `send()` is a no-op. */
export function AskExperience({ meetingId, token }: Props) {
  const [open, setOpen] = useState(false);
  const { messages, send, streamingId } = useChat(meetingId, token);
  if (open) {
    return (
      <ChatWindow
        messages={messages}
        streamingId={streamingId}
        onSend={(content) => {
          void send(content);
        }}
        onCollapse={() => setOpen(false)}
      />
    );
  }
  return <AskPill onExpand={() => setOpen(true)} />;
}
