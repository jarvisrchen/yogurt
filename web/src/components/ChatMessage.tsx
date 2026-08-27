import type { ReactNode } from "react";
import type { ChatMessage as Msg } from "../lib/api";

interface Props {
  message: Msg;
  isStreaming?: boolean;
}

/**
 * Inline bold: splits on `**text**` and wraps matches in `<strong>`. No
 * other inline syntax (italic, links, code) — the LLM's chat replies only
 * ever reach for bold + bullets in practice, and a fuller parser is more
 * surface than a 480px chat bubble needs.
 */
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

/**
 * Task NOTES-12(c) — smallest-correct assistant-reply renderer: newlines
 * become paragraph breaks, consecutive `- `/`* ` lines become a `<ul>`,
 * everything else gets bold-inline handling. Deliberately not a general
 * markdown parser (no headings/links/code) — chat replies are short
 * conversational answers, not documents.
 */
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

/**
 * Phase 6 (Plan 06-02) — single chat bubble.
 *
 * User bubbles: right-aligned, blueberry background (`--color-blue`),
 * white text. Assistant bubbles: left-aligned, cream `--color-card`
 * background with a `--color-line` border. The asymmetric rounded
 * corners (rounded-br-md / rounded-bl-md) point the speech bubble
 * toward its speaker per PRD §16 chat motif.
 *
 * `isStreaming` only renders the blinking caret on the active assistant
 * bubble — never on user bubbles. The caret uses the shared `animate-blink`
 * token (index.css `--animate-blink`, 1.0s step-end) rather than Tailwind's
 * built-in `animate-pulse` so it reads as the same cursor-blink motif as
 * the notes editor (task NOTES-12(d)).
 */
export function ChatMessage({ message, isStreaming = false }: Props) {
  const isUser = message.role === "user";
  // A finished assistant turn whose content is empty/whitespace (the model
  // returned nothing — e.g. a question with no transcript to answer from)
  // would otherwise render as a blank box that reads as a hung request.
  // Show a muted placeholder instead. Never applies while still streaming.
  const isEmptyFinishedReply =
    !isUser && !isStreaming && message.content.trim() === "";
  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={
          isUser
            ? "max-w-[78%] px-3 py-2 rounded-2xl rounded-br-md bg-[var(--color-blue)] text-white text-[14px] leading-relaxed"
            : "max-w-[78%] px-3 py-2 rounded-2xl rounded-bl-md bg-[var(--color-card)] border border-[var(--color-line)] text-[var(--color-ink)] text-[14px] leading-relaxed"
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
