import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import type { ChatMessage as Msg } from "../lib/api";
import { ChatMessage } from "./ChatMessage";
import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";
import { onControl, usePointerDrag } from "../lib/usePointerDrag";
import { useFits } from "../lib/useFits";

// Pull the header down this far, or flick it faster than SWIPE_FLICK px/ms, and the window collapses.
// A pull shorter than SWIPE_MIN is a tap with a shaky finger, however fast.
const SWIPE = 80;
const SWIPE_FLICK = 0.5;
const SWIPE_MIN = 8;

export interface ChatCopy {
  title: string;
  placeholder: string;
  hint: string;
  suggestions: string[];
}

export const MEETING_CHAT_COPY: ChatCopy = {
  title: "Ask the meeting",
  placeholder: "Ask this meeting…",
  hint: "Ask anything about what's been said so far.",
  suggestions: ["Catch me up", "What did I miss?"],
};

export const LIBRARY_CHAT_COPY: ChatCopy = {
  title: "Ask across meetings",
  placeholder: "Ask across meetings…",
  hint: "Ask about anything from your past meetings.",
  suggestions: ["What did I decide last week?", "What are my open action items?"],
};

interface Props {
  messages: Msg[];
  streamingId: string | null;
  onSend: (content: string) => void;
  onCollapse: () => void;
  copy?: ChatCopy;
  /** Shown under each finished reply, such as the meetings it cites. */
  renderExtra?: (message: Msg) => ReactNode;
  /** Where the window sits. The default floats it over the bottom center of the page. */
  position?: string;
}

/** `.ask-floating` in index.css keeps it under the notes and clear of an open transcript dock. */
export const FLOATING = "fixed bottom-6 -translate-x-1/2 ask-floating";

/** Sticky: outside-click does not collapse the window (no document mousedown listener). Only the header caret, a swipe down on the header, and Escape do. */
export function ChatWindow({
  messages,
  streamingId,
  onSend,
  onCollapse,
  copy = MEETING_CHAT_COPY,
  renderExtra,
  position = FLOATING,
}: Props) {
  const [draft, setDraft] = useState("");
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const regionRef = useRef<HTMLDivElement | null>(null);
  const { start } = usePointerDrag("chat-dragging");

  // A touch on the messages swipes too while they fit, since there is nothing to scroll. Once they overflow, they scroll and the header swipes.
  const [scrollerEl, setScrollerEl] = useState<HTMLDivElement | null>(null);
  const scrollerCallback = useCallback((el: HTMLDivElement | null) => {
    scrollerRef.current = el;
    setScrollerEl(el);
  }, []);
  const fits = useFits(scrollerEl);

  // The pull moves the window through its style, so a drag does not re-render every message.
  function swipe(e: React.PointerEvent) {
    const el = regionRef.current;
    if (!el || onControl(e.target)) return;
    let pull = 0;
    // popUp holds `transform`, and the floating position centers with Tailwind's x translate, so the pull moves only the y of `translate`.
    const place = (y: number) => (el.style.translate = y ? `var(--tw-translate-x, 0) ${y}px` : "");
    start(
      e,
      (_, dy) => {
        pull = Math.max(0, dy);
        el.style.transition = "none";
        place(pull);
      },
      (moved, cancelled, speed) => {
        if (moved && !cancelled && pull >= SWIPE_MIN && (pull > SWIPE || speed > SWIPE_FLICK)) return onCollapse();
        if (!pull) return;
        // The spring back clears its transition after, so the dock's slide of `.ask-floating` still applies.
        el.style.transition = "translate 200ms ease-out";
        el.addEventListener("transitionend", () => (el.style.transition = ""), { once: true });
        place(0);
      },
    );
  }

  useEffect(() => {
    const el = scrollerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Escape typed into another field, such as a search box, belongs to that field. The window's own input handles it below.
  useKeyboardShortcut({ key: "Escape", ignoreWhenTyping: true }, onCollapse);

  function trySubmit() {
    const trimmed = draft.trim();
    // One question streams at a time, and a follow-up typed meanwhile stays in the field until it can go.
    if (!trimmed || streamingId) return;
    onSend(trimmed);
    setDraft("");
  }

  return (
    <div
      role="region"
      ref={regionRef}
      aria-label={`${copy.title} chat`}
      className={`${position} max-h-[60vh] flex flex-col bg-[var(--color-paper)] border border-[var(--color-line)] rounded-panel shadow-pop anim-popUp z-30`}
    >
      <header onPointerDown={swipe} className="flex items-center justify-between px-4 py-3 border-b border-[var(--color-line)] touch-none select-none cursor-grab">
        <div className="flex items-center gap-2">
          {/* Placeholder for the swirl logo: same shape and tone as the AskPill arrow circle. */}
          <div
            aria-hidden="true"
            className="h-5 w-5 rounded-full bg-[var(--color-blue)]"
          />
          <span className="text-[14px] font-semibold text-[var(--color-ink)]">
            {copy.title}
          </span>
        </div>
        <button
          type="button"
          onClick={onCollapse}
          aria-label="Collapse chat"
          className="inline-flex items-center justify-center h-7 w-7 rounded-chip text-[var(--color-mut)] hover:bg-[var(--color-blsoft)] hover:text-[var(--color-blue)] transition-colors"
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
        ref={scrollerCallback}
        onPointerDown={(e) => fits && e.pointerType === "touch" && swipe(e)}
        className={`flex-1 overflow-y-auto px-4 py-3 space-y-2 min-h-[140px] ${fits ? "touch-none" : ""}`}
      >
        {messages.length === 0 ? (
          <div className="flex flex-col items-center gap-3 py-6">
            <p className="italic text-[13px] text-center text-[var(--color-mut)]">
              {copy.hint}
            </p>
            <div className="flex flex-wrap justify-center gap-2">
              {copy.suggestions.map((q) => (
                <button
                  key={q}
                  type="button"
                  onClick={() => onSend(q)}
                  className="px-3 py-1.5 rounded-chip text-[13px] text-[var(--color-ink)] bg-[var(--color-blsoft)] hover:bg-[var(--color-blue)] hover:text-white transition-colors"
                >
                  {q}
                </button>
              ))}
            </div>
          </div>
        ) : (
          messages.map((m) => (
            <ChatMessage
              key={m.id}
              message={m}
              isStreaming={streamingId === m.id}
            >
              {renderExtra?.(m)}
            </ChatMessage>
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
              // Enter that commits an IME composition picks the characters and must not send.
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                trySubmit();
              } else if (e.key === "Escape") {
                e.preventDefault();
                onCollapse();
              }
            }}
            placeholder={copy.placeholder}
            className="w-full bg-transparent text-[14px] text-[var(--color-ink)] placeholder:text-[var(--color-mut)] outline-none"
          />
        </div>
      </footer>
    </div>
  );
}
