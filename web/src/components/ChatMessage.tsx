import type { ReactNode } from "react";
import type { ChatMessage as Msg } from "../lib/api";

interface Props {
  message: Msg;
  isStreaming?: boolean;
}

/** Bold only (`**text**`): chat replies don't use other inline syntax. */
function renderInline(text: string, keyPrefix: string): ReactNode {
  const parts = text.split(/(\*\*[^*]+\*\*)/g).filter((p) => p !== "");
  return parts.map((part, i) =>
    part.startsWith("**") && part.endsWith("**") ? (
      <strong key={`${keyPrefix}-${i}`}>{part.slice(2, -2)}</strong>
    ) : (
      <span key={`${keyPrefix}-${i}`}>{part}</span>
    ),
  );
}

/** Deliberately not a markdown parser: paragraphs, `- `/`* ` bullet lists and bold cover short chat replies. */
function MiniMarkdown({ content }: { content: string }) {
  const blocks: ReactNode[] = [];
  let bulletBuf: string[] = [];
  const flushBullets = () => {
    if (bulletBuf.length === 0) return;
    const items = bulletBuf;
    bulletBuf = [];
    blocks.push(
      <ul key={`ul-${blocks.length}`} className="list-disc pl-4 my-1 space-y-0.5">
        {items.map((item, i) => (
          <li key={i}>{renderInline(item, `li-${blocks.length}-${i}`)}</li>
        ))}
      </ul>,
    );
  };
  content.split("\n").forEach((line, i) => {
    const bulletMatch = /^\s*[-*]\s+(.*)$/.exec(line);
    if (bulletMatch) {
      bulletBuf.push(bulletMatch[1]!);
      return;
    }
    flushBullets();
    if (line.trim() !== "") {
      blocks.push(
        <p key={`p-${i}`} className="m-0">
          {renderInline(line, `p-${i}`)}
        </p>,
      );
    }
  });
  flushBullets();
  return <>{blocks}</>;
}

export function ChatMessage({ message, isStreaming = false }: Props) {
  const isUser = message.role === "user";
  // A finished turn with empty content (e.g. no transcript to answer from) would render a blank box that reads as a hung request.
  // Show a muted placeholder instead. Never while streaming.
  const isEmptyFinishedReply =
    !isUser && !isStreaming && message.content.trim() === "";
  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={
          isUser
            ? "max-w-[78%] px-3 py-2 rounded-panel rounded-br-chip bg-[var(--color-blue)] text-white text-[14px] leading-relaxed"
            : "max-w-[78%] px-3 py-2 rounded-panel rounded-bl-chip bg-[var(--color-card)] border border-[var(--color-line)] text-[var(--color-ink)] text-[14px] leading-relaxed"
        }
      >
        {isEmptyFinishedReply ? (
          <span className="italic text-[var(--color-mut)]">No response.</span>
        ) : isUser ? (
          message.content
        ) : (
          <MiniMarkdown content={message.content} />
        )}
        {isStreaming && !isUser && (
          <span
            aria-hidden="true"
            className="inline-block w-[6px] h-[14px] ml-1 align-middle bg-[var(--color-ink)] opacity-60 animate-blink"
          />
        )}
      </div>
    </div>
  );
}
