import { useState } from "react";
import { AskPill } from "./AskPill";
import { ChatWindow } from "./ChatWindow";
import { useChat } from "../hooks/useChat";

interface Props {
  meetingId: string | null;
  token: string | null;
}

/**
 * Phase 6 (Plan 06-02) — wrapper that morphs between the collapsed pill
 * and the expanded chat window. The 260ms popUp keyframe + shared
 * bottom-center anchor make the morph read as one motion to the user.
 *
 * `meetingId` / `token` can both be null during initial bootstrap
 * (before the session token resolves, or while `/meeting/new` is still
 * minting the meeting id). We render the pill anyway so its presence is
 * never gated on data load — the underlying `useChat` hook is dormant
 * in that state and `send()` becomes a no-op.
 */
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
