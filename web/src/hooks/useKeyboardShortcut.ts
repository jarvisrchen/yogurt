import { useEffect } from "react";

/**
 * Phase 6 (Plan 06-02) — generic keyboard-shortcut hook.
 *
 * Attaches a `keydown` listener on `window` that fires `handler(e)` when
 * the configured key (case-insensitive) is pressed, optionally requiring
 * the platform meta key (⌘ on macOS, Ctrl on Windows/Linux). The handler
 * receives the raw `KeyboardEvent` so consumers can `preventDefault`,
 * `stopPropagation`, or read modifier state if they need to — the hook
 * also calls `e.preventDefault()` BEFORE invoking `handler` so the browser
 * default for ⌘K (focus the URL bar in some browsers) does not interfere.
 *
 * The dependency array intentionally includes `handler` so consumers
 * passing inline arrow functions get the latest closure each render —
 * mirrors the React docs' recommendation for event-listener effects.
 */
export interface UseKeyboardShortcutOptions {
  /** The key as it appears in `KeyboardEvent.key` (case-insensitive). */
  key: string;
  /** Require ⌘ on macOS or Ctrl on Windows/Linux. Defaults to `false`. */
  metaOrCtrl?: boolean;
  /** When `false`, the listener is detached. Defaults to `true`. */
  enabled?: boolean;
  /**
   * When `true`, the shortcut does not fire (and does not
   * `preventDefault`) while an input, textarea, select, or
   * contenteditable element has focus. Defaults to `false`.
   */
  ignoreWhenTyping?: boolean;
}

export function useKeyboardShortcut(
  opts: UseKeyboardShortcutOptions,
  handler: (e: KeyboardEvent) => void,
): void {
  const { key, metaOrCtrl = false, enabled = true, ignoreWhenTyping = false } = opts;
  useEffect(() => {
    if (!enabled) return;
    const lowerKey = key.toLowerCase();
    const listener = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() !== lowerKey) return;
      if (metaOrCtrl && !(e.metaKey || e.ctrlKey)) return;
      if (ignoreWhenTyping && isEditableTarget(e.target)) return;
      e.preventDefault();
      handler(e);
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [key, metaOrCtrl, enabled, ignoreWhenTyping, handler]);
}

/** True when the event target is a text-entry surface (input, textarea,
 *  select, or a contenteditable region such as the TipTap editor). */
function isEditableTarget(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  return (
    t.isContentEditable ||
    t.tagName === "INPUT" ||
    t.tagName === "TEXTAREA" ||
    t.tagName === "SELECT"
  );
}
