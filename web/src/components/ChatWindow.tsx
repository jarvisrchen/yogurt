import { useEffect, useRef, useState } from "react";
import type { ChatMessage as Msg } from "../lib/api";
import { ChatMessage } from "./ChatMessage";
import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";

interface Props {
  messages: Msg[];
  streamingId: string | null;
  onSend: (content: string) => void;
  onCollapse: () => void;
}

/**
 * Phase 6 (Plan 06-02) — expanded chat panel.
 *
 * Anchored fixed bottom-center at 480px wide (same horizontal anchor as
 * `<AskPill />` so the popUp keyframe reads as a single morph).
 *
 * Sticky semantics (CHAT spec): outside-click does NOT collapse the
 * window — only the chevron caret in the header does. We deliberately
 * do not attach a document mousedown listener. Escape does collapse it
 * (task NOTES-12(b)) — a keyboard user shouldn't need to tab to the caret.
 *
 * Auto-scroll-to-bottom keeps the latest streamed bubble in view when
 * new content arrives.
 *
 * Task NOTES-12(a): the input autofocuses on mount so a user who just hit
 * ⌘K or clicked the pill can start typing immediately — previously the
 * panel opened with focus left on whatever the user last clicked, so the
 * first few keystrokes went nowhere.
 */
export function ChatWindow({
  messages,
  streamingId,
  onSend,
  onCollapse,
}: Props) {
  const [draft, setDraft] = useState("");
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const el = scrollerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useKeyboardShortcut({ key: "Escape" }, onCollapse);

  function trySubmit() {
    const trimmed = draft.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setDraft("");
  }

  return (
    <div
      role="region"
      aria-label="Ask the meeting chat"
      className="fixed bottom-6 left-1/2 -translate-x-1/2 w-[480px] max-h-[60vh] flex flex-col bg-[var(--color-paper)] border border-[var(--color-line)] rounded-2xl shadow-pop anim-popUp z-30"
    >
      <header className="flex items-center justify-between px-4 py-3 border-b border-[var(--color-line)]">
        <div className="flex items-center gap-2">
          {/* Phase 6 placeholder for the yogurt swirl logo (brand asset
              lands as a Phase 9 polish item). The blueberry dot is the
              same shape + tone the AskPill arrow circle uses, so the
              swap is a one-line change. */}
          <div
            aria-hidden="true"
            className="h-5 w-5 rounded-full bg-[var(--color-blue)]"
          />
          <span className="text-[14px] font-semibold text-[var(--color-ink)]">
            Ask the meeting
          </span>
        </div>
        <button
          type="button"
          onClick={onCollapse}
          aria-label="Collapse chat"
          className="inline-flex items-center justify-center h-7 w-7 rounded-md text-[var(--color-mut)] hover:bg-[var(--color-blsoft)] hover:text-[var(--color-blue)] transition-colors"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path
              d="M3.5 5.5L7 9L10.5 5.5"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      </header>
      <div
        ref={scrollerRef}
        className="flex-1 overflow-y-auto px-4 py-3 space-y-2 min-h-[140px]"
      >
        {messages.length === 0 ? (
          <p className="italic text-[13px] text-center text-[var(--color-mut)] py-6">
            Ask anything about what&apos;s been said so far.
          </p>
        ) : (
          messages.map((m) => (
            <ChatMessage
              key={m.id}
              message={m}
              isStreaming={streamingId === m.id}
            />
          ))
        )}
      </div>
      <footer className="px-3 py-3 border-t border-[var(--color-line)]">
        <div className="flex items-center h-9 px-3 rounded-full bg-[var(--color-card)] border border-[var(--color-line)] focus-within:ring-2 focus-within:ring-blue/40 transition-colors">
          <input
            ref={inputRef}
            type="text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                trySubmit();
              }
            }}
            placeholder="Ask this meeting…"
            className="w-full bg-transparent text-[14px] text-[var(--color-ink)] placeholder:text-[var(--color-mut)] outline-none"
          />
        </div>
      </footer>
    </div>
  );
}
