import { useEffect } from "react";

export interface UseKeyboardShortcutOptions {
  key: string;
  metaOrCtrl?: boolean;
  enabled?: boolean;
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

const NON_TEXT_INPUT_TYPES = new Set([
  "checkbox",
  "radio",
  "button",
  "submit",
  "reset",
  "range",
  "color",
  "file",
  "image",
]);

function isEditableTarget(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  if (t.isContentEditable || t.tagName === "TEXTAREA" || t.tagName === "SELECT") {
    return true;
  }
  if (t.tagName === "INPUT") {
    return !NON_TEXT_INPUT_TYPES.has((t as HTMLInputElement).type);
  }
  return false;
}
