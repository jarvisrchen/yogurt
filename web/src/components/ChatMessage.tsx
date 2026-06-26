import type { ChatMessage as Msg } from "../lib/api";

interface Props {
  message: Msg;
  isStreaming?: boolean;
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
 * bubble — never on user bubbles.
 */
export function ChatMessage({ message, isStreaming = false }: Props) {
  const isUser = message.role === "user";
  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={
          isUser
            ? "max-w-[78%] px-3 py-2 rounded-2xl rounded-br-md bg-[var(--color-blue)] text-white text-[14px] leading-relaxed"
            : "max-w-[78%] px-3 py-2 rounded-2xl rounded-bl-md bg-[var(--color-card)] border border-[var(--color-line)] text-[var(--color-ink)] text-[14px] leading-relaxed"
        }
      >
        {message.content}
        {isStreaming && !isUser && (
          <span
            aria-hidden="true"
            className="inline-block w-[6px] h-[14px] ml-1 align-middle bg-[var(--color-ink)] opacity-60 animate-pulse"
          />
        )}
      </div>
    </div>
  );
}
